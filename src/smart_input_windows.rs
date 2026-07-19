#![allow(non_snake_case)]

#[cfg(target_os = "windows")]
use super::*;

#[cfg(target_os = "windows")]
fn foreground_process_name_lower() -> Option<String> {
    unsafe { process_name_lower_for_hwnd(GetForegroundWindow()) }
}

#[cfg(target_os = "windows")]
fn foreground_is_current_process() -> bool {
    let Some(foreground) = foreground_process_name_lower() else {
        return false;
    };
    current_process_name_lower().as_deref() == Some(foreground.as_str())
}

#[cfg(target_os = "windows")]
unsafe fn process_name_lower_for_hwnd(hwnd: HWND) -> Option<String> {
    if hwnd.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    if GetWindowThreadProcessId(hwnd, &mut process_id as *mut u32) == 0 || process_id == 0 {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
    if process.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) != 0;
    CloseHandle(process);
    if !ok || size == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
}
#[cfg(target_os = "windows")]
fn foreground_app_candidate() -> Option<TextExpanderAppCandidate> {
    unsafe { app_candidate_for_hwnd(GetForegroundWindow()) }
}

#[cfg(target_os = "windows")]
unsafe fn app_candidate_for_hwnd(hwnd: HWND) -> Option<TextExpanderAppCandidate> {
    let exe = process_name_lower_for_hwnd(hwnd)?;
    let title = window_title(hwnd).unwrap_or_default();
    Some(TextExpanderAppCandidate { exe, title })
}

#[cfg(target_os = "windows")]
unsafe fn window_title(hwnd: HWND) -> Option<String> {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    if copied <= 0 {
        return None;
    }
    Some(
        String::from_utf16_lossy(&buffer[..copied as usize])
            .trim()
            .to_owned(),
    )
}

#[cfg(target_os = "windows")]
pub(super) fn platform_open_window_candidates() -> Vec<TextExpanderAppCandidate> {
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
        let apps = &mut *(lparam as *mut Vec<TextExpanderAppCandidate>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let Some(candidate) = app_candidate_for_hwnd(hwnd) else {
            return 1;
        };
        if candidate.title.is_empty() {
            return 1;
        }
        if current_process_name_lower().as_deref() == Some(candidate.exe.as_str()) {
            return 1;
        }
        if !apps.iter().any(|app| app.exe == candidate.exe) {
            apps.push(candidate);
        }
        1
    }

    let mut apps: Vec<TextExpanderAppCandidate> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_proc), &mut apps as *mut _ as isize);
    }
    apps.sort_by(|a, b| a.exe.cmp(&b.exe));
    apps
}

#[cfg(target_os = "windows")]
fn remember_current_foreground_app() {
    if let Some(candidate) = foreground_app_candidate() {
        remember_foreground_app(candidate);
    }
}

#[cfg(target_os = "windows")]
pub(super) fn foreground_app_blacklisted(app_blacklist: &[String]) -> bool {
    if app_blacklist.is_empty() {
        return false;
    }
    foreground_process_name_lower()
        .map(|name| {
            let name_stem = name.strip_suffix(".exe").unwrap_or(&name);
            app_blacklist.iter().any(|blocked| {
                let blocked_stem = blocked.strip_suffix(".exe").unwrap_or(blocked);
                name == *blocked || name_stem == blocked_stem
            })
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
static EMOJI_ASSIGNMENT_BACKEND_READY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "windows")]
pub(super) fn emoji_assignment_backend_ready() -> bool {
    EMOJI_ASSIGNMENT_BACKEND_READY.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(target_os = "windows")]
pub(super) fn start() {
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::spawn(|| unsafe {
            run_windows_keyboard_hook_loop();
        });
    });
}

#[cfg(target_os = "windows")]
fn symbol_for_vk(vk: u32) -> Option<(char, u16)> {
    let base_keycode = base_keycode_for_vk(vk)?;
    windows_smart_symbol_for_transport(
        base_keycode,
        modifier_down(VK_CONTROL),
        modifier_down(VK_SHIFT),
        modifier_down(VK_MENU),
        modifier_down(VK_LWIN) || modifier_down(VK_RWIN),
        active_transport_modifiers(),
    )
}

