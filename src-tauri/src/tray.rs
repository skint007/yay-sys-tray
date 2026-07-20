use crate::checker::{self, CheckResult};
use crate::icons;
use crate::tailscale::{self, HostResult};
use crate::AppState;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Listener, Manager,
};
use tokio::sync::RwLock;

/// Shared state for the tray module.
pub struct TrayState {
    pub local_result: RwLock<Option<CheckResult>>,
    pub remote_results: RwLock<Vec<HostResult>>,
    pub last_check: RwLock<Option<chrono::DateTime<chrono::Local>>>,
    pub previous_count: RwLock<u32>,
    pub show_updates_item: RwLock<Option<tauri::menu::MenuItem<tauri::Wry>>>,
    /// Serializes every check/re-check so their state writes and tray-icon
    /// updates never interleave. When several update terminals close at once we
    /// get a burst of `update-finished` events; without this they would run
    /// concurrent re-checks that mutate the (single-threaded, non-thread-safe)
    /// GTK tray from multiple worker threads at the same time and crash.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Monotonic token identifying the one animation allowed to drive the tray
    /// icon. Starting an animation bumps it; any older animation loop whose
    /// captured token no longer matches must stop. This guarantees at most one
    /// animation task touches the tray, even across overlapping re-checks —
    /// replacing an earlier `watch`-channel cancel that raced on rapid restart.
    anim_generation: AtomicU64,
    /// Guards the actual `set_icon` call so two threads (e.g. a stale animation
    /// loop that hasn't yet observed a generation bump and the task that
    /// superseded it) can never call into GTK concurrently.
    icon_lock: Mutex<()>,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            local_result: RwLock::new(None),
            remote_results: RwLock::new(Vec::new()),
            last_check: RwLock::new(None),
            previous_count: RwLock::new(0),
            show_updates_item: RwLock::new(None),
            refresh_lock: tokio::sync::Mutex::new(()),
            anim_generation: AtomicU64::new(0),
            icon_lock: Mutex::new(()),
        }
    }

    /// Bump and return a fresh animation generation, superseding any running
    /// animation (spin or bounce).
    fn next_anim_generation(&self) -> u64 {
        self.anim_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// The generation of the animation currently allowed to run.
    fn current_anim_generation(&self) -> u64 {
        self.anim_generation.load(Ordering::SeqCst)
    }

    /// Stop any running animation without starting a new one (used before
    /// setting a static icon).
    fn stop_animation(&self) {
        self.anim_generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// Set the tray icon under `icon_lock` so concurrent callers can't drive GTK
/// from two threads at once. A no-op if the tray is gone.
fn set_tray_icon(app_handle: &tauri::AppHandle, icon: Image<'static>) {
    let Some(tray) = get_default_tray(app_handle) else {
        return;
    };
    let tray_state = app_handle.state::<TrayState>();
    let _guard = tray_state
        .icon_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _ = tray.set_icon(Some(icon));
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
            "show_updates" => open_window(app_handle, "updates"),
            "update_system" => {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    crate::terminal::run_local_update(handle, false).await;
                });
            }
            "settings" => open_window(app_handle, "settings"),
            "about" => open_window(app_handle, "about"),
            "quit" => {
                app_handle.exit(0);
            }
            _ => {}
        })
        // Note: on Linux the tray uses libappindicator, which delivers no
        // click events and always shows the menu on any click, so the menu is
        // the only interaction. We keep it informative instead (see the
        // dynamic "Show Updates (N)" label in update_tray_state).
        .build(app)?;

    // Store the show_updates menu item for later enable/disable
    {
        let tray_state = app.state::<TrayState>();
        let mut item = tray_state.show_updates_item.blocking_write();
        *item = Some(show_updates);
    }

    // Re-check only the target whose update terminal just closed (a single
    // host, or "local") instead of rescanning the whole fleet every time.
    let handle = app.handle().clone();
    app.listen("update-finished", move |event| {
        let scope = serde_json::from_str::<serde_json::Value>(event.payload())
            .ok()
            .and_then(|v| v.get("scope").and_then(|s| s.as_str()).map(String::from))
            .unwrap_or_else(|| "local".to_string());
        let h = handle.clone();
        tauri::async_runtime::spawn(async move {
            handle_update_finished(h, scope).await;
        });
    });

    Ok(())
}

