use crate::checker::{self, CheckResult};
use crate::icons;
use crate::tailscale::{self, HostResult};
use crate::AppState;

use tauri::image::Image;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Listener, Manager,
};
use tokio::sync::{watch, RwLock};

/// Shared state for the tray module.
pub struct TrayState {
    pub local_result: RwLock<Option<CheckResult>>,
    pub remote_results: RwLock<Vec<HostResult>>,
    pub last_check: RwLock<Option<chrono::DateTime<chrono::Local>>>,
    pub previous_count: RwLock<u32>,
    pub show_updates_item: RwLock<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    spin_cancel: watch::Sender<bool>,
    bounce_cancel: watch::Sender<bool>,
}

impl TrayState {
    pub fn new() -> Self {
        let (spin_tx, _) = watch::channel(false);
        let (bounce_tx, _) = watch::channel(false);
        Self {
            local_result: RwLock::new(None),
            remote_results: RwLock::new(Vec::new()),
            last_check: RwLock::new(None),
            previous_count: RwLock::new(0),
            show_updates_item: RwLock::new(None),
            spin_cancel: spin_tx,
            bounce_cancel: bounce_tx,
        }
    }

    pub fn cancel_spin(&self) {
        let _ = self.spin_cancel.send(true);
    }

    pub fn cancel_bounce(&self) {
        let _ = self.bounce_cancel.send(true);
    }

    pub fn spin_receiver(&self) -> watch::Receiver<bool> {
        self.spin_cancel.subscribe()
    }

    pub fn bounce_receiver(&self) -> watch::Receiver<bool> {
        self.bounce_cancel.subscribe()
    }

    /// Reset the cancel signal so new animations can start.
    pub fn reset_spin(&self) {
        let _ = self.spin_cancel.send(false);
    }

    pub fn reset_bounce(&self) {
        let _ = self.bounce_cancel.send(false);
    }
}

pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Build tray menu
    let check_now = MenuItemBuilder::with_id("check_now", "Check Now").build(app)?;
    let show_updates = MenuItemBuilder::with_id("show_updates", "Show Updates")
        .enabled(false)
        .build(app)?;
    let update_system = MenuItemBuilder::with_id("update_system", "Update System").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let about = MenuItemBuilder::with_id("about", "About").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&check_now)
        .item(&show_updates)
        .item(&update_system)
        .separator()
        .item(&settings)
        .item(&about)
        .separator()
        .item(&quit)
        .build()?;

    let icon = icons::create_ok_icon();

    let _tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Yay Update Checker\nNo updates checked yet")
        .menu(&menu)
        .on_menu_event(move |app_handle, event| match event.id().as_ref() {
            "check_now" => {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    start_check(handle).await;
                });
            }
            "show_updates" => {
                let _ = app_handle.emit("open-window", serde_json::json!({"view": "updates"}));
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "update_system" => {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    crate::terminal::run_local_update(handle, false).await;
                });
            }
            "settings" => {
                let _ = app_handle.emit("open-window", serde_json::json!({"view": "settings"}));
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "about" => {
                let _ = app_handle.emit("open-window", serde_json::json!({"view": "about"}));
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray_icon, event| {
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                let app_handle = tray_icon.app_handle();
                let _ = app_handle.emit("open-window", serde_json::json!({"view": "updates"}));
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    // Store the show_updates menu item for later enable/disable
    {
        let tray_state = app.state::<TrayState>();
        let mut item = tray_state.show_updates_item.blocking_write();
        *item = Some(show_updates);
    }

    // Listen for update-finished events to trigger a re-check
    let handle = app.handle().clone();
    app.listen("update-finished", move |_| {
        let h = handle.clone();
        tauri::async_runtime::spawn(async move {
            // Check if self-update was pending
            // (yay-sys-tray-git was in the updates list)
            let tray_state = h.state::<TrayState>();
            let self_update = {
                let result = tray_state.local_result.read().await;
                result
                    .as_ref()
                    .map(|r| r.updates.iter().any(|u| u.package == "yay-sys-tray-git"))
                    .unwrap_or(false)
            };
            if self_update {
                log::info!("Self-update detected, restarting service");
                let _ = crate::system::restart_service().await;
                return;
            }
            // Otherwise re-check
            start_check(h).await;
        });
    });

    Ok(())
}

/// Start the periodic check timer.
pub fn start_periodic_check(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Initial check after 2 seconds
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        start_check(app_handle.clone()).await;

        // Then periodically
        loop {
            let interval = {
                let state = app_handle.state::<AppState>();
                let config = state.config.read().await;
                config.check_interval_minutes
            };
            let duration = std::time::Duration::from_secs(interval as u64 * 60);
            tokio::time::sleep(duration).await;
            start_check(app_handle.clone()).await;
        }
    });
}

