#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
pub(crate) mod app_icon;
mod device;
mod diagnostics;
mod emoji_catalog;
mod firmware;
#[cfg(not(target_arch = "wasm32"))]
mod hid;
mod i18n;
mod keyboard;
mod keycode;
mod keycode_picker;
mod layouts;
#[cfg(target_os = "linux")]
mod linux_setup;
mod popup_state;
#[cfg(not(target_arch = "wasm32"))]
mod qmk_hid_host;
mod smart_input;
mod text_expander;
mod ui_style;

use app::EntropyApp;

const APP_TITLE: &str = "Entropy";
const APP_ID: &str = "entropy";
const SINGLE_INSTANCE_ENV: &str = "ENTROPY_SINGLE_INSTANCE";

#[cfg(target_os = "windows")]
struct SingleInstanceGuard(*mut core::ffi::c_void);

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct SingleInstanceGuard(i32);

#[cfg(target_os = "windows")]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = flock(self.0, LOCK_UN);
            let _ = <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
fn try_acquire_single_instance() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;

    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = OsStr::new("Global\\EntropySingleInstance")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(null_mut(), 1, name.as_ptr());
        if handle.is_null() {
            return true;
        }
        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        if already_exists {
            CloseHandle(handle);
            false
        } else {
            let _guard = Box::leak(Box::new(SingleInstanceGuard(handle)));
            true
        }
    }
}

fn notify_existing_instance() {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    let _ = std::fs::create_dir_all(&dir);
    let signal_path = dir.join("single_instance_signal");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string());
    let _ = std::fs::write(signal_path, now_ms);
}

fn single_instance_enabled_from_env(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("0" | "false" | "off" | "no")
    )
}

fn single_instance_enabled() -> bool {
    let value = std::env::var(SINGLE_INSTANCE_ENV).ok();
    single_instance_enabled_from_env(value.as_deref())
}

#[cfg(target_os = "windows")]
fn restore_existing_instance_window() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetWindowTextLengthW, GetWindowTextW, SetForegroundWindow,
        ShowWindow, SW_RESTORE, SW_SHOW,
    };

    unsafe extern "system" fn find_entropy_window(hwnd: HWND, lparam: LPARAM) -> i32 {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return 1;
        }

        let mut title = vec![0u16; len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
        if copied <= 0 {
            return 1;
        }

        let title = String::from_utf16_lossy(&title[..copied as usize]);
        if title == APP_TITLE || title.starts_with("Entropy (v") {
            let out = lparam as *mut HWND;
            if !out.is_null() {
                unsafe {
                    *out = hwnd;
                }
            }
            return 0;
        }

        1
    }

    let exact_title: Vec<u16> = OsStr::new(APP_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hwnd = FindWindowW(std::ptr::null(), exact_title.as_ptr());
        if hwnd.is_null() {
            let mut found: HWND = std::ptr::null_mut();
            EnumWindows(Some(find_entropy_window), &mut found as *mut HWND as LPARAM);
            hwnd = found;
        }

        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn try_acquire_single_instance() -> bool {
    use std::fs::OpenOptions;
    use std::os::fd::IntoRawFd;

    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("entropy");
    if std::fs::create_dir_all(&dir).is_err() {
        return true;
    }

    let Ok(file) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("single_instance.lock"))
    else {
        return true;
    };
    let fd = file.into_raw_fd();
    let locked = unsafe { flock(fd, LOCK_EX | LOCK_NB) == 0 };
    if locked {
        let _guard = Box::leak(Box::new(SingleInstanceGuard(fd)));
        true
    } else {
        unsafe {
            let _ = <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd);
        }
        false
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lpMutexAttributes: *mut core::ffi::c_void,
        bInitialOwner: i32,
        lpName: *const u16,
    ) -> *mut core::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(hObject: *mut core::ffi::c_void) -> i32;
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_EX: i32 = 2;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_NB: i32 = 4;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const LOCK_UN: i32 = 8;

#[cfg(any(target_os = "linux", target_os = "macos"))]
extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn try_acquire_single_instance() -> bool {
    true
}

fn eframe_persistence_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_ID);
    let _ = std::fs::create_dir_all(&dir);
    dir.join("eframe_state.ron")
}

fn initial_window_size() -> [f32; 2] {
    #[derive(serde::Deserialize)]
    struct StartupSettings {
        #[serde(default)]
        window_size: Option<[f32; 2]>,
    }

    let path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_ID)
        .join("app_settings.json");
    let Some(size) = std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str::<StartupSettings>(&data).ok())
        .and_then(|settings| settings.window_size)
    else {
        return [1200.0, 700.0];
    };

    if size[0].is_finite() && size[1].is_finite() {
        [size[0].clamp(800.0, 10000.0), size[1].clamp(500.0, 10000.0)]
    } else {
        [1200.0, 700.0]
    }
}

fn main() -> eframe::Result<()> {
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    if hid::run_hid_proxy_if_requested() {
        return Ok(());
    }

    diagnostics::init(diagnostics::settings_file_enabled());

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    {
        if single_instance_enabled() {
            if !try_acquire_single_instance() {
                notify_existing_instance();
                #[cfg(target_os = "windows")]
                restore_existing_instance_window();
                return Ok(());
            }
        } else {
            log::info!("Single-instance guard disabled by {SINGLE_INSTANCE_ENV}");
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let _single_instance_available = try_acquire_single_instance();

    #[cfg(target_os = "macos")]
    hid::initialize_macos_hid_on_main_thread();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_TITLE)
            .with_app_id(APP_ID)
            .with_icon(app_icon::egui_icon(64))
            .with_inner_size(initial_window_size())
            .with_min_inner_size([800.0, 500.0]),
        persistence_path: Some(eframe_persistence_path()),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| {
            // Roboto as primary UI font, with Unicode/symbol fallbacks.
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "roboto".to_owned(),
                egui::FontData::from_static(include_bytes!("../assets/Roboto-Regular.ttf")).into(),
            );
            fonts.font_data.insert(
                "dejavu".to_owned(),
                egui::FontData::from_static(include_bytes!("../assets/DejaVuSans.ttf")).into(),
            );
            fonts.font_data.insert(
                "noto_symbols".to_owned(),
                egui::FontData::from_static(include_bytes!(
                    "../assets/NotoSansSymbols2-Regular.ttf"
                ))
                .into(),
            );
            fonts.font_data.insert(
                "noto_emoji".to_owned(),
                egui::FontData::from_static(include_bytes!("../assets/NotoEmoji-Regular.ttf"))
                    .into(),
            );
            let prop = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            prop.insert(0, "roboto".to_owned());
            prop.push("dejavu".to_owned());
            prop.push("noto_symbols".to_owned());
            prop.push("noto_emoji".to_owned());
            let mono = fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default();
            mono.push("dejavu".to_owned());
            mono.push("noto_symbols".to_owned());
            mono.push("noto_emoji".to_owned());
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(EntropyApp::new(cc)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_instance_is_enabled_by_default() {
        assert!(single_instance_enabled_from_env(None));
    }

    #[test]
    fn single_instance_can_be_disabled_for_debugging() {
        for value in ["0", "false", "off", "no", " FALSE "] {
            assert!(!single_instance_enabled_from_env(Some(value)));
        }
    }

    #[test]
    fn single_instance_stays_enabled_for_other_values() {
        for value in ["1", "true", "yes", "anything"] {
            assert!(single_instance_enabled_from_env(Some(value)));
        }
    }
}
