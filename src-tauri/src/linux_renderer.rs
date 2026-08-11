//! Linux WebKitGTK renderer compatibility workarounds.

use std::ffi::{CStr, CString, OsStr};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

const RENDERER_OVERRIDES: [&str; 3] = [
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
    "__NV_DISABLE_EXPLICIT_SYNC",
];
const COMPOSITOR_DRM_DEVICE_VARS: [&str; 3] =
    ["KWIN_DRM_DEVICES", "AQ_DRM_DEVICES", "WLR_DRM_DEVICES"];
const WLR_RENDER_DRM_DEVICE: &str = "WLR_RENDER_DRM_DEVICE";

/// Disable WebKitGTK's DMABUF renderer on NVIDIA Wayland sessions.
///
/// WebKitGTK/GTK3 can commit an NVIDIA explicit-sync surface without an
/// acquire point, causing the Wayland compositor to disconnect the entire app
/// with protocol error 71. This must run before Tauri initializes GTK/WebKit.
/// Existing renderer overrides always take precedence over this default.
pub fn configure() -> bool {
    let session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let inherited_wayland_socket = std::env::var("WAYLAND_SOCKET").ok();
    let inherited_wayland_socket_present = inherited_wayland_socket
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let default_wayland_display =
        default_wayland_socket_available(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
            .then(|| "wayland-0".to_string());
    let wayland_socket = inherited_wayland_socket
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(default_wayland_display.as_deref());
    let wayland_display_for_probe = (!inherited_wayland_socket_present)
        .then_some(
            wayland_display
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(default_wayland_display.as_deref()),
        )
        .flatten();
    let x11_display = std::env::var("DISPLAY").ok();
    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    let override_present = RENDERER_OVERRIDES
        .iter()
        .any(|name| std::env::var_os(name).is_some());

    if !should_disable_dmabuf(
        nvidia_renderer_present(wayland_display_for_probe, inherited_wayland_socket_present),
        session_type.as_deref(),
        wayland_display.as_deref(),
        wayland_socket,
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

fn default_wayland_socket_available(runtime_dir: Option<&OsStr>) -> bool {
    let Some(runtime_dir) = runtime_dir else {
        return false;
    };
    std::fs::metadata(Path::new(runtime_dir).join("wayland-0"))
        .is_ok_and(|metadata| metadata.file_type().is_socket())
}

#[derive(Clone, Copy)]
struct SessionEnvironment<'a> {
    session_type: Option<&'a str>,
    wayland_display: Option<&'a str>,
    wayland_socket: Option<&'a str>,
    x11_display: Option<&'a str>,
    gdk_backend: Option<&'a str>,
}

fn should_disable_dmabuf(
    nvidia_renderer_present: bool,
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    wayland_socket: Option<&str>,
    x11_display: Option<&str>,
    gdk_backend: Option<&str>,
    override_present: bool,
) -> bool {
    should_disable_dmabuf_with_probes(
        nvidia_renderer_present,
        SessionEnvironment {
            session_type,
            wayland_display,
            wayland_socket,
            x11_display,
            gdk_backend,
        },
        override_present,
        wayland_display_is_usable,
        x11_display_is_usable,
    )
}

fn should_disable_dmabuf_with_probes(
    nvidia_renderer_present: bool,
    session: SessionEnvironment<'_>,
    override_present: bool,
    wayland_probe: impl Fn(Option<&str>, Option<&str>) -> bool,
    x11_probe: impl Fn(Option<&str>) -> bool,
) -> bool {
    nvidia_renderer_present
        && !override_present
        && is_wayland_session_with_probes(session, wayland_probe, x11_probe)
}

fn is_wayland_session_with_probes(
    session: SessionEnvironment<'_>,
    wayland_probe: impl Fn(Option<&str>, Option<&str>) -> bool,
    x11_probe: impl Fn(Option<&str>) -> bool,
) -> bool {
    let SessionEnvironment {
        session_type,
        wayland_display,
        wayland_socket,
        x11_display,
        gdk_backend,
    } = session;
    let wayland_available = wayland_display.is_some_and(|value| !value.trim().is_empty())
        || wayland_socket.is_some_and(|value| !value.trim().is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"));
    let x11_available = x11_display.is_some_and(|value| !value.trim().is_empty())
        || session_type.is_some_and(|value| value.eq_ignore_ascii_case("x11"));

    if let Some(backends) = gdk_backend.filter(|value| !value.trim().is_empty()) {
        for backend in backends.split(',').map(str::trim) {
            if backend.eq_ignore_ascii_case("wayland") && wayland_available {
                if wayland_probe(wayland_display, wayland_socket) {
                    return true;
                }
                continue;
            }
            if backend.eq_ignore_ascii_case("x11") && x11_available {
                if x11_probe(x11_display) {
                    return false;
                }
                continue;
            }
            if backend == "*" {
                return wayland_available && wayland_probe(wayland_display, wayland_socket);
            }
        }
        return false;
    }

    wayland_available && wayland_probe(wayland_display, wayland_socket)
}

fn wayland_display_is_usable(wayland_display: Option<&str>, wayland_socket: Option<&str>) -> bool {
    // GTK will consume an inherited socket directly. Opening or duplicating it
    // here would share the protocol stream, so leave validation to GTK.
    if std::env::var("WAYLAND_SOCKET").is_ok_and(|value| !value.trim().is_empty()) {
        return true;
    }

    type WlDisplayConnect = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type WlDisplayDisconnect = unsafe extern "C" fn(*mut c_void);

    // SAFETY: The symbols use libwayland-client's C ABI, the library outlives
    // the copied function pointers, and a successful connection is closed once.
    unsafe {
        let wayland = match libloading::Library::new("libwayland-client.so.0") {
            Ok(wayland) => wayland,
            Err(_) => return false,
        };
        let Ok(wl_display_connect) = wayland.get::<WlDisplayConnect>(b"wl_display_connect\0")
        else {
            return false;
        };
        let Ok(wl_display_disconnect) =
            wayland.get::<WlDisplayDisconnect>(b"wl_display_disconnect\0")
        else {
            return false;
        };
        let display_name = wayland_display
            .filter(|value| !value.trim().is_empty())
            .or_else(|| wayland_socket.filter(|value| !value.trim().is_empty()));
        let display_name = display_name.and_then(|value| CString::new(value).ok());
        let display_name_ptr = display_name
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let display = wl_display_connect(display_name_ptr);
        if display.is_null() {
            return false;
        }
        wl_display_disconnect(display);
        true
    }
}

fn x11_display_is_usable(display: Option<&str>) -> bool {
    type XOpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type XCloseDisplay = unsafe extern "C" fn(*mut c_void) -> c_int;

    // SAFETY: The symbols use libX11's C ABI, the library outlives the copied
    // function pointers, and a successfully-opened display is closed once.
    unsafe {
        let x11 = match libloading::Library::new("libX11.so.6") {
            Ok(x11) => x11,
            Err(_) => return false,
        };
        let Ok(x_open_display) = x11.get::<XOpenDisplay>(b"XOpenDisplay\0") else {
            return false;
        };
        let Ok(x_close_display) = x11.get::<XCloseDisplay>(b"XCloseDisplay\0") else {
            return false;
        };
        let display_name = display.and_then(|value| CString::new(value).ok());
        let display_name_ptr = display_name
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let x_display = x_open_display(display_name_ptr);
        if x_display.is_null() {
            return false;
        }
        x_close_display(x_display);
        true
    }
}

#[derive(Debug, Clone)]
struct DrmDevice {
    card_name: String,
    nvidia_driver: bool,
}

fn nvidia_renderer_present(
    wayland_display: Option<&str>,
    inherited_wayland_socket_present: bool,
) -> bool {
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
    let compositor_card =
        selected_renderer_card(&devices).or_else(|| selected_compositor_card(&devices));
    let mixed_gpu_system = devices.iter().any(|device| device.nvidia_driver)
        && devices.iter().any(|device| !device.nvidia_driver);
    let active_egl_renderer = (mixed_gpu_system
        && compositor_card.is_none()
        && !nvidia_renderer_requested)
        .then(|| mixed_gpu_renderer_is_nvidia(wayland_display, inherited_wayland_socket_present))
        .flatten();

    primary_renderer_is_nvidia(
        &devices,
        compositor_card.as_deref(),
        nvidia_renderer_requested,
        active_egl_renderer,
    )
}

fn mixed_gpu_renderer_is_nvidia(
    wayland_display: Option<&str>,
    inherited_wayland_socket_present: bool,
) -> Option<bool> {
    // An inherited Wayland socket is the connection GTK will use, but probing
    // a duplicated descriptor would share and corrupt its protocol stream.
    // Prefer the safe workaround when NVIDIA is one of the possible renderers.
    inherited_wayland_socket_present
        .then_some(true)
        .or_else(|| wayland_egl_vendor_is_nvidia(wayland_display))
}

fn egl_vendor_list_selects_nvidia(value: &str) -> bool {
    let mut vendor_files = value
        .split(':')
        .map(str::trim)
        .filter(|path| !path.is_empty());
    let Some(first) = vendor_files.next() else {
        return false;
    };

    std::iter::once(first)
        .chain(vendor_files)
        .all(egl_vendor_manifest_selects_nvidia)
}

fn egl_vendor_manifest_selects_nvidia(path: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };

    serde_json::from_str::<serde_json::Value>(&contents).is_ok_and(|manifest| {
        manifest
            .pointer("/ICD/library_path")
            .and_then(serde_json::Value::as_str)
            .and_then(|library_path| Path::new(library_path).file_name())
            .is_some_and(|library| {
                library
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("nvidia")
            })
    })
}

/// Ask EGL which vendor it selects for a fresh connection to this Wayland
/// display. This matches the renderer WebKitGTK will use more closely than PCI
/// firmware flags do on hybrid-GPU systems.
fn wayland_egl_vendor_is_nvidia(wayland_display: Option<&str>) -> Option<bool> {
    type WlDisplayConnect = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type WlDisplayDisconnect = unsafe extern "C" fn(*mut c_void);
    type EglGetPlatformDisplay =
        unsafe extern "C" fn(c_uint, *mut c_void, *const c_void) -> *mut c_void;
    type EglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;
    type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> c_int;
    type EglQueryString = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
    type EglTerminate = unsafe extern "C" fn(*mut c_void) -> c_int;

    const EGL_VENDOR: c_int = 0x3053;
    const EGL_PLATFORM_WAYLAND_KHR: c_uint = 0x31D8;

    // SAFETY: Every symbol is loaded from the library that defines its C ABI.
    // The Wayland and EGL handles stay alive until all copied function pointers
    // have been called, and every successfully-created display is released.
    unsafe {
        let wayland = libloading::Library::new("libwayland-client.so.0").ok()?;
        let egl = libloading::Library::new("libEGL.so.1").ok()?;
        let wl_display_connect: WlDisplayConnect = *wayland.get(b"wl_display_connect\0").ok()?;
        let wl_display_disconnect: WlDisplayDisconnect =
            *wayland.get(b"wl_display_disconnect\0").ok()?;
        let egl_get_platform_display: EglGetPlatformDisplay = egl
            .get::<EglGetPlatformDisplay>(b"eglGetPlatformDisplay\0")
            .or_else(|_| egl.get::<EglGetPlatformDisplay>(b"eglGetPlatformDisplayEXT\0"))
            .map(|symbol| *symbol)
            .ok()
            .or_else(|| {
                let egl_get_proc_address: EglGetProcAddress =
                    *egl.get(b"eglGetProcAddress\0").ok()?;
                let symbol = egl_get_proc_address(c"eglGetPlatformDisplayEXT".as_ptr());
                (!symbol.is_null()).then(|| std::mem::transmute(symbol))
            })?;
        let egl_initialize: EglInitialize = *egl.get(b"eglInitialize\0").ok()?;
        let egl_query_string: EglQueryString = *egl.get(b"eglQueryString\0").ok()?;
        let egl_terminate: EglTerminate = *egl.get(b"eglTerminate\0").ok()?;

        let display_name = CString::new(wayland_display?).ok()?;
        let wl_display = wl_display_connect(display_name.as_ptr());
        if wl_display.is_null() {
            return None;
        }

        let egl_display =
            egl_get_platform_display(EGL_PLATFORM_WAYLAND_KHR, wl_display, std::ptr::null());
        if egl_display.is_null() {
            wl_display_disconnect(wl_display);
            return None;
        }

        let mut major = 0;
        let mut minor = 0;
        if egl_initialize(egl_display, &mut major, &mut minor) == 0 {
            wl_display_disconnect(wl_display);
            return None;
        }

        let vendor = egl_query_string(egl_display, EGL_VENDOR);
        let selected_nvidia = (!vendor.is_null()).then(|| {
            CStr::from_ptr(vendor)
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("nvidia")
        });

        egl_terminate(egl_display);
        wl_display_disconnect(wl_display);
        selected_nvidia
    }
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
    })
}

