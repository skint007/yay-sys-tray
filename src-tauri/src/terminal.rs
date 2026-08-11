use crate::tailscale::ssh_target;
use crate::tray::TrayState;
use crate::AppState;
use tauri::{Emitter, Manager};
use tokio::process::Command;

/// How to invoke a given terminal emulator. One row per known terminal instead
/// of the same knowledge scattered across three parallel match arms, so adding
/// a terminal is a single table entry and the flags for one terminal can't drift
/// apart.
struct TermSpec {
    /// Leading argv — the program plus any subcommand (e.g. `["wezterm", "start"]`).
    argv0: &'static [&'static str],
    /// Flag/separator placed just before the command to run (`-e`, `--`, `-x`),
    /// or `None` when the command is positional (kitty/foot).
    exec_flag: Option<&'static str>,
    /// Flag that keeps the window open after the command exits, if supported.
    hold_flag: Option<&'static str>,
    /// Flag that makes the launcher stay alive until the command exits, for
    /// client/server terminals whose CLI otherwise returns as soon as the
    /// server owns the window. Without it the update looks finished the moment
    /// it starts.
    wait_flag: Option<&'static str>,
    /// Flag that sets the window title, if supported.
    title_flag: Option<&'static str>,
}

/// Look up a terminal's invocation spec. Unknown terminals fall back to a
/// minimal `<term> -e <cmd>` with no hold/title flags: `-e` is the most widely
/// supported exec flag, and blindly adding `--hold`/title flags a terminal
/// doesn't accept makes it reject the whole command line and never launch.
fn term_spec(terminal: &str) -> TermSpec {
    match terminal {
        "kitty" => TermSpec { argv0: &["kitty"], exec_flag: None, wait_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        "konsole" => TermSpec { argv0: &["konsole"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("--hold"), title_flag: None },
        "alacritty" => TermSpec { argv0: &["alacritty"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        "foot" => TermSpec { argv0: &["foot"], exec_flag: None, wait_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        "xterm" => TermSpec { argv0: &["xterm"], exec_flag: Some("-e"), wait_flag: None, hold_flag: Some("-hold"), title_flag: Some("-T") },
        "xfce4-terminal" => TermSpec { argv0: &["xfce4-terminal"], exec_flag: Some("-x"), wait_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        // gnome-terminal/ptyxis/wezterm take the command after `--`; none has a
        // usable hold flag, so leave it off rather than break the launch.
        "gnome-terminal" => TermSpec { argv0: &["gnome-terminal"], exec_flag: Some("--"), wait_flag: Some("--wait"), hold_flag: None, title_flag: None },
        "ptyxis" => TermSpec { argv0: &["ptyxis"], exec_flag: Some("--"), wait_flag: None, hold_flag: None, title_flag: None },
        "wezterm" => TermSpec { argv0: &["wezterm", "start"], exec_flag: Some("--"), wait_flag: None, hold_flag: None, title_flag: None },
        _ => TermSpec { argv0: &[], exec_flag: Some("-e"), wait_flag: None, hold_flag: None, title_flag: None },
    }
}

/// Build a terminal command prefix, optionally holding the window open and
/// setting its title. Order: `program [wait] [hold] [title T] [exec_flag]` then
/// the command the caller appends.
fn terminal_prefix(terminal: &str, title: Option<&str>, hold: bool) -> Vec<String> {
    let spec = term_spec(terminal);
    let mut out: Vec<String> = Vec::new();

    if spec.argv0.is_empty() {
        // Unknown terminal: the configured name is the program.
        out.push(terminal.to_string());
    } else {
        out.extend(spec.argv0.iter().map(|s| s.to_string()));
    }

    if let Some(flag) = spec.wait_flag {
        out.push(flag.to_string());
    }
    if hold {
        if let Some(flag) = spec.hold_flag {
            out.push(flag.to_string());
        }
    }
    if let (Some(title), Some(flag)) = (title, spec.title_flag) {
        out.push(flag.to_string());
        out.push(title.to_string());
    }
    if let Some(flag) = spec.exec_flag {
        out.push(flag.to_string());
    }

    out
}

/// Wrap a reboot command with the configured delay (Ctrl+C in the terminal cancels).
fn delayed_reboot_cmd(reboot_cmd: &str, delay: u32) -> String {
    if delay == 0 {
        reboot_cmd.to_string()
    } else {
        format!("echo 'Rebooting in {delay}s (Ctrl+C to cancel)...' && sleep {delay} && {reboot_cmd}")
    }
}

/// Assemble a pacman/yay command line as a single shell string, appending
/// `--noconfirm` and an optional `&& <reboot>` chain. Used for the cases that
/// must run through a shell (the reboot chain, and every remote command, which
/// runs under the ssh login shell). One place so the noconfirm/reboot logic
/// can't diverge across the six update/remove entry points.
fn build_shell_cmd(base: &str, noconfirm: bool, reboot: Option<(&str, u32)>) -> String {
    let mut cmd = base.to_string();
    if noconfirm {
        cmd.push_str(" --noconfirm");
    }
    if let Some((reboot_cmd, delay)) = reboot {
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd(reboot_cmd, delay)));
    }
    cmd
}

/// The reboot chain to append when `restart` is set, else `None`.
fn reboot_chain(restart: bool, reboot_cmd: &'static str, delay: u32) -> Option<(&'static str, u32)> {
    restart.then_some((reboot_cmd, delay))
}

/// argv for an interactive remote command.
///
/// `-t` forces a pty on the far side. Without one `sudo` has nowhere to prompt
/// and dies with "a terminal is required to read the password", so remote
/// updates only ever worked on hosts with NOPASSWD. These commands always run
/// inside a terminal emulator, so the local stdin `-t` needs is present.
///
/// Deliberately not used by the *check* path in `tailscale.rs`: that output is
/// parsed, and a pty would echo and line-wrap it.
fn ssh_argv(target: String, cmd: String) -> Vec<String> {
    vec!["ssh".to_string(), "-t".to_string(), target, cmd]
}

/// Append the reboot chain to a command that already carries its own flags.
/// Separate from [`build_shell_cmd`] because the remote commands below end in
/// `fi`, and a trailing ` --noconfirm` would land after it rather than on the
/// pacman/yay call.
fn with_reboot(base: String, reboot: Option<(&str, u32)>) -> String {
    match reboot {
        Some((cmd, delay)) => format!("{base} && {}", delayed_reboot_cmd(cmd, delay)),
        None => base,
    }
}

/// Full-system update for a remote host.
///
/// yay covers repo and AUR packages in one pass, so it is preferred wherever
/// the host has it. Plain `pacman -Syu` skips foreign packages entirely, so on
/// a host without yay the AUR updates this app reports cannot be applied at
/// all. `aur_pending` is how many of them the last check found: when the
/// fallback runs with any of those outstanding it says so first, because
/// pacman's own "there is nothing to do" reads as "already up to date" (#29).
/// The yay check runs on the host, so no extra probe round-trip is needed.
fn remote_full_update_cmd(noconfirm: bool, aur_pending: usize) -> String {
    let flag = if noconfirm { " --noconfirm" } else { "" };
    let pacman = format!("sudo pacman -Syu{flag}");
    let without_yay = if aur_pending > 0 {
        // Repo packages still update — the warning qualifies the run rather
        // than replacing it, matching how remote_install_cmd handles a mixed
        // selection.
        format!(
            "echo 'yay is not installed on this host, so its {aur_pending} AUR update(s) \
             cannot be applied from here; updating repo packages only' >&2; {pacman}"
        )
    } else {
        pacman
    };
    format!("if command -v yay >/dev/null 2>&1; then yay -Syu{flag}; else {without_yay}; fi")
}

/// Install a chosen set of packages on a remote host.
///
/// `yay -S` accepts repo and AUR names together, so the preferred branch just
/// passes everything. The pacman fallback is given repo names only on purpose:
/// `pacman -S` aborts the whole transaction on a name missing from the sync
/// databases, so including an AUR name there would stop the repo packages from
/// installing too. When that costs the user something, the command says so
/// rather than quietly installing a subset.
fn remote_install_cmd(selected: &[String], repo_only: &[String], noconfirm: bool) -> String {
    let flag = if noconfirm { " --noconfirm" } else { "" };
    let with_yay = format!("yay -S {}{flag}", selected.join(" "));

    let without_yay = if repo_only.is_empty() {
        "echo 'yay is not installed on this host, and every selected package is from the AUR' >&2; exit 1"
            .to_string()
    } else if repo_only.len() == selected.len() {
        format!("sudo pacman -S {}{flag}", repo_only.join(" "))
    } else {
        format!(
            "echo 'yay is not installed on this host, skipping the selected AUR packages' >&2; \
             sudo pacman -S {}{flag}",
            repo_only.join(" ")
        )
    };

    format!("if command -v yay >/dev/null 2>&1; then {with_yay}; else {without_yay}; fi")
}

/// Passwordless installs can reboot without sudo (a NOPASSWD systemctl call).
fn local_reboot_cmd(passwordless: bool) -> &'static str {
    if passwordless {
        "systemctl reboot"
    } else {
        "sudo reboot"
    }
}

/// What kind of run a terminal was carrying. Both kinds re-check the target
/// afterwards, but only an update run can satisfy "close the window after
/// updating" — removing a package is not an update.
#[derive(Clone, Copy)]
enum FinishedAction {
    Update,
    Remove,
}

impl FinishedAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Remove => "remove",
        }
    }
}

