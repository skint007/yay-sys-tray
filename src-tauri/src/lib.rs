mod checker;
mod commands;
mod config;
mod icons;
mod system;
mod tailscale;
mod terminal;
mod tray;

use config::AppConfig;
use tauri::Manager;
use tokio::sync::RwLock;

pub struct AppState {
    pub config: RwLock<AppConfig>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            config: RwLock::new(config),
        })
        .manage(tray::TrayState::new())
        .setup(|app| {
            tray::setup_tray(app)?;

            // Intercept window close to hide instead of quit
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;

            // Start periodic update checks
            tray::start_periodic_check(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::start_check,
            commands::get_check_result,
            commands::run_local_update,
            commands::run_remote_update,
            commands::run_remove,
            commands::is_arch_linux,
            commands::get_pactree,
            commands::discover_tailscale_tags,
            commands::manage_autostart,
            commands::manage_passwordless_updates,
            commands::get_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