/// Show the main window on a given view (updates/settings/about) and focus it.
fn open_window(app_handle: &tauri::AppHandle, view: &str) {
    let _ = app_handle.emit("open-window", serde_json::json!({ "view": view }));
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Compute the next occurrence of a weekly scheduled check (day: 0=Mon..6=Sun).
fn next_scheduled(day: u32, time: &str, now: chrono::DateTime<chrono::Local>) -> chrono::DateTime<chrono::Local> {
    use chrono::{Datelike, Duration, Local, TimeZone};
    let mut parts = time.split(':');
    let h: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let m: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);

    let now_wd = now.weekday().num_days_from_monday() as i64;
    let days_ahead = (day as i64 - now_wd).rem_euclid(7);
    let date = now.date_naive() + Duration::days(days_ahead);
    let naive = date.and_hms_opt(h, m, 0).unwrap_or_else(|| now.naive_local());
    let mut target = Local
        .from_local_datetime(&naive)
        .single()
        .unwrap_or(now);
    if target <= now {
        target += Duration::days(7);
    }
    target
}

/// Wall-clock watchdog: fires interval and scheduled checks. Polling on a 60s
/// tick (rather than a long sleep) means sleep/resume won't drop a due check.
pub fn start_periodic_check(app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Initial check after 2 seconds
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        start_check(app_handle.clone()).await;

        let mut scheduled_target: Option<chrono::DateTime<chrono::Local>> = None;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let now = chrono::Local::now();

            let (interval_enabled, interval_min, sched_enabled, sched_day, sched_time) = {
                let state = app_handle.state::<AppState>();
                let config = state.config.read().await;
                (
                    config.check_interval_enabled,
                    config.check_interval_minutes,
                    config.scheduled_check_enabled,
                    config.scheduled_check_day,
                    config.scheduled_check_time.clone(),
                )
            };

            let mut fire = false;

            if interval_enabled {
                let last = *app_handle.state::<TrayState>().last_check.read().await;
                match last {
                    Some(last) => {
                        if now - last >= chrono::Duration::minutes(interval_min as i64) {
                            fire = true;
                        }
                    }
                    // No successful check has ever landed (e.g. the startup check
                    // errored transiently), so `last_check` is still None. Retry
                    // each tick until one succeeds instead of sitting on the
                    // error icon forever.
                    None => fire = true,
                }
            }

            if sched_enabled {
                if scheduled_target.is_none() {
                    scheduled_target = Some(next_scheduled(sched_day, &sched_time, now));
                }
                if let Some(target) = scheduled_target {
                    if now >= target {
                        fire = true;
                        scheduled_target = Some(next_scheduled(sched_day, &sched_time, now));
                    }
                }
            } else {
                scheduled_target = None;
            }

            if fire {
                start_check(app_handle.clone()).await;
            }
        }
    });
}

/// Run a check and update the tray state.
pub async fn start_check(app_handle: tauri::AppHandle) {
    log::info!("Starting update check");

    // Serialize with any other in-flight check/re-check so tray mutations and
    // state writes never overlap. Held for the whole scan.
    let refresh_state = app_handle.state::<TrayState>();
    let _refresh_guard = refresh_state.refresh_lock.lock().await;

    let (
        animations_enabled,
        notify_mode,
        tailscale_enabled,
        tailscale_tags,
        tailscale_timeout,
        tailscale_ssh_user,
    ) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (
            config.animations,
            config.notify.clone(),
            config.tailscale_enabled,
            config.tailscale_tags.clone(),
            config.tailscale_timeout,
            config.tailscale_ssh_user.clone(),
        )
    };

    start_spin_animation(app_handle.clone(), animations_enabled);

    // Discover the remote hosts up front so the Updates window can render the
    // full scan list (local first, then each tagged peer) and track progress.
    let remote_hostnames = if tailscale_enabled && !tailscale_tags.is_empty() {
        let tags: Vec<String> = tailscale_tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        log::info!("Discovering Tailscale peers with tags: {:?}", tags);
        tailscale::discover_peers(&tags).await
    } else {
        log::info!("Tailscale disabled or no tags configured");
        Vec::new()
    };

    // Announce the full host set so the UI can show queued/checking/done states.
    let mut host_list = vec![serde_json::json!({ "key": "local", "name": "Local" })];
    for hostname in &remote_hostnames {
        host_list.push(serde_json::json!({ "key": hostname, "name": hostname }));
    }
    let _ = app_handle.emit("check-started", serde_json::json!({ "hosts": host_list }));

    // Run local check
    log::info!("Running local update check");
    let _ = app_handle.emit("check-host-start", "local");
    let result = checker::check_updates().await;
    log::info!("Local check result: {} updates, err={}",
        result.as_ref().map(|r| r.updates.len()).unwrap_or(0),
        result.as_ref().err().map(|e| e.as_str()).unwrap_or("none"));
    let _ = app_handle.emit(
        "check-host-done",
        serde_json::json!({
            "key": "local",
            "count": result.as_ref().map(|r| r.updates.len()).unwrap_or(0),
            "needs_restart": result.as_ref().map(|r| r.needs_restart).unwrap_or(false),
            "error": result.is_err(),
        }),
    );

    // Run remote checks (each emits its own progress as it completes).
    let remote_hosts = if !remote_hostnames.is_empty() {
        let hosts = tailscale::check_hosts(
            &app_handle,
            remote_hostnames,
            tailscale_timeout,
            &tailscale_ssh_user,
        )
        .await;
        log::info!("Tailscale check finished for {} hosts", hosts.len());
        for h in &hosts {
            log::info!("  {}: {} updates, error={:?}", h.hostname, h.updates.len(), h.error);
        }
        hosts
    } else {
        Vec::new()
    };

    // The spin animation is superseded below: update_tray_state (Ok) and
    // update_tray_error (Err) each bump the animation generation, which stops it.
    let tray_state = app_handle.state::<TrayState>();

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

            set_show_updates_label(&app_handle, total_count).await;

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
            // The remote scan already completed before the local check was
            // evaluated — keep its results so the Updates window can still show
            // remote hosts even though the local check errored.
            *tray_state.remote_results.write().await = remote_hosts.clone();
            *tray_state.last_check.write().await = Some(chrono::Local::now());
            let _ = app_handle.emit("check-error", &err);
            update_tray_error(&app_handle);
        }
    }
}

