#![allow(non_snake_case)]

#[cfg(any(target_os = "windows", target_os = "macos", test))]
use crate::text_expander::TextExpansionEngine;
use crate::text_expander::{TextExpansionConfig, TextExpansionRule};
use std::sync::{Mutex, OnceLock, RwLock};

#[cfg(any(target_os = "windows", test))]
#[path = "smart_input_windows.rs"]
mod smart_input_windows;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextExpanderAppCandidate {
    pub exe: String,
    pub title: String,
}

#[cfg(target_os = "macos")]
type ForegroundCacheState = Option<(std::time::Instant, Option<TextExpanderAppCandidate>)>;

static TEXT_EXPANDER_CONFIG: OnceLock<RwLock<TextExpansionConfig>> = OnceLock::new();
#[cfg(any(target_os = "windows", target_os = "macos", test))]
static TEXT_EXPANDER_ENGINE: OnceLock<Mutex<TextExpansionEngine>> = OnceLock::new();
static RECENT_FOREGROUND_APPS: OnceLock<Mutex<Vec<TextExpanderAppCandidate>>> = OnceLock::new();

fn text_expander_config() -> &'static RwLock<TextExpansionConfig> {
    TEXT_EXPANDER_CONFIG.get_or_init(|| RwLock::new(TextExpansionConfig::default()))
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn text_expander_engine() -> &'static Mutex<TextExpansionEngine> {
    TEXT_EXPANDER_ENGINE.get_or_init(|| Mutex::new(TextExpansionEngine::default()))
}

fn recent_foreground_apps() -> &'static Mutex<Vec<TextExpanderAppCandidate>> {
    RECENT_FOREGROUND_APPS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn current_process_name_lower() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_owned()))
        .and_then(|name| name.to_str().map(|name| name.to_ascii_lowercase()))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn remember_foreground_app(candidate: TextExpanderAppCandidate) {
    let exe = candidate.exe.trim().to_ascii_lowercase();
    if exe.is_empty() {
        return;
    }
    if current_process_name_lower().as_deref() == Some(exe.as_str()) {
        return;
    }
    let title = candidate.title.trim().to_owned();
    if let Ok(mut apps) = recent_foreground_apps().lock() {
        apps.retain(|app| app.exe != exe);
        apps.insert(0, TextExpanderAppCandidate { exe, title });
        apps.truncate(12);
    }
}

pub fn text_expander_app_candidates() -> Vec<TextExpanderAppCandidate> {
    let mut apps = platform_open_window_candidates();
    if let Ok(recent) = recent_foreground_apps().lock() {
        for candidate in recent.iter().rev() {
            if !apps.iter().any(|app| app.exe == candidate.exe) {
                apps.insert(0, candidate.clone());
            }
        }
    }
    apps.truncate(16);
    apps
}