fn selected_compositor_card(devices: &[DrmDevice]) -> Option<String> {
    COMPOSITOR_DRM_DEVICE_VARS.iter().find_map(|name| {
        let value = std::env::var(name).ok()?;
        first_available_card_from_device_list(&value, devices)
    })
}

fn selected_renderer_card(devices: &[DrmDevice]) -> Option<String> {
    let value = std::env::var(WLR_RENDER_DRM_DEVICE).ok()?;
    first_available_card_from_device_list(&value, devices)
}

#[cfg(test)]
fn first_card_from_device_list(value: &str) -> Option<String> {
    card_names_from_device_list(value).into_iter().next()
}

fn first_available_card_from_device_list(value: &str, devices: &[DrmDevice]) -> Option<String> {
    card_names_from_device_list(value)
        .into_iter()
        .find(|card| devices.iter().any(|device| device.card_name == *card))
}

fn card_names_from_device_list(value: &str) -> Vec<String> {
    let mut cards = Vec::new();
    let mut remaining = value.trim();

    while !remaining.is_empty() {
        let list_separator = remaining.char_indices().find_map(|(index, character)| {
            (character == ':' && remaining[index + 1..].trim_start().starts_with('/'))
                .then_some(index)
        });
        let path_end = list_separator.unwrap_or(remaining.len());
        if let Some(card) = card_name_from_path(remaining[..path_end].trim()) {
            cards.push(card);
        }
        let Some(separator) = list_separator else {
            break;
        };
        remaining = remaining[separator + 1..].trim_start();
    }

    cards
}

