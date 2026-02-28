use crate::AppState;
use std::collections::HashMap;
use tauri::{Emitter, Manager};
use tokio::process::Command;

/// Terminal command prefixes for various terminal emulators.
fn terminal_cmds() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([
        ("kitty", vec!["kitty", "--hold"]),
        ("konsole", vec!["konsole", "--hold", "-e"]),
        ("alacritty", vec!["alacritty", "--hold", "-e"]),
        ("foot", vec!["foot", "--hold"]),
        ("xterm", vec!["xterm", "-hold", "-e"]),
    ])
}

fn get_terminal_prefix(terminal: &str) -> Vec<String> {
    let cmds = terminal_cmds();
    if let Some(prefix) = cmds.get(terminal) {
        prefix.iter().map(|s| s.to_string()).collect()
    } else {
        vec![terminal.to_string(), "-e".to_string()]
    }
}

/// Launch a local system update in a terminal.
pub async fn run_local_update(app_handle: tauri::AppHandle, restart: bool) {
    let (terminal, noconfirm) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (config.terminal.clone(), config.noconfirm)
    };

    let prefix = get_terminal_prefix(&terminal);

    let yay_cmd = if restart {
        let mut cmd = "yay -Syu".to_string();
        if noconfirm {
            cmd.push_str(" --noconfirm");
        }
        cmd.push_str(" && sudo reboot");
        vec!["bash".to_string(), "-c".to_string(), cmd]
    } else {
        let mut cmd = vec!["yay".to_string(), "-Syu".to_string()];
        if noconfirm {
            cmd.push("--noconfirm".to_string());
        }
        cmd
    };

    let mut full_cmd = prefix;
    full_cmd.extend(yay_cmd);

    spawn_and_wait(app_handle, full_cmd).await;
}

/// Launch a remote system update via SSH in a terminal.
pub async fn run_remote_update(
    app_handle: tauri::AppHandle,
    hostname: &str,
    restart: bool,
) {
    let (terminal, noconfirm) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (config.terminal.clone(), config.noconfirm)
    };

    let prefix = get_terminal_prefix(&terminal);

    let mut cmd = "sudo pacman -Syu".to_string();
    if noconfirm {
        cmd.push_str(" --noconfirm");
    }
    if restart {
        cmd.push_str(" && sudo reboot");
    }

    let ssh_cmd = vec!["ssh".to_string(), hostname.to_string(), cmd];

    let mut full_cmd = prefix;
    full_cmd.extend(ssh_cmd);

    spawn_and_wait(app_handle, full_cmd).await;
}

/// Remove a package in a terminal.
pub async fn run_remove(app_handle: tauri::AppHandle, package: &str, flags: &str) {
    let (terminal, noconfirm) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (config.terminal.clone(), config.noconfirm)
    };

    let prefix = get_terminal_prefix(&terminal);

    let mut yay_cmd = vec![
        "yay".to_string(),
        format!("-{flags}"),
        package.to_string(),
    ];
    if noconfirm {
        yay_cmd.push("--noconfirm".to_string());
    }

    let mut full_cmd = prefix;
    full_cmd.extend(yay_cmd);

    spawn_and_wait(app_handle, full_cmd).await;
}

/// Spawn a terminal command, wait for it to finish, then emit update-finished.
async fn spawn_and_wait(app_handle: tauri::AppHandle, cmd: Vec<String>) {
    if cmd.is_empty() {
        return;
    }

    let program = &cmd[0];
    let args = &cmd[1..];

    match Command::new(program).args(args).spawn() {
        Ok(mut child) => {
            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let _ = child.wait().await;
                let _ = handle.emit("update-finished", ());
            });
        }
        Err(e) => {
            log::error!("Failed to spawn terminal: {e}");
        }
    }
}
