use std::path::Path;
use tokio::process::Command;

pub fn is_arch_linux() -> bool {
    Path::new("/etc/arch-release").exists()
}

pub async fn manage_autostart(enable: bool) -> Result<(), String> {
    let action = if enable { "enable" } else { "disable" };
    let output = Command::new("systemctl")
        .args(["--user", action, "yay-sys-tray.service"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

pub async fn manage_passwordless_updates(enable: bool) -> Result<bool, String> {
    let username = std::env::var("USER").unwrap_or_default();
    if username.is_empty() {
        return Err("Could not determine username".into());
    }

    let sudoers_file = "/etc/sudoers.d/yay-sys-tray";

    if enable {
        let rule = format!("{username} ALL=(ALL) NOPASSWD: /usr/bin/pacman\n");
        let mut child = Command::new("pkexec")
            .args(["tee", sudoers_file])
            .kill_on_drop(true)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;

        if let Some(ref mut stdin) = child.stdin {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(rule.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        let status = child.wait().await.map_err(|e| e.to_string())?;

        Ok(status.success())
    } else {
        let status = Command::new("pkexec")
            .args(["rm", "-f", sudoers_file])
            .status()
            .await
            .map_err(|e| e.to_string())?;

        Ok(status.success())
    }
}

pub async fn restart_service() -> Result<(), String> {
    Command::new("systemctl")
        .args(["--user", "restart", "yay-sys-tray"])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_pactree(package: &str, reverse: bool) -> Result<String, String> {
    let mut args = vec![package];
    if reverse {
        args.insert(0, "-r");
    }

    let output = Command::new("pactree")
        .args(&args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