struct TermCfg {
    terminal: String,
    noconfirm: bool,
    hold: bool,
    passwordless: bool,
    delay: u32,
    ssh_user: String,
}

async fn term_cfg(app_handle: &tauri::AppHandle) -> TermCfg {
    let state = app_handle.state::<AppState>();
    let config = state.config.read().await;
    TermCfg {
        terminal: config.terminal.clone(),
        noconfirm: config.noconfirm,
        hold: config.hold_terminal,
        passwordless: config.passwordless_updates,
        delay: config.restart_delay_seconds,
        ssh_user: config.tailscale_ssh_user.clone(),
    }
}

/// Launch a local full system update in a terminal.
pub async fn run_local_update(app_handle: tauri::AppHandle, restart: bool) {
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some("Updating: local"), cfg.hold);

    let yay_cmd = if restart {
        let reboot = local_reboot_cmd(cfg.passwordless);
        let cmd = build_shell_cmd("yay -Syu", cfg.noconfirm, Some((reboot, cfg.delay)));
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
        let mut cmd = vec!["yay".to_string(), "-Syu".to_string()];
        if cfg.noconfirm {
            cmd.push("--noconfirm".to_string());
        }
        cmd
    };

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string(), FinishedAction::Update).await;
}

/// Update only the selected local packages (`yay -S <pkgs>`).
pub async fn run_local_update_packages(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    restart: bool,
) {
    if packages.is_empty() {
        return run_local_update(app_handle, restart).await;
    }
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some("Updating: selected"), cfg.hold);

    let yay_cmd = if restart {
        let reboot = local_reboot_cmd(cfg.passwordless);
        let base = format!("yay -S {}", packages.join(" "));
        let cmd = build_shell_cmd(&base, cfg.noconfirm, Some((reboot, cfg.delay)));
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
        // No reboot chain needed, so pass packages as separate argv (no shell).
        let mut cmd = vec!["yay".to_string(), "-S".to_string()];
        cmd.extend(packages);
        if cfg.noconfirm {
            cmd.push("--noconfirm".to_string());
        }
        cmd
    };

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string(), FinishedAction::Update).await;
}

