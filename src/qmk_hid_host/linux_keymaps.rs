//! Compile installed XKB layouts without selecting a layout or changing input.
use crate::key_legend_detection::{KeyGlyph, SystemKeymap, SystemLayoutDescriptor};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    sync::{Arc, OnceLock},
};

#[repr(C)]
struct Names {
    rules: *const c_char,
    model: *const c_char,
    layout: *const c_char,
    variant: *const c_char,
    options: *const c_char,
}

struct Xkb(libloading::Library);
impl Xkb {
    fn get() -> Option<&'static Self> {
        static LIBRARY: OnceLock<Option<Xkb>> = OnceLock::new();
        LIBRARY
            .get_or_init(|| unsafe { libloading::Library::new("libxkbcommon.so.0").ok().map(Xkb) })
            .as_ref()
    }
    unsafe fn function<T: Copy>(&self, name: &[u8]) -> Option<T> {
        self.0.get::<T>(name).ok().map(|p| *p)
    }
}

pub(super) fn keysym_glyph(symbol: u32) -> KeyGlyph {
    let Some(api) = Xkb::get() else {
        return KeyGlyph::default();
    };
    unsafe {
        let Some(to_utf32) =
            api.function::<unsafe extern "C" fn(u32) -> u32>(b"xkb_keysym_to_utf32\0")
        else {
            return KeyGlyph::default();
        };
        if let Some(c) = char::from_u32(to_utf32(symbol)).filter(|c| !c.is_control()) {
            return KeyGlyph::new(c.to_string(), false);
        }
        let Some(get_name) = api.function::<unsafe extern "C" fn(u32, *mut c_char, usize) -> i32>(
            b"xkb_keysym_get_name\0",
        ) else {
            return KeyGlyph::default();
        };
        let mut name = [0 as c_char; 128];
        if get_name(symbol, name.as_mut_ptr(), name.len()) <= 0 {
            return KeyGlyph::default();
        }
        let name = CStr::from_ptr(name.as_ptr()).to_string_lossy();
        let Some(dead) = name.strip_prefix("dead_") else {
            return KeyGlyph::default();
        };
        let glyph = match dead {
            "grave" => "`",
            "acute" => "´",
            "circumflex" => "^",
            "tilde" => "~",
            "macron" => "¯",
            "breve" => "˘",
            "abovedot" => "˙",
            "diaeresis" => "¨",
            "abovering" => "˚",
            "doubleacute" => "˝",
            "caron" => "ˇ",
            "cedilla" => "¸",
            "ogonek" => "˛",
            "belowdot" => "̣",
            "voiced_sound" => "゛",
            "semivoiced_sound" => "゜",
            _ => dead,
        };
        KeyGlyph::new(glyph.into(), true)
    }
}

pub(super) fn load(id: &str, name: &str, options: &str) -> Option<Arc<SystemKeymap>> {
    // The shared detector samples frequently; compile each unchanged source once.
    type Cached = (
        String,
        String,
        String,
        std::time::Instant,
        Arc<SystemKeymap>,
    );
    static CACHE: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    let mut cached = CACHE.lock().ok()?;
    if let Some((old_id, old_name, old_options, at, map)) = cached.as_ref() {
        if (old_id.as_str(), old_name.as_str(), old_options.as_str()) == (id, name, options)
            && at.elapsed() < std::time::Duration::from_secs(10)
        {
            return Some(map.clone());
        }
    }
    let map = compile(id, name, options)?;
    *cached = Some((
        id.into(),
        name.into(),
        options.into(),
        std::time::Instant::now(),
        map.clone(),
    ));
    Some(map)
}

