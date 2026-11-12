//! Native X11 evidence. This connection belongs exclusively to the layout lane.
use super::layout_code_index;
use crate::key_legend_detection::{
    xkb_group, DetectionFailure, Glyphs, LayoutObservation, NativeLayoutIdentity,
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
    cached: Vec<(usize, bool, Glyphs)>,
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

    unsafe fn glyphs(&mut self, group: usize, iso: bool) -> Option<Glyphs> {
        if let Some((_, _, glyphs)) = self
            .cached
            .iter()
            .find(|(g, i, _)| *g == group && *i == iso)
        {
            return Some(glyphs.clone());
        }
        let keys = (*(*self.keyboard).names).keys;
        let mut glyphs = Vec::new();
        for (hid, name) in PRINTABLE_KEYS
            .iter()
            .chain(iso.then_some(&(100, *b"LSGT")).iter().copied())
        {
            let keycode = ((*self.keyboard).min_key_code..=(*self.keyboard).max_key_code)
                .find(|code| (*keys.add(*code as usize)).name.map(|c| c as u8) == *name)?;
            let mut pair = ['\0'; 2];
            for shift in 0..2 {
                let (mut consumed, mut symbol) = (0, 0);
                if (self.xlib.XkbTranslateKeyCode)(
                    self.keyboard,
                    keycode,
                    ((group as u32) << 13) | shift,
                    &mut consumed,
                    &mut symbol,
                ) == 0
                {
                    return None;
                }
                pair[shift as usize] = keysym_char(symbol).or_else(|| {
                    log::debug!("X11 legend: unsupported keysym {symbol:#x} at HID {hid:#x}");
                    None
                })?;
            }
            glyphs.push((*hid, pair[0], pair[1]));
        }
        self.cached.push((group, iso, glyphs.clone()));
        Some(glyphs)
    }

    pub(super) fn current_layout(&mut self) -> LayoutObservation {
        let mut language = None;
        let identity = unsafe {
            (|| {
                self.changed();
                self.read_map().ok_or(DetectionFailure::Unavailable)?;
                let group = self.group().ok_or(DetectionFailure::Unavailable)?;
                // Firmware still accepts language-only hints, even when another
                // symbols component makes the richer legend identity unsupported.
                language = self.symbols.split('+').find_map(|component| {
                    let (name, assigned) = match component.rsplit_once(':') {
                        Some((name, number)) => {
                            (name, number.parse::<usize>().ok()?.checked_sub(1)?)
                        }
                        None => (component, 0),
                    };
                    (assigned == group)
                        .then(|| layout_code_index(name.split('(').next()?))
                        .flatten()
                });
                let (layout, variant) =
                    xkb_group(&self.symbols, group).ok_or(DetectionFailure::Unsupported)?;
                let rules = self.rules()?;
                let options = rules.as_ref().map(|r| r[4].clone()).unwrap_or_default();
                if let Some(rules) = &rules {
                    let rule_layout = rules[2]
                        .split(',')
                        .nth(group)
                        .ok_or(DetectionFailure::Ambiguous)?;
                    let rule_variant = rules[3].split(',').nth(group).unwrap_or("");
                    if rule_layout != layout || rule_variant != variant {
                        return Err(DetectionFailure::Ambiguous);
                    }
                }
                // A declared ANSI model excludes LSGT. Otherwise require the
                // represented ISO position too; do not guess physical geometry.
                let iso = !rules
                    .as_ref()
                    .is_some_and(|r| matches!(r[1].as_str(), "pc101" | "pc104"));
                let glyphs = self
                    .glyphs(group, iso)
                    .ok_or(DetectionFailure::Unsupported)?;
                if self.group() != Some(group) || self.rules()? != rules || self.changed() {
                    return Err(DetectionFailure::Ambiguous);
                }
                Ok(NativeLayoutIdentity::Xkb {
                    backend: "X11",
                    layout,
                    variant: Some(variant),
                    options,
                    glyphs,
                })
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

fn keysym_char(symbol: u64) -> Option<char> {
    match symbol {
        0x20..=0x7e => char::from_u32(symbol as u32),
        0x01000000..=0x0110ffff => char::from_u32((symbol & 0x00ffffff) as u32),
        0x06a3 => Some('ё'),
        0x06b3 => Some('Ё'),
        0x06b0 => Some('№'),
        0x06c0..=0x06df => "юабцдефгхийклмнопярстужвьызшэщчъ"
            .chars()
            .nth((symbol - 0x06c0) as usize),
        0x06e0..=0x06ff => "ЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬЫЗШЭЩЧЪ"
            .chars()
            .nth((symbol - 0x06e0) as usize),
        _ => None,
    }
}

const PRINTABLE_KEYS: &[(u16, [u8; 4])] = &[
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
            ("us", "", crate::keycode::KeyLegendLayout::English),
            ("ru", "", crate::keycode::KeyLegendLayout::Russian),
            (
                "ru",
                "typewriter",
                crate::keycode::KeyLegendLayout::RussianTypewriter,
            ),
            ("us", "", crate::keycode::KeyLegendLayout::English),
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
                observation.identity.profile(),
                Ok(expected),
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
        for (group, family) in [
            (2, crate::keycode::KeyLegendLayout::RussianTypewriter),
            (0, crate::keycode::KeyLegendLayout::English),
            (1, crate::keycode::KeyLegendLayout::Russian),
        ] {
            unsafe {
                (tracker.xlib.XkbLockGroup)(tracker.display, CORE, group);
                (tracker.xlib.XSync)(tracker.display, 0);
            }
            let observation = tracker.current_layout();
            assert_eq!(
                observation.identity.profile(),
                Ok(family),
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
        assert_eq!(
            tracker.current_layout().identity.profile(),
            Ok(crate::keycode::KeyLegendLayout::English)
        );
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
        assert_eq!(
            tracker.current_layout().identity.profile(),
            Err(DetectionFailure::Unsupported)
        );
    }
}
