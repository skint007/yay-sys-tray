use crate::config::AppConfig;
use crate::system;
use crate::tailscale;
use crate::terminal;
use crate::tray::{self, TrayState};
use crate::AppState;
use serde::Serialize;
use tauri::{Manager, State};

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.read().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    config.save()?;
    let mut current = state.config.write().await;
    *current = config;
    Ok(())
}

#[tauri::command]
pub async fn start_check(app_handle: tauri::AppHandle) -> Result<(), String> {
    tray::start_check(app_handle).await;
    Ok(())
}

#[tauri::command]
pub fn is_arch_linux() -> bool {
    system::is_arch_linux()
}

#[tauri::command]
pub async fn get_pactree(
    app_handle: tauri::AppHandle,
    package: String,
    reverse: bool,
    hostname: Option<String>,
) -> Result<String, String> {
    let (target, timeout) = match hostname {
        Some(h) if !h.is_empty() => {
            let (user, timeout) = {
                let state = app_handle.state::<AppState>();
                let config = state.config.read().await;
                (config.tailscale_ssh_user.clone(), config.tailscale_timeout)
            };
            (tailscale::ssh_target(&h, &user), timeout)
        }
        _ => (String::new(), 0),
    };
    system::get_pactree(&package, reverse, &target, timeout).await
}

#[tauri::command]
pub async fn manage_autostart(enable: bool) -> Result<(), String> {
    system::manage_autostart(enable).await
}

#[tauri::command]
pub async fn manage_passwordless_updates(enable: bool) -> Result<bool, String> {
    system::manage_passwordless_updates(enable).await
}

#[derive(Serialize)]
pub struct FullCheckResult {
    pub local: Option<crate::checker::CheckResult>,
    pub remote: Vec<crate::tailscale::HostResult>,
}

#[tauri::command]
pub async fn get_check_result(
    tray_state: State<'_, TrayState>,
) -> Result<Option<FullCheckResult>, String> {
    let local = tray_state.local_result.read().await.clone();
    let remote = tray_state.remote_results.read().await.clone();
    // Surface results if either side has data — a failed local check must not
    // hide successfully-scanned remote hosts (the frontend handles a null local).
    if local.is_none() && remote.is_empty() {
        return Ok(None);
    }
    Ok(Some(FullCheckResult { local, remote }))
}

#[tauri::command]
pub async fn run_local_update(app_handle: tauri::AppHandle, restart: bool) {
    terminal::run_local_update(app_handle, restart).await;
}

#[tauri::command]
pub async fn run_remote_update(app_handle: tauri::AppHandle, hostname: String, restart: bool) {
    terminal::run_remote_update(app_handle, &hostname, restart).await;
}

#[tauri::command]
pub async fn run_remove(app_handle: tauri::AppHandle, package: String, flags: String) {
    terminal::run_remove(app_handle, &package, &flags).await;
}

#[tauri::command]
pub async fn run_local_update_packages(
    app_handle: tauri::AppHandle,
    packages: Vec<String>,
    restart: bool,
) {
    terminal::run_local_update_packages(app_handle, packages, restart).await;
}

#[tauri::command]
pub async fn run_remote_update_packages(
    app_handle: tauri::AppHandle,
    tray_state: State<'_, TrayState>,
    hostname: String,
    packages: Vec<String>,
    restart: bool,
) -> Result<(), String> {
    // Resolve the selection against that host's last check rather than taking
    // the names as given. It is what separates repo packages from AUR ones —
    // the pacman fallback must never see an AUR name — and it keeps anything
    // that isn't a known update from reaching the command string at all.
    let (selected, repo_only) = {
        let hosts = tray_state.remote_results.read().await;
        let Some(host) = hosts.iter().find(|h| h.hostname == hostname) else {
            return Err(format!("No check results for {hostname}"));
        };
        let mut selected = Vec::new();
        let mut repo_only = Vec::new();
        for update in &host.updates {
            if packages.contains(&update.package) {
                selected.push(update.package.clone());
                if update.repository != "aur" {
                    repo_only.push(update.package.clone());
                }
            }
        }
        (selected, repo_only)
    };

    if selected.is_empty() {
        return Err(format!("None of the selected packages are pending on {hostname}"));
    }

    terminal::run_remote_update_packages(app_handle, &hostname, selected, repo_only, restart).await;
    Ok(())
}

#[tauri::command]
pub async fn run_remote_remove(
    app_handle: tauri::AppHandle,
    hostname: String,
    package: String,
    flags: String,
) {
    terminal::run_remote_remove(app_handle, &hostname, &package, &flags).await;
}

/// Update the named remote hosts; each reboots only if it actually needs a restart.
#[tauri::command]
pub async fn run_update_selected(
    app_handle: tauri::AppHandle,
    tray_state: State<'_, TrayState>,
    hostnames: Vec<String>,
    restart: bool,
) -> Result<(), String> {
    let restart_map: std::collections::HashMap<String, bool> = {
        let hosts = tray_state.remote_results.read().await;
        hosts.iter().map(|h| (h.hostname.clone(), h.needs_restart)).collect()
    };
    for hostname in hostnames {
        let host_restart = restart && restart_map.get(&hostname).copied().unwrap_or(false);
        terminal::run_remote_update(app_handle.clone(), &hostname, host_restart).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn discover_tailscale_tags() -> Vec<String> {
    tailscale::discover_all_tags().await
}

#[tauri::command]
pub fn get_version() -> String {
    option_env!("YST_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

#[tauri::command]
pub fn quit_app(app_handle: tauri::AppHandle) {
    app_handle.exit(0);
}