fn compile(id: &str, name: &str, options: &str) -> Option<Arc<SystemKeymap>> {
    let spec = id.strip_prefix("xkb:")?;
    let (layout, variant) = spec.split_once(':').unwrap_or((spec, ""));
    if layout.is_empty() {
        return None;
    }
    let api = Xkb::get()?;
    let layout = CString::new(layout).ok()?;
    let variant = CString::new(variant).ok()?;
    let options = CString::new(options).ok()?;
    unsafe {
        let context_new =
            api.function::<unsafe extern "C" fn(u32) -> *mut c_void>(b"xkb_context_new\0")?;
        let context_unref =
            api.function::<unsafe extern "C" fn(*mut c_void)>(b"xkb_context_unref\0")?;
        let map_new = api
            .function::<unsafe extern "C" fn(*mut c_void, *const Names, u32) -> *mut c_void>(
                b"xkb_keymap_new_from_names\0",
            )?;
        let map_unref = api.function::<unsafe extern "C" fn(*mut c_void)>(b"xkb_keymap_unref\0")?;
        let key_by_name = api.function::<unsafe extern "C" fn(*mut c_void, *const c_char) -> u32>(
            b"xkb_keymap_key_by_name\0",
        )?;
        let state_new =
            api.function::<unsafe extern "C" fn(*mut c_void) -> *mut c_void>(b"xkb_state_new\0")?;
        let state_unref =
            api.function::<unsafe extern "C" fn(*mut c_void)>(b"xkb_state_unref\0")?;
        let update = api
            .function::<unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32, u32, u32) -> u32>(
                b"xkb_state_update_mask\0",
            )?;
        let symbol = api.function::<unsafe extern "C" fn(*mut c_void, u32) -> u32>(
            b"xkb_state_key_get_one_sym\0",
        )?;
        let mod_index = api.function::<unsafe extern "C" fn(*mut c_void, *const c_char) -> u32>(
            b"xkb_keymap_mod_get_index\0",
        )?;
        let context = context_new(0);
        if context.is_null() {
            return None;
        }
        let names = Names {
            rules: c"evdev".as_ptr(),
            model: c"pc105".as_ptr(),
            layout: layout.as_ptr(),
            variant: variant.as_ptr(),
            options: options.as_ptr(),
        };
        let map = map_new(context, &names, 0);
        context_unref(context);
        if map.is_null() {
            return None;
        }
        let state = state_new(map);
        if state.is_null() {
            map_unref(map);
            return None;
        }
        let shift = 1u32
            .checked_shl(mod_index(map, c"Shift".as_ptr()))
            .unwrap_or(0);
        let altgr = 1u32
            .checked_shl(mod_index(map, c"Mod5".as_ptr()))
            .unwrap_or(0);
        let mut keys = BTreeMap::new();
        for &(hid, name) in super::x11_layout::PRINTABLE_KEYS.iter().chain(EXTRA_KEYS) {
            let name = [name[0], name[1], name[2], name[3], 0];
            let code = key_by_name(map, name.as_ptr().cast());
            if code == u32::MAX {
                continue;
            }
            let levels = [0, shift, altgr, shift | altgr].map(|mods| {
                update(state, mods, 0, 0, 0, 0, 0);
                keysym_glyph(symbol(state, code))
            });
            keys.insert(hid, levels);
        }
        state_unref(state);
        map_unref(map);
        Some(Arc::new(SystemKeymap {
            id: id.into(),
            name: name.into(),
            keys,
        }))
    }
}

pub(super) const EXTRA_KEYS: &[(u16, [u8; 4])] = &[
    (50, *b"BKSL"),
    (100, *b"LSGT"),
    (44, *b"SPCE"),
    (135, *b"AB11"),
    (137, *b"AE13"),
];

pub(super) fn installed() -> Vec<SystemLayoutDescriptor> {
    let source = [
        "/usr/share/X11/xkb/rules/evdev.lst",
        "/usr/share/X11/xkb/rules/base.lst",
    ]
    .into_iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
    .unwrap_or_default();
    parse_list(&source)
}

fn parse_list(source: &str) -> Vec<SystemLayoutDescriptor> {
    let mut section = "";
    let mut layouts = Vec::new();
    for line in source.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(header) = line.strip_prefix('!') {
            section = header.trim();
            continue;
        }
        let Some((code, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if section == "layout" {
            layouts.push(SystemLayoutDescriptor {
                id: format!("xkb:{code}:"),
                name: rest.trim().into(),
            });
        } else if section == "variant" {
            let Some((layout, description)) = rest.trim().split_once(':') else {
                continue;
            };
            layouts.push(SystemLayoutDescriptor {
                id: format!("xkb:{}:{code}", layout.trim()),
                name: description.trim().into(),
            });
        }
    }
    layouts.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    layouts.dedup_by(|a, b| a.id == b.id);
    layouts
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_variants_keep_their_layout_identity() {
        let list = parse_list("! layout\n de German\n fr French\n! variant\n nodeadkeys de: German (no dead keys)\n oss fr: French (alternative)\n! option\n grp:alt_shift_toggle Toggle\n");
        assert_eq!(list.len(), 4);
        assert!(list.iter().any(|e| e.id == "xkb:fr:oss"));
    }
    #[test]
    fn real_installed_layouts_supply_non_catalog_glyphs_and_modifier_levels() {
        let german = load("xkb:de:", "German", "").unwrap();
        assert_eq!(german.keys[&28][0].text, "z");
        assert_eq!(german.keys[&20][2].text, "@");
        assert!(german.keys[&46][0].dead);
        let french = load("xkb:fr:", "French", "").unwrap();
        assert_eq!(french.keys[&4][0].text, "q");
        assert_eq!(french.keys[&30][0].text, "&");
        let dvorak = load("xkb:us:dvorak", "Dvorak", "").unwrap();
        assert_eq!(dvorak.keys[&20][0].text, "'");
        let typewriter = load("xkb:ru:typewriter", "Russian Typewriter", "").unwrap();
        assert_eq!(typewriter.keys[&30][0].text, "№");
    }
}