pub fn set_text_expander_config(
    enabled: bool,
    rules: Vec<TextExpansionRule>,
    app_blacklist: Vec<String>,
) {
    let config = TextExpansionConfig {
        enabled,
        rules: rules.clone(),
        app_blacklist: app_blacklist
            .into_iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect(),
    };
    if let Ok(mut guard) = text_expander_config().write() {
        *guard = config;
    }
    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    {
        if let Ok(mut engine) = text_expander_engine().lock() {
            engine.set_rules(rules);
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
fn text_expander_enabled() -> bool {
    text_expander_config()
        .read()
        .map(|config| config.enabled && config.rules.iter().any(|rule| rule.enabled))
        .unwrap_or(false)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn text_expander_suppressed_for_context() -> bool {
    text_expander_config()
        .read()
        .map(|config| foreground_app_blacklisted(&config.app_blacklist))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn foreground_app_blacklisted(app_blacklist: &[String]) -> bool {
    smart_input_windows::foreground_app_blacklisted(app_blacklist)
}

#[cfg(target_os = "macos")]
fn foreground_app_blacklisted(app_blacklist: &[String]) -> bool {
    macos::foreground_app_blacklisted(app_blacklist)
}

#[cfg(target_os = "windows")]
pub fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
    smart_input_windows::platform_open_window_candidates()
}

#[cfg(target_os = "macos")]
pub fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
    macos::platform_open_window_candidates()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
    Vec::new()
}

#[cfg(target_os = "linux")]
pub fn text_expander_runs_outside_entropy_process() -> bool {
    linux_input_method_env().contains("ibus")
}

#[cfg(not(target_os = "linux"))]
pub fn text_expander_runs_outside_entropy_process() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn linux_input_method_env() -> String {
    let im_vars = ["GTK_IM_MODULE", "QT_IM_MODULE", "XMODIFIERS"];
    im_vars
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(target_os = "linux")]
fn linux_command_available(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[cfg(target_os = "linux")]
pub fn refresh_installed_ibus_backend() {
    let Some(source) = linux_bundled_ibus_engine_path() else {
        return;
    };
    let Some(installed) = linux_installed_ibus_engine_path() else {
        return;
    };
    if !installed.exists() {
        return;
    }
    let source_bytes = match std::fs::read(&source) {
        Ok(bytes) => bytes,
        Err(err) => {
            log::warn!("Smart Input: failed to read bundled IBus backend: {err}");
            return;
        }
    };
    if std::fs::read(&installed).ok().as_deref() == Some(source_bytes.as_slice()) {
        return;
    }
    if let Err(err) = std::fs::write(&installed, &source_bytes) {
        log::warn!("Smart Input: failed to update installed IBus backend: {err}");
        return;
    }
    set_user_executable(&installed);
    refresh_ibus_registry();
}

#[cfg(target_os = "linux")]
fn linux_bundled_ibus_engine_path() -> Option<std::path::PathBuf> {
    crate::linux_setup::bundled_ibus_engine_path()
}

#[cfg(target_os = "linux")]
fn linux_installed_ibus_engine_path() -> Option<std::path::PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/share"))
        })?;
    Some(data_home.join("entropy/ibus/entropy-ibus-engine"))
}

#[cfg(target_os = "linux")]
fn set_user_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        if let Err(err) = std::fs::set_permissions(path, permissions) {
            log::warn!("Smart Input: failed to chmod installed IBus backend: {err}");
        }
    }
}

/// Reloads the IBus registry without touching the filesystem.
///
/// A declarative install (the NixOS or home-manager module, a distribution
/// package) puts the engine in place while the running daemon still serves the
/// registry it read at startup, so the layouts are missing until it reloads.
/// Nothing user-local is added to IBUS_COMPONENT_PATH here: whatever registered
/// the engine is already on it.
#[cfg(target_os = "linux")]
pub(crate) fn reload_ibus_registry() -> Result<(), String> {
    if !linux_command_available("ibus") {
        return Err("ibus is not installed".to_owned());
    }
    run_ibus_command("write-cache")?;
    run_ibus_command("restart")
}

#[cfg(target_os = "linux")]
fn run_ibus_command(arg: &str) -> Result<(), String> {
    match std::process::Command::new("ibus").arg(arg).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let details = String::from_utf8_lossy(&output.stderr)
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or_default()
                .to_owned();
            if details.is_empty() {
                Err(format!("ibus {arg} failed: {}", output.status))
            } else {
                Err(format!("ibus {arg} failed: {details}"))
            }
        }
        Err(err) => Err(format!("could not run ibus {arg}: {err}")),
    }
}

