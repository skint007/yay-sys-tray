// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF renderer crashes on some Wayland setups (notably
    // NVIDIA), aborting with "Error 71 (Protocol error) dispatching to Wayland
    // display". Disable it under Wayland unless the user set the var themselves
    // (set it to "0" to opt back in). Negligible cost for this lightweight UI.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    yay_sys_tray_lib::run();
}
