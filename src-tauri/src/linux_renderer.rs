//! Linux WebKitGTK renderer compatibility workarounds.

use std::path::Path;

const RENDERER_OVERRIDES: [&str; 3] = [
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
    "__NV_DISABLE_EXPLICIT_SYNC",
];
const COMPOSITOR_DRM_DEVICE_VARS: [&str; 3] =
    ["KWIN_DRM_DEVICES", "AQ_DRM_DEVICES", "WLR_DRM_DEVICES"];

/// Disable WebKitGTK's DMABUF renderer on NVIDIA Wayland sessions.
///
/// WebKitGTK/GTK3 can commit an NVIDIA explicit-sync surface without an
/// acquire point, causing the Wayland compositor to disconnect the entire app
/// with protocol error 71. This must run before Tauri initializes GTK/WebKit.
/// Existing renderer overrides always take precedence over this default.
pub fn configure() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").ok();
    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    let override_present = RENDERER_OVERRIDES
        .iter()
        .any(|name| std::env::var_os(name).is_some());

    if !should_disable_dmabuf(
        nvidia_renderer_present(),
        session_type.as_deref(),
        wayland_display.as_deref(),
        x11_display.as_deref(),
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
    nvidia_renderer_present: bool,
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    x11_display: Option<&str>,
    gdk_backend: Option<&str>,
    override_present: bool,
) -> bool {
    nvidia_renderer_present
        && !override_present
        && is_wayland_session(session_type, wayland_display, x11_display, gdk_backend)
}

fn is_wayland_session(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    x11_display: Option<&str>,
    gdk_backend: Option<&str>,
) -> bool {
    let wayland_available = wayland_display.is_some_and(|value| !value.trim().is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"));
    let x11_available = x11_display.is_some_and(|value| !value.trim().is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("x11"));

    if let Some(backends) = gdk_backend.filter(|value| !value.trim().is_empty()) {
        for backend in backends.split(',').map(str::trim) {
            if backend.eq_ignore_ascii_case("wayland") && wayland_available {
                return true;
            }
            if backend.eq_ignore_ascii_case("x11") && x11_available {
                return false;
            }
            if backend == "*" {
                return wayland_available;
            }
        }
        return false;
    }

    wayland_available
}

#[derive(Debug, Clone)]
struct DrmDevice {
    card_name: String,
    nvidia_driver: bool,
    boot_vga: bool,
}

fn nvidia_renderer_present() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };

    let devices: Vec<DrmDevice> = entries
        .filter_map(Result::ok)
        .filter(|entry| is_drm_card(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| read_drm_device(&entry.path()))
        .collect();
    let nvidia_renderer_requested = std::env::var("__NV_PRIME_RENDER_OFFLOAD")
        .is_ok_and(|value| !value.trim().is_empty() && value.trim() != "0")
        || std::env::var("__GLX_VENDOR_LIBRARY_NAME")
            .is_ok_and(|value| value.eq_ignore_ascii_case("nvidia"))
        || std::env::var("__EGL_VENDOR_LIBRARY_FILENAMES")
            .is_ok_and(|value| egl_vendor_list_selects_nvidia(&value));
    let compositor_card = selected_compositor_card();

    primary_renderer_is_nvidia(
        &devices,
        compositor_card.as_deref(),
        nvidia_renderer_requested,
    )
}

fn egl_vendor_list_selects_nvidia(value: &str) -> bool {
    value.split(':').any(|path| {
        Path::new(path.trim()).file_name().is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .contains("nvidia")
        })
    })
}

fn is_drm_card(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn read_drm_device(card_path: &Path) -> Option<DrmDevice> {
    let device_path = card_path.join("device");
    let vendor = std::fs::read_to_string(device_path.join("vendor")).ok()?;
    let driver = std::fs::read_link(device_path.join("driver"))
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_owned()));

    Some(DrmDevice {
        card_name: card_path.file_name()?.to_string_lossy().into_owned(),
        nvidia_driver: vendor.trim().eq_ignore_ascii_case("0x10de")
            && driver.as_deref().is_some_and(|name| name == "nvidia"),
        boot_vga: std::fs::read_to_string(device_path.join("boot_vga"))
            .is_ok_and(|value| value.trim() == "1"),
    })
}

fn selected_compositor_card() -> Option<String> {
    COMPOSITOR_DRM_DEVICE_VARS.iter().find_map(|name| {
        let value = std::env::var(name).ok()?;
        first_card_from_device_list(&value)
    })
}

fn first_card_from_device_list(value: &str) -> Option<String> {
    let value = value.trim();
    let list_separator = value.char_indices().find_map(|(index, character)| {
        (character == ':' && value[index + 1..].trim_start().starts_with('/')).then_some(index)
    });
    let path = value[..list_separator.unwrap_or(value.len())].trim();
    if path.is_empty() {
        return None;
    }
    let path = Path::new(path);
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let card = resolved.file_name()?.to_string_lossy();
    is_drm_card(&card).then(|| card.into_owned())
}