/// Run a check and update the tray state.
pub async fn start_check(app_handle: tauri::AppHandle) {
    log::info!("Starting update check");
    let _ = app_handle.emit("check-started", ());

    // Start spin animation
    let tray_state = app_handle.state::<TrayState>();
    tray_state.cancel_bounce();
    tray_state.reset_spin();

    let (animations_enabled, notify_mode, tailscale_enabled, tailscale_tags, tailscale_timeout) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (
            config.animations,
            config.notify.clone(),
            config.tailscale_enabled,
            config.tailscale_tags.clone(),
            config.tailscale_timeout,
        )
    };

    start_spin_animation(app_handle.clone(), animations_enabled);

    // Run local check
    log::info!("Running local update check");
    let result = checker::check_updates().await;
    log::info!("Local check result: {} updates, err={}",
        result.as_ref().map(|r| r.updates.len()).unwrap_or(0),
        result.as_ref().err().map(|e| e.as_str()).unwrap_or("none"));

    // Run remote check if Tailscale is enabled
    let remote_hosts = if tailscale_enabled && !tailscale_tags.is_empty() {
        let prefixed_tags: Vec<String> = tailscale_tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        log::info!("Running Tailscale check with tags: {:?}", prefixed_tags);
        match tailscale::check_remote(&prefixed_tags, tailscale_timeout).await {
            Ok(remote) => {
                log::info!("Tailscale check found {} hosts", remote.hosts.len());
                for h in &remote.hosts {
                    log::info!("  {}: {} updates, error={:?}", h.hostname, h.updates.len(), h.error);
                }
                remote.hosts
            }
            Err(e) => {
                log::error!("Tailscale check failed: {e}");
                Vec::new()
            }
        }
    } else {
        log::info!("Tailscale disabled or no tags configured");
        Vec::new()
    };

    // Stop spin animation
    tray_state.cancel_spin();

    match result {
        Ok(check_result) => {
            let total_count = check_result.updates.len() as u32
                + remote_hosts.iter().map(|h| h.updates.len() as u32).sum::<u32>();
            let old_count = *tray_state.previous_count.read().await;

            *tray_state.local_result.write().await = Some(check_result.clone());
            *tray_state.remote_results.write().await = remote_hosts.clone();
            *tray_state.last_check.write().await = Some(chrono::Local::now());
            *tray_state.previous_count.write().await = total_count;

            let _ = app_handle.emit("check-complete", &check_result);
            update_tray_state(&app_handle, &check_result, &remote_hosts, animations_enabled)
                .await;

            // Enable/disable "Show Updates" menu item
            if let Some(item) = tray_state.show_updates_item.read().await.as_ref() {
                let _ = item.set_enabled(total_count > 0);
            }

            // Send notification if configured
            let should_notify = match notify_mode.as_str() {
                "always" => total_count > 0,
                "new_only" => total_count > old_count,
                _ => false,
            };
            if should_notify {
                send_notification(&app_handle, total_count);
            }
        }
        Err(err) => {
            log::error!("Update check failed: {err}");
            let _ = app_handle.emit("check-error", &err);
            update_tray_error(&app_handle);
        }
    }
}

