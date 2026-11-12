//! Read loaded layouts and translate physical scan codes without activation.
use crate::key_legend_detection::{
    DetectionFailure, KeyGlyph, LayoutObservation, NativeLayoutIdentity, SystemKeymap,
    SystemLayoutDescriptor,
};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME, LOCALE_SLOCALIZEDDISPLAYNAME},
    System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ},
    UI::{
        Input::KeyboardAndMouse::{
            GetKeyboardLayout, GetKeyboardLayoutList, MapVirtualKeyExW, ToUnicodeEx,
        },
        WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
};

fn handles() -> Vec<usize> {
    unsafe {
        let count = GetKeyboardLayoutList(0, std::ptr::null_mut());
        if count <= 0 {
            return Vec::new();
        }
        let mut layouts = vec![std::ptr::null_mut(); count as usize];
        let count = GetKeyboardLayoutList(count, layouts.as_mut_ptr());
        layouts
            .into_iter()
            .take(count.max(0) as usize)
            .map(|h| h as usize)
            .collect()
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn descriptor(hkl: usize) -> SystemLayoutDescriptor {
    let mut buffer = [0u16; 256];
    let locale_name = unsafe {
        let n = GetLocaleInfoW(
            (hkl & 0xffff) as u32,
            LOCALE_SLOCALIZEDDISPLAYNAME,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        );
        (n > 1).then(|| String::from_utf16_lossy(&buffer[..n as usize - 1]))
    };
    let records = super::windows_platform::keyboard_layout_registry().unwrap_or_default();
    let klid = crate::key_legend_detection::windows_klid(hkl, &records).ok();
    let registry_name = klid.as_ref().and_then(|id| unsafe {
        let subkey = wide(&format!(
            "SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\\{id}"
        ));
        let mut bytes = std::mem::size_of_val(&buffer) as u32;
        (RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            wide("Layout Text").as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        ) == 0)
            .then(|| {
                String::from_utf16_lossy(&buffer[..(bytes as usize / 2).min(buffer.len())])
                    .trim_end_matches('\0')
                    .to_owned()
            })
    });
    SystemLayoutDescriptor {
        id: format!("windows:{hkl:016X}"),
        name: registry_name
            .or(locale_name)
            .unwrap_or_else(|| format!("Windows {hkl:016X}")),
    }
}

pub(super) fn installed_layouts() -> Vec<SystemLayoutDescriptor> {
    handles().into_iter().map(descriptor).collect()
}

fn read_keymap(hkl: usize) -> Option<Arc<SystemKeymap>> {
    if hkl == 0 {
        return None;
    }
    let source = descriptor(hkl);
    let mut keys = BTreeMap::new();
    for &(hid, scan) in KEYS {
        let mut levels: [KeyGlyph; 4] = Default::default();
        unsafe {
            let vk = MapVirtualKeyExW(scan, 3, hkl as _);
            if vk == 0 {
                continue;
            }
            for (level, mods) in [0, 1, 2, 3].into_iter().enumerate() {
                let mut state = [0u8; 256];
                if mods & 1 != 0 {
                    state[0x10] = 0x80;
                    state[0xa0] = 0x80;
                }
                if mods & 2 != 0 {
                    state[0x11] = 0x80;
                    state[0xa2] = 0x80;
                    state[0x12] = 0x80;
                    state[0xa5] = 0x80;
                }
                let mut output = [0u16; 16];
                // Windows 10 1607+: flag 4 leaves the OS dead-key state intact.
                let n = ToUnicodeEx(
                    vk,
                    scan,
                    state.as_ptr(),
                    output.as_mut_ptr(),
                    output.len() as i32,
                    4,
                    hkl as _,
                );
                if n == 0 || n.unsigned_abs() as usize > output.len() {
                    continue;
                }
                if let Ok(text) = String::from_utf16(&output[..n.unsigned_abs() as usize]) {
                    levels[level] = KeyGlyph::new(text, n < 0);
                }
            }
        }
        keys.insert(hid, levels);
    }
    Some(Arc::new(SystemKeymap {
        id: source.id,
        name: source.name,
        third_level_modifier: crate::key_legend_detection::ThirdLevelModifier::AltGr,
        keys,
    }))
}

pub(super) fn load_layout(id: &str) -> Option<Arc<SystemKeymap>> {
    let hkl = usize::from_str_radix(id.strip_prefix("windows:")?, 16).ok()?;
    handles().contains(&hkl).then(|| read_keymap(hkl)).flatten()
}

pub(super) struct Tracker {
    cached: Option<(usize, Instant, Arc<SystemKeymap>)>,
}
impl Tracker {
    pub(super) fn new() -> Self {
        Self { cached: None }
    }
    pub(super) fn current_layout(&mut self) -> LayoutObservation {
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return None.into();
            }
            let thread = GetWindowThreadProcessId(window, std::ptr::null_mut());
            if thread == 0 {
                return None.into();
            }
            let hkl = GetKeyboardLayout(thread) as usize;
            if hkl == 0 {
                return None.into();
            }
            let mut buffer = [0u16; 16];
            let n = GetLocaleInfoW(
                (hkl & 0xffff) as u32,
                LOCALE_SISO639LANGNAME,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            );
            let language = (n > 1)
                .then(|| String::from_utf16_lossy(&buffer[..n as usize - 1]))
                .and_then(|s| super::layout_code_index(&s));
            let mapping = self
                .cached
                .as_ref()
                .filter(|(id, at, _)| *id == hkl && at.elapsed() < Duration::from_secs(10))
                .map(|(_, _, map)| map.clone())
                .or_else(|| {
                    let map = read_keymap(hkl)?;
                    self.cached = Some((hkl, Instant::now(), map.clone()));
                    Some(map)
                });
            let current = GetForegroundWindow();
            if current != window
                || GetWindowThreadProcessId(current, std::ptr::null_mut()) != thread
                || GetKeyboardLayout(thread) as usize != hkl
            {
                return LayoutObservation {
                    language: None,
                    identity: NativeLayoutIdentity::Unavailable(DetectionFailure::Ambiguous),
                };
            }
            LayoutObservation {
                language,
                identity: mapping.map(NativeLayoutIdentity::Mapped).unwrap_or(
                    NativeLayoutIdentity::Unavailable(DetectionFailure::Unavailable),
                ),
            }
        }
    }
}