fn primary_renderer_is_nvidia(
    devices: &[DrmDevice],
    compositor_card: Option<&str>,
    nvidia_renderer_requested: bool,
) -> bool {
    let nvidia_device_present = devices.iter().any(|device| device.nvidia_driver);
    if nvidia_renderer_requested {
        return nvidia_device_present;
    }
    if let Some(selected) = compositor_card {
        if let Some(device) = devices.iter().find(|device| device.card_name == selected) {
            return device.nvidia_driver;
        }
    }

    devices
        .iter()
        .any(|device| device.boot_vga && device.nvidia_driver)
        || (devices.len() == 1 && nvidia_device_present)
}

#[cfg(test)]
mod tests {
    use super::{
        egl_vendor_list_selects_nvidia, first_card_from_device_list, is_wayland_session,
        primary_renderer_is_nvidia, should_disable_dmabuf, DrmDevice,
    };

    fn device(card_name: &str, nvidia_driver: bool, boot_vga: bool) -> DrmDevice {
        DrmDevice {
            card_name: card_name.to_string(),
            nvidia_driver,
            boot_vga,
        }
    }

    #[test]
    fn disables_dmabuf_for_nvidia_wayland_session() {
        assert!(should_disable_dmabuf(
            true,
            Some("wayland"),
            Some("wayland-0"),
            None,
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
            None,
            false,
        ));
    }

    #[test]
    fn explicit_wayland_gdk_backend_takes_precedence() {
        assert!(should_disable_dmabuf(
            true,
            Some("x11"),
            Some("wayland-0"),
            Some(":0"),
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
            Some(":0"),
            Some("x11"),
            false,
        ));
    }

    #[test]
    fn first_available_gdk_backend_takes_precedence() {
        assert!(!is_wayland_session(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("x11,wayland"),
        ));
        assert!(is_wayland_session(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("wayland,x11"),
        ));
    }

    #[test]
    fn wildcard_gdk_backend_uses_wayland_session_indicators() {
        assert!(is_wayland_session(
            Some("wayland"),
            Some("wayland-0"),
            Some(":0"),
            Some("*")
        ));
        assert!(!is_wayland_session(
            Some("x11"),
            None,
            Some(":0"),
            Some("x11,*")
        ));
    }

    #[test]
    fn skips_non_nvidia_wayland_session() {
        assert!(!should_disable_dmabuf(
            false,
            Some("wayland"),
            Some("wayland-0"),
            None,
            None,
            false,
        ));
    }

    #[test]
    fn skips_nvidia_x11_session() {
        assert!(!should_disable_dmabuf(
            true,
            Some("x11"),
            None,
            Some(":0"),
            None,
            false,
        ));
    }

    #[test]
    fn preserves_existing_renderer_override() {
        assert!(!should_disable_dmabuf(
            true,
            Some("wayland"),
            Some("wayland-0"),
            None,
            None,
            true,
        ));
    }

    #[test]
    fn sole_nvidia_device_is_the_renderer() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", true, false)],
            None,
            false,
        ));
    }

    #[test]
    fn primary_nvidia_device_is_the_renderer_on_multi_gpu_system() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", true, true), device("card1", false, false),],
            None,
            false,
        ));
    }

    #[test]
    fn secondary_nvidia_device_does_not_trigger_workaround() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", false, true), device("card1", true, false),],
            None,
            false,
        ));
    }

    #[test]
    fn compositor_can_select_secondary_nvidia_device() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", false, true), device("card1", true, false),],
            Some("card1"),
            false,
        ));
    }

    #[test]
    fn reads_the_primary_card_from_compositor_device_list() {
        assert_eq!(
            first_card_from_device_list("/dev/dri/card1:/dev/dri/card0").as_deref(),
            Some("card1")
        );
        assert_eq!(first_card_from_device_list("  "), None);
    }

    #[test]
    fn resolves_persistent_compositor_device_symlink() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "yay-sys-tray-drm-device-test-{}",
            std::process::id()
        ));
        let by_path = root.join("dev/dri/by-path");
        std::fs::create_dir_all(&by_path).unwrap();
        std::fs::write(root.join("dev/dri/card1"), []).unwrap();
        let persistent_path = by_path.join("pci-0000:01:00.0-card");
        symlink("../card1", &persistent_path).unwrap();

        assert_eq!(
            first_card_from_device_list(&persistent_path.to_string_lossy()).as_deref(),
            Some("card1")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compositor_can_select_secondary_integrated_device() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", true, true), device("card1", false, false),],
            Some("card1"),
            false,
        ));
    }

    #[test]
    fn nvidia_offload_selects_secondary_nvidia_device() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", false, true), device("card1", true, false),],
            None,
            true,
        ));
    }

    #[test]
    fn glvnd_egl_vendor_file_can_select_nvidia() {
        assert!(egl_vendor_list_selects_nvidia(
            "/usr/share/glvnd/egl_vendor.d/10_nvidia.json"
        ));
        assert!(!egl_vendor_list_selects_nvidia(
            "/usr/share/glvnd/egl_vendor.d/50_mesa.json"
        ));
    }

    #[test]
    fn nouveau_device_does_not_trigger_workaround() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", false, true)],
            None,
            false,
        ));
    }
}
