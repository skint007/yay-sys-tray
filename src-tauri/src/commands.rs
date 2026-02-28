use crate::config::AppConfig;
use crate::system;
use crate::tailscale;
use crate::terminal;
use crate::tray::{self, TrayState};
use crate::AppState;
use serde::Serialize;
use tauri::State;

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
pub async fn get_pactree(package: String, reverse: bool) -> Result<String, String> {
    system::get_pactree(&package, reverse).await
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
    if local.is_none() {
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
pub async fn discover_tailscale_tags() -> Vec<String> {
    tailscale::discover_all_tags().await
}

#[tauri::command]
pub fn get_version() -> String {
    option_env!("YST_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}
