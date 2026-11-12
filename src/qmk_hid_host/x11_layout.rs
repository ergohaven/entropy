//! Native X11 evidence. This connection belongs exclusively to the layout lane.
use super::layout_code_index;
use crate::key_legend_detection::{
    DetectionFailure, LayoutObservation, NativeLayoutIdentity, SystemKeymap,
};
use std::ffi::CStr;
use x11_dl::xlib;

const CORE: u32 = 0x100;
const CHANGES: u64 = 1 | 2 | (1 << 6); // new keyboard, map, names

pub(super) struct X11LayoutTracker {
    xlib: xlib::Xlib,
    display: *mut xlib::Display,
    keyboard: xlib::XkbDescPtr,
    event_type: i32,
    symbols: String,
    cached: Vec<(usize, std::sync::Arc<SystemKeymap>)>,
}

impl X11LayoutTracker {
    pub(super) fn new() -> Option<Self> {
        unsafe {
            let xlib = xlib::Xlib::open().ok()?;
            let display = (xlib.XOpenDisplay)(std::ptr::null());
            if display.is_null() {
                return None;
            }
            let (mut opcode, mut event_type, mut error, mut major, mut minor) = (0, 0, 0, 1, 0);
            if (xlib.XkbQueryExtension)(
                display,
                &mut opcode,
                &mut event_type,
                &mut error,
                &mut major,
                &mut minor,
            ) == 0
                || (xlib.XkbSelectEvents)(display, CORE, CHANGES as _, CHANGES as _) == 0
            {
                (xlib.XCloseDisplay)(display);
                return None;
            }
            Some(Self {
                xlib,
                display,
                keyboard: std::ptr::null_mut(),
                event_type,
                symbols: String::new(),
                cached: Vec::new(),
            })
        }
    }

    unsafe fn changed(&mut self) -> bool {
        (self.xlib.XSync)(self.display, 0);
        let mut changed = false;
        while (self.xlib.XPending)(self.display) > 0 {
            let mut event: xlib::XEvent = std::mem::zeroed();
            (self.xlib.XNextEvent)(self.display, &mut event);
            changed |= event.get_type() == self.event_type;
        }
        if changed {
            self.clear_map();
        }
        changed
    }

    unsafe fn clear_map(&mut self) {
        if !self.keyboard.is_null() {
            (self.xlib.XkbFreeKeyboard)(self.keyboard, 0, 1);
        }
        self.keyboard = std::ptr::null_mut();
        self.symbols.clear();
        self.cached.clear();
    }

    unsafe fn read_map(&mut self) -> Option<()> {
        if !self.keyboard.is_null() {
            return Some(());
        }
        self.keyboard = (self.xlib.XkbGetMap)(self.display, 0x7f, CORE);
        if self.keyboard.is_null() {
            return None;
        }
        if (self.xlib.XkbGetNames)(self.display, (1 << 2) | (1 << 9), self.keyboard) != 0 {
            self.clear_map();
            return None;
        }
        let names = (*self.keyboard).names;
        if names.is_null() || (*names).keys.is_null() {
            self.clear_map();
            return None;
        }
        let ptr = (self.xlib.XGetAtomName)(self.display, (*names).symbols);
        if ptr.is_null() {
            self.clear_map();
            return None;
        }
        let symbols = CStr::from_ptr(ptr).to_str().ok().map(str::to_owned);
        (self.xlib.XFree)(ptr.cast());
        self.symbols = symbols?;
        Some(())
    }

    unsafe fn group(&self) -> Option<usize> {
        let mut state: xlib::XkbStateRec = std::mem::zeroed();
        ((self.xlib.XkbGetState)(self.display, CORE, &mut state) == 0)
            .then_some(state.group as usize)
    }