/// Re-check after a terminal-launched update closes. Only the affected target
/// is re-scanned: a pending self-update restarts the service, "local" re-checks
/// the local system, and a hostname re-checks just that one remote host.
async fn handle_update_finished(app_handle: tauri::AppHandle, scope: String) {
    if scope == "local" {
        // A self-update (yay-sys-tray-git was in the local list) needs a service
        // restart to load the new binary rather than a plain re-check.
        let self_update = {
            let tray_state = app_handle.state::<TrayState>();
            let result = tray_state.local_result.read().await;
            result
                .as_ref()
                .map(|r| {
                    r.updates.iter().any(|u| {
                        matches!(
                            u.package.as_str(),
                            "yay-sys-tray-git" | "yay-sys-tray-bin" | "yay-sys-tray"
                        )
                    })
                })
                .unwrap_or(false)
        };
        if self_update {
            log::info!("Self-update detected, restarting service");
            let _ = crate::system::restart_service().await;
            return;
        }
        recheck_local(app_handle).await;
    } else {
        recheck_remote(app_handle, scope).await;
    }
}

/// Re-run the local check and refresh the tray, leaving remote results intact.
async fn recheck_local(app_handle: tauri::AppHandle) {
    log::info!("Re-checking local system after update");
    // Serialize with concurrent checks/re-checks (a burst of closing terminals).
    let refresh_state = app_handle.state::<TrayState>();
    let _refresh_guard = refresh_state.refresh_lock.lock().await;
    let result = checker::check_updates().await;
    let tray_state = app_handle.state::<TrayState>();
    match result {
        Ok(check_result) => {
            *tray_state.local_result.write().await = Some(check_result.clone());
            *tray_state.last_check.write().await = Some(chrono::Local::now());
            let remote = tray_state.remote_results.read().await.clone();
            refresh_after_recheck(&app_handle, &check_result, &remote).await;
            let _ = app_handle.emit("check-complete", &check_result);
        }
        Err(err) => {
            log::error!("Local re-check failed: {err}");
            let _ = app_handle.emit("check-error", &err);
            update_tray_error(&app_handle);
        }
    }
}

/// Re-check a single remote host and refresh, leaving local + other hosts intact.
async fn recheck_remote(app_handle: tauri::AppHandle, hostname: String) {
    log::info!("Re-checking remote host {hostname} after update");
    // Serialize with concurrent checks/re-checks (a burst of closing terminals).
    let refresh_state = app_handle.state::<TrayState>();
    let _refresh_guard = refresh_state.refresh_lock.lock().await;
    let (timeout, ssh_user) = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        (config.tailscale_timeout, config.tailscale_ssh_user.clone())
    };

    let results =
        tailscale::check_hosts(&app_handle, vec![hostname.clone()], timeout, &ssh_user).await;

    let tray_state = app_handle.state::<TrayState>();
    {
        let mut remote = tray_state.remote_results.write().await;
        if let Some(updated) = results.into_iter().next() {
            match remote.iter_mut().find(|h| h.hostname == hostname) {
                Some(slot) => *slot = updated,
                None => {
                    remote.push(updated);
                    remote.sort_by(|a, b| a.hostname.cmp(&b.hostname));
                }
            }
        }
    }
    *tray_state.last_check.write().await = Some(chrono::Local::now());

    let local = tray_state.local_result.read().await.clone();
    let remote = tray_state.remote_results.read().await.clone();
    if let Some(local) = local {
        refresh_after_recheck(&app_handle, &local, &remote).await;
    }
    let _ = app_handle.emit("check-complete", serde_json::json!({}));
}