/// How many of a host's pending updates came from the AUR at its last check.
/// Read from the stored results rather than probed, so the warning the fallback
/// prints names the same updates the window is showing.
async fn pending_aur_count(app_handle: &tauri::AppHandle, hostname: &str) -> usize {
    let tray_state = app_handle.state::<TrayState>();
    let hosts = tray_state.remote_results.read().await;
    hosts
        .iter()
        .find(|h| h.hostname == hostname)
        .map(|h| h.updates.iter().filter(|u| u.is_aur()).count())
        .unwrap_or(0)
}

/// Launch a remote full system update via SSH in a terminal.
pub async fn run_remote_update(app_handle: tauri::AppHandle, hostname: &str, restart: bool) {
    let cfg = term_cfg(&app_handle).await;
    let aur_pending = pending_aur_count(&app_handle, hostname).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname}")), cfg.hold);

    let cmd = with_reboot(
        remote_full_update_cmd(cfg.noconfirm, aur_pending),
        reboot_chain(restart, "sudo reboot", cfg.delay),
    );

    spawn_with(app_handle, prefix, ssh_argv(target, cmd), hostname.to_string(), FinishedAction::Update).await;
}

/// Update only the selected packages on a remote host. `selected` is every
/// chosen package; `repo_only` is the subset that lives in a sync database,
/// which is all the pacman fallback may be given.
pub async fn run_remote_update_packages(
    app_handle: tauri::AppHandle,
    hostname: &str,
    selected: Vec<String>,
    repo_only: Vec<String>,
    restart: bool,
) {
    if selected.is_empty() {
        return run_remote_update(app_handle, hostname, restart).await;
    }
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix =
        terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname} (selected)")), cfg.hold);

    let cmd = with_reboot(
        remote_install_cmd(&selected, &repo_only, cfg.noconfirm),
        reboot_chain(restart, "sudo reboot", cfg.delay),
    );

    spawn_with(app_handle, prefix, ssh_argv(target, cmd), hostname.to_string(), FinishedAction::Update).await;
}

