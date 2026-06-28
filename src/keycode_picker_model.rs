use crate::keycode::KeycodeCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasicPickerLayout {
    Qwerty,
    Dvorak,
    ColemakDh,
    Workman,
    Norman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerViewMode {
    Layout,
    List,
}

impl PickerViewMode {
    pub(super) const ALL: [PickerViewMode; 2] = [PickerViewMode::List, PickerViewMode::Layout];

    pub(super) fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::List)
    }

    pub(super) fn index(self) -> usize {
        match self {
            PickerViewMode::List => 0,
            PickerViewMode::Layout => 1,
        }
    }

    pub(super) fn i18n_key(self) -> &'static str {
        match self {
            PickerViewMode::Layout => "key_picker.view_layout",
            PickerViewMode::List => "key_picker.view_list",
        }
    }
}

impl Default for PickerViewMode {
    fn default() -> Self {
        Self::List
    }
}

impl BasicPickerLayout {
    pub(super) const ALL: [BasicPickerLayout; 5] = [
        BasicPickerLayout::Qwerty,
        BasicPickerLayout::Dvorak,
        BasicPickerLayout::ColemakDh,
        BasicPickerLayout::Workman,
        BasicPickerLayout::Norman,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            BasicPickerLayout::Qwerty => "QWERTY",
            BasicPickerLayout::Dvorak => "Dvorak",
            BasicPickerLayout::ColemakDh => "Colemak-DH",
            BasicPickerLayout::Workman => "Workman",
            BasicPickerLayout::Norman => "Norman",
        }
    }

    pub(super) fn map_value(self, value: u16) -> u16 {
        match self {
            BasicPickerLayout::Qwerty => value,
            BasicPickerLayout::Dvorak => match value {
                0x0014 => 0x0034, // Q position -> '
                0x001A => 0x0036, // W position -> ,
                0x0008 => 0x0037, // E position -> .
                0x0015 => 0x0013, // R position -> P
                0x0017 => 0x001C, // T position -> Y
                0x001C => 0x0009, // Y position -> F
                0x0018 => 0x000A, // U position -> G
                0x000C => 0x0006, // I position -> C
                0x0012 => 0x0015, // O position -> R
                0x0013 => 0x000F, // P position -> L
                0x002F => 0x0038, // [ position -> /
                0x0030 => 0x002E, // ] position -> =
                0x0004 => 0x0004, // A
                0x0016 => 0x0012, // S position -> O
                0x0007 => 0x0008, // D position -> E
                0x0009 => 0x0018, // F position -> U
                0x000A => 0x000C, // G position -> I
                0x000B => 0x0007, // H position -> D
                0x000D => 0x000B, // J position -> H
                0x000E => 0x0017, // K position -> T
                0x000F => 0x0011, // L position -> N
                0x0033 => 0x0016, // ; position -> S
                0x0034 => 0x002D, // ' position -> -
                0x001D => 0x0033, // Z position -> ;
                0x001B => 0x0014, // X position -> Q
                0x0006 => 0x000D, // C position -> J
                0x0019 => 0x000E, // V position -> K
                0x0005 => 0x001B, // B position -> X
                0x0011 => 0x0005, // N position -> B
                0x0010 => 0x0010, // M
                0x0036 => 0x001A, // , position -> W
                0x0037 => 0x0019, // . position -> V
                0x0038 => 0x001D, // / position -> Z
                _ => value,
            },
            BasicPickerLayout::ColemakDh => match value {
                0x0014 => 0x0014, // Q
                0x001A => 0x001A, // W
                0x0008 => 0x0009, // E position -> F
                0x0015 => 0x0013, // R position -> P
                0x0017 => 0x0005, // T position -> B
                0x001C => 0x000D, // Y position -> J
                0x0018 => 0x000F, // U position -> L
                0x000C => 0x0018, // I position -> U
                0x0012 => 0x001C, // O position -> Y
                0x0013 => 0x0033, // P position -> ;
                0x0004 => 0x0004, // A
                0x0016 => 0x0015, // S position -> R
                0x0007 => 0x0016, // D position -> S
                0x0009 => 0x0017, // F position -> T
                0x000A => 0x000A, // G
                0x000B => 0x0010, // H position -> M
                0x000D => 0x0011, // J position -> N
                0x000E => 0x0008, // K position -> E
                0x000F => 0x000C, // L position -> I
                0x0033 => 0x0012, // ; position -> O
                0x001D => 0x001D, // Z
                0x001B => 0x001B, // X
                0x0006 => 0x0006, // C
                0x0019 => 0x0007, // V position -> D
                0x0005 => 0x0019, // B position -> V
                0x0011 => 0x000E, // N position -> K
                0x0010 => 0x000B, // M position -> H
                _ => value,
            },
            BasicPickerLayout::Workman => match value {
                0x0014 => 0x0014, // Q
                0x001A => 0x0007, // W position -> D
                0x0008 => 0x0015, // E position -> R
                0x0015 => 0x001A, // R position -> W
                0x0017 => 0x0005, // T position -> B
                0x001C => 0x000D, // Y position -> J
                0x0018 => 0x0009, // U position -> F
                0x000C => 0x0018, // I position -> U
                0x0012 => 0x0013, // O position -> P
                0x0013 => 0x0033, // P position -> ;
                0x0004 => 0x0004, // A
                0x0016 => 0x0016, // S
                0x0007 => 0x000B, // D position -> H
                0x0009 => 0x0017, // F position -> T
                0x000A => 0x000A, // G
                0x000B => 0x001C, // H position -> Y
                0x000D => 0x0011, // J position -> N
                0x000E => 0x0008, // K position -> E
                0x000F => 0x0012, // L position -> O
                0x0033 => 0x000C, // ; position -> I
                0x001D => 0x001D, // Z
                0x001B => 0x001B, // X
                0x0006 => 0x0010, // C position -> M
                0x0019 => 0x0006, // V position -> C
                0x0005 => 0x0019, // B position -> V
                0x0011 => 0x000E, // N position -> K
                0x0010 => 0x000F, // M position -> L
                _ => value,
            },
            BasicPickerLayout::Norman => match value {
                0x0014 => 0x0014, // Q
                0x001A => 0x001A, // W
                0x0008 => 0x0007, // E position -> D
                0x0015 => 0x0009, // R position -> F
                0x0017 => 0x000E, // T position -> K
                0x001C => 0x000D, // Y position -> J
                0x0018 => 0x0018, // U
                0x000C => 0x0015, // I position -> R
                0x0012 => 0x000F, // O position -> L
                0x0013 => 0x0033, // P position -> ;
                0x0004 => 0x0004, // A
                0x0016 => 0x0016, // S
                0x0007 => 0x0008, // D position -> E
                0x0009 => 0x0017, // F position -> T
                0x000A => 0x000A, // G
                0x000B => 0x001C, // H position -> Y
                0x000D => 0x0011, // J position -> N
                0x000E => 0x000C, // K position -> I
                0x000F => 0x0012, // L position -> O
                0x0033 => 0x000B, // ; position -> H
                0x001D => 0x001D, // Z
                0x001B => 0x001B, // X
                0x0006 => 0x0006, // C
                0x0019 => 0x0019, // V
                0x0005 => 0x0005, // B
                0x0011 => 0x0013, // N position -> P
                0x0010 => 0x0010, // M
                _ => value,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeycodeTab {
    Basic,
    Symbols,
    Emoji,
    Function,
    Navigation,
    Modifiers,
    Layers,
    Media,
    Mouse,
    Numpad,
    Special,
    Rgb,
    Macro,
    TapDance,
    Quantum,
    Custom,
}

impl KeycodeTab {
    pub const VIAL_TABS: &'static [KeycodeTab] = &[
        KeycodeTab::Basic,
        KeycodeTab::Symbols,
        KeycodeTab::Modifiers,
        KeycodeTab::Special,
        KeycodeTab::Rgb,
        KeycodeTab::Macro,
        KeycodeTab::TapDance,
        KeycodeTab::Custom,
        KeycodeTab::Emoji,
    ];

    pub(super) fn i18n_key(self) -> &'static str {
        match self {
            KeycodeTab::Basic => "key_picker.tab_basic",
            KeycodeTab::Symbols => "key_picker.tab_symbols",
            KeycodeTab::Emoji => "key_picker.tab_emoji",
            KeycodeTab::Function => "key_picker.tab_function",
            KeycodeTab::Navigation => "key_picker.tab_navigation",
            KeycodeTab::Modifiers => "key_picker.tab_modifiers",
            KeycodeTab::Layers => "key_picker.tab_layers",
            KeycodeTab::Media => "key_picker.tab_media",
            KeycodeTab::Mouse => "key_picker.tab_mouse",
            KeycodeTab::Numpad => "key_picker.tab_numpad",
            KeycodeTab::Special => "key_picker.tab_special",
            KeycodeTab::Rgb => "key_picker.tab_rgb",
            KeycodeTab::Macro => "key_picker.tab_macro",
            KeycodeTab::TapDance => "key_picker.tab_tap_dance",
            KeycodeTab::Quantum => "key_picker.tab_quantum",
            KeycodeTab::Custom => "key_picker.tab_custom",
        }
    }

    pub(super) fn vial_matches(&self, kc: &crate::keycode::Keycode) -> bool {
        match self {
            KeycodeTab::Basic => {
                matches!(kc.category, KeycodeCategory::Basic) && !is_symbol(kc.value)
            }
            KeycodeTab::Symbols => {
                matches!(kc.category, KeycodeCategory::Basic) && is_symbol(kc.value)
            }
            KeycodeTab::Emoji => false,
            KeycodeTab::Function => {
                matches!(kc.category, KeycodeCategory::Function) && kc.value <= 0x0045
            }
            KeycodeTab::Navigation => matches!(kc.category, KeycodeCategory::Navigation),
            KeycodeTab::Modifiers => matches!(kc.category, KeycodeCategory::Modifier),
            KeycodeTab::Layers => matches!(kc.category, KeycodeCategory::Layer),
            KeycodeTab::Media => {
                matches!(kc.category, KeycodeCategory::Media | KeycodeCategory::Mouse)
            }
            KeycodeTab::Mouse => matches!(kc.category, KeycodeCategory::Mouse),
            KeycodeTab::Numpad => matches!(kc.category, KeycodeCategory::Numpad),
            KeycodeTab::Special => matches!(kc.category, KeycodeCategory::Special),
            _ => false,
        }
    }

    pub(super) fn preferred_for_vial_keycode(value: u16, is_custom_keycode: bool) -> KeycodeTab {
        if is_custom_keycode {
            return KeycodeTab::Custom;
        }
        if (0x7700..=0x77FF).contains(&value) {
            return KeycodeTab::Macro;
        }
        if (0x5700..=0x57FF).contains(&value) {
            return KeycodeTab::TapDance;
        }
        if crate::smart_input::smart_symbol_for_keycode(value).is_some() {
            return KeycodeTab::Symbols;
        }

        if let Some(kc) = crate::keycode::find_keycode(value) {
            if kc.name.starts_with("RGB_") {
                return KeycodeTab::Rgb;
            }

            return match kc.category {
                KeycodeCategory::Basic if is_symbol(kc.value) => KeycodeTab::Symbols,
                KeycodeCategory::Basic
                | KeycodeCategory::Function
                | KeycodeCategory::Navigation => KeycodeTab::Basic,
                KeycodeCategory::Modifier | KeycodeCategory::Layer => KeycodeTab::Modifiers,
                KeycodeCategory::Media | KeycodeCategory::Mouse | KeycodeCategory::Numpad => {
                    KeycodeTab::Special
                }
                KeycodeCategory::Special => KeycodeTab::Special,
            };
        }

        if is_modifier_or_layer_action(value) {
            return KeycodeTab::Modifiers;
        }

        KeycodeTab::Basic
    }
}

fn is_symbol(value: u16) -> bool {
    matches!(value,
        0x002D..=0x0038 |
        0x0064 |
        0x021E..=0x0238
    )
}

fn is_modifier_or_layer_action(value: u16) -> bool {
    matches!(
        value,
        0x0100..=0x3FFF | 0x4000..=0x4FFF | 0x5000..=0x52FF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_tab_tracks_existing_keycode_groups() {
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x0014, false),
            KeycodeTab::Basic
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x002D, false),
            KeycodeTab::Symbols
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x00E0, false),
            KeycodeTab::Modifiers
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x5220, false),
            KeycodeTab::Modifiers
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x7820, false),
            KeycodeTab::Rgb
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x7700, false),
            KeycodeTab::Macro
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x5700, false),
            KeycodeTab::TapDance
        );
        assert_eq!(
            KeycodeTab::preferred_for_vial_keycode(0x7E00, true),
            KeycodeTab::Custom
        );
    }

    #[test]
    fn picker_view_mode_defaults_to_list_and_can_switch_to_layout() {
        assert_eq!(PickerViewMode::default(), PickerViewMode::List);
        assert_eq!(PickerViewMode::from_index(1), PickerViewMode::Layout);
        assert_eq!(PickerViewMode::Layout.index(), 1);
    }
}