    unsafe fn rules(&self) -> Result<Option<Vec<String>>, DetectionFailure> {
        let atom = (self.xlib.XInternAtom)(self.display, c"_XKB_RULES_NAMES".as_ptr(), 1);
        if atom == 0 {
            return Ok(None);
        }
        let (mut actual, mut format, mut count, mut after) = (0, 0, 0, 0);
        let mut data = std::ptr::null_mut();
        let status = (self.xlib.XGetWindowProperty)(
            self.display,
            (self.xlib.XDefaultRootWindow)(self.display),
            atom,
            0,
            4096,
            0,
            xlib::AnyPropertyType as _,
            &mut actual,
            &mut format,
            &mut count,
            &mut after,
            &mut data,
        );
        if data.is_null() {
            return if status == 0 && actual == 0 {
                Ok(None)
            } else {
                Err(DetectionFailure::Unavailable)
            };
        }
        let result = if status == 0 && format == 8 && after == 0 {
            let bytes = std::slice::from_raw_parts(data, count as usize);
            std::str::from_utf8(bytes)
                .ok()
                .map(|value| {
                    value
                        .split('\0')
                        .take(5)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .filter(|fields| fields.len() == 5)
                .ok_or(DetectionFailure::Ambiguous)
                .map(Some)
        } else {
            Err(DetectionFailure::Unavailable)
        };
        (self.xlib.XFree)(data.cast());
        result
    }

    unsafe fn keymap(&mut self, group: usize) -> Option<std::sync::Arc<SystemKeymap>> {
        if let Some((_, map)) = self.cached.iter().find(|(g, _)| *g == group) {
            return Some(map.clone());
        }
        let keys = (*(*self.keyboard).names).keys;
        let mut glyphs = std::collections::BTreeMap::new();
        for &(hid, name) in PRINTABLE_KEYS
            .iter()
            .chain(super::linux_keymaps::EXTRA_KEYS)
        {
            let Some(code) = ((*self.keyboard).min_key_code..=(*self.keyboard).max_key_code)
                .find(|code| (*keys.add(*code as usize)).name.map(|c| c as u8) == name)
            else {
                continue;
            };
            let levels = [0, 1, 128, 129].map(|mods| {
                let (mut consumed, mut symbol) = (0, 0);
                if (self.xlib.XkbTranslateKeyCode)(
                    self.keyboard,
                    code,
                    ((group as u32) << 13) | mods,
                    &mut consumed,
                    &mut symbol,
                ) == 0
                {
                    return Default::default();
                }
                super::linux_keymaps::keysym_glyph(symbol as u32)
            });
            glyphs.insert(hid, levels);
        }
        let name = self
            .rules()
            .ok()
            .flatten()
            .map(|r| {
                let layout = r[2].split(',').nth(group).unwrap_or("?");
                let variant = r[3].split(',').nth(group).unwrap_or("");
                if variant.is_empty() {
                    layout.to_owned()
                } else {
                    format!("{layout} ({variant})")
                }
            })
            .unwrap_or_else(|| format!("X11 group {}", group + 1));
        let map = std::sync::Arc::new(SystemKeymap {
            id: format!("x11:{}:{group}", self.symbols),
            name,
            keys: glyphs,
        });
        self.cached.push((group, map.clone()));
        Some(map)
    }

    pub(super) fn current_layout(&mut self) -> LayoutObservation {
        let mut language = None;
        let identity = unsafe {
            (|| {
                self.changed();
                self.read_map().ok_or(DetectionFailure::Unavailable)?;
                let group = self.group().ok_or(DetectionFailure::Unavailable)?;
                language = self
                    .rules()
                    .ok()
                    .flatten()
                    .and_then(|r| r[2].split(',').nth(group).and_then(layout_code_index));
                let keymap = self.keymap(group).ok_or(DetectionFailure::Unavailable)?;
                if self.group() != Some(group) || self.changed() {
                    return Err(DetectionFailure::Ambiguous);
                }
                Ok(NativeLayoutIdentity::Mapped(keymap))
            })()
            .unwrap_or_else(NativeLayoutIdentity::Unavailable)
        };
        LayoutObservation { language, identity }
    }
}

impl Drop for X11LayoutTracker {
    fn drop(&mut self) {
        unsafe {
            self.clear_map();
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}

pub(super) const PRINTABLE_KEYS: &[(u16, [u8; 4])] = &[
    (4, *b"AC01"),
    (5, *b"AB05"),
    (6, *b"AB03"),
    (7, *b"AC03"),
    (8, *b"AD03"),
    (9, *b"AC04"),
    (10, *b"AC05"),
    (11, *b"AC06"),
    (12, *b"AD08"),
    (13, *b"AC07"),
    (14, *b"AC08"),
    (15, *b"AC09"),
    (16, *b"AB07"),
    (17, *b"AB06"),
    (18, *b"AD09"),
    (19, *b"AD10"),
    (20, *b"AD01"),
    (21, *b"AD04"),
    (22, *b"AC02"),
    (23, *b"AD05"),
    (24, *b"AD07"),
    (25, *b"AB04"),
    (26, *b"AD02"),
    (27, *b"AB02"),
    (28, *b"AD06"),
    (29, *b"AB01"),
    (30, *b"AE01"),
    (31, *b"AE02"),
    (32, *b"AE03"),
    (33, *b"AE04"),
    (34, *b"AE05"),
    (35, *b"AE06"),
    (36, *b"AE07"),
    (37, *b"AE08"),
    (38, *b"AE09"),
    (39, *b"AE10"),
    (45, *b"AE11"),
    (46, *b"AE12"),
    (47, *b"AD11"),
    (48, *b"AD12"),
    (49, *b"BKSL"),
    (51, *b"AC10"),
    (52, *b"AC11"),
    (53, *b"TLDE"),
    (54, *b"AB08"),
    (55, *b"AB09"),
    (56, *b"AB10"),
];

#[cfg(test)]
mod tests {
    use super::*;

    // Run only on an isolated Xvfb server. This test changes that server's
    // keymap; normal tests and production detection never select a layout.
    #[test]
    #[ignore = "requires an isolated X11 test display and setxkbmap"]
    fn live_x11_mapping_changes_invalidate_cached_glyphs() {
        let _ = env_logger::builder().is_test(true).try_init();
        assert_eq!(
            std::env::var("DISPLAY").unwrap(),
            std::env::var("ENTROPY_X11_TEST_DISPLAY").unwrap()
        );
        let mut tracker = X11LayoutTracker::new().unwrap();
        for (layout, variant, expected) in [
            ("us", "", '1'),
            ("ru", "", '1'),
            ("ru", "typewriter", '№'),
            ("fr", "", '&'),
            ("de", "", '1'),
            ("us", "dvorak", '1'),
            ("us", "", '1'),
        ] {
            assert!(std::process::Command::new("setxkbmap")
                .args(["-model", "pc104", "-layout", layout, "-variant", variant, "-option", ""])
                .status()
                .unwrap()
                .success());
            unsafe {
                (tracker.xlib.XkbLockGroup)(tracker.display, CORE, 0);
                (tracker.xlib.XSync)(tracker.display, 0);
            }
            let observation = tracker.current_layout();
            assert_eq!(
                match observation.identity {
                    NativeLayoutIdentity::Mapped(ref map) => map.keys[&30][0].text.clone(),
                    _ => String::new(),
                },
                expected.to_string(),
                "{observation:?}"
            );
        }
        assert!(std::process::Command::new("setxkbmap")
            .args([
                "-model",
                "pc104",
                "-layout",
                "us,ru,ru",
                "-variant",
                ",,typewriter",
                "-option",
                "grp:alt_shift_toggle"
            ])
            .status()
            .unwrap()
            .success());
        for (group, family) in [(2, '№'), (0, '1'), (1, '1')] {
            unsafe {
                (tracker.xlib.XkbLockGroup)(tracker.display, CORE, group);
                (tracker.xlib.XSync)(tracker.display, 0);
            }
            let observation = tracker.current_layout();
            assert_eq!(
                match observation.identity {
                    NativeLayoutIdentity::Mapped(ref map) => map.keys[&30][0].text.clone(),
                    _ => String::new(),
                },
                family.to_string(),
                "{observation:?}"
            );
        }

        assert!(std::process::Command::new("setxkbmap")
            .args(["-model", "pc104", "-layout", "us", "-variant", "", "-option", ""])
            .status()
            .unwrap()
            .success());
        unsafe {
            (tracker.xlib.XkbLockGroup)(tracker.display, CORE, 0);
            (tracker.xlib.XSync)(tracker.display, 0);
        }
        let NativeLayoutIdentity::Mapped(map) = tracker.current_layout().identity else {
            panic!("missing keymap")
        };
        assert_eq!(map.keys[&30][0].text, "1");
        let display = std::env::var("DISPLAY").unwrap();
        let output = std::process::Command::new("xkbcomp")
            .args(["-xkb", &display, "-"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let mut map = String::from_utf8(output.stdout).unwrap();
        let key = map.find("key <AE01>").unwrap();
        let start = key + map[key..].find('[').unwrap() + 1;
        let end = start + map[start..].find(']').unwrap();
        map.replace_range(start..end, " a, b ");
        use std::io::Write;
        let mut compiler = std::process::Command::new("xkbcomp")
            .args(["-w", "0", "-", &display])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(map.as_bytes())
            .unwrap();
        assert!(compiler.wait().unwrap().success());
        let NativeLayoutIdentity::Mapped(map) = tracker.current_layout().identity else {
            panic!("missing changed keymap")
        };
        assert_eq!(map.keys[&30][0].text, "a");
        assert_eq!(map.keys[&30][1].text, "b");
    }
}