// USB HID usage -> set-1 physical scan code (not a US virtual-key name).
const KEYS: &[(u16, u32)] = &[
    (4, 0x1e),
    (5, 0x30),
    (6, 0x2e),
    (7, 0x20),
    (8, 0x12),
    (9, 0x21),
    (10, 0x22),
    (11, 0x23),
    (12, 0x17),
    (13, 0x24),
    (14, 0x25),
    (15, 0x26),
    (16, 0x32),
    (17, 0x31),
    (18, 0x18),
    (19, 0x19),
    (20, 0x10),
    (21, 0x13),
    (22, 0x1f),
    (23, 0x14),
    (24, 0x16),
    (25, 0x2f),
    (26, 0x11),
    (27, 0x2d),
    (28, 0x15),
    (29, 0x2c),
    (30, 2),
    (31, 3),
    (32, 4),
    (33, 5),
    (34, 6),
    (35, 7),
    (36, 8),
    (37, 9),
    (38, 10),
    (39, 11),
    (44, 0x39),
    (45, 12),
    (46, 13),
    (47, 26),
    (48, 27),
    (49, 43),
    (50, 43),
    (51, 39),
    (52, 40),
    (53, 41),
    (54, 51),
    (55, 52),
    (56, 53),
    (100, 86),
    (135, 0x73),
    (137, 0x7d),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_windows_layouts_supply_glyphs_without_activation() {
        let before = unsafe { GetKeyboardLayout(0) };
        let layouts = installed_layouts();
        assert!(!layouts.is_empty());
        for source in layouts {
            let map = load_layout(&source.id).expect("loaded layout remains readable");
            assert_eq!(map.id, source.id);
            assert!(!map.keys[&4][0].text.is_empty(), "{}", map.name);
            assert!(!map.keys[&30][1].text.is_empty(), "{}", map.name);
        }
        assert_eq!(unsafe { GetKeyboardLayout(0) }, before);
    }
}