#[cfg(target_os = "linux")]
fn refresh_ibus_registry() {
    if !linux_command_available("ibus") {
        return;
    }
    let component_dir = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .map(|data_home| data_home.join("ibus/component"));
    if let Some(component_dir) = component_dir {
        let mut component_path = component_dir.to_string_lossy().to_string();
        if let Ok(existing) = std::env::var("IBUS_COMPONENT_PATH") {
            component_path.push(':');
            component_path.push_str(&existing);
        } else {
            component_path.push_str(":/usr/share/ibus/component");
        }
        let _ = std::process::Command::new("ibus")
            .arg("write-cache")
            .env("IBUS_COMPONENT_PATH", component_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = std::process::Command::new("ibus")
        .arg("restart")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(target_os = "windows")]
pub fn start() {
    smart_input_windows::start();
}

#[cfg(target_os = "macos")]
pub struct MacosTextExpanderStatus {
    pub accessibility_granted: bool,
    pub input_monitoring_granted: bool,
    pub event_tap_active: bool,
    pub last_event_ms_ago: Option<u128>,
    pub failure_reason: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacosEventTapStartupDecision {
    TryCreateTap,
    WaitForPermission(&'static str),
}

#[cfg(any(target_os = "macos", test))]
fn macos_event_tap_startup_decision(
    _input_monitoring_preflight_granted: bool,
    accessibility_granted: bool,
) -> MacosEventTapStartupDecision {
    if !accessibility_granted {
        return MacosEventTapStartupDecision::WaitForPermission(
            "Accessibility permission is required for Text Expander",
        );
    }

    MacosEventTapStartupDecision::TryCreateTap
}

#[cfg(any(target_os = "macos", test))]
fn macos_effective_input_monitoring_granted(
    input_monitoring_preflight_granted: bool,
    event_tap_active: bool,
) -> bool {
    input_monitoring_preflight_granted || event_tap_active
}

#[cfg(target_os = "macos")]
pub fn macos_text_expander_status() -> MacosTextExpanderStatus {
    macos::status_snapshot()
}

#[cfg(target_os = "macos")]
pub fn request_input_monitoring_access() -> bool {
    macos::request_input_monitoring_access()
}

#[cfg(target_os = "macos")]
pub fn input_monitoring_access_granted() -> bool {
    macos::input_monitoring_granted()
}

#[cfg(target_os = "macos")]
pub fn restart_event_tap() {
    macos::restart_event_tap();
}

#[cfg(target_os = "macos")]
pub fn start() {
    macos::ensure_event_tap_thread();
}

#[cfg(target_os = "linux")]
pub fn start() {}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn start() {}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    type CGEventTapProxy = *mut c_void;
    type CGEventRef = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;

    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
    const K_CG_EVENT_TAP_DISABLED: u32 = 0xFFFF_FFFE;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFF;
    const K_CG_EVENT_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_KEY_UP: u32 = 11;
    const K_CG_KEYBOARD_EVENT_KEYCODE: i32 = 9;
    const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 1 << 17;
    const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 1 << 18;
    const K_CG_EVENT_FLAG_MASK_ALTERNATE: u64 = 1 << 19;
    const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 1 << 20;
    const K_CG_ANNOTATED_SESSION_EVENT_TAP: u32 = 1;
    const MAC_KEY_DELETE: u16 = 0x33;
    const MAC_KEY_RETURN: u16 = 0x24;
    const MAC_KEY_TAB: u16 = 0x30;
    const MAC_KEY_ESCAPE: u16 = 0x35;
    const MAC_KEY_LEFT: u16 = 0x7B;
    const MAC_KEY_RIGHT: u16 = 0x7C;
    const MAC_KEY_DOWN: u16 = 0x7D;
    const MAC_KEY_UP: u16 = 0x7E;

    static MACOS_EXPANDING_TEXT: AtomicBool = AtomicBool::new(false);
    static FOREGROUND_CACHE: OnceLock<Mutex<ForegroundCacheState>> = OnceLock::new();
    static TAP_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
    static EVENT_TAP_ACTIVE: AtomicBool = AtomicBool::new(false);
    static TAP_PORT_ADDR: AtomicUsize = AtomicUsize::new(0);
    static TAP_RUN_LOOP_ADDR: AtomicUsize = AtomicUsize::new(0);
    static LAST_EVENT_AT: Mutex<Option<Instant>> = Mutex::new(None);
    static FAILURE_REASON: Mutex<Option<String>> = Mutex::new(None);
    static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

    struct TapThreadGuard;
    impl Drop for TapThreadGuard {
        fn drop(&mut self) {
            EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
            TAP_PORT_ADDR.store(0, Ordering::SeqCst);
            TAP_RUN_LOOP_ADDR.store(0, Ordering::SeqCst);
            TAP_THREAD_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    pub fn status_snapshot() -> MacosTextExpanderStatus {
        let last_event_ms_ago = LAST_EVENT_AT
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|at| at.elapsed().as_millis()));
        let event_tap_active = EVENT_TAP_ACTIVE.load(Ordering::SeqCst);
        MacosTextExpanderStatus {
            accessibility_granted: accessibility_granted(),
            input_monitoring_granted: macos_effective_input_monitoring_granted(
                input_monitoring_granted(),
                event_tap_active,
            ),
            event_tap_active,
            last_event_ms_ago,
            failure_reason: FAILURE_REASON.lock().ok().and_then(|guard| guard.clone()),
        }
    }

    pub fn request_input_monitoring_access() -> bool {
        if input_monitoring_granted() {
            return true;
        }
        unsafe { CGRequestListenEventAccess() }
    }

    pub fn ensure_event_tap_thread() {
        if TAP_THREAD_RUNNING.load(Ordering::SeqCst) {
            return;
        }
        if TAP_THREAD_RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        std::thread::spawn(|| {
            let _guard = TapThreadGuard;
            unsafe {
                run_event_tap_loop();
            }
        });
    }

    pub fn restart_event_tap() {
        RESTART_REQUESTED.store(true, Ordering::SeqCst);
        let run_loop_addr = TAP_RUN_LOOP_ADDR.load(Ordering::SeqCst);
        if run_loop_addr != 0 {
            unsafe {
                CFRunLoopStop(run_loop_addr as CFRunLoopRef);
            }
        }
        if !TAP_THREAD_RUNNING.load(Ordering::SeqCst) {
            ensure_event_tap_thread();
        }
    }

    fn accessibility_granted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub(super) fn input_monitoring_granted() -> bool {
        unsafe { CGPreflightListenEventAccess() }
    }

    fn set_failure_reason(reason: impl Into<String>) {
        let reason = reason.into();
        log::warn!("Smart Input: {reason}");
        if let Ok(mut guard) = FAILURE_REASON.lock() {
            *guard = Some(reason);
        }
        EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
    }

    fn clear_failure_reason() {
        if let Ok(mut guard) = FAILURE_REASON.lock() {
            guard.take();
        }
    }

    fn note_event_received() {
        if let Ok(mut guard) = LAST_EVENT_AT.lock() {
            *guard = Some(Instant::now());
        }
    }

    unsafe fn run_event_tap_loop() {
        loop {
            clear_failure_reason();

            match macos_event_tap_startup_decision(
                input_monitoring_granted(),
                accessibility_granted(),
            ) {
                MacosEventTapStartupDecision::TryCreateTap => {}
                MacosEventTapStartupDecision::WaitForPermission(reason) => {
                    set_failure_reason(reason);
                    std::thread::sleep(Duration::from_secs(2));
                    if !RESTART_REQUESTED.swap(false, Ordering::SeqCst) {
                        return;
                    }
                    continue;
                }
            }

            let mask = (1u64 << K_CG_EVENT_KEY_DOWN) | (1u64 << K_CG_EVENT_KEY_UP);
            let tap = CGEventTapCreate(
                K_CG_HID_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_DEFAULT,
                mask,
                Some(event_tap_callback),
                null_mut(),
            );
            if tap.is_null() {
                set_failure_reason(
                    "Failed to create keyboard event tap; check Accessibility and Input Monitoring",
                );
                std::thread::sleep(Duration::from_secs(2));
                if !RESTART_REQUESTED.swap(false, Ordering::SeqCst) {
                    return;
                }
                continue;
            }

            TAP_PORT_ADDR.store(tap as usize, Ordering::SeqCst);

            let source = CFMachPortCreateRunLoopSource(null_mut(), tap, 0);
            if source.is_null() {
                set_failure_reason("Failed to create macOS run-loop source for event tap");
                CFRelease(tap as *const c_void);
                TAP_PORT_ADDR.store(0, Ordering::SeqCst);
                return;
            }

            let run_loop = CFRunLoopGetCurrent();
            TAP_RUN_LOOP_ADDR.store(run_loop as usize, Ordering::SeqCst);
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            EVENT_TAP_ACTIVE.store(true, Ordering::SeqCst);
            log::info!("Smart Input: macOS event tap started");
            CFRunLoopRun();
            EVENT_TAP_ACTIVE.store(false, Ordering::SeqCst);
            TAP_PORT_ADDR.store(0, Ordering::SeqCst);
            TAP_RUN_LOOP_ADDR.store(0, Ordering::SeqCst);
            CFRelease(source as *const c_void);
            CFRelease(tap as *const c_void);

            if RESTART_REQUESTED.swap(false, Ordering::SeqCst) {
                continue;
            }
            break;
        }
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        _user_info: *mut c_void,
    ) -> CGEventRef {
        if event_type == K_CG_EVENT_TAP_DISABLED || event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
        {
            let tap_addr = TAP_PORT_ADDR.load(Ordering::SeqCst);
            if tap_addr != 0 {
                log::warn!("Smart Input: macOS event tap disabled; re-enabling");
                CGEventTapEnable(tap_addr as CFMachPortRef, true);
            }
            return event;
        }

        note_event_received();

        if event_type != K_CG_EVENT_KEY_DOWN && event_type != K_CG_EVENT_KEY_UP {
            return event;
        }
        if MACOS_EXPANDING_TEXT.load(Ordering::Relaxed) {
            return event;
        }
        let keycode = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
        let flags = CGEventGetFlags(event);
        if event_type == K_CG_EVENT_KEY_DOWN {
            handle_text_expander_key_down(event, keycode, flags);
        }
        event
    }

    pub(super) fn foreground_app_blacklisted(app_blacklist: &[String]) -> bool {
        if app_blacklist.is_empty() {
            return false;
        }
        foreground_app_candidate()
            .map(|app| {
                app_blacklist.iter().any(|blocked| {
                    app.exe == *blocked
                        || app
                            .exe
                            .strip_suffix(".app")
                            .is_some_and(|stem| stem == blocked)
                        || blocked
                            .strip_suffix(".app")
                            .is_some_and(|stem| stem == app.exe)
                })
            })
            .unwrap_or(false)
    }

    pub(super) fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
        let script = r#"tell application "System Events" to get name of every application process whose background only is false"#;
        let Some(output) = run_osascript(script) else {
            return Vec::new();
        };
        let current = current_process_name_lower();
        let mut apps = Vec::new();
        for raw_name in output.split(',') {
            let exe = raw_name.trim().to_ascii_lowercase();
            if exe.is_empty() || current.as_deref() == Some(exe.as_str()) {
                continue;
            }
            if !apps
                .iter()
                .any(|app: &TextExpanderAppCandidate| app.exe == exe)
            {
                apps.push(TextExpanderAppCandidate {
                    exe,
                    title: String::new(),
                });
            }
        }
        apps.sort_by(|a, b| a.exe.cmp(&b.exe));
        apps
    }

    fn handle_text_expander_key_down(event: CGEventRef, keycode: u16, flags: u64) {
        if !text_expander_enabled() {
            return;
        }
        if foreground_is_current_process() {
            return;
        }
        if text_expander_suppressed_for_context() {
            if let Ok(mut engine) = text_expander_engine().lock() {
                engine.reset();
            }
            return;
        }
        if keycode == MAC_KEY_DELETE {
            if let Ok(mut engine) = text_expander_engine().lock() {
                engine.backspace();
            }
            return;
        }
        if should_reset_text_expander_for_keycode(keycode) {
            if let Ok(mut engine) = text_expander_engine().lock() {
                engine.reset();
            }
            return;
        }
        let command = flags & K_CG_EVENT_FLAG_MASK_COMMAND != 0;
        let ctrl = flags & K_CG_EVENT_FLAG_MASK_CONTROL != 0;
        let alt = flags & K_CG_EVENT_FLAG_MASK_ALTERNATE != 0;
        if command || ctrl || alt {
            return;
        }
        if let Some(ch) = unsafe { text_expander_char_for_event(event) } {
            let expansion = text_expander_engine()
                .lock()
                .ok()
                .and_then(|mut engine| engine.push_char(ch));
            if let Some(expansion) = expansion {
                schedule_text_expansion(expansion);
            }
        }
    }

    fn should_reset_text_expander_for_keycode(keycode: u16) -> bool {
        matches!(
            keycode,
            MAC_KEY_RETURN
                | MAC_KEY_TAB
                | MAC_KEY_ESCAPE
                | MAC_KEY_LEFT
                | MAC_KEY_RIGHT
                | MAC_KEY_DOWN
                | MAC_KEY_UP
        )
    }

    unsafe fn text_expander_char_for_event(event: CGEventRef) -> Option<char> {
        let mut len = 0usize;
        let mut buffer = [0u16; 8];
        CGEventKeyboardGetUnicodeString(event, buffer.len(), &mut len, buffer.as_mut_ptr());
        if len == 0 {
            return None;
        }
        char::decode_utf16(buffer[..len.min(buffer.len())].iter().copied())
            .next()
            .and_then(Result::ok)
            .filter(|ch| !ch.is_control())
    }

    fn schedule_text_expansion(expansion: crate::text_expander::TextExpansionMatch) {
        std::thread::spawn(move || unsafe {
            std::thread::sleep(Duration::from_millis(12));
            send_text_expansion(&expansion);
        });
    }

    unsafe fn send_text_expansion(expansion: &crate::text_expander::TextExpansionMatch) {
        MACOS_EXPANDING_TEXT.store(true, Ordering::Relaxed);
        for _ in 0..expansion.typed_trigger_chars {
            send_key_tap(MAC_KEY_DELETE);
        }
        send_unicode_text(&expansion.replacement);
        for _ in 0..expansion.cursor_back_chars {
            send_key_tap(MAC_KEY_LEFT);
        }
        MACOS_EXPANDING_TEXT.store(false, Ordering::Relaxed);
    }

    unsafe fn send_key_tap(virtual_key: u16) {
        for key_down in [true, false] {
            let event = CGEventCreateKeyboardEvent(null_mut(), virtual_key, key_down);
            if event.is_null() {
                continue;
            }
            CGEventSetFlags(event, 0);
            CGEventPost(K_CG_ANNOTATED_SESSION_EVENT_TAP, event);
            CFRelease(event as *const c_void);
        }
    }

    unsafe fn send_unicode_text(text: &str) {
        for ch in text.chars() {
            send_unicode_char(ch);
        }
    }

    fn foreground_is_current_process() -> bool {
        let Some(app) = foreground_app_candidate() else {
            return false;
        };
        current_process_name_lower().as_deref() == Some(app.exe.as_str())
    }

    fn foreground_app_candidate() -> Option<TextExpanderAppCandidate> {
        let cache = FOREGROUND_CACHE.get_or_init(|| Mutex::new(None));
        if let Ok(guard) = cache.lock() {
            if let Some((checked_at, candidate)) = &*guard {
                if checked_at.elapsed() < Duration::from_millis(500) {
                    return candidate.clone();
                }
            }
        }

        let candidate = query_foreground_app_candidate();
        if let Some(candidate) = &candidate {
            remember_foreground_app(candidate.clone());
        }
        if let Ok(mut guard) = cache.lock() {
            *guard = Some((Instant::now(), candidate.clone()));
        }
        candidate
    }

    fn query_foreground_app_candidate() -> Option<TextExpanderAppCandidate> {
        let script = r#"tell application "System Events"
set frontApp to first application process whose frontmost is true
set appName to name of frontApp
set appTitle to ""
try
    set appTitle to name of front window of frontApp
end try
return appName & linefeed & appTitle
end tell"#;
        let output = run_osascript(script)?;
        let mut lines = output.lines();
        let exe = lines.next()?.trim().to_ascii_lowercase();
        if exe.is_empty() {
            return None;
        }
        let title = lines.next().unwrap_or_default().trim().to_owned();
        Some(TextExpanderAppCandidate { exe, title })
    }

    fn run_osascript(script: &str) -> Option<String> {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    unsafe fn send_unicode_char(symbol: char) {
        let mut buffer = [0u16; 2];
        let units = symbol.encode_utf16(&mut buffer);
        for key_down in [true, false] {
            let event = CGEventCreateKeyboardEvent(null_mut(), 0, key_down);
            if event.is_null() {
                continue;
            }
            CGEventSetFlags(event, 0);
            CGEventKeyboardSetUnicodeString(event, units.len(), units.as_ptr());
            CGEventPost(K_CG_ANNOTATED_SESSION_EVENT_TAP, event);
            CFRelease(event as *const c_void);
        }
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            eventsOfInterest: u64,
            callback: Option<
                unsafe extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef,
            >,
            userInfo: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetIntegerValueField(event: CGEventRef, field: i32) -> i64;
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventCreateKeyboardEvent(
            source: *mut c_void,
            virtualKey: u16,
            keyDown: bool,
        ) -> CGEventRef;
        fn CGEventKeyboardSetUnicodeString(
            event: CGEventRef,
            stringLength: usize,
            unicodeString: *const u16,
        );
        fn CGEventKeyboardGetUnicodeString(
            event: CGEventRef,
            maxStringLength: usize,
            actualStringLength: *mut usize,
            unicodeString: *mut u16,
        );
        fn CGEventPost(tap: u32, event: CGEventRef);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopCommonModes: CFStringRef;
        fn CFMachPortCreateRunLoopSource(
            allocator: *mut c_void,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: CFRunLoopRef);
        fn CFRelease(cf: *const c_void);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEXT_EXPANDER_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn rule(trigger: &str, replacement: &str) -> TextExpansionRule {
        TextExpansionRule {
            enabled: true,
            trigger: trigger.to_owned(),
            replacement: replacement.to_owned(),
        }
    }

    fn push_text(text: &str) -> Option<crate::text_expander::TextExpansionMatch> {
        let mut matched = None;
        let mut engine = text_expander_engine().lock().unwrap();
        engine.reset();
        for ch in text.chars() {
            matched = engine.push_char(ch);
        }
        matched
    }

    #[test]
    fn text_expander_runtime_config_enables_loaded_rules() {
        let _guard = TEXT_EXPANDER_TEST_LOCK.lock().unwrap();
        set_text_expander_config(
            true,
            vec![rule(":hello", "Привет")],
            vec![" Notepad.EXE ".to_owned()],
        );

        assert!(text_expander_enabled());
        assert_eq!(
            text_expander_config().read().unwrap().app_blacklist,
            vec!["notepad.exe".to_owned()]
        );
        assert_eq!(push_text(":hello").unwrap().replacement, "Привет");
    }

    #[test]
    fn text_expander_runtime_config_replaces_previous_rules() {
        let _guard = TEXT_EXPANDER_TEST_LOCK.lock().unwrap();
        set_text_expander_config(true, vec![rule(":old", "Old")], Vec::new());
        assert_eq!(push_text(":old").unwrap().replacement, "Old");

        set_text_expander_config(true, vec![rule(":new", "New")], Vec::new());

        assert!(push_text(":old").is_none());
        assert_eq!(push_text(":new").unwrap().replacement, "New");
    }

    #[test]
    fn text_expander_runtime_disabled_config_does_not_report_enabled() {
        let _guard = TEXT_EXPANDER_TEST_LOCK.lock().unwrap();
        set_text_expander_config(false, vec![rule(":hello", "Привет")], Vec::new());

        assert!(!text_expander_enabled());
    }

    #[test]
    fn macos_input_monitoring_preflight_denied_still_attempts_event_tap() {
        assert_eq!(
            macos_event_tap_startup_decision(false, true),
            MacosEventTapStartupDecision::TryCreateTap
        );
    }

    #[test]
    fn macos_active_event_tap_counts_as_input_monitoring_granted() {
        assert!(macos_effective_input_monitoring_granted(false, true));
    }
}
