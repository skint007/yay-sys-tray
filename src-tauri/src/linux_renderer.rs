//! Linux WebKitGTK renderer compatibility workarounds.

use std::path::Path;

const RENDERER_OVERRIDES: [&str; 3] = [
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
    "__NV_DISABLE_EXPLICIT_SYNC",
];

/// Disable WebKitGTK's DMABUF renderer on NVIDIA Wayland sessions.
///
/// WebKitGTK/GTK3 can commit an NVIDIA explicit-sync surface without an
/// acquire point, causing the Wayland compositor to disconnect the entire app
/// with protocol error 71. This must run before Tauri initializes GTK/WebKit.
/// Existing renderer overrides always take precedence over this default.
pub fn configure() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    let override_present = RENDERER_OVERRIDES
        .iter()
        .any(|name| std::env::var_os(name).is_some());

    if !should_disable_dmabuf(
        nvidia_gpu_present(),
        session_type.as_deref(),
        wayland_display.as_deref(),
        gdk_backend.as_deref(),
        override_present,
    ) {
        return false;
    }

    // This is deliberately called before Tauri starts and before the process
    // has spawned threads, so GTK and its WebKit subprocesses inherit it.
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    true
}

fn should_disable_dmabuf(
    nvidia_gpu_present: bool,
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    gdk_backend: Option<&str>,
    override_present: bool,
) -> bool {
    nvidia_gpu_present
        && !override_present
        && is_wayland_session(session_type, wayland_display, gdk_backend)
}

fn is_wayland_session(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    gdk_backend: Option<&str>,
) -> bool {
    if let Some(backends) = gdk_backend.filter(|value| !value.trim().is_empty()) {
        return backends
            .split(',')
            .any(|backend| backend.trim().eq_ignore_ascii_case("wayland"));
    }

    session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        || wayland_display.is_some_and(|value| !value.trim().is_empty())
}

fn nvidia_gpu_present() -> bool {
    if Path::new("/proc/driver/nvidia/version").is_file() {
        return true;
    }

    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };

    entries.filter_map(Result::ok).any(|entry| {
        std::fs::read_to_string(entry.path().join("device/vendor"))
            .is_ok_and(|vendor| vendor.trim().eq_ignore_ascii_case("0x10de"))
    })
}

#[cfg(test)]
mod tests {
    use super::should_disable_dmabuf;

    #[test]
    fn disables_dmabuf_for_nvidia_wayland_session() {
        assert!(should_disable_dmabuf(
            true,
            Some("wayland"),
            Some("wayland-0"),
            None,
            false,
        ));
    }

    #[test]
    fn wayland_display_is_enough_when_session_type_is_missing() {
        assert!(should_disable_dmabuf(
            true,
            None,
            Some("wayland-0"),
            None,
            false,
        ));
    }

    #[test]
    fn explicit_wayland_gdk_backend_takes_precedence() {
        assert!(should_disable_dmabuf(
            true,
            Some("x11"),
            None,
            Some("wayland,x11"),
            false,
        ));
    }

    #[test]
    fn explicit_x11_gdk_backend_skips_wayland_workaround() {
        assert!(!should_disable_dmabuf(
            true,
            Some("wayland"),
            Some("wayland-0"),
            Some("x11"),
            false,
        ));
    }

    #[test]
    fn skips_non_nvidia_wayland_session() {
        assert!(!should_disable_dmabuf(
            false,
            Some("wayland"),
            Some("wayland-0"),
            None,
            false,
        ));
    }

    #[test]
    fn skips_nvidia_x11_session() {
        assert!(!should_disable_dmabuf(true, Some("x11"), None, None, false,));
    }

    #[test]
    fn preserves_existing_renderer_override() {
        assert!(!should_disable_dmabuf(
            true,
            Some("wayland"),
            Some("wayland-0"),
            None,
            true,
        ));
    }
}