#[cfg(target_os = "windows")]
fn base_keycode_for_vk(vk: u32) -> Option<u16> {
    match vk {
        0x7C..=0x87 => Some(KC_F13 + (vk - 0x7C) as u16),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn modifier_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) & 0x8000u16 as i16 != 0 }
}

#[cfg(target_os = "windows")]
fn modifier_group_for_vk(vk: u32) -> u16 {
    match vk as i32 {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => MOD_SHIFT,
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => MOD_CTRL,
        VK_MENU | VK_LMENU | VK_RMENU => MOD_ALT,
        VK_LWIN | VK_RWIN => MOD_GUI,
        _ => 0,
    }
}

#[cfg(target_os = "windows")]
fn update_active_transport_modifier_state(vk: u32, is_key_down: bool, is_key_up: bool) {
    let group = modifier_group_for_vk(vk);
    if group == 0 {
        return;
    }
    if is_key_down {
        ACTIVE_TRANSPORT_MODIFIERS.fetch_or(group as u32, std::sync::atomic::Ordering::Relaxed);
    } else if is_key_up {
        ACTIVE_TRANSPORT_MODIFIERS.fetch_and(!(group as u32), std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(target_os = "windows")]
fn active_transport_modifiers() -> u16 {
    (ACTIVE_TRANSPORT_MODIFIERS.load(std::sync::atomic::Ordering::Relaxed)
        & (MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_GUI) as u32) as u16
}

#[cfg(target_os = "windows")]
fn suppress_transport_modifier_keyups(trigger_keycode: u16) {
    TRANSPORT_MODIFIER_KEYUPS_TO_SUPPRESS.fetch_or(
        trigger_keycode as u32 & (MOD_CTRL | MOD_SHIFT | MOD_ALT | MOD_GUI) as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[cfg(target_os = "windows")]
fn should_suppress_transport_modifier_keyup(vk: u32) -> bool {
    let group = modifier_group_for_vk(vk) as u32;
    if group == 0 {
        return false;
    }

    let mut pending =
        TRANSPORT_MODIFIER_KEYUPS_TO_SUPPRESS.load(std::sync::atomic::Ordering::Relaxed);
    loop {
        if pending & group == 0 {
            return false;
        }
        let next = pending & !group;
        match TRANSPORT_MODIFIER_KEYUPS_TO_SUPPRESS.compare_exchange_weak(
            pending,
            next,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(current) => pending = current,
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn run_windows_keyboard_hook_loop() {
    let module = GetModuleHandleW(std::ptr::null());
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0);
    if hook.is_null() {
        log::warn!("Smart Input: failed to install keyboard hook");
        return;
    }
    EMOJI_ASSIGNMENT_BACKEND_READY.store(true, std::sync::atomic::Ordering::Release);

    remember_current_foreground_app();
    let foreground_hook = SetWinEventHook(
        EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_FOREGROUND,
        std::ptr::null_mut(),
        Some(foreground_event_proc),
        0,
        0,
        WINEVENT_OUTOFCONTEXT,
    );
    if foreground_hook.is_null() {
        log::warn!("Smart Input: failed to install foreground app tracker");
    }

    let mut msg = MSG::default();
    while GetMessageW(&mut msg as *mut MSG, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg as *const MSG);
        DispatchMessageW(&msg as *const MSG);
    }

    if !foreground_hook.is_null() {
        UnhookWinEvent(foreground_hook);
    }
    UnhookWindowsHookEx(hook);
    EMOJI_ASSIGNMENT_BACKEND_READY.store(false, std::sync::atomic::Ordering::Release);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn foreground_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND {
        if let Some(candidate) = app_candidate_for_hwnd(hwnd) {
            remember_foreground_app(candidate);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn keyboard_proc(n_code: i32, w_param: usize, l_param: isize) -> isize {
    if n_code == HC_ACTION {
        let info = &*(l_param as *const KBDLLHOOKSTRUCT);
        let is_key_down = w_param == WM_KEYDOWN || w_param == WM_SYSKEYDOWN;
        let is_key_up = w_param == WM_KEYUP || w_param == WM_SYSKEYUP;
        let injected = info.flags & LLKHF_INJECTED != 0;
        if !injected {
            update_active_transport_modifier_state(info.vkCode, is_key_down, is_key_up);
            if is_key_down {
                remember_current_foreground_app();
            }
            if foreground_is_current_process() {
                return CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param);
            }
            if is_key_up && should_suppress_transport_modifier_keyup(info.vkCode) {
                return 1;
            }

            if let Some(base_keycode) = base_keycode_for_vk(info.vkCode) {
                match windows_host_text_transport_for_event(
                    base_keycode,
                    modifier_down(VK_CONTROL),
                    modifier_down(VK_SHIFT),
                    modifier_down(VK_MENU),
                    modifier_down(VK_LWIN) || modifier_down(VK_RWIN),
                    active_transport_modifiers(),
                    is_key_down,
                ) {
                    crate::host_text_transport::TransportOutcome::Started => {
                        let trigger = crate::host_text_transport::START_TRIGGER_KEYCODE;
                        suppress_transport_modifier_keyups(trigger);
                        neutralize_transport_gui_menu(trigger);
                        release_transport_modifiers(trigger);
                        return 1;
                    }
                    crate::host_text_transport::TransportOutcome::Complete(text) => {
                        schedule_host_text(text);
                        return 1;
                    }
                    crate::host_text_transport::TransportOutcome::Consumed => return 1,
                    crate::host_text_transport::TransportOutcome::PassThrough => {}
                }
            }

            if is_key_down && text_expander_enabled() {
                if text_expander_suppressed_for_context() {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.reset();
                    }
                } else if info.vkCode == VK_BACK as u32 {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.backspace();
                    }
                } else if should_reset_text_expander_for_vk(info.vkCode) {
                    if let Ok(mut engine) = text_expander_engine().lock() {
                        engine.reset();
                    }
                } else if let Some(ch) = text_expander_char_for_key(info) {
                    let expansion = text_expander_engine()
                        .lock()
                        .ok()
                        .and_then(|mut engine| engine.push_char(ch));
                    if let Some(expansion) = expansion {
                        schedule_text_expansion(expansion);
                    }
                }
            }

            if let Some((symbol, trigger_keycode)) = symbol_for_vk(info.vkCode) {
                if is_key_down {
                    suppress_transport_modifier_keyups(trigger_keycode);
                    schedule_unicode_char(symbol, trigger_keycode);
                }
                if is_key_down || is_key_up {
                    return 1;
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), n_code, w_param, l_param)
}

#[cfg(target_os = "windows")]
unsafe fn text_expander_char_for_key(info: &KBDLLHOOKSTRUCT) -> Option<char> {
    if modifier_down(VK_CONTROL)
        || modifier_down(VK_MENU)
        || modifier_down(VK_LWIN)
        || modifier_down(VK_RWIN)
    {
        return None;
    }

    let mut keyboard_state = [0u8; 256];
    if GetKeyboardState(keyboard_state.as_mut_ptr()) == 0 {
        return None;
    }
    set_keyboard_state_down(&mut keyboard_state, VK_SHIFT, modifier_down(VK_SHIFT));
    set_keyboard_state_down(&mut keyboard_state, VK_CONTROL, false);
    set_keyboard_state_down(&mut keyboard_state, VK_MENU, false);
    set_keyboard_state_down(&mut keyboard_state, VK_LWIN, false);
    set_keyboard_state_down(&mut keyboard_state, VK_RWIN, false);
    if GetKeyState(VK_CAPITAL) & 1 != 0 {
        keyboard_state[VK_CAPITAL as usize] |= 1;
    }
    if (info.vkCode as usize) < keyboard_state.len() {
        keyboard_state[info.vkCode as usize] = 0x80;
    }

    let mut buffer = [0u16; 8];
    let len = ToUnicodeEx(
        info.vkCode,
        info.scanCode,
        keyboard_state.as_ptr(),
        buffer.as_mut_ptr(),
        buffer.len() as i32,
        0,
        foreground_keyboard_layout(),
    );
    if len <= 0 {
        return None;
    }

    char::decode_utf16(buffer[..len as usize].iter().copied())
        .next()
        .and_then(Result::ok)
        .filter(|ch| !ch.is_control())
}

#[cfg(target_os = "windows")]
unsafe fn foreground_keyboard_layout() -> *mut core::ffi::c_void {
    let hwnd = GetForegroundWindow();
    if hwnd.is_null() {
        return GetKeyboardLayout(0);
    }
    let thread_id = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
    if thread_id == 0 {
        return GetKeyboardLayout(0);
    }
    GetKeyboardLayout(thread_id)
}

#[cfg(target_os = "windows")]
fn set_keyboard_state_down(keyboard_state: &mut [u8; 256], vk: i32, down: bool) {
    if vk >= 0 && (vk as usize) < keyboard_state.len() {
        if down {
            keyboard_state[vk as usize] |= 0x80;
        } else {
            keyboard_state[vk as usize] &= !0x80;
        }
    }
}

#[cfg(target_os = "windows")]
fn should_reset_text_expander_for_vk(vk: u32) -> bool {
    matches!(
        vk as i32,
        VK_RETURN | VK_TAB | VK_ESCAPE | VK_LEFT | VK_UP | VK_RIGHT | VK_DOWN
    )
}

#[cfg(target_os = "windows")]
fn schedule_text_expansion(expansion: crate::text_expander::TextExpansionMatch) {
    std::thread::spawn(move || unsafe {
        std::thread::sleep(std::time::Duration::from_millis(8));
        send_text_expansion(&expansion);
    });
}

#[cfg(target_os = "windows")]
unsafe fn send_text_expansion(expansion: &crate::text_expander::TextExpansionMatch) {
    for _ in 0..expansion.typed_trigger_chars {
        send_vk_tap(VK_BACK as u16);
    }

    let pasted = should_use_clipboard_paste(&expansion.replacement)
        && paste_text_with_clipboard_restore(&expansion.replacement);
    if !pasted {
        send_unicode_text(&expansion.replacement);
    }

    for _ in 0..expansion.cursor_back_chars {
        send_vk_tap(VK_LEFT as u16);
    }
}

#[cfg(target_os = "windows")]
fn should_use_clipboard_paste(text: &str) -> bool {
    text.chars().count() > 64 || text.contains(['\n', '\r', '\t'])
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum WindowsTextUnit {
    Character(char),
    LineBreak,
}

#[cfg(any(target_os = "windows", test))]
fn windows_text_units(text: &str) -> impl Iterator<Item = WindowsTextUnit> + '_ {
    let mut chars = text.chars().peekable();
    std::iter::from_fn(move || {
        let ch = chars.next()?;
        Some(match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                WindowsTextUnit::LineBreak
            }
            '\n' => WindowsTextUnit::LineBreak,
            _ => WindowsTextUnit::Character(ch),
        })
    })
}

#[cfg(any(target_os = "windows", test))]
fn windows_clipboard_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for unit in windows_text_units(text) {
        match unit {
            WindowsTextUnit::Character(ch) => output.push(ch),
            WindowsTextUnit::LineBreak => output.push_str("\r\n"),
        }
    }
    output
}

#[cfg(target_os = "windows")]
unsafe fn paste_text_with_clipboard_restore(text: &str) -> bool {
    let _guard = match CLIPBOARD_PASTE_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(previous_text) = read_clipboard_text() else {
        return false;
    };
    if !set_clipboard_text(text) {
        return false;
    }
    std::thread::sleep(std::time::Duration::from_millis(12));
    let pasted = send_ctrl_v();
    std::thread::sleep(std::time::Duration::from_millis(80));
    if !set_clipboard_text(&previous_text) {
        log::warn!("Smart Input: failed to restore clipboard text after paste");
    }
    pasted
}

#[cfg(target_os = "windows")]
unsafe fn read_clipboard_text() -> Option<String> {
    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return None;
    }
    let handle = GetClipboardData(CF_UNICODETEXT);
    if handle.is_null() {
        CloseClipboard();
        return None;
    }
    let locked = GlobalLock(handle) as *const u16;
    if locked.is_null() {
        CloseClipboard();
        return None;
    }
    let mut len = 0usize;
    while *locked.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(locked, len));
    GlobalUnlock(handle);
    CloseClipboard();
    Some(text)
}

#[cfg(target_os = "windows")]
unsafe fn set_clipboard_text(text: &str) -> bool {
    if OpenClipboard(std::ptr::null_mut()) == 0 {
        return false;
    }
    if EmptyClipboard() == 0 {
        CloseClipboard();
        return false;
    }
    let clipboard_text = windows_clipboard_text(text);
    let utf16 = clipboard_text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if handle.is_null() {
        CloseClipboard();
        return false;
    }
    let locked = GlobalLock(handle) as *mut u16;
    if locked.is_null() {
        GlobalFree(handle);
        CloseClipboard();
        return false;
    }
    std::ptr::copy_nonoverlapping(utf16.as_ptr(), locked, utf16.len());
    GlobalUnlock(handle);
    if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
        GlobalFree(handle);
        CloseClipboard();
        return false;
    }
    CloseClipboard();
    true
}

#[cfg(target_os = "windows")]
unsafe fn send_ctrl_v() -> bool {
    let inputs = [
        INPUT::keyboard_vk(VK_CONTROL as u16, false),
        INPUT::keyboard_vk(VK_V as u16, false),
        INPUT::keyboard_vk(VK_V as u16, true),
        INPUT::keyboard_vk(VK_CONTROL as u16, true),
    ];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for Ctrl+V");
        send_vk_keyup(VK_V as u16);
        send_vk_keyup(VK_CONTROL as u16);
    }
    sent >= 2
}

