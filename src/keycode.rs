//! QMK/Vial keycode definitions — protocol v6
//! Reference: vial-gui/src/main/python/keycodes/keycodes_v6.py

fn gui_name_for_target_os(target_os: &str) -> &'static str {
    match target_os {
        "macos" => "Cmd",
        "windows" => "Win",
        _ => "Super",
    }
}

/// Returns the platform-appropriate generic label for the GUI/Super/Win/Cmd key.
/// Side-specific info belongs in tooltips, not the keycap label.
pub fn gui_label(_right: bool) -> &'static str {
    gui_name_for_target_os(std::env::consts::OS)
}

/// Short GUI label for use in compound labels (e.g. MT, mod combos).
pub fn gui_sym() -> &'static str {
    gui_name_for_target_os(std::env::consts::OS)
}

pub fn gui_mod_name() -> &'static str {
    gui_name_for_target_os(std::env::consts::OS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum KeyLegendLayout {
    #[default]
    English,
    Russian,
    RussianPrimary,
}

impl KeyLegendLayout {
    pub const ALL: [KeyLegendLayout; 2] = [KeyLegendLayout::English, KeyLegendLayout::Russian];

    pub fn i18n_key(self) -> &'static str {
        match self {
            KeyLegendLayout::English => "ui.key_legends_english",
            KeyLegendLayout::Russian | KeyLegendLayout::RussianPrimary => "ui.key_legends_russian",
        }
    }

    pub fn is_multilingual(self) -> bool {
        !matches!(self, KeyLegendLayout::English)
    }

    pub fn order_i18n_key(self) -> Option<&'static str> {
        match self {
            KeyLegendLayout::English => None,
            KeyLegendLayout::Russian => Some("main_menu.key_legends_english_first"),
            KeyLegendLayout::RussianPrimary => Some("main_menu.key_legends_russian_first"),
        }
    }

    pub fn toggled_order(self) -> Self {
        match self {
            KeyLegendLayout::English => KeyLegendLayout::English,
            KeyLegendLayout::Russian => KeyLegendLayout::RussianPrimary,
            KeyLegendLayout::RussianPrimary => KeyLegendLayout::Russian,
        }
    }
}


fn osm_mod_bits(value: u16) -> Option<u16> {
    (0x52A0..=0x52BF).contains(&value).then_some(value & 0x1F)
}

fn osm_mod_short_name(bits: u16) -> String {
    let gui = gui_sym();
    match bits & 0x0F {
        0x01 => "Ctrl".to_string(),
        0x02 => "Shift".to_string(),
        0x04 => "Alt".to_string(),
        0x08 => gui.to_string(),
        0x03 => "CS".to_string(),
        0x05 => "CA".to_string(),
        0x09 => format!("C{gui}"),
        0x0B => format!("CS{gui}"),
        0x06 => "SA".to_string(),
        0x0A => format!("S{gui}"),
        0x0E => format!("SA{gui}"),
        0x0D => format!("CA{gui}"),
        0x0C => format!("A{gui}"),
        0x07 => "Meh".to_string(),
        0x0F => "Hyper".to_string(),
        _ => "Mod".to_string(),
    }
}

fn osm_mod_full_name(bits: u16) -> String {
    let right = bits & 0x10 != 0;
    let side = if right { "Right" } else { "Left" };
    let gui = gui_mod_name();
    match bits & 0x0F {
        0x01 => format!("{side} Ctrl"),
        0x02 => format!("{side} Shift"),
        0x04 => format!("{side} Alt"),
        0x08 => format!("{side} {gui}"),
        0x03 => format!("{side} Ctrl+Shift"),
        0x05 => format!("{side} Ctrl+Alt"),
        0x09 => format!("{side} Ctrl+{gui}"),
        0x0B => format!("{side} Ctrl+Shift+{gui}"),
        0x06 => format!("{side} Shift+Alt"),
        0x0A => format!("{side} Shift+{gui}"),
        0x0E => format!("{side} Shift+Alt+{gui}"),
        0x0D => format!("{side} Ctrl+Alt+{gui}"),
        0x0C => format!("{side} Alt+{gui}"),
        0x07 => format!("{side} Meh (Ctrl+Shift+Alt)"),
        0x0F => format!("{side} Hyper (Ctrl+Shift+Alt+{gui})"),
        _ => "modifier".to_string(),
    }
}

pub fn key_label_font_sizes(label: &str) -> (Option<f32>, f32) {
    if label.starts_with("Hold ") && label.contains('/') {
        let top = label.split_once('/').map(|(top, _)| top).unwrap_or(label);
        let top_size = if top.chars().count() > 13 { 7.4 } else { 8.5 };
        return (Some(top_size), 10.8);
    }

    let lines: Vec<&str> = label.split('\n').collect();
    let is_symbol_line = |line: &str| {
        let trimmed = line.trim();
        !trimmed.is_empty()
            && trimmed.chars().count() <= 3
            && trimmed.chars().all(|c| !c.is_alphanumeric() && !c.is_whitespace())
    };

    match lines.as_slice() {
        [single] => {
            let trimmed = single.trim();
            let size = if trimmed == "↵" {
                16.0
            } else if trimmed == "⟵" {
                12.5
            } else if is_symbol_line(single) {
                14.0
            } else {
                12.0
            };
            (None, size)
        }
        [top, bottom] => {
            let top_size = if is_symbol_line(top) { 10.5 } else { 9.0 };
            let bottom_size = if is_symbol_line(bottom) { 12.0 } else { 11.0 };
            (Some(top_size), bottom_size)
        }
        [top, _, bottom] => {
            let top_size = if top.chars().count() > 13 { 7.4 } else { 8.3 };
            let bottom_size = if is_symbol_line(bottom) { 11.0 } else { 10.4 };
            (Some(top_size), bottom_size)
        }
        _ => (Some(9.0), 11.0),
    }
}