fn card_name_from_path(path: &str) -> Option<String> {
    card_name_from_path_with_sysfs(path, Path::new("/sys/class/drm"))
}

fn card_name_from_path_with_sysfs(path: &str, sys_class_drm: &Path) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let path = Path::new(path);
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let node = resolved.file_name()?.to_string_lossy();
    if is_drm_card(&node) {
        return Some(node.into_owned());
    }
    if !node.strip_prefix("renderD").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return None;
    }

    std::fs::read_dir(sys_class_drm.join(node.as_ref()).join("device/drm"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| is_drm_card(name))
}

fn primary_renderer_is_nvidia(
    devices: &[DrmDevice],
    compositor_card: Option<&str>,
    nvidia_renderer_requested: bool,
    active_egl_renderer: Option<bool>,
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

    let all_devices_are_nvidia =
        !devices.is_empty() && devices.iter().all(|device| device.nvidia_driver);
    if all_devices_are_nvidia {
        return true;
    }

    active_egl_renderer.is_some_and(|nvidia| nvidia && nvidia_device_present)
}

#[cfg(test)]
mod tests {
    use super::{
        card_name_from_path_with_sysfs, default_wayland_socket_available,
        egl_vendor_list_selects_nvidia, first_available_card_from_device_list,
        first_card_from_device_list, is_wayland_session_with_probes, mixed_gpu_renderer_is_nvidia,
        primary_renderer_is_nvidia, should_disable_dmabuf, should_disable_dmabuf_with_probes,
        DrmDevice, SessionEnvironment,
    };

