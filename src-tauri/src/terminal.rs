use crate::tailscale::ssh_target;
use crate::AppState;
use tauri::{Emitter, Manager};
use tokio::process::Command;

/// Base terminal command (without hold/title flags).
fn terminal_base(terminal: &str) -> Vec<String> {
    let base: &[&str] = match terminal {
        "kitty" => &["kitty"],
        "konsole" => &["konsole", "-e"],
        "alacritty" => &["alacritty", "-e"],
        "foot" => &["foot"],
        "xterm" => &["xterm", "-e"],
        _ => return vec![terminal.to_string(), "-e".to_string()],
    };
    base.iter().map(|s| s.to_string()).collect()
}

fn hold_flag(terminal: &str) -> &'static str {
    match terminal {
        "xterm" => "-hold",
        _ => "--hold",
    }
}

fn title_flag(terminal: &str) -> Option<&'static str> {
    match terminal {
        "kitty" | "alacritty" | "foot" => Some("--title"),
        "xterm" => Some("-T"),
        _ => None, // konsole has no simple title flag
    }
}

/// Build a terminal command prefix, optionally holding the window open and
/// setting its title.
fn terminal_prefix(terminal: &str, title: Option<&str>, hold: bool) -> Vec<String> {
    let mut base = terminal_base(terminal);

    if hold {
        // Insert the hold flag right after the program name.
        base.insert(1, hold_flag(terminal).to_string());
    }

    if let (Some(title), Some(flag)) = (title, title_flag(terminal)) {
        // Place the title flag before "-e" if present, else append.
        if let Some(pos) = base.iter().position(|s| s == "-e") {
            base.insert(pos, title.to_string());
            base.insert(pos, flag.to_string());
        } else {
            base.push(flag.to_string());
            base.push(title.to_string());
        }
    }

    base
}

/// Wrap a reboot command with the configured delay (Ctrl+C in the terminal cancels).
fn delayed_reboot_cmd(reboot_cmd: &str, delay: u32) -> String {
    if delay == 0 {
        reboot_cmd.to_string()
    } else {
        format!("echo 'Rebooting in {delay}s (Ctrl+C to cancel)...' && sleep {delay} && {reboot_cmd}")
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
        let mut cmd = "yay -Syu".to_string();
        if cfg.noconfirm {
            cmd.push_str(" --noconfirm");
        }
        let reboot = if cfg.passwordless { "systemctl reboot" } else { "sudo reboot" };
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd(reboot, cfg.delay)));
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
        let mut cmd = format!("yay -S {}", packages.join(" "));
        if cfg.noconfirm {
            cmd.push_str(" --noconfirm");
        }
        let reboot = if cfg.passwordless { "systemctl reboot" } else { "sudo reboot" };
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd(reboot, cfg.delay)));
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
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

    let mut cmd = "sudo pacman -Syu".to_string();
    if cfg.noconfirm {
        cmd.push_str(" --noconfirm");
    }
    if restart {
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd("sudo reboot", cfg.delay)));
    }

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

    let mut cmd = format!("sudo pacman -S {}", packages.join(" "));
    if cfg.noconfirm {
        cmd.push_str(" --noconfirm");
    }
    if restart {
        cmd.push_str(&format!(" && {}", delayed_reboot_cmd("sudo reboot", cfg.delay)));
    }

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

    let mut cmd = format!("sudo pacman -{flags} {package}");
    if cfg.noconfirm {
        cmd.push_str(" --noconfirm");
    }

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