/// Remove a local package in a terminal.
pub async fn run_remove(app_handle: tauri::AppHandle, package: &str, flags: &str) {
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Removing: {package}")), cfg.hold);

    let mut yay_cmd = vec!["yay".to_string(), format!("-{flags}"), package.to_string()];
    if cfg.noconfirm {
        yay_cmd.push("--noconfirm".to_string());
    }

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string(), FinishedAction::Remove).await;
}

/// Remove a package on a remote host via SSH.
pub async fn run_remote_remove(
    app_handle: tauri::AppHandle,
    hostname: &str,
    package: &str,
    flags: &str,
) {
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix =
        terminal_prefix(&cfg.terminal, Some(&format!("Removing: {package} ({hostname})")), cfg.hold);

    let base = format!("sudo pacman -{flags} {package}");
    let cmd = build_shell_cmd(&base, cfg.noconfirm, None);

    spawn_with(app_handle, prefix, ssh_argv(target, cmd), hostname.to_string(), FinishedAction::Remove).await;
}

async fn spawn_with(
    app_handle: tauri::AppHandle,
    prefix: Vec<String>,
    cmd: Vec<String>,
    scope: String,
    action: FinishedAction,
) {
    let mut full = prefix;
    full.extend(cmd);
    spawn_and_wait(app_handle, full, scope, action).await;
}