fn gui_key_tooltip(right: bool) -> String {
    #[cfg(target_os = "macos")]
    {
        if right {
            "Right Cmd, macOS modifier key and app shortcuts".to_string()
        } else {
            "Left Cmd, macOS modifier key and app shortcuts".to_string()
        }
    }
    #[cfg(target_os = "windows")]
    {
        if right {
            "Right Win, Windows modifier key and OS shortcuts".to_string()
        } else {
            "Left Win, Windows modifier key and OS shortcuts".to_string()
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if right {
            "Right Super, desktop modifier key and OS shortcuts".to_string()
        } else {
            "Left Super, desktop modifier key and OS shortcuts".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keycode {
    pub value: u16,
    pub name: &'static str,
    pub label: &'static str,
    pub category: KeycodeCategory,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeycodeCategory {
    Basic,
    Modifier,
    Function,
    Navigation,
    Numpad,
    Media,
    Mouse,
    Layer,
    Special,
}

pub const fn is_extended_function_key(value: u16) -> bool {
    matches!(value, 0x0068..=0x0073)
}

pub const KEYCODES: &[Keycode] = &[
    // ── Special ──────────────────────────────────────────────────────────────
    Keycode { value: 0x0000, name: "KC_NO",     label: "✕",   category: KeycodeCategory::Special },
    Keycode { value: 0x0001, name: "KC_TRNS",   label: "▽",   category: KeycodeCategory::Special },
    Keycode { value: 0x7000, name: "QK_MAGIC_SWAP_CONTROL_CAPS_LOCK",      label: "Caps/Ctrl\nSwap",    category: KeycodeCategory::Special },
    Keycode { value: 0x7001, name: "QK_MAGIC_UNSWAP_CONTROL_CAPS_LOCK",    label: "Caps/Ctrl\nRestore", category: KeycodeCategory::Special },
    Keycode { value: 0x7002, name: "QK_MAGIC_TOGGLE_CONTROL_CAPS_LOCK",    label: "Caps/Ctrl\nToggle",  category: KeycodeCategory::Special },
    Keycode { value: 0x7003, name: "QK_MAGIC_CAPS_LOCK_AS_CONTROL_OFF",    label: "Caps Lock\nas Caps", category: KeycodeCategory::Special },
    Keycode { value: 0x7004, name: "QK_MAGIC_CAPS_LOCK_AS_CONTROL_ON",     label: "Caps Lock\nas Ctrl", category: KeycodeCategory::Special },
    Keycode { value: 0x7005, name: "QK_MAGIC_SWAP_LALT_LGUI",              label: "LAlt/LGui\nSwap",    category: KeycodeCategory::Special },
    Keycode { value: 0x7006, name: "QK_MAGIC_UNSWAP_LALT_LGUI",            label: "LAlt/LGui\nRestore", category: KeycodeCategory::Special },
    Keycode { value: 0x7007, name: "QK_MAGIC_SWAP_RALT_RGUI",              label: "RAlt/RGui\nSwap",    category: KeycodeCategory::Special },
    Keycode { value: 0x7008, name: "QK_MAGIC_UNSWAP_RALT_RGUI",            label: "RAlt/RGui\nRestore", category: KeycodeCategory::Special },
    Keycode { value: 0x7009, name: "QK_MAGIC_GUI_ON",                      label: "GUI Keys\nOn",       category: KeycodeCategory::Special },
    Keycode { value: 0x700A, name: "QK_MAGIC_GUI_OFF",                     label: "GUI Keys\nOff",      category: KeycodeCategory::Special },
    Keycode { value: 0x700B, name: "QK_MAGIC_TOGGLE_GUI",                  label: "GUI Keys\nToggle",   category: KeycodeCategory::Special },
    Keycode { value: 0x700C, name: "QK_MAGIC_SWAP_GRAVE_ESC",              label: "` / Esc\nSwap",      category: KeycodeCategory::Special },
    Keycode { value: 0x700D, name: "QK_MAGIC_UNSWAP_GRAVE_ESC",            label: "` / Esc\nRestore",   category: KeycodeCategory::Special },
    Keycode { value: 0x700E, name: "QK_MAGIC_SWAP_BACKSLASH_BACKSPACE",    label: "\\ / Bksp\nSwap",    category: KeycodeCategory::Special },
    Keycode { value: 0x700F, name: "QK_MAGIC_UNSWAP_BACKSLASH_BACKSPACE",  label: "\\ / Bksp\nRestore", category: KeycodeCategory::Special },
    Keycode { value: 0x7010, name: "QK_MAGIC_TOGGLE_BACKSLASH_BACKSPACE",  label: "\\ / Bksp\nToggle",  category: KeycodeCategory::Special },
    Keycode { value: 0x7011, name: "QK_MAGIC_NKRO_ON",                     label: "NKRO\nOn",           category: KeycodeCategory::Special },
    Keycode { value: 0x7012, name: "QK_MAGIC_NKRO_OFF",                    label: "NKRO\nOff",          category: KeycodeCategory::Special },
    Keycode { value: 0x7013, name: "QK_MAGIC_TOGGLE_NKRO",                 label: "NKRO\nToggle",       category: KeycodeCategory::Special },
    Keycode { value: 0x7014, name: "QK_MAGIC_SWAP_ALT_GUI",                label: "Alt/Gui\nSwap",      category: KeycodeCategory::Special },
    Keycode { value: 0x7015, name: "QK_MAGIC_UNSWAP_ALT_GUI",              label: "Alt/Gui\nRestore",   category: KeycodeCategory::Special },
    Keycode { value: 0x7016, name: "QK_MAGIC_TOGGLE_ALT_GUI",              label: "Alt/Gui\nToggle",    category: KeycodeCategory::Special },
    Keycode { value: 0x7017, name: "QK_MAGIC_SWAP_LCTL_LGUI",              label: "LCtrl/LGui\nSwap",   category: KeycodeCategory::Special },
    Keycode { value: 0x7018, name: "QK_MAGIC_UNSWAP_LCTL_LGUI",            label: "LCtrl/LGui\nRestore",category: KeycodeCategory::Special },
    Keycode { value: 0x7019, name: "QK_MAGIC_SWAP_RCTL_RGUI",              label: "RCtrl/RGui\nSwap",   category: KeycodeCategory::Special },
    Keycode { value: 0x701A, name: "QK_MAGIC_UNSWAP_RCTL_RGUI",            label: "RCtrl/RGui\nRestore",category: KeycodeCategory::Special },
    Keycode { value: 0x701B, name: "QK_MAGIC_SWAP_CTL_GUI",                label: "Ctrl/Gui\nSwap",     category: KeycodeCategory::Special },
    Keycode { value: 0x701C, name: "QK_MAGIC_UNSWAP_CTL_GUI",              label: "Ctrl/Gui\nRestore",  category: KeycodeCategory::Special },
    Keycode { value: 0x701D, name: "QK_MAGIC_TOGGLE_CTL_GUI",              label: "Ctrl/Gui\nToggle",   category: KeycodeCategory::Special },
    Keycode { value: 0x701E, name: "QK_MAGIC_EE_HANDS_LEFT",               label: "EE Hands\nLeft",     category: KeycodeCategory::Special },
    Keycode { value: 0x701F, name: "QK_MAGIC_EE_HANDS_RIGHT",              label: "EE Hands\nRight",    category: KeycodeCategory::Special },
    Keycode { value: 0x7020, name: "QK_MAGIC_SWAP_ESCAPE_CAPS_LOCK",       label: "Caps/Esc\nSwap",     category: KeycodeCategory::Special },
    Keycode { value: 0x7021, name: "QK_MAGIC_UNSWAP_ESCAPE_CAPS_LOCK",     label: "Caps/Esc\nRestore",  category: KeycodeCategory::Special },
    Keycode { value: 0x7022, name: "QK_MAGIC_TOGGLE_ESCAPE_CAPS_LOCK",     label: "Caps/Esc\nToggle",   category: KeycodeCategory::Special },

    // ── Letters ──────────────────────────────────────────────────────────────
    Keycode { value: 0x0004, name: "KC_A", label: "A", category: KeycodeCategory::Basic },
    Keycode { value: 0x0005, name: "KC_B", label: "B", category: KeycodeCategory::Basic },
    Keycode { value: 0x0006, name: "KC_C", label: "C", category: KeycodeCategory::Basic },
    Keycode { value: 0x0007, name: "KC_D", label: "D", category: KeycodeCategory::Basic },
    Keycode { value: 0x0008, name: "KC_E", label: "E", category: KeycodeCategory::Basic },
    Keycode { value: 0x0009, name: "KC_F", label: "F", category: KeycodeCategory::Basic },
    Keycode { value: 0x000A, name: "KC_G", label: "G", category: KeycodeCategory::Basic },
    Keycode { value: 0x000B, name: "KC_H", label: "H", category: KeycodeCategory::Basic },
    Keycode { value: 0x000C, name: "KC_I", label: "I", category: KeycodeCategory::Basic },
    Keycode { value: 0x000D, name: "KC_J", label: "J", category: KeycodeCategory::Basic },
    Keycode { value: 0x000E, name: "KC_K", label: "K", category: KeycodeCategory::Basic },
    Keycode { value: 0x000F, name: "KC_L", label: "L", category: KeycodeCategory::Basic },
    Keycode { value: 0x0010, name: "KC_M", label: "M", category: KeycodeCategory::Basic },
    Keycode { value: 0x0011, name: "KC_N", label: "N", category: KeycodeCategory::Basic },
    Keycode { value: 0x0012, name: "KC_O", label: "O", category: KeycodeCategory::Basic },
    Keycode { value: 0x0013, name: "KC_P", label: "P", category: KeycodeCategory::Basic },
    Keycode { value: 0x0014, name: "KC_Q", label: "Q", category: KeycodeCategory::Basic },
    Keycode { value: 0x0015, name: "KC_R", label: "R", category: KeycodeCategory::Basic },
    Keycode { value: 0x0016, name: "KC_S", label: "S", category: KeycodeCategory::Basic },
    Keycode { value: 0x0017, name: "KC_T", label: "T", category: KeycodeCategory::Basic },
    Keycode { value: 0x0018, name: "KC_U", label: "U", category: KeycodeCategory::Basic },
    Keycode { value: 0x0019, name: "KC_V", label: "V", category: KeycodeCategory::Basic },
    Keycode { value: 0x001A, name: "KC_W", label: "W", category: KeycodeCategory::Basic },
    Keycode { value: 0x001B, name: "KC_X", label: "X", category: KeycodeCategory::Basic },
    Keycode { value: 0x001C, name: "KC_Y", label: "Y", category: KeycodeCategory::Basic },
    Keycode { value: 0x001D, name: "KC_Z", label: "Z", category: KeycodeCategory::Basic },

    // ── Numbers ───────────────────────────────────────────────────────────────
    Keycode { value: 0x001E, name: "KC_1", label: "1", category: KeycodeCategory::Basic },
    Keycode { value: 0x001F, name: "KC_2", label: "2", category: KeycodeCategory::Basic },
    Keycode { value: 0x0020, name: "KC_3", label: "3", category: KeycodeCategory::Basic },
    Keycode { value: 0x0021, name: "KC_4", label: "4", category: KeycodeCategory::Basic },
    Keycode { value: 0x0022, name: "KC_5", label: "5", category: KeycodeCategory::Basic },
    Keycode { value: 0x0023, name: "KC_6", label: "6", category: KeycodeCategory::Basic },
    Keycode { value: 0x0024, name: "KC_7", label: "7", category: KeycodeCategory::Basic },
    Keycode { value: 0x0025, name: "KC_8", label: "8", category: KeycodeCategory::Basic },
    Keycode { value: 0x0026, name: "KC_9", label: "9", category: KeycodeCategory::Basic },
    Keycode { value: 0x0027, name: "KC_0", label: "0", category: KeycodeCategory::Basic },

    // ── Control ───────────────────────────────────────────────────────────────
    Keycode { value: 0x0028, name: "KC_ENTER",  label: "↵",    category: KeycodeCategory::Basic },
    Keycode { value: 0x0029, name: "KC_ESCAPE", label: "Esc",  category: KeycodeCategory::Basic },
    Keycode { value: 0x002A, name: "KC_BSPACE", label: "⟵",    category: KeycodeCategory::Basic },
    Keycode { value: 0x002B, name: "KC_TAB",    label: "Tab",  category: KeycodeCategory::Basic },
    Keycode { value: 0x002C, name: "KC_SPACE",  label: "Space",category: KeycodeCategory::Basic },
    Keycode { value: 0x0039, name: "KC_CAPSLOCK",label: "Caps\nLock", category: KeycodeCategory::Basic },
    Keycode { value: 0x0065, name: "KC_APPLICATION", label: "Menu", category: KeycodeCategory::Basic },
    Keycode { value: 0x0074, name: "KC_EXEC",        label: "Exec",   category: KeycodeCategory::Basic },
    Keycode { value: 0x0075, name: "KC_HELP",        label: "Help",   category: KeycodeCategory::Basic },
    Keycode { value: 0x0077, name: "KC_SELECT",      label: "Select", category: KeycodeCategory::Basic },
    Keycode { value: 0x0078, name: "KC_STOP",        label: "Stop",   category: KeycodeCategory::Basic },
    Keycode { value: 0x0079, name: "KC_AGAIN",       label: "Again",  category: KeycodeCategory::Basic },
    Keycode { value: 0x007A, name: "KC_UNDO",        label: "Undo",   category: KeycodeCategory::Basic },
    Keycode { value: 0x007B, name: "KC_CUT",         label: "Cut",    category: KeycodeCategory::Basic },
    Keycode { value: 0x007C, name: "KC_COPY",        label: "Copy",   category: KeycodeCategory::Basic },
    Keycode { value: 0x007D, name: "KC_PSTE",        label: "Paste",  category: KeycodeCategory::Basic },
    Keycode { value: 0x007E, name: "KC_FIND",        label: "Find",   category: KeycodeCategory::Basic },

    // ── Punctuation ───────────────────────────────────────────────────────────
    Keycode { value: 0x002D, name: "KC_MINUS",   label: "_
-",  category: KeycodeCategory::Basic },
    Keycode { value: 0x002E, name: "KC_EQUAL",   label: "+
=",  category: KeycodeCategory::Basic },
    Keycode { value: 0x002F, name: "KC_LBRACKET",label: "{
[",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0030, name: "KC_RBRACKET",label: "}
]",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0031, name: "KC_BSLASH",  label: "|
\\", category: KeycodeCategory::Basic },
    Keycode { value: 0x0032, name: "KC_NONUS_HASH", label: "~
#", category: KeycodeCategory::Basic },
    Keycode { value: 0x0033, name: "KC_SCOLON",  label: ":
;",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0034, name: "KC_QUOTE",   label: "\"
'", category: KeycodeCategory::Basic },
    Keycode { value: 0x0035, name: "KC_GRAVE",   label: "~
`",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0036, name: "KC_COMMA",   label: "<
,",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0037, name: "KC_DOT",     label: ">
.",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0038, name: "KC_SLASH",   label: "?
/",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0064, name: "KC_NONUS_BSLASH", label: "|
\\", category: KeycodeCategory::Basic },

    // ── Shifted symbols (LSFT(kc) shortcuts) ─────────────────────────────────
    // These are 0x0200 | basic_kc
    Keycode { value: 0x021E, name: "KC_EXLM",  label: "!",  category: KeycodeCategory::Basic },
    Keycode { value: 0x021F, name: "KC_AT",    label: "@",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0220, name: "KC_HASH",  label: "#",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0221, name: "KC_DLR",   label: "$",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0222, name: "KC_PERC",  label: "%",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0223, name: "KC_CIRC",  label: "^",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0224, name: "KC_AMPR",  label: "&",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0225, name: "KC_ASTR",  label: "*",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0226, name: "KC_LPRN",  label: "(",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0227, name: "KC_RPRN",  label: ")",  category: KeycodeCategory::Basic },
    Keycode { value: 0x022D, name: "KC_UNDS",  label: "_",  category: KeycodeCategory::Basic },
    Keycode { value: 0x022E, name: "KC_PLUS",  label: "+",  category: KeycodeCategory::Basic },
    Keycode { value: 0x022F, name: "KC_LCBR",  label: "{",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0230, name: "KC_RCBR",  label: "}",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0231, name: "KC_PIPE",  label: "|",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0233, name: "KC_COLN",  label: ":",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0234, name: "KC_DQUO",  label: "\"", category: KeycodeCategory::Basic },
    Keycode { value: 0x0235, name: "KC_TILD",  label: "~",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0236, name: "KC_LT",    label: "<",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0237, name: "KC_GT",    label: ">",  category: KeycodeCategory::Basic },
    Keycode { value: 0x0238, name: "KC_QUES",  label: "?",  category: KeycodeCategory::Basic },

    // ── Function keys ─────────────────────────────────────────────────────────
    Keycode { value: 0x003A, name: "KC_F1",  label: "F1",  category: KeycodeCategory::Function },
    Keycode { value: 0x003B, name: "KC_F2",  label: "F2",  category: KeycodeCategory::Function },
    Keycode { value: 0x003C, name: "KC_F3",  label: "F3",  category: KeycodeCategory::Function },
    Keycode { value: 0x003D, name: "KC_F4",  label: "F4",  category: KeycodeCategory::Function },
    Keycode { value: 0x003E, name: "KC_F5",  label: "F5",  category: KeycodeCategory::Function },
    Keycode { value: 0x003F, name: "KC_F6",  label: "F6",  category: KeycodeCategory::Function },
    Keycode { value: 0x0040, name: "KC_F7",  label: "F7",  category: KeycodeCategory::Function },
    Keycode { value: 0x0041, name: "KC_F8",  label: "F8",  category: KeycodeCategory::Function },
    Keycode { value: 0x0042, name: "KC_F9",  label: "F9",  category: KeycodeCategory::Function },
    Keycode { value: 0x0043, name: "KC_F10", label: "F10", category: KeycodeCategory::Function },
    Keycode { value: 0x0044, name: "KC_F11", label: "F11", category: KeycodeCategory::Function },
    Keycode { value: 0x0045, name: "KC_F12", label: "F12", category: KeycodeCategory::Function },
    Keycode { value: 0x0068, name: "KC_F13", label: "F13", category: KeycodeCategory::Function },
    Keycode { value: 0x0069, name: "KC_F14", label: "F14", category: KeycodeCategory::Function },
    Keycode { value: 0x006A, name: "KC_F15", label: "F15", category: KeycodeCategory::Function },
    Keycode { value: 0x006B, name: "KC_F16", label: "F16", category: KeycodeCategory::Function },
    Keycode { value: 0x006C, name: "KC_F17", label: "F17", category: KeycodeCategory::Function },
    Keycode { value: 0x006D, name: "KC_F18", label: "F18", category: KeycodeCategory::Function },
    Keycode { value: 0x006E, name: "KC_F19", label: "F19", category: KeycodeCategory::Function },
    Keycode { value: 0x006F, name: "KC_F20", label: "F20", category: KeycodeCategory::Function },
    Keycode { value: 0x0070, name: "KC_F21", label: "F21", category: KeycodeCategory::Function },
    Keycode { value: 0x0071, name: "KC_F22", label: "F22", category: KeycodeCategory::Function },
    Keycode { value: 0x0072, name: "KC_F23", label: "F23", category: KeycodeCategory::Function },
    Keycode { value: 0x0073, name: "KC_F24", label: "F24", category: KeycodeCategory::Function },

    // ── Navigation ────────────────────────────────────────────────────────────
    Keycode { value: 0x0046, name: "KC_PSCREEN",   label: "Print\nScreen", category: KeycodeCategory::Navigation },
    Keycode { value: 0x0047, name: "KC_SCROLLLOCK", label: "Scroll\nLock",  category: KeycodeCategory::Navigation },
    Keycode { value: 0x0048, name: "KC_PAUSE",     label: "Pause",        category: KeycodeCategory::Navigation },
    Keycode { value: 0x0049, name: "KC_INSERT",    label: "Insert",       category: KeycodeCategory::Navigation },
    Keycode { value: 0x004A, name: "KC_HOME",      label: "Home",         category: KeycodeCategory::Navigation },
    Keycode { value: 0x004B, name: "KC_PGUP",      label: "Page\nUp",      category: KeycodeCategory::Navigation },
    Keycode { value: 0x004C, name: "KC_DELETE",    label: "Delete",       category: KeycodeCategory::Navigation },
    Keycode { value: 0x004D, name: "KC_END",       label: "End",          category: KeycodeCategory::Navigation },
    Keycode { value: 0x004E, name: "KC_PGDOWN",    label: "Page\nDown",    category: KeycodeCategory::Navigation },
    Keycode { value: 0x004F, name: "KC_RIGHT",     label: "➡",   category: KeycodeCategory::Navigation },
    Keycode { value: 0x0050, name: "KC_LEFT",      label: "⬅",   category: KeycodeCategory::Navigation },
    Keycode { value: 0x0051, name: "KC_DOWN",      label: "⬇",   category: KeycodeCategory::Navigation },
    Keycode { value: 0x0052, name: "KC_UP",        label: "⬆",   category: KeycodeCategory::Navigation },

    // ── Numpad ────────────────────────────────────────────────────────────────
    Keycode { value: 0x0053, name: "KC_NUMLOCK",     label: "NmLk",  category: KeycodeCategory::Numpad },
    Keycode { value: 0x0054, name: "KC_KP_SLASH",    label: "Num/",    category: KeycodeCategory::Numpad },
    Keycode { value: 0x0055, name: "KC_KP_ASTERISK", label: "Num*",    category: KeycodeCategory::Numpad },
    Keycode { value: 0x0056, name: "KC_KP_MINUS",    label: "Num-",    category: KeycodeCategory::Numpad },
    Keycode { value: 0x0057, name: "KC_KP_PLUS",     label: "Num+",    category: KeycodeCategory::Numpad },
    Keycode { value: 0x0058, name: "KC_KP_ENTER",    label: "Num⏎", category: KeycodeCategory::Numpad },
    Keycode { value: 0x0059, name: "KC_KP_1",  label: "Num1", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005A, name: "KC_KP_2",  label: "Num2", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005B, name: "KC_KP_3",  label: "Num3", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005C, name: "KC_KP_4",  label: "Num4", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005D, name: "KC_KP_5",  label: "Num5", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005E, name: "KC_KP_6",  label: "Num6", category: KeycodeCategory::Numpad },
    Keycode { value: 0x005F, name: "KC_KP_7",  label: "Num7", category: KeycodeCategory::Numpad },
    Keycode { value: 0x0060, name: "KC_KP_8",  label: "Num8", category: KeycodeCategory::Numpad },
    Keycode { value: 0x0061, name: "KC_KP_9",  label: "Num9", category: KeycodeCategory::Numpad },
    Keycode { value: 0x0062, name: "KC_KP_0",  label: "Num0", category: KeycodeCategory::Numpad },
    Keycode { value: 0x0063, name: "KC_KP_DOT",   label: "Num.",  category: KeycodeCategory::Numpad },
    Keycode { value: 0x0067, name: "KC_KP_EQUAL", label: "Num=",  category: KeycodeCategory::Numpad },
    Keycode { value: 0x0085, name: "KC_KP_COMMA", label: "Num,",  category: KeycodeCategory::Numpad },

    // ── Modifiers ─────────────────────────────────────────────────────────────
    Keycode { value: 0x00E0, name: "KC_LCTRL",  label: "Ctrl",  category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E1, name: "KC_LSHIFT", label: "Shift", category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E2, name: "KC_LALT",   label: "Alt",   category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E3, name: "KC_LGUI",   label: "GUI",   category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E4, name: "KC_RCTRL",  label: "Ctrl",  category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E5, name: "KC_RSHIFT", label: "Shift", category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E6, name: "KC_RALT",   label: "Alt",   category: KeycodeCategory::Modifier },
    Keycode { value: 0x00E7, name: "KC_RGUI",   label: "GUI",   category: KeycodeCategory::Modifier },

    // ── Media / App / System ──────────────────────────────────────────────────
    Keycode { value: 0x00A5, name: "KC_PWR",   label: "⏻\nPower",   category: KeycodeCategory::Media },
    Keycode { value: 0x00A6, name: "KC_SLEP",  label: "🌙\nSleep",   category: KeycodeCategory::Media },
    Keycode { value: 0x00A7, name: "KC_WAKE",  label: "☀\nWake",    category: KeycodeCategory::Media },
    Keycode { value: 0x00A8, name: "KC_MUTE",  label: "🔇\nMute",    category: KeycodeCategory::Media },
    Keycode { value: 0x00A9, name: "KC_VOLU",  label: "🔊\nVol+",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AA, name: "KC_VOLD",  label: "🔉\nVol-",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AB, name: "KC_MNXT",  label: "⏭\nNext",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AC, name: "KC_MPRV",  label: "⏮\nPrev",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AD, name: "KC_MSTP",  label: "⏹\nStop",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AE, name: "KC_MPLY",  label: "⏯\nPlay",    category: KeycodeCategory::Media },
    Keycode { value: 0x00AF, name: "KC_MSEL",  label: "🎵\nMedia",   category: KeycodeCategory::Media },
    Keycode { value: 0x00B0, name: "KC_EJCT",  label: "⏏\nEject",   category: KeycodeCategory::Media },
    Keycode { value: 0x00B1, name: "KC_MAIL",  label: "✉\nMail",    category: KeycodeCategory::Media },
    Keycode { value: 0x00B2, name: "KC_CALC",  label: "🖩\nCalc",    category: KeycodeCategory::Media },
    Keycode { value: 0x00B3, name: "KC_MYCM",  label: "💻\nFiles",   category: KeycodeCategory::Media },
    Keycode { value: 0x00B4, name: "KC_WSCH",  label: "🔍\nSearch",  category: KeycodeCategory::Media },
    Keycode { value: 0x00B5, name: "KC_WHOM",  label: "🏠\nHome",    category: KeycodeCategory::Media },
    Keycode { value: 0x00B6, name: "KC_WBAK",  label: "⬅\nBack",    category: KeycodeCategory::Media },
    Keycode { value: 0x00B7, name: "KC_WFWD",  label: "➡\nForward", category: KeycodeCategory::Media },
    Keycode { value: 0x00B8, name: "KC_WSTP",  label: "⏹\nStop",    category: KeycodeCategory::Media },
    Keycode { value: 0x00B9, name: "KC_WREF",  label: "↻\nRefresh", category: KeycodeCategory::Media },
    Keycode { value: 0x00BA, name: "KC_WFAV",  label: "★\nFavs",    category: KeycodeCategory::Media },
    Keycode { value: 0x00BB, name: "KC_MFFD",  label: "⏩\nFwd",     category: KeycodeCategory::Media },
    Keycode { value: 0x00BC, name: "KC_MRWD",  label: "⏪\nRew",     category: KeycodeCategory::Media },
    Keycode { value: 0x00BD, name: "KC_BRIU",  label: "☀+\nBright", category: KeycodeCategory::Media },
    Keycode { value: 0x00BE, name: "KC_BRID",  label: "☀-\nBright", category: KeycodeCategory::Media },
    Keycode { value: 0x00BF, name: "KC_MCTL",  label: "Ctrl\nView", category: KeycodeCategory::Media },
    Keycode { value: 0x00C0, name: "KC_LPAD",  label: "Launch\nPad", category: KeycodeCategory::Media },

    // ── Mouse ─────────────────────────────────────────────────────────────────
    Keycode { value: 0x00CD, name: "KC_MS_U",     label: "Mouse\nUp",    category: KeycodeCategory::Mouse },
    Keycode { value: 0x00CE, name: "KC_MS_D",     label: "Mouse\nDown",  category: KeycodeCategory::Mouse },
    Keycode { value: 0x00CF, name: "KC_MS_L",     label: "Mouse\nLeft",  category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D0, name: "KC_MS_R",     label: "Mouse\nRight", category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D1, name: "KC_BTN1",     label: "Mouse\n1",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D2, name: "KC_BTN2",     label: "Mouse\n2",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D3, name: "KC_BTN3",     label: "Mouse\n3",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D4, name: "KC_BTN4",     label: "Mouse\n4",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D5, name: "KC_BTN5",     label: "Mouse\n5",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D6, name: "KC_BTN6",     label: "Mouse\n6",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D7, name: "KC_BTN7",     label: "Mouse\n7",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D8, name: "KC_BTN8",     label: "Mouse\n8",     category: KeycodeCategory::Mouse },
    Keycode { value: 0x00D9, name: "KC_WH_U",     label: "Scroll\nUp",   category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DA, name: "KC_WH_D",     label: "Scroll\nDown", category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DB, name: "KC_WH_L",     label: "Scroll\nLeft", category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DC, name: "KC_WH_R",     label: "Scroll\nRight",category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DD, name: "KC_MS_ACCEL0", label: "Accel\n0",    category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DE, name: "KC_MS_ACCEL1", label: "Accel\n1",    category: KeycodeCategory::Mouse },
    Keycode { value: 0x00DF, name: "KC_MS_ACCEL2", label: "Accel\n2",    category: KeycodeCategory::Mouse },


    // ── QMK special ───────────────────────────────────────────────────────────
    Keycode { value: 0x7C16, name: "KC_GESC",  label: "Esc\n~", category: KeycodeCategory::Special },
    Keycode { value: 0x7C00, name: "QK_BOOT",  label: "⚡\nBoot", category: KeycodeCategory::Special },
    Keycode { value: 0x7C02, name: "DB_TOGG",  label: "🐛\nDebug", category: KeycodeCategory::Special },
    Keycode { value: 0x7800, name: "QK_LOCK",  label: "🔒\nLock",category: KeycodeCategory::Special },
    Keycode { value: 0x7C1A, name: "KC_LSPO",  label: "Shift\n(",   category: KeycodeCategory::Special },
    Keycode { value: 0x7C1B, name: "KC_RSPC",  label: "Shift\n)",   category: KeycodeCategory::Special },
    Keycode { value: 0x7C18, name: "KC_LCPO",  label: "Ctrl\n(",    category: KeycodeCategory::Special },
    Keycode { value: 0x7C19, name: "KC_RCPC",  label: "Ctrl\n)",    category: KeycodeCategory::Special },
    Keycode { value: 0x7C1C, name: "KC_LAPO",  label: "Alt\n(",      category: KeycodeCategory::Special },
    Keycode { value: 0x7C1D, name: "KC_RAPC",  label: "Alt\n)",      category: KeycodeCategory::Special },
    Keycode { value: 0x7C1E, name: "KC_SFTENT",label: "Shift\nEnter", category: KeycodeCategory::Special },
    Keycode { value: 0x0087, name: "KC_RO",    label: "JIS\n\\ _",   category: KeycodeCategory::Special },
    Keycode { value: 0x0088, name: "KC_KANA",  label: "JIS\nKana",   category: KeycodeCategory::Special },
    Keycode { value: 0x0089, name: "KC_JYEN",  label: "JIS\n¥ |",    category: KeycodeCategory::Special },
    Keycode { value: 0x008A, name: "KC_HENK",  label: "JIS\nHenkan", category: KeycodeCategory::Special },
    Keycode { value: 0x008B, name: "KC_MHEN",  label: "JIS\nMuhenk", category: KeycodeCategory::Special },
    Keycode { value: 0x008C, name: "KC_INT6",  label: "JIS\nNum ,",  category: KeycodeCategory::Special },
    Keycode { value: 0x0090, name: "KC_LANG1", label: "Hangul\nEng", category: KeycodeCategory::Special },
    Keycode { value: 0x0091, name: "KC_LANG2", label: "Hangul\nHanja", category: KeycodeCategory::Special },
    Keycode { value: 0x0092, name: "KC_LANG3", label: "JIS\nKatak",  category: KeycodeCategory::Special },
    Keycode { value: 0x0093, name: "KC_LANG4", label: "JIS\nHirag",  category: KeycodeCategory::Special },
    Keycode { value: 0x0094, name: "KC_LANG5", label: "JIS\nZenHan", category: KeycodeCategory::Special },
    Keycode { value: 0x7C14, name: "QK_MAKE",             label: "Make",         category: KeycodeCategory::Special },
    Keycode { value: 0x7C15, name: "KC_ASTG",             label: "Auto\nShift",  category: KeycodeCategory::Special },
    Keycode { value: 0x7C52, name: "CMB_TOG",             label: "Combo\nToggle",category: KeycodeCategory::Special },
    Keycode { value: 0x7C73, name: "QK_CAPS_WORD_TOGGLE", label: "Caps\nWord",   category: KeycodeCategory::Special },
    Keycode { value: 0x7C79, name: "QK_REPEAT_KEY",       label: "Repeat",       category: KeycodeCategory::Special },
    Keycode { value: 0x7C7A, name: "QK_ALT_REPEAT_KEY",   label: "Alt\nRepeat",  category: KeycodeCategory::Special },
    // RGB Light
    Keycode { value: 0x7820, name: "RGB_TOG",  label: "RGB\nTog",  category: KeycodeCategory::Special },
    Keycode { value: 0x7821, name: "RGB_MOD",  label: "RGB\nMod",  category: KeycodeCategory::Special },
    Keycode { value: 0x7822, name: "RGB_RMOD", label: "RGB\nRMod", category: KeycodeCategory::Special },
    Keycode { value: 0x7825, name: "RGB_HUI",  label: "RGB\nH+",   category: KeycodeCategory::Special },
    Keycode { value: 0x7826, name: "RGB_HUD",  label: "RGB\nH-",   category: KeycodeCategory::Special },
    Keycode { value: 0x7827, name: "RGB_SAI",  label: "RGB\nS+",   category: KeycodeCategory::Special },
    Keycode { value: 0x7828, name: "RGB_SAD",  label: "RGB\nS-",   category: KeycodeCategory::Special },
    Keycode { value: 0x7829, name: "RGB_VAI",  label: "RGB\nV+",   category: KeycodeCategory::Special },
    Keycode { value: 0x782A, name: "RGB_VAD",  label: "RGB\nV-",   category: KeycodeCategory::Special },
    Keycode { value: 0x7823, name: "RGB_SPI",  label: "RGB\nSpd+", category: KeycodeCategory::Special },
    Keycode { value: 0x7824, name: "RGB_SPD",  label: "RGB\nSpd-", category: KeycodeCategory::Special },
];

/// Normalizes symbol-looking physical keys to the printable shifted keycodes
/// users expect when assigning an output action.
pub fn normalize_output_symbol_keycode(value: u16) -> u16 {
    match value {
        // KC_NONUS_HASH is a physical ISO key position. On common Windows ANSI
        // layouts it can produce backslash, while the UI label looks like "#".
        0x0032 => 0x0220,
        _ => value,
    }
}

pub fn find_keycode(value: u16) -> Option<&'static Keycode> {
    KEYCODES.iter().find(|k| k.value == value)
}

fn magic_keycode_label(value: u16) -> Option<String> {
    let gui = gui_mod_name();
    match value {
        0x7005 => Some(format!("LAlt/{}\nSwap", gui)),
        0x7006 => Some(format!("LAlt/{}\nRestore", gui)),
        0x7007 => Some(format!("RAlt/{}\nSwap", gui)),
        0x7008 => Some(format!("RAlt/{}\nRestore", gui)),
        0x7009 => Some(format!("{} Keys\nOn", gui)),
        0x700A => Some(format!("{} Keys\nOff", gui)),
        0x700B => Some(format!("{} Keys\nToggle", gui)),
        0x7014 => Some(format!("Alt/{}\nSwap", gui)),
        0x7015 => Some(format!("Alt/{}\nRestore", gui)),
        0x7016 => Some(format!("Alt/{}\nToggle", gui)),
        0x7017 => Some(format!("LCtrl/{}\nSwap", gui)),
        0x7018 => Some(format!("LCtrl/{}\nRestore", gui)),
        0x7019 => Some(format!("RCtrl/{}\nSwap", gui)),
        0x701A => Some(format!("RCtrl/{}\nRestore", gui)),
        0x701B => Some(format!("Ctrl/{}\nSwap", gui)),
        0x701C => Some(format!("Ctrl/{}\nRestore", gui)),
        0x701D => Some(format!("Ctrl/{}\nToggle", gui)),
        _ => None,
    }
}

fn magic_keycode_tooltip(value: u16) -> Option<String> {
    let gui = gui_mod_name();
    match value {
        0x7000 => Some("Swap Caps Lock and Left Control".to_string()),
        0x7001 => Some("Unswap Caps Lock and Left Control".to_string()),
        0x7002 => Some("Toggle Caps Lock and Left Control swap".to_string()),
        0x7003 => Some("Stop treating Caps Lock as Control".to_string()),
        0x7004 => Some("Treat Caps Lock as Control".to_string()),
        0x7005 => Some(format!("Swap Left Alt and {}", gui)),
        0x7006 => Some(format!("Unswap Left Alt and {}", gui)),
        0x7007 => Some(format!("Swap Right Alt and {}", gui)),
        0x7008 => Some(format!("Unswap Right Alt and {}", gui)),
        0x7009 => Some(format!("Enable the {} keys", gui)),
        0x700A => Some(format!("Disable the {} keys", gui)),
        0x700B => Some(format!("Toggles the status of the {} keys", gui)),
        0x700C => Some("Swap ` and Escape".to_string()),
        0x700D => Some("Unswap ` and Escape".to_string()),
        0x700E => Some("Swap \\ and Backspace".to_string()),
        0x700F => Some("Unswap \\ and Backspace".to_string()),
        0x7010 => Some("Toggle \\ and Backspace swap state".to_string()),
        0x7011 => Some("Enable N-key rollover".to_string()),
        0x7012 => Some("Disable N-key rollover".to_string()),
        0x7013 => Some("Toggle N-key rollover".to_string()),
        0x7014 => Some(format!("Swap Alt and {} on both sides", gui)),
        0x7015 => Some(format!("Unswap Alt and {} on both sides", gui)),
        0x7016 => Some(format!("Toggle Alt and {} swap on both sides", gui)),
        0x7017 => Some(format!("Swap Left Control and {}", gui)),
        0x7018 => Some(format!("Unswap Left Control and {}", gui)),
        0x7019 => Some(format!("Swap Right Control and {}", gui)),
        0x701A => Some(format!("Unswap Right Control and {}", gui)),
        0x701B => Some(format!("Swap Control and {} on both sides", gui)),
        0x701C => Some(format!("Unswap Control and {} on both sides", gui)),
        0x701D => Some(format!("Toggle Control and {} swap on both sides", gui)),
        0x701E => Some("Set the master half of a split keyboard as the left hand (for EE_HANDS)".to_string()),
        0x701F => Some("Set the master half of a split keyboard as the right hand (for EE_HANDS)".to_string()),
        0x7020 => Some("Swap Caps Lock and Escape".to_string()),
        0x7021 => Some("Unswap Caps Lock and Escape".to_string()),
        0x7022 => Some("Toggle Caps Lock and Escape swap".to_string()),
        _ => None,
    }
}
use crate::keyboard::CustomKeycode;

/// Returns a human-readable label for a keycode.
/// Uses vial protocol v6 keycode ranges.
pub fn keycode_label_with_custom(value: u16, custom: &[CustomKeycode]) -> String {
    keycode_label_with_names(value, custom, &[])
}

pub fn keycode_label_with_names_and_layout(
    value: u16,
    custom: &[CustomKeycode],
    layer_names: &[String],
    key_legend_layout: KeyLegendLayout,
) -> String {
    apply_key_legend_layout(
        value,
        keycode_label_with_names(value, custom, layer_names),
        key_legend_layout,
    )
}

fn compact_compound_key_label(kc: &Keycode) -> String {
    let mut lines = kc.label.lines();
    if let (Some(top), Some(bottom), None) = (lines.next(), lines.next(), lines.next()) {
        let is_short_symbol = |line: &str| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && trimmed.chars().count() <= 3
                && trimmed
                    .chars()
                    .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
        };
        if is_short_symbol(top) || is_short_symbol(bottom) {
            return kc.label.to_string();
        }
    }

    simple_key_name(kc)
}

fn compound_keycode_label(prefix: &str, kc: u16) -> String {
    let kc_str = find_keycode(kc)
        .map(compact_compound_key_label)
        .unwrap_or_else(|| "?".to_string());
    format!("{}\n{}", prefix, kc_str)
}

pub fn keycode_label_with_names(value: u16, custom: &[CustomKeycode], layer_names: &[String]) -> String {
    // Returns "OpName(n)\nLayerName" or "OpName(n)" if layer has no custom name
    let layer_label = |op: &str, n: u16| -> String {
        match layer_names.get(n as usize) {
            Some(s) if !s.is_empty() && s != &n.to_string() => format!("{}({})\n{}", op, n, s),
            _ => format!("{}({})", op, n),
        }
    };
    // Plain layer number for use inside compound labels (LT, MT descriptions)
    let layer_name = |n: u16| -> String {
        match layer_names.get(n as usize) {
            Some(s) if !s.is_empty() && s != &n.to_string() => format!("{}({})", s, n),
            _ => n.to_string(),
        }
    };
    // Plain modifiers in the main layout should use readable text, not hold-action labels.
    match value {
        0x00E0 | 0x00E4 => return "Ctrl".to_string(),
        0x00E1 | 0x00E5 => return "Shift".to_string(),
        0x00E2 | 0x00E6 => return "Alt".to_string(),
        0x00E3 => return gui_label(false).to_string(),
        0x00E7 => return gui_label(true).to_string(),
        _ => {}
    }

    if let Some(label) = magic_keycode_label(value) {
        return label;
    }
    if let Some(kc) = find_keycode(value) {
        return kc.label.to_string();
    }

    // Custom keycodes MUST be checked before LT/MT ranges!
    // Vial GUI maps customKeycodes to USER00.. at QK_KB + index.
    // Protocol v6: QK_KB = 0x7E00.
    const QK_KB: u16 = 0x7E00;
    if value >= QK_KB {
        let idx = (value - QK_KB) as usize;
        if let Some(custom_keycode) = custom.get(idx) {
            if !custom_keycode.label.is_empty() {
                return custom_keycode.label.clone();
            }
        }
        return format!("USER{}", value - QK_KB);
    }

    // One-shot mod: 0x52A0..=0x52BF (Vial protocol v6)
    if let Some(bits) = osm_mod_bits(value) {
        return format!("OSM\n{}", osm_mod_short_name(bits));
    }

    // Layer ops — vial v6 protocol (0x5000..0x5FFF) MUST come before LT check!
    // because 0x52xx & 0xC000 == 0x4000 which would match LT range
    // QK_TO             = 0x5200, stride 1
    // QK_MOMENTARY      = 0x5220, stride 1  → MO(n)
    // QK_DEF_LAYER      = 0x5240            → DF(n)
    // QK_TOGGLE_LAYER   = 0x5260            → TG(n)
    // QK_ONE_SHOT_LAYER = 0x5280            → OSL(n)
    // QK_LAYER_TAP_TOG  = 0x52C0            → TT(n)
    // QK_PERSISTENT_DEF = 0x52E0            → PDF(n)
    if (0x5200..0x5300).contains(&value) {
        let sub = value & 0xFF;
        return match (value >> 5) & 0x7 {
            0 => layer_label("TO",  sub & 0x1F),
            1 => layer_label("MO",  sub & 0x1F),
            2 => layer_label("DF",  sub & 0x1F),
            3 => layer_label("TG",  sub & 0x1F),
            4 => layer_label("OSL", sub & 0x1F),
            5 => format!("OSM\n{}", osm_mod_short_name(sub & 0x1F)),
            6 => layer_label("TT",  sub & 0x1F),
            7 => layer_label("PDF", sub & 0x1F),
            _ => format!("{:04X}", value),
        };
    }

    // QK_LAYER_MOD: 0x5000 | (layer << 4) | mods
    if (0x5000..0x5200).contains(&value) {
        let layer = (value >> 4) & 0xF;
        return format!("LM/{}", layer_name(layer));
    }

    // QK_LAYER_TAP: 0x4000 | (layer << 8) | kc  (checked AFTER all 0x5xxx ranges)
    if value & 0xF000 == 0x4000 {
        let layer = (value >> 8) & 0xF;
        let kc = value & 0xFF;
        return compound_keycode_label(&format!("LT {}", layer_name(layer)), kc);
    }

    // QK_MOD_TAP: 0x2000 | (mods << 8) | kc
    if value & 0xE000 == 0x2000 {
        let kc = value & 0xFF;
        let mods = (value >> 8) & 0x1F;
        let right = (value >> 12) & 0x1 != 0;
        let mod_str = decode_mods(mods, right);
        return compound_keycode_label(&mod_str, kc);
    }

    // Modifier+key combos: 0x0100..0x1F00 | kc
    // LCTL=0x0100, LSFT=0x0200, LALT=0x0400, LGUI=0x0800
    // RCTL=0x1100, RSFT=0x1200, RALT=0x1400, RGUI=0x1800
    // MEH=0x0700, HYPR=0x0F00, LSA=0x0600, LCA=0x0500
    if (0x0100..0x2000).contains(&value) && (value & 0xFF) != 0 {
        let mods = value >> 8;
        let kc = value & 0xFF;
        let gui = gui_sym();
        let mod_str: String = match mods {
            0x01 => "Ctrl".into(),
            0x02 => "Shift".into(),
            0x04 => "Alt".into(),
            0x08 => gui.into(),
            0x03 => "Ctrl+Shift".into(),
            0x05 => "Ctrl+Alt".into(),
            0x06 => "Shift+Alt".into(),
            0x07 => "Meh".into(),
            0x09 => format!("Ctrl+{}", gui),
            0x0C => format!("Alt+{}", gui),
            0x0F => "Hyper".into(),
            0x0A => format!("Shift+{}", gui),
            0x11 => "RCtrl".into(),
            0x12 => "RShift".into(),
            0x14 => "RAlt".into(),
            0x18 => format!("R{}", gui),
            _ => "Mod".into(),
        };
        return compound_keycode_label(&mod_str, kc);
    }

    if value == 0x0001 { return "▽".to_string(); }
    if value == 0x0000 { return "✕".to_string(); }

    // Macro keycodes: 0x7700..0x77FF
    if (0x7700..=0x77FF).contains(&value) {
        return format!("M{}", value - 0x7700);
    }
    // Tap Dance keycodes: 0x5700..0x57FF
    if (0x5700..=0x57FF).contains(&value) {
        return format!("TD{}", value - 0x5700);
    }

    format!("{:04X}", value)
}

pub fn modifier_label_from_bits(mods: u16) -> String {
    match mods {
        0x01 | 0x11 => "Ctrl".to_string(),
        0x02 | 0x12 => "Shift".to_string(),
        0x04 | 0x14 => "Alt".to_string(),
        0x08 | 0x18 => gui_sym().to_string(),
        0x03 | 0x13 => "Ctrl+Shift".to_string(),
        0x05 | 0x15 => "Ctrl+Alt".to_string(),
        0x06 | 0x16 => "Shift+Alt".to_string(),
        0x07 | 0x17 => "Meh".to_string(),
        0x09 | 0x19 => format!("Ctrl+{}", gui_sym()),
        0x0F | 0x1F => "Hyper".to_string(),
        0x0A | 0x1A => format!("Shift+{}", gui_sym()),
        _ => "Mod".to_string(),
    }
}

fn decode_mods(mods: u16, _right: bool) -> String {
    modifier_label_from_bits(mods)
}

pub fn keycode_label(value: u16) -> String {
    keycode_label_with_custom(value, &[])
}

fn apply_key_legend_layout(value: u16, label: String, key_legend_layout: KeyLegendLayout) -> String {
    match key_legend_layout {
        KeyLegendLayout::English => label,
        KeyLegendLayout::Russian => russian_key_legend(value, false).unwrap_or(label),
        KeyLegendLayout::RussianPrimary => russian_key_legend(value, true).unwrap_or(label),
    }
}

fn dual_legend(primary: &str, secondary: &str) -> String {
    format!("{}\n{}", primary, secondary)
}

fn symbol_legend(base: &str, english: &str, russian: &str, russian_primary: bool) -> String {
    let shifted = if english == russian {
        english.to_string()
    } else if russian_primary {
        format!("{}  {}", russian, english)
    } else {
        format!("{}  {}", english, russian)
    };
    format!("{}\n{}", shifted, base)
}

fn shifted_symbol_legend(english: &str, russian: &str, russian_primary: bool) -> String {
    if english == russian {
        english.to_string()
    } else if russian_primary {
        format!("{}\n{}", russian, english)
    } else {
        format!("{}\n{}", english, russian)
    }
}

fn russian_key_legend(value: u16, russian_primary: bool) -> Option<String> {
    // Physical-style legends: both English and Russian legends are shown in the
    // same key-position system. The app setting controls which one is primary.
    let pair = |english: &str, russian: &str| {
        if russian_primary {
            dual_legend(russian, english)
        } else {
            dual_legend(english, russian)
        }
    };
    let symbol = |base: &str, english: &str, russian: &str| {
        symbol_legend(base, english, russian, russian_primary)
    };
    let shifted_symbol = |english: &str, russian: &str| {
        shifted_symbol_legend(english, russian, russian_primary)
    };

    match value {
        0x0004 => Some(pair("A", "Ф")),
        0x0005 => Some(pair("B", "И")),
        0x0006 => Some(pair("C", "С")),
        0x0007 => Some(pair("D", "В")),
        0x0008 => Some(pair("E", "У")),
        0x0009 => Some(pair("F", "А")),
        0x000A => Some(pair("G", "П")),
        0x000B => Some(pair("H", "Р")),
        0x000C => Some(pair("I", "Ш")),
        0x000D => Some(pair("J", "О")),
        0x000E => Some(pair("K", "Л")),
        0x000F => Some(pair("L", "Д")),
        0x0010 => Some(pair("M", "Ь")),
        0x0011 => Some(pair("N", "Т")),
        0x0012 => Some(pair("O", "Щ")),
        0x0013 => Some(pair("P", "З")),
        0x0014 => Some(pair("Q", "Й")),
        0x0015 => Some(pair("R", "К")),
        0x0016 => Some(pair("S", "Ы")),
        0x0017 => Some(pair("T", "Е")),
        0x0018 => Some(pair("U", "Г")),
        0x0019 => Some(pair("V", "М")),
        0x001A => Some(pair("W", "Ц")),
        0x001B => Some(pair("X", "Ч")),
        0x001C => Some(pair("Y", "Н")),
        0x001D => Some(pair("Z", "Я")),

        0x001E => Some("1".into()),
        0x001F => Some("2".into()),
        0x0020 => Some("3".into()),
        0x0021 => Some("4".into()),
        0x0022 => Some("5".into()),
        0x0023 => Some("6".into()),
        0x0024 => Some("7".into()),
        0x0025 => Some("8".into()),
        0x0026 => Some("9".into()),
        0x0027 => Some("0".into()),
        0x002D => Some(symbol("-", "_", "_")),
        0x002E => Some(symbol("=", "+", "+")),
        0x002F => Some(symbol("[", "{", "Х")),
        0x0030 => Some(symbol("]", "}", "Ъ")),
        0x0033 => Some(symbol(";", ":", "Ж")),
        0x0034 => Some(symbol("'", "\"", "Э")),
        0x0035 => Some(symbol("`", "~", "Ё")),
        0x0036 => Some(symbol(",", "<", "Б")),
        0x0037 => Some(symbol(".", ">", "Ю")),
        0x0038 => Some(symbol("/", "?", ".")),

        0x021E => Some(shifted_symbol("!", "!")),
        0x021F => Some(shifted_symbol("@", "\"")),
        0x0220 => Some(shifted_symbol("#", "№")),
        0x0221 => Some(shifted_symbol("$", ";")),
        0x0222 => Some(shifted_symbol("%", "%")),
        0x0223 => Some(shifted_symbol("^", ":")),
        0x0224 => Some(shifted_symbol("&", "?")),
        0x0225 => Some(shifted_symbol("*", "*")),
        0x0226 => Some(shifted_symbol("(", "(")),
        0x0227 => Some(shifted_symbol(")", ")")),
        0x022D => Some(shifted_symbol("_", "_")),
        0x022E => Some(shifted_symbol("+", "+")),
        0x022F => Some(shifted_symbol("{", "Х")),
        0x0230 => Some(shifted_symbol("}", "Ъ")),
        0x0233 => Some(shifted_symbol(":", "Ж")),
        0x0234 => Some(shifted_symbol("\"", "Э")),
        0x0235 => Some(shifted_symbol("~", "Ё")),
        0x0236 => Some(shifted_symbol("<", "Б")),
        0x0237 => Some(shifted_symbol(">", "Ю")),
        0x0238 => Some(shifted_symbol("?", ".")),
        _ => None,
    }
}

/// Keyboard layout selected in the operating system, i.e. what the host
/// actually types when the firmware sends a HID keycode.
///
/// Legends (`KeyLegendLayout`) describe what is printed on the keycap and may
/// show several alphabets at once; this enum describes a single active input
/// mapping and is therefore a separate type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyOutputLayout {
    #[default]
    English,
    Russian,
}

impl From<KeyLegendLayout> for KeyOutputLayout {
    fn from(legend: KeyLegendLayout) -> Self {
        match legend {
            KeyLegendLayout::English => Self::English,
            KeyLegendLayout::Russian | KeyLegendLayout::RussianPrimary => Self::Russian,
        }
    }
}

/// Characters a keycode types in the given input layout: the direct output and
/// the Shift output, when the key produces one.
///
/// Accepts both plain HID keycodes and QMK modifier-wrapped ones (`LSFT(kc)`);
/// keys that do not produce printable text return `None`.
pub fn printable_output(value: u16, layout: KeyOutputLayout) -> Option<(char, Option<char>)> {
    if let Some(base) = shift_wrapped_base_keycode(value) {
        let (_, shifted) = printable_output_for_hid(base, layout)?;
        return shifted.map(|shifted| (shifted, None));
    }
    printable_output_for_hid(value, layout)
}

/// Unwraps `LSFT(kc)`/`RSFT(kc)` QMK keycodes; other modifier combinations do
/// not have a plain printable output and are rejected.
fn shift_wrapped_base_keycode(value: u16) -> Option<u16> {
    if !(0x0100..0x2000).contains(&value) {
        return None;
    }
    let modifiers = (value >> 8) & 0x1F;
    (modifiers & 0x0F == 0x02).then_some(value & 0x00FF)
}

fn printable_output_for_hid(keycode: u16, layout: KeyOutputLayout) -> Option<(char, Option<char>)> {
    if let Some(index) = keycode.checked_sub(0x0004).filter(|index| *index < 26) {
        let letter = match layout {
            KeyOutputLayout::English => char::from(b'a' + index as u8),
            KeyOutputLayout::Russian => RUSSIAN_LETTER_ROW[index as usize],
        };
        let uppercase = letter.to_uppercase().next().unwrap_or(letter);
        return Some((letter, Some(uppercase)));
    }

    let english = |direct: char, shifted: char| Some((direct, Some(shifted)));
    match (keycode, layout) {
        (0x001e, KeyOutputLayout::English) => english('1', '!'),
        (0x001f, KeyOutputLayout::English) => english('2', '@'),
        (0x0020, KeyOutputLayout::English) => english('3', '#'),
        (0x0021, KeyOutputLayout::English) => english('4', '$'),
        (0x0022, KeyOutputLayout::English) => english('5', '%'),
        (0x0023, KeyOutputLayout::English) => english('6', '^'),
        (0x0024, KeyOutputLayout::English) => english('7', '&'),
        (0x0025, KeyOutputLayout::English) => english('8', '*'),
        (0x0026, KeyOutputLayout::English) => english('9', '('),
        (0x0027, KeyOutputLayout::English) => english('0', ')'),
        (0x002d, KeyOutputLayout::English) => english('-', '_'),
        (0x002e, KeyOutputLayout::English) => english('=', '+'),
        (0x002f, KeyOutputLayout::English) => english('[', '{'),
        (0x0030, KeyOutputLayout::English) => english(']', '}'),
        (0x0031, KeyOutputLayout::English) => english('\\', '|'),
        (0x0033, KeyOutputLayout::English) => english(';', ':'),
        (0x0034, KeyOutputLayout::English) => english('\'', '"'),
        (0x0035, KeyOutputLayout::English) => english('`', '~'),
        (0x0036, KeyOutputLayout::English) => english(',', '<'),
        (0x0037, KeyOutputLayout::English) => english('.', '>'),
        (0x0038, KeyOutputLayout::English) => english('/', '?'),

        (0x001e, KeyOutputLayout::Russian) => english('1', '!'),
        (0x001f, KeyOutputLayout::Russian) => english('2', '"'),
        (0x0020, KeyOutputLayout::Russian) => english('3', '№'),
        (0x0021, KeyOutputLayout::Russian) => english('4', ';'),
        (0x0022, KeyOutputLayout::Russian) => english('5', '%'),
        (0x0023, KeyOutputLayout::Russian) => english('6', ':'),
        (0x0024, KeyOutputLayout::Russian) => english('7', '?'),
        (0x0025, KeyOutputLayout::Russian) => english('8', '*'),
        (0x0026, KeyOutputLayout::Russian) => english('9', '('),
        (0x0027, KeyOutputLayout::Russian) => english('0', ')'),
        (0x002d, KeyOutputLayout::Russian) => english('-', '_'),
        (0x002e, KeyOutputLayout::Russian) => english('=', '+'),
        (0x002f, KeyOutputLayout::Russian) => english('х', 'Х'),
        (0x0030, KeyOutputLayout::Russian) => english('ъ', 'Ъ'),
        (0x0031, KeyOutputLayout::Russian) => english('\\', '/'),
        (0x0033, KeyOutputLayout::Russian) => english('ж', 'Ж'),
        (0x0034, KeyOutputLayout::Russian) => english('э', 'Э'),
        (0x0035, KeyOutputLayout::Russian) => english('ё', 'Ё'),
        (0x0036, KeyOutputLayout::Russian) => english('б', 'Б'),
        (0x0037, KeyOutputLayout::Russian) => english('ю', 'Ю'),
        (0x0038, KeyOutputLayout::Russian) => english('.', ','),
        _ => None,
    }
}

/// Russian ЙЦУКЕН letters in HID keycode order, i.e. `KC_A`..`KC_Z`.
const RUSSIAN_LETTER_ROW: [char; 26] = [
    'ф', 'и', 'с', 'в', 'у', 'а', 'п', 'р', 'ш', 'о', 'л', 'д', 'ь', 'т', 'щ', 'з', 'й', 'к', 'ы',
    'е', 'г', 'м', 'ц', 'ч', 'н', 'я',
];

/// Returns a human-readable tooltip for a keycode.
pub fn keycode_tooltip(value: u16, custom: &[CustomKeycode], layer_names: &[String]) -> String {
    let layer_display = |n: u16| -> String {
        match layer_names.get(n as usize) {
            Some(s) if !s.is_empty() && s != &n.to_string() => format!("\"{}\" ({})", s, n),
            _ => format!("layer {}", n),
        }
    };
    let mod_name = |m: u16, _right: bool| -> String {
        match m & 0x0F {
            0x01 => "Ctrl".into(),
            0x02 => "Shift".into(),
            0x04 => "Alt".into(),
            0x08 => gui_mod_name().into(),
            0x07 => "Meh (Ctrl+Shift+Alt)".into(),
            0x0F => format!("Hyper (Ctrl+Shift+Alt+{})", gui_mod_name()),
            0x03 => "Ctrl+Shift".into(),
            0x05 => "Ctrl+Alt".into(),
            0x06 => "Shift+Alt".into(),
            _ => "modifier".into(),
        }
    };
    let side = |v: u16| if v & 0x10 != 0 { "Right " } else { "Left " };

    // ── KC_NO / KC_TRNS ──────────────────────────────────────────────────────
    if value == 0x0000 {
        return "No key — this key does nothing".to_string();
    }
    if value == 0x0001 {
        return "Transparent — uses the key assigned on the layer below".to_string();
    }
    if let Some(tip) = magic_keycode_tooltip(value) {
        return tip;
    }
    // ── One-shot mod: 0x52A0..=0x52BF (Vial protocol v6) ─────────────────────
    if let Some(bits) = osm_mod_bits(value) {
        let full_name = osm_mod_full_name(bits);
        return format!("One-Shot {full_name} — applies {full_name} to the next keypress only");
    }

    // ── Layer ops 0x5200..0x52FF ─────────────────────────────────────────────
    if (0x5200..0x5300).contains(&value) {
        let sub = value & 0xFF;
        return match (value >> 5) & 0x7 {
            0 => format!("TO({}) — switch to {} and stay there", sub & 0x1F, layer_display(sub & 0x1F)),
            1 => format!("MO({}) — activate {} while held, return when released", sub & 0x1F, layer_display(sub & 0x1F)),
            2 => format!("DF({}) — set {} as the default base layer", sub & 0x1F, layer_display(sub & 0x1F)),
            3 => format!("TG({}) — toggle {} on/off", sub & 0x1F, layer_display(sub & 0x1F)),
            4 => format!("OSL({}) — activate {} for next keypress only", sub & 0x1F, layer_display(sub & 0x1F)),
            5 => {
                let m = mod_name(sub & 0x1F, sub >= 0x10);
                format!("One-Shot {} — activates {} for the very next keypress only", m, m)
            }
            6 => format!("TT({}) — tap to toggle {}, hold to activate while held", sub & 0x1F, layer_display(sub & 0x1F)),
            7 => format!("PDF({}) — permanently set {} as the default layer", sub & 0x1F, layer_display(sub & 0x1F)),
            _ => format!("Unknown layer op (0x{:04X})", value),
        };
    }

    // ── QK_LAYER_MOD 0x5000..0x51FF ─────────────────────────────────────────
    if (0x5000..0x5200).contains(&value) {
        let layer = (value >> 4) & 0xF;
        let mods = value & 0xF;
        let m = mod_name(mods, false);
        return format!("LM({}, {}) — activate {} with {} held while key is pressed", layer, m, layer_display(layer), m);
    }

    // ── QK_LAYER_TAP 0x4000..0x4FFF ─────────────────────────────────────────
    if value & 0xF000 == 0x4000 {
        let layer = (value >> 8) & 0xF;
        let kc = value & 0xFF;
        let kc_str = find_keycode(kc)
            .map(simple_key_name)
            .unwrap_or_else(|| format!("0x{:02X}", kc));
        return format!("Layer Tap — tap for {}, hold to activate {}", kc_str, layer_display(layer));
    }

    // ── QK_MOD_TAP 0x2000..0x3FFF ───────────────────────────────────────────
    if value & 0xE000 == 0x2000 {
        let kc = value & 0xFF;
        let mods = (value >> 8) & 0x1F;
        let right = (value >> 12) & 0x1 != 0;
        let kc_str = find_keycode(kc)
            .map(simple_key_name)
            .unwrap_or_else(|| format!("0x{:02X}", kc));
        let m = mod_name(mods, right);
        let side_str = side(mods);
        return format!("Mod Tap — tap for {}, hold for {}{}", kc_str, side_str, m);
    }

    // ── Modifier+key combos 0x0100..0x1FFF ──────────────────────────────────
    if (0x0100..0x2000).contains(&value) && (value & 0xFF) != 0 {
        let mods = value >> 8;
        let kc = value & 0xFF;
        let kc_str = find_keycode(kc)
            .map(simple_key_name)
            .unwrap_or_else(|| format!("0x{:02X}", kc));
        let combo = match mods {
            0x01 => "Ctrl+",
            0x02 => "Shift+",
            0x04 => "Alt+",
            0x08 => return format!("Shortcut: {}+{}", gui_mod_name(), kc_str),
            0x03 => "Ctrl+Shift+",
            0x05 => "Ctrl+Alt+",
            0x06 => "Shift+Alt+",
            0x07 => "Meh (Ctrl+Shift+Alt)+",
            0x09 => return format!("Shortcut: Ctrl+{}+{}", gui_mod_name(), kc_str),
            0x0C => return format!("Shortcut: Alt+{}+{}", gui_mod_name(), kc_str),
            0x0F => return format!("Shortcut: Hyper (Ctrl+Shift+Alt+{})+{}", gui_mod_name(), kc_str),
            0x0A => return format!("Shortcut: Shift+{}+{}", gui_mod_name(), kc_str),
            0x11 => "Right Ctrl+",
            0x12 => "Right Shift+",
            0x14 => "Right Alt+",
            0x18 => return format!("Shortcut: Right {}+{}", gui_mod_name(), kc_str),
            _ => "Modifier+",
        };
        return format!("Shortcut: {}{}", combo, kc_str);
    }

    // ── Custom keycodes ──────────────────────────────────────────────────────
    // Vial GUI maps customKeycodes to USER00.. at QK_KB + index.
    // Protocol v6: QK_KB = 0x7E00.
    const QK_KB: u16 = 0x7E00;
    if value >= QK_KB {
        let idx = (value - QK_KB) as usize;
        if let Some(custom_keycode) = custom.get(idx) {
            return custom_keycode.title.clone();
        }
        return format!("Custom key (0x{:04X})", value);
    }

    // ── Simple keycodes ──────────────────────────────────────────────────────
    if let Some(kc) = find_keycode(value) {
        return simple_key_tooltip(kc);
    }

    // Macro keycodes
    if (0x7700..=0x77FF).contains(&value) {
        return format!("Macro {} — sends a sequence of keystrokes", value - 0x7700);
    }
    // Tap Dance keycodes
    if (0x5700..=0x57FF).contains(&value) {
        return format!("Tap Dance {} — different actions on tap, hold, double tap", value - 0x5700);
    }

    format!("Unknown keycode (0x{:04X})", value)
}

/// Human-readable short name for a simple keycode (used inside compound descriptions).
fn simple_key_name(kc: &Keycode) -> String {
    if let Some(modifier) = kc.label.strip_prefix("Hold\n") {
        return modifier.to_string();
    }

    match kc.name {
        "KC_SPACE"  => "Space".to_string(),
        "KC_ENTER"  => "Enter".to_string(),
        "KC_ESCAPE" => "Escape".to_string(),
        "KC_BSPACE" => "Backspace".to_string(),
        "KC_TAB"    => "Tab".to_string(),
        "KC_DELETE" => "Delete".to_string(),
        "KC_LGUI"   => gui_label(false).to_string(),
        "KC_RGUI"   => gui_label(true).to_string(),
        _ => kc.label.replace('\n', "/"),
    }
}

/// Full tooltip for a plain keycode.
fn simple_key_tooltip(kc: &Keycode) -> String {
    let desc: &str = match kc.name {
        // Special
        "KC_NO"          => "No key — this key does nothing",
        "KC_TRNS"        => "Transparent — uses the key assigned on the layer below",
        // Control
        "KC_ENTER"       => "Enter — confirm / new line",
        "KC_ESCAPE"      => "Escape — cancel / close",
        "KC_BSPACE"      => "Backspace — delete character before cursor",
        "KC_TAB"         => "Tab — indent / move focus forward",
        "KC_SPACE"       => "Space",
        "KC_CAPSLOCK"    => "Caps Lock — toggle uppercase input",
        "KC_APPLICATION" => "Menu key — open right-click context menu",
        // Punctuation
        "KC_MINUS"      => "Minus — type -, Shift gives underscore (_)",
        "KC_EQUAL"      => "Equals — type =, Shift gives plus (+)",
        "KC_LBRACKET"   => "Left bracket — type [, Shift gives left brace ({)",
        "KC_RBRACKET"   => "Right bracket — type ], Shift gives right brace (})",
        "KC_BSLASH"     => "Backslash — type \\, Shift gives pipe (|)",
        "KC_NONUS_HASH" => "Non-US hash key — type #, Shift gives tilde (~)",
        "KC_SCOLON"     => "Semicolon key — tap for semicolon (;), Shift gives colon (:)",
        "KC_QUOTE"      => "Quote — type apostrophe ('), Shift gives double quote (\")",
        "KC_GRAVE"      => "Grave accent — type `, Shift gives tilde (~)",
        "KC_COMMA"      => "Comma — type comma (,), Shift gives less-than (<)",
        "KC_DOT"        => "Period — type dot (.), Shift gives greater-than (>)",
        "KC_SLASH"      => "Slash — type /, Shift gives question mark (?)",
        "KC_NONUS_BSLASH" => "Non-US backslash key — type \\, Shift gives pipe (|)",
        // Modifiers
        "KC_LCTRL"  => "Left Ctrl — modifier key (hold to activate shortcuts)",
        "KC_RCTRL"  => "Right Ctrl — modifier key (hold to activate shortcuts)",
        "KC_LSHIFT" => "Left Shift — hold to type uppercase / shifted symbols",
        "KC_RSHIFT" => "Right Shift — hold to type uppercase / shifted symbols",
        "KC_LALT"   => "Left Alt — modifier key (hold to activate shortcuts)",
        "KC_RALT"   => "Right Alt / AltGr — access special characters",
        "KC_LGUI"   => return gui_key_tooltip(false),
        "KC_RGUI"   => return gui_key_tooltip(true),
        // Navigation
        "KC_UP"       => "Arrow Up",
        "KC_DOWN"     => "Arrow Down",
        "KC_LEFT"     => "Arrow Left",
        "KC_RIGHT"    => "Arrow Right",
        "KC_HOME"     => "Home — jump to beginning of line",
        "KC_END"      => "End — jump to end of line",
        "KC_PGUP"     => "Page Up — scroll up one page",
        "KC_PGDOWN"   => "Page Down — scroll down one page",
        "KC_INSERT"   => "Insert — toggle insert/overwrite mode",
        "KC_DELETE"   => "Delete — delete character after cursor",
        "KC_PSCREEN"  => "Print Screen — take a screenshot",
        "KC_SCROLLLOCK" => "Scroll Lock",
        "KC_PAUSE"    => "Pause / Break",
        // Media
        "KC_MUTE" => "Mute / Unmute audio",
        "KC_VOLU" => "Volume Up",
        "KC_VOLD" => "Volume Down",
        "KC_MNXT" => "Next Track",
        "KC_MPRV" => "Previous Track",
        "KC_MSTP" => "Stop playback",
        "KC_MPLY" => "Play / Pause",
        "KC_MSEL" => "Open media player",
        "KC_MAIL" => "Open email client",
        "KC_CALC" => "Open calculator",
        "KC_MYCM" => "Open My Computer / file manager",
        "KC_WSCH" => "Browser search",
        "KC_WHOM" => "Browser home page",
        "KC_WBAK" => "Browser back",
        "KC_WFWD" => "Browser forward",
        "KC_WSTP" => "Browser stop loading",
        "KC_WREF" => "Browser refresh",
        "KC_WFAV" => "Browser favourites",
        "KC_SLEP" => "Sleep — put computer to sleep",
        "KC_WAKE" => "Wake — wake computer from sleep",
        "KC_BRIU" => "Brightness Up",
        "KC_BRID" => "Brightness Down",
        "KC_PWR"  => "Power — system power button",
        "KC_EXEC" => "Execute — run the currently selected action or file",
        "KC_HELP" => "Help — open help for the current app or context",
        "KC_SELECT" => "Select — select the current item",
        "KC_STOP" => "Stop — cancel the current action or loading",
        "KC_AGAIN" => "Again — repeat the previous action",
        "KC_UNDO" => "Undo — revert the last action",
        "KC_CUT" => "Cut — remove selection and copy it to clipboard",
        "KC_COPY" => "Copy — copy selection to clipboard",
        "KC_PSTE" => "Paste — insert clipboard contents",
        "KC_FIND" => "Find — search in the current document or view",
        "KC_EJCT" => "Eject — eject removable media",
        "KC_MFFD" => "Fast Forward — jump forward in media",
        "KC_MRWD" => "Rewind — jump backward in media",
        "KC_MCTL" => "Mission Control / Task View — show open windows and spaces",
        "KC_LPAD" => "Launchpad / app launcher",
        // Mouse
        "KC_MS_U" => "Mouse cursor — move up",
        "KC_MS_D" => "Mouse cursor — move down",
        "KC_MS_L" => "Mouse cursor — move left",
        "KC_MS_R" => "Mouse cursor — move right",
        "KC_BTN1" => "Mouse button 1 — left click",
        "KC_BTN2" => "Mouse button 2 — right click",
        "KC_BTN3" => "Mouse button 3 — middle click",
        "KC_BTN4" => "Mouse button 4 — back",
        "KC_BTN5" => "Mouse button 5 — forward",
        "KC_BTN6" => "Mouse button 6",
        "KC_BTN7" => "Mouse button 7",
        "KC_BTN8" => "Mouse button 8",
        "KC_WH_U" => "Mouse wheel — scroll up",
        "KC_WH_D" => "Mouse wheel — scroll down",
        "KC_WH_L" => "Mouse wheel — scroll left",
        "KC_WH_R" => "Mouse wheel — scroll right",
        "KC_MS_ACCEL0" => "Mouse acceleration 0 — slowest cursor speed profile",
        "KC_MS_ACCEL1" => "Mouse acceleration 1 — medium cursor speed profile",
        "KC_MS_ACCEL2" => "Mouse acceleration 2 — fastest cursor speed profile",
        // Numpad
        "KC_NUMLOCK"     => "Num Lock — toggle numpad number input",
        "KC_KP_SLASH"    => "Numpad ÷ (divide)",
        "KC_KP_ASTERISK" => "Numpad × (multiply)",
        "KC_KP_MINUS"    => "Numpad − (minus)",
        "KC_KP_PLUS"     => "Numpad + (plus)",
        "KC_KP_ENTER"    => "Numpad Enter",
        "KC_KP_DOT"      => "Numpad . (decimal point)",
        "KC_KP_EQUAL"    => "Numpad = (equals)",
        "KC_KP_COMMA"    => "Numpad , (comma)",
        // QMK special
        "KC_GESC"   => return format!("Grave/Escape — sends Esc normally, ` when Shift or {} is held", gui_mod_name()),
        "QK_BOOT"   => "Bootloader — put device into flash mode",
        "DB_TOGG"   => "Debug toggle — enable/disable debug output",
        "QK_LOCK"   => "Lock — lock a key in pressed state until pressed again",
        "KC_LSPO"   => "Left Shift when held, ( when tapped",
        "KC_RSPC"   => "Right Shift when held, ) when tapped",
        "KC_LCPO"   => "Left Control when held, ( when tapped",
        "KC_RCPC"   => "Right Control when held, ) when tapped",
        "KC_LAPO"   => "Left Alt when held, ( when tapped",
        "KC_RAPC"   => "Right Alt when held, ) when tapped",
        "KC_SFTENT" => "Right Shift when held, Enter when tapped",
        "KC_RO"     => "JIS \\ and _",
        "KC_KANA"   => "JIS Katakana/Hiragana",
        "KC_JYEN"   => "JIS ¥ and |",
        "KC_HENK"   => "JIS Henkan",
        "KC_MHEN"   => "JIS Muhenkan",
        "KC_INT6"   => "JIS Numpad ,",
        "KC_LANG1"  => "Hangul/English",
        "KC_LANG2"  => "Hanja",
        "KC_LANG3"  => "JIS Katakana",
        "KC_LANG4"  => "JIS Hiragana",
        "KC_LANG5"  => "JIS Zenkaku/Hankaku",
        "QK_MAKE"             => "Compile firmware",
        "KC_ASTG"             => "Toggles the state of the Auto Shift feature",
        "CMB_TOG"             => "Toggles Combo feature on and off",
        "QK_CAPS_WORD_TOGGLE" => "Capitalizes until end of current word",
        "QK_REPEAT_KEY"       => "Repeats the last pressed key",
        "QK_ALT_REPEAT_KEY"   => "Alt repeats the last pressed key",
        // RGB
        "RGB_TOG"  => "RGB lighting — toggle on/off",
        "RGB_MOD"  => "RGB lighting — next animation mode",
        "RGB_RMOD" => "RGB lighting — previous animation mode",
        "RGB_HUI"  => "RGB lighting — hue +",
        "RGB_HUD"  => "RGB lighting — hue −",
        "RGB_SAI"  => "RGB lighting — saturation +",
        "RGB_SAD"  => "RGB lighting — saturation −",
        "RGB_VAI"  => "RGB lighting — brightness +",
        "RGB_VAD"  => "RGB lighting — brightness −",
        "RGB_SPI"  => "RGB lighting — animation speed +",
        "RGB_SPD"  => "RGB lighting — animation speed −",
        _ => "",
    };

    if !desc.is_empty() {
        return desc.to_string();
    }

    // Fallback: letters, digits, F-keys, punctuation
    match kc.category {
        KeycodeCategory::Basic => {
            let label = kc.label.replace('\n', " / ");
            format!("Key: {}", label)
        }
        KeycodeCategory::Function => format!("{} function key", kc.label),
        KeycodeCategory::Numpad   => format!("Numpad {}", kc.label.trim_start_matches("Num")),
        _ => kc.label.replace('\n', " / "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_gui_legends_use_cmd_text() {
        assert_eq!(gui_name_for_target_os("macos"), "Cmd");
    }

    #[test]
    fn printable_output_follows_the_english_input_mapping() {
        assert_eq!(
            printable_output(0x0004, KeyOutputLayout::English),
            Some(('a', Some('A')))
        );
        assert_eq!(
            printable_output(0x001f, KeyOutputLayout::English),
            Some(('2', Some('@')))
        );
        assert_eq!(
            printable_output(0x0038, KeyOutputLayout::English),
            Some(('/', Some('?')))
        );
    }

    #[test]
    fn printable_output_follows_the_russian_input_mapping() {
        assert_eq!(
            printable_output(0x0004, KeyOutputLayout::Russian),
            Some(('ф', Some('Ф')))
        );
        assert_eq!(
            printable_output(0x001f, KeyOutputLayout::Russian),
            Some(('2', Some('"')))
        );
        assert_eq!(
            printable_output(0x0038, KeyOutputLayout::Russian),
            Some(('.', Some(',')))
        );
        assert_eq!(
            printable_output(0x002f, KeyOutputLayout::Russian),
            Some(('х', Some('Х')))
        );
    }

    #[test]
    fn printable_output_resolves_shift_wrapped_keycodes_to_the_shifted_character() {
        assert_eq!(
            printable_output(0x021e, KeyOutputLayout::English),
            Some(('!', None))
        );
        assert_eq!(
            printable_output(0x0220, KeyOutputLayout::Russian),
            Some(('№', None))
        );
        assert_eq!(
            printable_output(0x0204, KeyOutputLayout::Russian),
            Some(('Ф', None))
        );
    }

    #[test]
    fn printable_output_rejects_non_typing_keys_and_other_modifiers() {
        assert_eq!(printable_output(0x0000, KeyOutputLayout::English), None);
        assert_eq!(printable_output(0x0029, KeyOutputLayout::English), None);
        // LCTL(KC_A) types nothing on its own.
        assert_eq!(printable_output(0x0104, KeyOutputLayout::English), None);
    }

    #[test]
    fn key_output_layout_follows_the_selected_legend_family() {
        assert_eq!(
            KeyOutputLayout::from(KeyLegendLayout::English),
            KeyOutputLayout::English
        );
        assert_eq!(
            KeyOutputLayout::from(KeyLegendLayout::RussianPrimary),
            KeyOutputLayout::Russian
        );
    }
}
