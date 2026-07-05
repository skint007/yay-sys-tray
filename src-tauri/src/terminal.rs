use crate::tailscale::ssh_target;
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
    /// Flag that sets the window title, if supported.
    title_flag: Option<&'static str>,
}

/// Look up a terminal's invocation spec. Unknown terminals fall back to a
/// minimal `<term> -e <cmd>` with no hold/title flags: `-e` is the most widely
/// supported exec flag, and blindly adding `--hold`/title flags a terminal
/// doesn't accept makes it reject the whole command line and never launch.
fn term_spec(terminal: &str) -> TermSpec {
    match terminal {
        "kitty" => TermSpec { argv0: &["kitty"], exec_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        "konsole" => TermSpec { argv0: &["konsole"], exec_flag: Some("-e"), hold_flag: Some("--hold"), title_flag: None },
        "alacritty" => TermSpec { argv0: &["alacritty"], exec_flag: Some("-e"), hold_flag: Some("--hold"), title_flag: Some("--title") },
        "foot" => TermSpec { argv0: &["foot"], exec_flag: None, hold_flag: Some("--hold"), title_flag: Some("--title") },
        "xterm" => TermSpec { argv0: &["xterm"], exec_flag: Some("-e"), hold_flag: Some("-hold"), title_flag: Some("-T") },
        "xfce4-terminal" => TermSpec { argv0: &["xfce4-terminal"], exec_flag: Some("-x"), hold_flag: Some("--hold"), title_flag: Some("--title") },
        // gnome-terminal/ptyxis/wezterm take the command after `--`; none has a
        // usable hold flag, so leave it off rather than break the launch.
        "gnome-terminal" => TermSpec { argv0: &["gnome-terminal"], exec_flag: Some("--"), hold_flag: None, title_flag: None },
        "ptyxis" => TermSpec { argv0: &["ptyxis"], exec_flag: Some("--"), hold_flag: None, title_flag: None },
        "wezterm" => TermSpec { argv0: &["wezterm", "start"], exec_flag: Some("--"), hold_flag: None, title_flag: None },
        _ => TermSpec { argv0: &[], exec_flag: Some("-e"), hold_flag: None, title_flag: None },
    }
}

/// Build a terminal command prefix, optionally holding the window open and
/// setting its title. Order: `program [hold] [title T] [exec_flag]` then the
/// command the caller appends.
fn terminal_prefix(terminal: &str, title: Option<&str>, hold: bool) -> Vec<String> {
    let spec = term_spec(terminal);
    let mut out: Vec<String> = Vec::new();

    if spec.argv0.is_empty() {
        // Unknown terminal: the configured name is the program.
        out.push(terminal.to_string());
    } else {
        out.extend(spec.argv0.iter().map(|s| s.to_string()));
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

/// Passwordless installs can reboot without sudo (a NOPASSWD systemctl call).
fn local_reboot_cmd(passwordless: bool) -> &'static str {
    if passwordless {
        "systemctl reboot"
    } else {
        "sudo reboot"
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

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string()).await;
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

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string()).await;
}

/// Launch a remote full system update via SSH in a terminal.
pub async fn run_remote_update(app_handle: tauri::AppHandle, hostname: &str, restart: bool) {
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname}")), cfg.hold);

    let cmd = build_shell_cmd(
        "sudo pacman -Syu",
        cfg.noconfirm,
        reboot_chain(restart, "sudo reboot", cfg.delay),
    );

    spawn_with(app_handle, prefix, vec!["ssh".to_string(), target, cmd], hostname.to_string()).await;
}

/// Update only the selected packages on a remote host.
pub async fn run_remote_update_packages(
    app_handle: tauri::AppHandle,
    hostname: &str,
    packages: Vec<String>,
    restart: bool,
) {
    if packages.is_empty() {
        return run_remote_update(app_handle, hostname, restart).await;
    }
    let cfg = term_cfg(&app_handle).await;
    let target = ssh_target(hostname, &cfg.ssh_user);
    let prefix =
        terminal_prefix(&cfg.terminal, Some(&format!("Updating: {hostname} (selected)")), cfg.hold);

    let base = format!("sudo pacman -S {}", packages.join(" "));
    let cmd = build_shell_cmd(&base, cfg.noconfirm, reboot_chain(restart, "sudo reboot", cfg.delay));

    spawn_with(app_handle, prefix, vec!["ssh".to_string(), target, cmd], hostname.to_string()).await;
}

/// Remove a local package in a terminal.
pub async fn run_remove(app_handle: tauri::AppHandle, package: &str, flags: &str) {
    let cfg = term_cfg(&app_handle).await;
    let prefix = terminal_prefix(&cfg.terminal, Some(&format!("Removing: {package}")), cfg.hold);

    let mut yay_cmd = vec!["yay".to_string(), format!("-{flags}"), package.to_string()];
    if cfg.noconfirm {
        yay_cmd.push("--noconfirm".to_string());
    }

    spawn_with(app_handle, prefix, yay_cmd, "local".to_string()).await;
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

    spawn_with(app_handle, prefix, vec!["ssh".to_string(), target, cmd], hostname.to_string()).await;
}

async fn spawn_with(
    app_handle: tauri::AppHandle,
    prefix: Vec<String>,
    cmd: Vec<String>,
    scope: String,
) {
    let mut full = prefix;
    full.extend(cmd);
    spawn_and_wait(app_handle, full, scope).await;
}

/// Spawn a terminal command, wait for it to finish, then emit update-finished
/// carrying the scope ("local" or a hostname) so only that target gets
/// re-checked rather than the whole fleet.
async fn spawn_and_wait(app_handle: tauri::AppHandle, cmd: Vec<String>, scope: String) {
    if cmd.is_empty() {
        return;
    }
    let program = cmd[0].clone();
    let args: Vec<String> = cmd[1..].to_vec();

    match Command::new(&program).args(&args).spawn() {
        Ok(mut child) => {
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
                let _ = app_handle.emit("update-finished", serde_json::json!({ "scope": scope }));
            });
        }
        Err(e) => log::error!("Failed to spawn terminal: {e}"),
    }
}