/// Update the tray icon and tooltip based on check result.
async fn update_tray_state(
    app_handle: &tauri::AppHandle,
    result: &CheckResult,
    remote_hosts: &[HostResult],
    animations: bool,
) {
    let Some(tray) = get_default_tray(app_handle) else {
        return;
    };

    let local_count = result.updates.len() as u32;
    let remote_update_count: u32 = remote_hosts
        .iter()
        .map(|h| h.updates.len() as u32)
        .sum();
    let total_count = local_count + remote_update_count;
    let remote_needs_restart = remote_hosts.iter().any(|h| h.needs_restart);
    let any_restart = result.needs_restart || remote_needs_restart;
    let reboot_needed = result
        .reboot_info
        .as_ref()
        .map(|r| r.needed)
        .unwrap_or(false);

    // Build tooltip
    let tray_state = app_handle.state::<TrayState>();
    let last_check = tray_state.last_check.read().await;
    let mut lines = Vec::new();

    if !remote_hosts.is_empty() {
        // Multi-host display
        let mut local_label = format!("Local: {local_count} update(s)");
        if result.needs_restart {
            local_label.push_str(" (restart)");
        }
        lines.push(local_label);

        for host in remote_hosts {
            if let Some(ref err) = host.error {
                lines.push(format!("{}: {err}", host.hostname));
            } else if host.updates.is_empty() {
                lines.push(format!("{}: up to date", host.hostname));
            } else {
                let mut label = format!("{}: {} update(s)", host.hostname, host.updates.len());
                if host.needs_restart {
                    label.push_str(" (restart)");
                }
                lines.push(label);
            }
        }
    } else if total_count == 0 {
        if reboot_needed {
            lines.push("Reboot required".to_string());
        } else {
            lines.push("System up to date".to_string());
        }
    } else {
        lines.push(format!("{total_count} update(s) available"));
        if result.needs_restart {
            let pkgs = result.restart_packages.join(", ");
            lines.push(format!("Restart: {pkgs}"));
        }
    }

    if let Some(time) = *last_check {
        lines.push(format!("Last check: {}", time.format("%H:%M")));
    }

    let _ = tray.set_tooltip(Some(&lines.join("\n")));

    // Set icon
    let tray_state = app_handle.state::<TrayState>();
    tray_state.reset_bounce();

    if total_count == 0 && reboot_needed {
        let icon = icons::create_reboot_icon();
        if animations {
            start_bounce_animation(app_handle.clone(), icon, 1000, 16);
        } else {
            let _ = tray.set_icon(Some(icons::create_reboot_icon()));
        }
    } else if total_count == 0 {
        let _ = tray.set_icon(Some(icons::create_ok_icon()));
    } else if any_restart {
        let icon = icons::create_restart_icon(total_count);
        if animations {
            start_bounce_animation(app_handle.clone(), icon, 500, 0);
        } else {
            let _ = tray.set_icon(Some(icons::create_restart_icon(total_count)));
        }
    } else {
        let icon = icons::create_updates_icon(total_count);
        if animations {
            start_bounce_animation(app_handle.clone(), icon, 500, 0);
        } else {
            let _ = tray.set_icon(Some(icons::create_updates_icon(total_count)));
        }
    }
}

fn send_notification(app_handle: &tauri::AppHandle, count: u32) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app_handle
        .notification()
        .builder()
        .title("Yay Update Checker")
        .body(format!("{count} update(s) available"))
        .show();
}

fn update_tray_error(app_handle: &tauri::AppHandle) {
    if let Some(tray) = get_default_tray(app_handle) {
        let _ = tray.set_icon(Some(icons::create_error_icon()));
        let _ = tray.set_tooltip(Some("Yay Update Checker\nCheck failed"));
    }
}

fn get_default_tray(app_handle: &tauri::AppHandle) -> Option<tauri::tray::TrayIcon> {
    app_handle.tray_by_id("main")
}

/// Start the spin animation on the tray icon.
fn start_spin_animation(app_handle: tauri::AppHandle, enabled: bool) {
    let frames = icons::create_checking_frames(12);

    if !enabled {
        // Just set static checking icon
        if let Some(tray) = get_default_tray(&app_handle) {
            let _ = tray.set_icon(Some(frames.into_iter().next().unwrap()));
        }
        return;
    }

    let tray_state = app_handle.state::<TrayState>();
    let mut cancel_rx = tray_state.spin_receiver();

    tauri::async_runtime::spawn(async move {
        let mut idx = 0;
        loop {
            if let Some(tray) = get_default_tray(&app_handle) {
                let _ = tray.set_icon(Some(frames[idx].clone()));
            }
            idx = (idx + 1) % frames.len();

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(80)) => {},
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

/// Start a bounce animation on the tray icon.
fn start_bounce_animation(
    app_handle: tauri::AppHandle,
    base_icon: Image<'static>,
    interval_ms: u64,
    max_ticks: usize,
) {
    let tray_state = app_handle.state::<TrayState>();
    let mut cancel_rx = tray_state.bounce_receiver();

    let small = icons::create_scaled_icon(&base_icon, 0.65);

    tauri::async_runtime::spawn(async move {
        let mut tick = 0;
        let mut show_small = false;

        // Set initial full-size icon
        if let Some(tray) = get_default_tray(&app_handle) {
            let _ = tray.set_icon(Some(base_icon.clone()));
        }

        loop {
            if max_ticks > 0 && tick >= max_ticks {
                // End on full-size
                if let Some(tray) = get_default_tray(&app_handle) {
                    let _ = tray.set_icon(Some(base_icon.clone()));
                }
                break;
            }

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(interval_ms)) => {},
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        break;
                    }
                }
            }

            show_small = !show_small;
            if let Some(tray) = get_default_tray(&app_handle) {
                if show_small {
                    let _ = tray.set_icon(Some(small.clone()));
                } else {
                    let _ = tray.set_icon(Some(base_icon.clone()));
                }
            }
            tick += 1;
        }
    });
}
