//! Compile installed XKB layouts without selecting a layout or changing input.
use crate::key_legend_detection::{KeyGlyph, SystemKeymap, SystemLayoutDescriptor};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
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

    fn include_paths(&self) -> Option<Vec<PathBuf>> {
        unsafe {
            let context_new =
                self.function::<unsafe extern "C" fn(u32) -> *mut c_void>(b"xkb_context_new\0")?;
            let context_unref =
                self.function::<unsafe extern "C" fn(*mut c_void)>(b"xkb_context_unref\0")?;
            let count = self.function::<unsafe extern "C" fn(*mut c_void) -> u32>(
                b"xkb_context_num_include_paths\0",
            )?;
            let path = self.function::<unsafe extern "C" fn(*mut c_void, u32) -> *const c_char>(
                b"xkb_context_include_path_get\0",
            )?;
            let context = context_new(0);
            if context.is_null() {
                return None;
            }
            let paths = (0..count(context))
                .filter_map(|i| {
                    let path = path(context, i);
                    (!path.is_null()).then(|| {
                        PathBuf::from(std::ffi::OsStr::from_bytes(CStr::from_ptr(path).to_bytes()))
                    })
                })
                .collect();
            context_unref(context);
            Some(paths)
        }
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

pub(super) fn load(id: &str, name: &str, model: &str, options: &str) -> Option<Arc<SystemKeymap>> {
    // The shared detector samples frequently; compile each unchanged source once.
    type Cached = (
        String,
        String,
        String,
        String,
        std::time::Instant,
        Arc<SystemKeymap>,
    );
    static CACHE: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    let mut cached = CACHE.lock().ok()?;
    if let Some((old_id, old_name, old_model, old_options, at, map)) = cached.as_ref() {
        if (
            old_id.as_str(),
            old_name.as_str(),
            old_model.as_str(),
            old_options.as_str(),
        ) == (id, name, model, options)
            && at.elapsed() < std::time::Duration::from_secs(10)
        {
            return Some(map.clone());
        }
    }
    let map = compile(id, name, model, options)?;
    *cached = Some((
        id.into(),
        name.into(),
        model.into(),
        options.into(),
        std::time::Instant::now(),
        map.clone(),
    ));
    Some(map)
}

fn compile(id: &str, name: &str, model: &str, options: &str) -> Option<Arc<SystemKeymap>> {
    let spec = id.strip_prefix("xkb:")?;
    let (layout, variant) = spec.split_once(':').unwrap_or((spec, ""));
    if layout.is_empty() {
        return None;
    }
    let api = Xkb::get()?;
    let layout = CString::new(layout).ok()?;
    let variant = CString::new(variant).ok()?;
    let options = CString::new(options).ok()?;
    let model = CString::new(model).ok()?;
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
        let consumed = api.function::<unsafe extern "C" fn(*mut c_void, u32) -> u32>(
            b"xkb_state_key_get_consumed_mods\0",
        )?;
        let context = context_new(0);
        if context.is_null() {
            return None;
        }
        let names = Names {
            rules: c"evdev".as_ptr(),
            model: model.as_ptr(),
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
                // Preserve Right Alt in shortcut labels when this key's type
                // does not use it to select a glyph (for example plain US).
                if mods & altgr != 0 && consumed(state, code) & altgr == 0 {
                    return KeyGlyph::default();
                }
                keysym_glyph(symbol(state, code))
            });
            keys.insert(hid, levels);
        }
        state_unref(state);
        map_unref(map);
        Some(Arc::new(SystemKeymap {
            id: id.into(),
            name: name.into(),
            third_level_modifier: crate::key_legend_detection::ThirdLevelModifier::RightAlt,
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
    let Some(roots) = Xkb::get().and_then(Xkb::include_paths) else {
        return Vec::new();
    };
    // Follow the same roots and precedence as keymap compilation, including
    // distro-specific store paths and XKB_CONFIG_ROOT overrides.
    let mut layouts = BTreeMap::new();
    for root in roots {
        let source = ["evdev.lst", "base.lst"]
            .into_iter()
            .find_map(|file| std::fs::read_to_string(root.join("rules").join(file)).ok());
        if let Some(source) = source {
            for layout in parse_list(&source) {
                layouts.entry(layout.id.clone()).or_insert(layout);
            }
        }
    }
    let mut layouts: Vec<_> = layouts.into_values().collect();
    layouts.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    layouts
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
        let german = load("xkb:de:", "German", "pc105", "").unwrap();
        assert!(installed().iter().any(|layout| layout.id == "xkb:de:"));
        assert_eq!(german.keys[&28][0].text, "z");
        assert_eq!(german.keys[&20][2].text, "@");
        assert!(german.keys[&46][0].dead);
        let french = load("xkb:fr:", "French", "pc105", "").unwrap();
        assert_eq!(french.keys[&4][0].text, "q");
        assert_eq!(french.keys[&30][0].text, "&");
        let dvorak = load("xkb:us:dvorak", "Dvorak", "pc105", "").unwrap();
        assert_eq!(dvorak.keys[&20][0].text, "'");
        let typewriter = load("xkb:ru:typewriter", "Russian Typewriter", "pc105", "").unwrap();
        assert_eq!(typewriter.keys[&30][0].text, "№");
    }

    #[test]
    fn keyboard_model_changes_glyphs_and_invalidates_the_mapping_cache() {
        for (model, expected) in [
            ("pc105", ["²", "~"]),
            ("macintosh", ["@", "#"]),
            ("pc105", ["²", "~"]),
        ] {
            let map = load("xkb:fr:", "French", model, "").unwrap();
            assert_eq!(
                [
                    map.keys[&53][0].text.as_str(),
                    map.keys[&53][1].text.as_str()
                ],
                expected
            );
        }
    }

    #[test]
    fn xkb_right_alt_shortcuts_without_a_third_level_keep_their_modifiers() {
        let map = load("xkb:us:", "US", "pc105", "").unwrap();
        assert_eq!(map.legend(0x1404, false, true), None);
        assert_eq!(map.legend(0x1604, false, true), None);
    }

    #[test]
    fn xkb_control_alt_shortcuts_keep_their_modifiers() {
        let map = load("xkb:fr:", "French", "pc105", "").unwrap();
        assert_eq!(map.keys[&8][2].text, "€");
        assert_eq!(map.legend(0x0508, false, true), None);
        assert_eq!(map.legend(0x1408, false, true).as_deref(), Some("€"));
        let layout = crate::keycode::KeyLegendLayout::System {
            keymap: map,
            english_first: false,
        };
        let label = crate::keycode::keycode_label_with_names_and_layout(0x0508, &[], &[], layout);
        assert!(label.starts_with("Ctrl+Alt\n"), "{label}");
    }

    #[test]
    fn installed_layouts_follow_xkb_data_root() {
        const CHILD: &str = "ENTROPY_TEST_XKB_DATA_ROOT";
        if std::env::var_os(CHILD).is_some() {
            assert!(installed()
                .iter()
                .any(|layout| layout.name == "Entropy alternate XKB root"));
            return;
        }
        // A separate process isolates libxkbcommon's environment from parallel tests.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("rules")).unwrap();
        std::fs::write(
            root.path().join("rules/evdev.lst"),
            "! layout\n fr Entropy alternate XKB root\n",
        )
        .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "qmk_hid_host::linux_keymaps::tests::installed_layouts_follow_xkb_data_root",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("XKB_CONFIG_ROOT", root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