/// Spawn a terminal command, wait for it to finish, then emit update-finished
/// carrying the scope ("local" or a hostname) so only that target gets
/// re-checked rather than the whole fleet, plus the action that ran so a
/// removal is never mistaken for a completed update.
async fn spawn_and_wait(
    app_handle: tauri::AppHandle,
    cmd: Vec<String>,
    scope: String,
    action: FinishedAction,
) {
    if cmd.is_empty() {
        return;
    }
    let program = cmd[0].clone();
    let args: Vec<String> = cmd[1..].to_vec();

    match Command::new(&program).args(&args).spawn() {
        Ok(mut child) => {
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
                let _ = app_handle.emit(
                    "update-finished",
                    serde_json::json!({ "scope": scope, "action": action.as_str() }),
                );
            });
        }
        Err(e) => log::error!("Failed to spawn terminal: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_update_prefers_yay_and_falls_back_to_pacman() {
        let cmd = remote_full_update_cmd(false, 0);
        assert!(cmd.contains("command -v yay"));
        assert!(cmd.contains("yay -Syu"));
        // pacman -Syu alone would never update the AUR packages this app now
        // reports for remote hosts, so yay must be the preferred branch.
        assert!(cmd.contains("sudo pacman -Syu"));
        assert!(!cmd.contains("--noconfirm"));
        assert_eq!(remote_full_update_cmd(true, 0).matches("--noconfirm").count(), 2);
    }

    #[test]
    fn full_update_without_aur_updates_stays_quiet() {
        // Nothing is being skipped, so a warning would be noise on every host
        // that simply has no AUR packages installed.
        let cmd = remote_full_update_cmd(false, 0);
        assert!(!cmd.contains("echo"));
    }

    #[test]
    fn helperless_full_update_names_the_aur_updates_it_cannot_apply() {
        // Without this, pacman prints "there is nothing to do" and the host
        // reads as up to date while its AUR updates sit there (#29).
        let cmd = remote_full_update_cmd(false, 3);
        let (with_yay, fallback) = cmd.split_once("else").expect("fallback branch");
        assert!(!with_yay.contains("echo"));
        assert!(fallback.contains("3 AUR update(s)"));
        assert!(fallback.contains(">&2"));
        // The repo half is still applied — the warning qualifies the run.
        assert!(fallback.contains("sudo pacman -Syu"));
    }

    #[test]
    fn helperless_warning_still_carries_noconfirm_once_per_branch() {
        let cmd = remote_full_update_cmd(true, 1);
        assert_eq!(cmd.matches("--noconfirm").count(), 2);
    }

    #[test]
    fn install_passes_everything_to_yay() {
        let selected = vec!["repo-pkg".to_string(), "aur-pkg".to_string()];
        let repo_only = vec!["repo-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &repo_only, false);
        assert!(cmd.contains("yay -S repo-pkg aur-pkg"));
    }

    #[test]
    fn pacman_fallback_never_receives_an_aur_name() {
        // `pacman -S` aborts the whole transaction on a name it can't resolve,
        // so an AUR name in the fallback would block the repo updates too.
        let selected = vec!["repo-pkg".to_string(), "aur-pkg".to_string()];
        let repo_only = vec!["repo-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &repo_only, false);
        let fallback = cmd.split("else").nth(1).expect("fallback branch");
        assert!(fallback.contains("sudo pacman -S repo-pkg"));
        assert!(!fallback.contains("aur-pkg"));
        assert!(fallback.contains("skipping the selected AUR packages"));
    }

    #[test]
    fn all_aur_selection_without_yay_fails_loudly() {
        // Nothing pacman can do here, and silently succeeding would leave the
        // user thinking the update ran.
        let selected = vec!["aur-pkg".to_string()];
        let cmd = remote_install_cmd(&selected, &[], false);
        let fallback = cmd.split("else").nth(1).expect("fallback branch");
        assert!(fallback.contains("exit 1"));
        assert!(!fallback.contains("pacman -S "));
    }

    #[test]
    fn repo_only_selection_has_no_warning_noise() {
        let selected = vec!["a".to_string(), "b".to_string()];
        let cmd = remote_install_cmd(&selected, &selected, true);
        assert!(cmd.contains("sudo pacman -S a b --noconfirm"));
        assert!(!cmd.contains("skipping"));
    }

    #[test]
    fn gnome_terminal_waits_for_the_command() {
        // Its CLI hands the window to the terminal server and returns straight
        // away otherwise, which would report the update as finished the moment
        // it started.
        let prefix = terminal_prefix("gnome-terminal", Some("Updating: local"), true);
        assert_eq!(prefix, vec!["gnome-terminal", "--wait", "--"]);
    }

    #[test]
    fn terminals_that_block_get_no_wait_flag() {
        // A flag the terminal doesn't accept makes it reject the whole command
        // line and never launch, so it goes only where it's known to exist.
        let prefix = terminal_prefix("kitty", None, false);
        assert_eq!(prefix, vec!["kitty"]);
        assert!(!terminal_prefix("konsole", None, true).contains(&"--wait".to_string()));
    }

    #[test]
    fn reboot_chain_lands_after_the_conditional() {
        // The command ends in `fi`, so the chain has to be appended rather than
        // folded in the way build_shell_cmd does it.
        let base = remote_full_update_cmd(false, 0);
        let chained = with_reboot(base, Some(("sudo reboot", 30)));
        assert!(chained.contains("fi &&"));
        assert!(chained.trim_end().ends_with("sudo reboot"));
    }
}