/// Recompute the tray icon/tooltip + menu from the given local & remote state.
/// Shared by the targeted re-checks; intentionally does not notify (it's a
/// post-update refresh, not a scheduled scan).
async fn refresh_after_recheck(
    app_handle: &tauri::AppHandle,
    local: &CheckResult,
    remote: &[HostResult],
) {
    let animations = {
        let state = app_handle.state::<AppState>();
        let config = state.config.read().await;
        config.animations
    };
    let total_count = local.updates.len() as u32
        + remote.iter().map(|h| h.updates.len() as u32).sum::<u32>();

    {
        let tray_state = app_handle.state::<TrayState>();
        *tray_state.previous_count.write().await = total_count;
    }
    update_tray_state(app_handle, local, remote, animations).await;
    set_show_updates_label(app_handle, total_count).await;
}

/// Enable/disable the "Show Updates" menu item and surface the count in its
/// label (the tray tooltip is a no-op on Linux/libappindicator).
async fn set_show_updates_label(app_handle: &tauri::AppHandle, total_count: u32) {
    let item = {
        let tray_state = app_handle.state::<TrayState>();
        let guard = tray_state.show_updates_item.read().await;
        guard.clone()
    };
    if let Some(item) = item {
        let _ = item.set_enabled(total_count > 0);
        let _ = item.set_text(if total_count > 0 {
            format!("Show Updates ({total_count})")
        } else {
            "Show Updates".to_string()
        });
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

    if total_count == 0 && !reboot_needed {
        // Static icon: stop any running animation, then set it (locked).
        tray_state.stop_animation();
        set_tray_icon(app_handle, icons::create_ok_icon());
    } else {
        // Pick the icon and its bounce timing once, then either animate it or set
        // it statically — no rebuilding the same icon in both branches.
        let (icon, interval_ms, max_ticks) = if total_count == 0 {
            (icons::create_reboot_icon(), 1000, 16)
        } else if any_restart {
            (icons::create_restart_icon(total_count), 500, 10)
        } else {
            (icons::create_updates_icon(total_count), 500, 10)
        };
        if animations {
            // start_bounce_animation bumps the generation, superseding the spin.
            start_bounce_animation(app_handle.clone(), icon, interval_ms, max_ticks);
        } else {
            tray_state.stop_animation();
            set_tray_icon(app_handle, icon);
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
    // Stop any running animation so it can't clobber the error icon.
    app_handle.state::<TrayState>().stop_animation();
    if let Some(tray) = get_default_tray(app_handle) {
        let _ = tray.set_tooltip(Some("Yay Update Checker\nCheck failed"));
    }
    set_tray_icon(app_handle, icons::create_error_icon());
}

fn get_default_tray(app_handle: &tauri::AppHandle) -> Option<tauri::tray::TrayIcon> {
    app_handle.tray_by_id("main")
}

/// Start the spin animation on the tray icon. Bumping the generation supersedes
/// any earlier animation (spin or bounce), so only this loop drives the icon.
fn start_spin_animation(app_handle: tauri::AppHandle, enabled: bool) {
    let frames = icons::create_checking_frames(12);
    let my_gen = app_handle.state::<TrayState>().next_anim_generation();

    if !enabled {
        // Just set static checking icon
        set_tray_icon(&app_handle, frames.into_iter().next().unwrap());
        return;
    }

    tauri::async_runtime::spawn(async move {
        let mut idx = 0;
        loop {
            if app_handle.state::<TrayState>().current_anim_generation() != my_gen {
                break;
            }
            set_tray_icon(&app_handle, frames[idx].clone());
            idx = (idx + 1) % frames.len();
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }
    });
}

/// Start a bounce animation on the tray icon. Bumping the generation supersedes
/// any earlier animation, so only this loop drives the icon.
fn start_bounce_animation(
    app_handle: tauri::AppHandle,
    base_icon: Image<'static>,
    interval_ms: u64,
    max_ticks: usize,
) {
    let my_gen = app_handle.state::<TrayState>().next_anim_generation();
    let small = icons::create_scaled_icon(&base_icon, 0.65);

    tauri::async_runtime::spawn(async move {
        let mut tick = 0;
        let mut show_small = false;

        // Set initial full-size icon
        set_tray_icon(&app_handle, base_icon.clone());

        loop {
            if app_handle.state::<TrayState>().current_anim_generation() != my_gen {
                break;
            }
            if max_ticks > 0 && tick >= max_ticks {
                // End on full-size
                set_tray_icon(&app_handle, base_icon.clone());
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;

            // Re-check after sleeping: a newer animation may have superseded us
            // while we slept, and we must not clobber the icon it set.
            if app_handle.state::<TrayState>().current_anim_generation() != my_gen {
                break;
            }
            show_small = !show_small;
            let icon = if show_small { small.clone() } else { base_icon.clone() };
            set_tray_icon(&app_handle, icon);
            tick += 1;
        }
    });
}