#[cfg(target_os = "windows")]
unsafe fn send_unicode_text(text: &str) {
    for text_unit in windows_text_units(text) {
        match text_unit {
            WindowsTextUnit::Character(ch) => {
                for unit in ch.encode_utf16(&mut [0; 2]) {
                    let down = INPUT::keyboard_unicode(*unit, false);
                    let up = INPUT::keyboard_unicode(*unit, true);
                    let inputs = [down, up];
                    let sent = SendInput(
                        inputs.len() as u32,
                        inputs.as_ptr(),
                        std::mem::size_of::<INPUT>() as i32,
                    );
                    if sent != inputs.len() as u32 {
                        log::warn!(
                            "Smart Input: SendInput failed for text expansion unit U+{:04X}",
                            *unit as u32
                        );
                    }
                }
            }
            WindowsTextUnit::LineBreak => send_text_expansion_line_break(),
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn send_text_expansion_line_break() {
    if modifier_down(VK_SHIFT) {
        send_vk_tap(VK_RETURN as u16);
        return;
    }

    // Shift+Enter inserts a line break without submitting chat and browser forms.
    let inputs = [
        INPUT::keyboard_vk(VK_SHIFT as u16, false),
        INPUT::keyboard_vk(VK_RETURN as u16, false),
        INPUT::keyboard_vk(VK_RETURN as u16, true),
        INPUT::keyboard_vk(VK_SHIFT as u16, true),
    ];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for text expansion line break");
        send_vk_keyup(VK_RETURN as u16);
        send_vk_keyup(VK_SHIFT as u16);
    }
}

#[cfg(target_os = "windows")]
unsafe fn send_vk_tap(vk: u16) {
    let down = INPUT::keyboard_vk(vk, false);
    let up = INPUT::keyboard_vk(vk, true);
    let inputs = [down, up];
    let sent = SendInput(
        inputs.len() as u32,
        inputs.as_ptr(),
        std::mem::size_of::<INPUT>() as i32,
    );
    if sent != inputs.len() as u32 {
        log::warn!("Smart Input: SendInput failed for VK tap 0x{vk:02X}");
    }
}

#[cfg(target_os = "windows")]
fn schedule_unicode_char(symbol: char, trigger_keycode: u16) {
    std::thread::spawn(move || unsafe {
        std::thread::sleep(std::time::Duration::from_millis(2));
        send_unicode_char(symbol, trigger_keycode);
    });
}

#[cfg(target_os = "windows")]
fn schedule_host_text(text: String) {
    std::thread::spawn(move || unsafe {
        std::thread::sleep(std::time::Duration::from_millis(2));
        if should_paste_universal_symbol_for_foreground_app()
            && paste_text_with_clipboard_restore(&text)
        {
            return;
        }
        send_unicode_text(&text);
    });
}

#[cfg(target_os = "windows")]
unsafe fn send_unicode_char(symbol: char, trigger_keycode: u16) {
    neutralize_transport_alt_menu(trigger_keycode);
    neutralize_transport_gui_menu(trigger_keycode);
    release_transport_modifiers(trigger_keycode);
    let symbol_text = symbol.to_string();
    if should_paste_universal_symbol_for_foreground_app()
        && paste_text_with_clipboard_restore(&symbol_text)
    {
        return;
    }
    for unit in symbol.encode_utf16(&mut [0; 2]) {
        let down = INPUT::keyboard_unicode(*unit, false);
        let up = INPUT::keyboard_unicode(*unit, true);
        let inputs = [down, up];
        let sent = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
        if sent != inputs.len() as u32 {
            log::warn!("Smart Input: SendInput failed for U+{:04X}", *unit as u32);
        }
    }
}

#[cfg(target_os = "windows")]
fn should_paste_universal_symbol_for_foreground_app() -> bool {
    foreground_process_name_lower()
        .as_deref()
        .map(super::app_prefers_clipboard_unicode_input)
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
unsafe fn neutralize_transport_alt_menu(trigger_keycode: u16) {
    if trigger_keycode & MOD_ALT == 0 {
        return;
    }
    // The foreground app already saw Alt down before the transport key was suppressed.
    // Send a harmless Alt+F24 tap so Windows does not treat the later Alt up as menu activation.
    send_vk_tap(VK_F24 as u16);
}

#[cfg(target_os = "windows")]
unsafe fn neutralize_transport_gui_menu(trigger_keycode: u16) {
    if trigger_keycode & MOD_GUI == 0 {
        return;
    }
    send_vk_tap(VK_F24 as u16);
}

#[cfg(target_os = "windows")]
unsafe fn release_transport_modifiers(trigger_keycode: u16) {
    if trigger_keycode & MOD_SHIFT != 0 {
        release_modifier_pair(VK_SHIFT, VK_LSHIFT, VK_RSHIFT);
    }
    if trigger_keycode & MOD_ALT != 0 {
        release_modifier_pair(VK_MENU, VK_LMENU, VK_RMENU);
    }
    if trigger_keycode & MOD_CTRL != 0 {
        release_modifier_pair(VK_CONTROL, VK_LCONTROL, VK_RCONTROL);
    }
    if trigger_keycode & MOD_GUI != 0 {
        release_modifier_pair(VK_LWIN, VK_LWIN, VK_RWIN);
    }
}

#[cfg(target_os = "windows")]
unsafe fn release_modifier_pair(generic: i32, left: i32, right: i32) {
    let left_down = modifier_down(left);
    let right_down = modifier_down(right);
    if left_down {
        send_vk_keyup(left as u16);
    }
    if right_down {
        send_vk_keyup(right as u16);
    }
    if !left_down && !right_down {
        send_vk_keyup(generic as u16);
    }
}

#[cfg(target_os = "windows")]
unsafe fn send_vk_keyup(vk: u16) {
    let input = INPUT::keyboard_vk(vk, true);
    let sent = SendInput(
        1,
        &input as *const INPUT,
        std::mem::size_of::<INPUT>() as i32,
    );
    if sent != 1 {
        log::warn!("Smart Input: SendInput failed for VK keyup 0x{:02X}", vk);
    }
}

#[cfg(target_os = "windows")]
const WH_KEYBOARD_LL: i32 = 13;
#[cfg(target_os = "windows")]
const HC_ACTION: i32 = 0;
#[cfg(target_os = "windows")]
const WM_KEYDOWN: usize = 0x0100;
#[cfg(target_os = "windows")]
const WM_SYSKEYDOWN: usize = 0x0104;
#[cfg(target_os = "windows")]
const WM_KEYUP: usize = 0x0101;
#[cfg(target_os = "windows")]
const WM_SYSKEYUP: usize = 0x0105;
#[cfg(target_os = "windows")]
const VK_SHIFT: i32 = 0x10;
#[cfg(target_os = "windows")]
const VK_CONTROL: i32 = 0x11;
#[cfg(target_os = "windows")]
const VK_MENU: i32 = 0x12;
#[cfg(target_os = "windows")]
const VK_CAPITAL: i32 = 0x14;
#[cfg(target_os = "windows")]
const VK_LSHIFT: i32 = 0xA0;
#[cfg(target_os = "windows")]
const VK_RSHIFT: i32 = 0xA1;
#[cfg(target_os = "windows")]
const VK_LCONTROL: i32 = 0xA2;
#[cfg(target_os = "windows")]
const VK_RCONTROL: i32 = 0xA3;
#[cfg(target_os = "windows")]
const VK_LMENU: i32 = 0xA4;
#[cfg(target_os = "windows")]
const VK_RMENU: i32 = 0xA5;
#[cfg(target_os = "windows")]
const VK_BACK: i32 = 0x08;
#[cfg(target_os = "windows")]
const VK_TAB: i32 = 0x09;
#[cfg(target_os = "windows")]
const VK_RETURN: i32 = 0x0D;
#[cfg(target_os = "windows")]
const VK_ESCAPE: i32 = 0x1B;
#[cfg(target_os = "windows")]
const VK_LEFT: i32 = 0x25;
#[cfg(target_os = "windows")]
const VK_UP: i32 = 0x26;
#[cfg(target_os = "windows")]
const VK_RIGHT: i32 = 0x27;
#[cfg(target_os = "windows")]
const VK_DOWN: i32 = 0x28;
#[cfg(target_os = "windows")]
const VK_LWIN: i32 = 0x5B;
#[cfg(target_os = "windows")]
const VK_RWIN: i32 = 0x5C;
#[cfg(target_os = "windows")]
const VK_V: i32 = 0x56;
#[cfg(target_os = "windows")]
const VK_F24: i32 = 0x87;
#[cfg(target_os = "windows")]
const LLKHF_INJECTED: u32 = 0x10;
#[cfg(target_os = "windows")]
const INPUT_KEYBOARD: u32 = 1;
#[cfg(target_os = "windows")]
const KEYEVENTF_KEYUP: u32 = 0x0002;
#[cfg(target_os = "windows")]
const KEYEVENTF_UNICODE: u32 = 0x0004;
#[cfg(target_os = "windows")]
const CF_UNICODETEXT: u32 = 13;
#[cfg(target_os = "windows")]
const GMEM_MOVEABLE: u32 = 0x0002;
#[cfg(target_os = "windows")]
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
#[cfg(target_os = "windows")]
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
#[cfg(target_os = "windows")]
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;

#[cfg(target_os = "windows")]
static TRANSPORT_MODIFIER_KEYUPS_TO_SUPPRESS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[cfg(target_os = "windows")]
static ACTIVE_TRANSPORT_MODIFIERS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[cfg(target_os = "windows")]
static CLIPBOARD_PASTE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "windows")]
type HHOOK = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type HWINEVENTHOOK = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type HINSTANCE = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type HWND = *mut core::ffi::c_void;
#[cfg(target_os = "windows")]
type HANDLE = *mut core::ffi::c_void;

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct KBDLLHOOKSTRUCT {
    vkCode: u32,
    scanCode: u32,
    flags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MSG {
    hwnd: HWND,
    message: u32,
    wParam: usize,
    lParam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
struct INPUT {
    input_type: u32,
    u: INPUT_UNION,
}

#[cfg(target_os = "windows")]
impl INPUT {
    fn keyboard_unicode(unit: u16, key_up: bool) -> Self {
        let flags = KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 };
        Self {
            input_type: INPUT_KEYBOARD,
            u: INPUT_UNION {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: unit,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn keyboard_vk(vk: u16, key_up: bool) -> Self {
        let flags = if key_up { KEYEVENTF_KEYUP } else { 0 };
        Self {
            input_type: INPUT_KEYBOARD,
            u: INPUT_UNION {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
union INPUT_UNION {
    mi: MOUSEINPUT,
    ki: KEYBDINPUT,
}

#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
const _: [(); 40] = [(); std::mem::size_of::<INPUT>()];
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
const _: [(); 28] = [(); std::mem::size_of::<INPUT>()];

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MOUSEINPUT {
    dx: i32,
    dy: i32,
    mouseData: u32,
    dwFlags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Clone, Copy)]
struct KEYBDINPUT {
    wVk: u16,
    wScan: u16,
    dwFlags: u32,
    time: u32,
    dwExtraInfo: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        idHook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        hmod: HINSTANCE,
        dwThreadId: u32,
    ) -> HHOOK;
    fn CallNextHookEx(hhk: HHOOK, nCode: i32, wParam: usize, lParam: isize) -> isize;
    fn UnhookWindowsHookEx(hhk: HHOOK) -> i32;
    fn SetWinEventHook(
        eventMin: u32,
        eventMax: u32,
        hmodWinEventProc: HINSTANCE,
        pfnWinEventProc: Option<
            unsafe extern "system" fn(HWINEVENTHOOK, u32, HWND, i32, i32, u32, u32),
        >,
        idProcess: u32,
        idThread: u32,
        dwFlags: u32,
    ) -> HWINEVENTHOOK;
    fn UnhookWinEvent(hWinEventHook: HWINEVENTHOOK) -> i32;
    fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn TranslateMessage(lpMsg: *const MSG) -> i32;
    fn DispatchMessageW(lpMsg: *const MSG) -> isize;
    fn SendInput(cInputs: u32, pInputs: *const INPUT, cbSize: i32) -> u32;
    fn GetAsyncKeyState(vKey: i32) -> i16;
    fn GetKeyboardState(lpKeyState: *mut u8) -> i32;
    fn GetKeyState(nVirtKey: i32) -> i16;
    fn GetKeyboardLayout(idThread: u32) -> *mut core::ffi::c_void;
    fn ToUnicodeEx(
        wVirtKey: u32,
        wScanCode: u32,
        lpKeyState: *const u8,
        pwszBuff: *mut u16,
        cchBuff: i32,
        wFlags: u32,
        dwhkl: *mut core::ffi::c_void,
    ) -> i32;
    fn GetForegroundWindow() -> HWND;
    fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut u32) -> u32;
    fn EnumWindows(
        lpEnumFunc: Option<unsafe extern "system" fn(HWND, isize) -> i32>,
        lParam: isize,
    ) -> i32;
    fn IsWindowVisible(hWnd: HWND) -> i32;
    fn GetWindowTextLengthW(hWnd: HWND) -> i32;
    fn GetWindowTextW(hWnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
    fn OpenClipboard(hWndNewOwner: HWND) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn GetClipboardData(uFormat: u32) -> HANDLE;
    fn SetClipboardData(uFormat: u32, hMem: HANDLE) -> HANDLE;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> HINSTANCE;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> HANDLE;
    fn QueryFullProcessImageNameW(
        hProcess: HANDLE,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;
    fn CloseHandle(hObject: HANDLE) -> i32;
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> HANDLE;
    fn GlobalLock(hMem: HANDLE) -> *mut core::ffi::c_void;
    fn GlobalUnlock(hMem: HANDLE) -> i32;
    fn GlobalFree(hMem: HANDLE) -> HANDLE;
}

#[cfg(test)]
mod tests {
    use super::{windows_clipboard_text, windows_text_units, WindowsTextUnit};

    #[test]
    fn normalizes_clipboard_line_endings_for_windows() {
        assert_eq!(windows_clipboard_text("line1\nline2"), "line1\r\nline2");
        assert_eq!(windows_clipboard_text("line1\r\nline2"), "line1\r\nline2");
        assert_eq!(windows_clipboard_text("line1\rline2"), "line1\r\nline2");
    }

    #[test]
    fn treats_each_newline_encoding_as_one_windows_line_break() {
        for text in ["line1\nline2", "line1\r\nline2", "line1\rline2"] {
            let units = windows_text_units(text).collect::<Vec<_>>();
            assert_eq!(
                units
                    .iter()
                    .filter(|unit| matches!(unit, WindowsTextUnit::LineBreak))
                    .count(),
                1
            );
            assert_eq!(
                units
                    .iter()
                    .filter_map(|unit| match unit {
                        WindowsTextUnit::Character(ch) => Some(*ch),
                        WindowsTextUnit::LineBreak => None,
                    })
                    .collect::<String>(),
                "line1line2"
            );
        }
    }
}