    fn device(card_name: &str, nvidia_driver: bool) -> DrmDevice {
        DrmDevice {
            card_name: card_name.to_string(),
            nvidia_driver,
        }
    }

    fn session<'a>(
        session_type: Option<&'a str>,
        wayland_display: Option<&'a str>,
        wayland_socket: Option<&'a str>,
        x11_display: Option<&'a str>,
        gdk_backend: Option<&'a str>,
    ) -> SessionEnvironment<'a> {
        SessionEnvironment {
            session_type,
            wayland_display,
            wayland_socket,
            x11_display,
            gdk_backend,
        }
    }

    #[test]
    fn disables_dmabuf_for_nvidia_wayland_session() {
        assert!(should_disable_dmabuf_with_probes(
            true,
            session(Some("wayland"), Some("wayland-0"), None, None, None),
            false,
            |_, _| true,
            |_| false,
        ));
    }

    #[test]
    fn wayland_display_is_enough_when_session_type_is_missing() {
        assert!(should_disable_dmabuf_with_probes(
            true,
            session(None, Some("wayland-0"), None, None, None),
            false,
            |_, _| true,
            |_| false,
        ));
    }

    #[test]
    fn wayland_socket_is_enough_for_explicit_wayland_backend() {
        assert!(is_wayland_session_with_probes(
            session(None, None, Some("7"), None, Some("wayland")),
            |_, _| true,
            |_| false,
        ));
    }

    #[test]
    fn finds_libwayland_default_socket() {
        use std::os::fd::AsRawFd;
        use std::os::unix::{fs::symlink, net::UnixStream};

        let root = std::env::temp_dir().join(format!(
            "yay-sys-tray-wayland-socket-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let socket_path = root.join("wayland-0");
        let (socket, _peer) = UnixStream::pair().unwrap();
        symlink(
            format!("/proc/self/fd/{}", socket.as_raw_fd()),
            &socket_path,
        )
        .unwrap();

        assert!(default_wayland_socket_available(Some(root.as_os_str())));

        std::fs::remove_file(&socket_path).unwrap();
        std::fs::write(&socket_path, []).unwrap();
        assert!(!default_wayland_socket_available(Some(root.as_os_str())));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_wayland_gdk_backend_takes_precedence() {
        assert!(should_disable_dmabuf_with_probes(
            true,
            session(
                Some("x11"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("wayland,x11"),
            ),
            false,
            |_, _| true,
            |_| true,
        ));
    }

    #[test]
    fn explicit_x11_gdk_backend_skips_wayland_workaround() {
        assert!(!is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("x11"),
            ),
            |_, _| true,
            |_| true,
        ));
    }

    #[test]
    fn wayland_fallback_is_treated_as_available() {
        assert!(is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("x11,wayland"),
            ),
            |_, _| true,
            |_| false,
        ));
        assert!(is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("wayland,x11"),
            ),
            |_, _| true,
            |_| true,
        ));
    }

    #[test]
    fn first_usable_gdk_backend_takes_precedence() {
        assert!(!is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("x11,wayland"),
            ),
            |_, _| true,
            |_| true,
        ));
    }

    #[test]
    fn unavailable_wayland_backend_falls_back_to_usable_x11() {
        assert!(!is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-stale"),
                None,
                Some(":0"),
                Some("wayland,x11"),
            ),
            |_, _| false,
            |_| true,
        ));
    }

    #[test]
    fn wildcard_gdk_backend_uses_wayland_session_indicators() {
        assert!(is_wayland_session_with_probes(
            session(
                Some("wayland"),
                Some("wayland-0"),
                None,
                Some(":0"),
                Some("*"),
            ),
            |_, _| true,
            |_| true,
        ));
        assert!(!is_wayland_session_with_probes(
            session(Some("x11"), None, None, Some(":0"), Some("x11,*")),
            |_, _| false,
            |_| true,
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
            None,
            true,
        ));
    }

    #[test]
    fn sole_nvidia_device_is_the_renderer() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", true)],
            None,
            false,
            None,
        ));
    }

    #[test]
    fn active_nvidia_egl_vendor_is_the_renderer_on_multi_gpu_system() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", true), device("card1", false)],
            None,
            false,
            Some(true),
        ));
    }

    #[test]
    fn inherited_wayland_socket_uses_safe_nvidia_fallback_on_multi_gpu_system() {
        assert_eq!(mixed_gpu_renderer_is_nvidia(None, true), Some(true));
    }

    #[test]
    fn secondary_nvidia_device_does_not_trigger_workaround() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", false), device("card1", true)],
            None,
            false,
            Some(false),
        ));
    }

    #[test]
    fn unmarked_multi_gpu_nvidia_system_triggers_workaround() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", true), device("card1", true)],
            None,
            false,
            None,
        ));
    }

    #[test]
    fn compositor_can_select_secondary_nvidia_device() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", false), device("card1", true)],
            Some("card1"),
            false,
            Some(false),
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
    fn compositor_device_list_skips_unavailable_cards() {
        assert_eq!(
            first_available_card_from_device_list(
                "/dev/dri/card9:/dev/dri/card1",
                &[device("card0", false), device("card1", true)]
            )
            .as_deref(),
            Some("card1")
        );
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
    fn resolves_wlroots_render_node_to_its_drm_card() {
        let root = std::env::temp_dir().join(format!(
            "yay-sys-tray-render-node-test-{}",
            std::process::id()
        ));
        let render_node = root.join("dev/dri/renderD128");
        let drm_device = root.join("sys/class/drm/renderD128/device/drm/card1");
        std::fs::create_dir_all(render_node.parent().unwrap()).unwrap();
        std::fs::write(&render_node, []).unwrap();
        std::fs::create_dir_all(&drm_device).unwrap();

        assert_eq!(
            card_name_from_path_with_sysfs(
                &render_node.to_string_lossy(),
                &root.join("sys/class/drm")
            )
            .as_deref(),
            Some("card1")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compositor_can_select_secondary_integrated_device() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", true), device("card1", false)],
            Some("card1"),
            false,
            Some(true),
        ));
    }

    #[test]
    fn nvidia_offload_selects_secondary_nvidia_device() {
        assert!(primary_renderer_is_nvidia(
            &[device("card0", false), device("card1", true)],
            None,
            true,
            Some(false),
        ));
    }

    #[test]
    fn glvnd_egl_vendor_file_can_select_nvidia() {
        let root = std::env::temp_dir().join(format!(
            "yay-sys-tray-egl-vendor-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let nvidia_manifest = root.join("custom-vendor.json");
        let mesa_manifest = root.join("another-vendor.json");
        std::fs::write(
            &nvidia_manifest,
            r#"{"file_format_version":"1.0.0","ICD":{"library_path":"libEGL_nvidia.so.0"}}"#,
        )
        .unwrap();
        std::fs::write(
            &mesa_manifest,
            r#"{"file_format_version":"1.0.0","ICD":{"library_path":"libEGL_mesa.so.0"}}"#,
        )
        .unwrap();

        assert!(egl_vendor_list_selects_nvidia(
            &nvidia_manifest.to_string_lossy()
        ));
        assert!(!egl_vendor_list_selects_nvidia(
            &mesa_manifest.to_string_lossy()
        ));
        assert!(!egl_vendor_list_selects_nvidia(&format!(
            "{}:{}",
            nvidia_manifest.display(),
            mesa_manifest.display()
        )));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nouveau_device_does_not_trigger_workaround() {
        assert!(!primary_renderer_is_nvidia(
            &[device("card0", false)],
            None,
            false,
            Some(true),
        ));
    }
}
