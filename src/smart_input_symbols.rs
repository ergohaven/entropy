#[derive(Clone, Copy, Debug)]
pub struct SmartSymbol {
    pub trigger_keycode: u16,
    pub symbol: char,
    pub name: &'static str,
}

pub(super) const KC_F13: u16 = 0x0068;
pub(super) const MOD_CTRL: u16 = 0x0100;
pub(super) const MOD_SHIFT: u16 = 0x0200;
pub(super) const MOD_ALT: u16 = 0x0400;
pub(super) const MOD_GUI: u16 = 0x0800;

pub const SMART_SYMBOLS: &[SmartSymbol] = &[
    // F13..F20
    SmartSymbol {
        trigger_keycode: KC_F13,
        symbol: '{',
        name: "Left brace",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 1,
        symbol: '}',
        name: "Right brace",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 2,
        symbol: '[',
        name: "Left bracket",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 3,
        symbol: ']',
        name: "Right bracket",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 4,
        symbol: '(',
        name: "Left parenthesis",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 5,
        symbol: ')',
        name: "Right parenthesis",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 6,
        symbol: '<',
        name: "Less-than",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 7,
        symbol: '>',
        name: "Greater-than",
    },
    // Shift+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | KC_F13,
        symbol: '!',
        name: "Exclamation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 1),
        symbol: '"',
        name: "Quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 2),
        symbol: '$',
        name: "Dollar sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 3),
        symbol: '%',
        name: "Percent sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 4),
        symbol: '&',
        name: "Ampersand",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 5),
        symbol: '\'',
        name: "Apostrophe",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 6),
        symbol: '*',
        name: "Asterisk",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 7),
        symbol: '+',
        name: "Plus sign",
    },
    // Ctrl+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_CTRL | KC_F13,
        symbol: '«',
        name: "Left guillemet",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 1),
        symbol: '»',
        name: "Right guillemet",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 2),
        symbol: '€',
        name: "Euro sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 3),
        symbol: '—',
        name: "Em dash",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 4),
        symbol: '–',
        name: "En dash",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 5),
        symbol: '•',
        name: "Bullet",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 6),
        symbol: '×',
        name: "Multiplication sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 7),
        symbol: '±',
        name: "Plus-minus sign",
    },
    // Alt+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_ALT | KC_F13,
        symbol: '.',
        name: "Full stop",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 1),
        symbol: ',',
        name: "Comma",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 2),
        symbol: ';',
        name: "Semicolon",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 3),
        symbol: ':',
        name: "Colon",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 4),
        symbol: '/',
        name: "Slash",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 5),
        symbol: '`',
        name: "Grave accent",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 6),
        symbol: '^',
        name: "Caret",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 7),
        symbol: '≠',
        name: "Not equal sign",
    },
    // Alt+Shift+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | KC_F13,
        symbol: '#',
        name: "Number sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 1),
        symbol: '@',
        name: "At sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 2),
        symbol: '№',
        name: "Numero sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 3),
        symbol: '₽',
        name: "Ruble sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 4),
        symbol: '=',
        name: "Equals sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 5),
        symbol: '?',
        name: "Question mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 6),
        symbol: '|',
        name: "Vertical bar",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | MOD_SHIFT | (KC_F13 + 7),
        symbol: '\\',
        name: "Backslash",
    },
    // Ctrl+Alt+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | KC_F13,
        symbol: 'б',
        name: "Cyrillic be",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 1),
        symbol: 'ю',
        name: "Cyrillic yu",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 2),
        symbol: 'ж',
        name: "Cyrillic zhe",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 3),
        symbol: 'э',
        name: "Cyrillic e",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 4),
        symbol: 'х',
        name: "Cyrillic ha",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 5),
        symbol: 'ъ',
        name: "Cyrillic Hard Sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 6),
        symbol: 'ё',
        name: "Cyrillic yo",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 7),
        symbol: '≈',
        name: "Almost equal sign",
    },
    // Ctrl+Alt+Shift+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | KC_F13,
        symbol: 'Б',
        name: "Cyrillic Be",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 1),
        symbol: 'Ю',
        name: "Cyrillic Yu",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 2),
        symbol: 'Ж',
        name: "Cyrillic Zhe",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 3),
        symbol: 'Э',
        name: "Cyrillic E",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 4),
        symbol: 'Х',
        name: "Cyrillic Ha",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 5),
        symbol: 'Ъ',
        name: "Cyrillic Hard Sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 6),
        symbol: 'Ё',
        name: "Cyrillic Yo",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 7),
        symbol: '✓',
        name: "Check mark",
    },
    // Ctrl+Shift+F13..F20
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | KC_F13,
        symbol: '°',
        name: "Degree sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 1),
        symbol: '‰',
        name: "Per mille sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 2),
        symbol: '′',
        name: "Prime",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 3),
        symbol: '″',
        name: "Double prime",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 4),
        symbol: '‘',
        name: "Left single quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 5),
        symbol: '’',
        name: "Right single quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 6),
        symbol: '„',
        name: "Double low quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 7),
        symbol: '“',
        name: "Left double quotation mark",
    },
    // Gui/Super/Win+F13..F17
    SmartSymbol {
        trigger_keycode: MOD_GUI | KC_F13,
        symbol: '§',
        name: "Section sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | (KC_F13 + 1),
        symbol: '”',
        name: "Right double quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | (KC_F13 + 2),
        symbol: '™',
        name: "Trade mark sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | (KC_F13 + 3),
        symbol: '~',
        name: "Tilde",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | (KC_F13 + 4),
        symbol: '_',
        name: "Underscore",
    },
    // Gui/Super/Win+Shift+F13..F17
    SmartSymbol {
        trigger_keycode: MOD_GUI | MOD_SHIFT | KC_F13,
        symbol: '←',
        name: "Leftwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | MOD_SHIFT | (KC_F13 + 1),
        symbol: '↑',
        name: "Upwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | MOD_SHIFT | (KC_F13 + 2),
        symbol: '→',
        name: "Rightwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | MOD_SHIFT | (KC_F13 + 3),
        symbol: '↓',
        name: "Downwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_GUI | MOD_SHIFT | (KC_F13 + 4),
        symbol: '↔',
        name: "Left right arrow",
    },
];

pub fn smart_symbol_for_keycode(keycode: u16) -> Option<SmartSymbol> {
    SMART_SYMBOLS
        .iter()
        .copied()
        .find(|symbol| symbol.trigger_keycode == keycode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_symbol_catalog_stays_at_74_entries() {
        assert_eq!(SMART_SYMBOLS.len(), 74);
    }

    #[test]
    fn arrow_symbols_use_gui_shift_function_key_slots() {
        let expected = [
            (MOD_GUI | MOD_SHIFT | KC_F13, '←'),
            (MOD_GUI | MOD_SHIFT | (KC_F13 + 1), '↑'),
            (MOD_GUI | MOD_SHIFT | (KC_F13 + 2), '→'),
            (MOD_GUI | MOD_SHIFT | (KC_F13 + 3), '↓'),
            (MOD_GUI | MOD_SHIFT | (KC_F13 + 4), '↔'),
        ];

        for (keycode, symbol) in expected {
            assert_eq!(
                smart_symbol_for_keycode(keycode).map(|smart_symbol| smart_symbol.symbol),
                Some(symbol)
            );
        }
    }

    #[test]
    fn universal_symbol_triggers_use_f13_through_f20_only() {
        for symbol in SMART_SYMBOLS {
            let base_keycode = symbol.trigger_keycode & 0x00ff;
            assert!(
                (KC_F13..=KC_F13 + 7).contains(&base_keycode),
                "{} uses F{}",
                symbol.name,
                base_keycode - KC_F13 + 13
            );
        }
    }

    #[test]
    fn remapped_universal_symbols_use_mac_friendly_transport_slots() {
        let expected = [
            (MOD_ALT | MOD_SHIFT | KC_F13, '#'),
            (MOD_ALT | MOD_SHIFT | (KC_F13 + 3), '₽'),
            (MOD_ALT | MOD_SHIFT | (KC_F13 + 4), '='),
            (MOD_GUI | KC_F13, '§'),
            (MOD_GUI | (KC_F13 + 1), '”'),
            (MOD_GUI | (KC_F13 + 4), '_'),
            (MOD_ALT | (KC_F13 + 7), '≠'),
            (MOD_CTRL | MOD_ALT | (KC_F13 + 7), '≈'),
            (MOD_CTRL | MOD_ALT | MOD_SHIFT | (KC_F13 + 7), '✓'),
        ];

        for (keycode, symbol) in expected {
            assert_eq!(
                smart_symbol_for_keycode(keycode).map(|smart_symbol| smart_symbol.symbol),
                Some(symbol)
            );
        }
    }

    #[test]
    fn linux_input_method_backends_include_arrow_symbols() {
        let ibus_backend = include_str!("../linux/ibus/entropy-ibus-engine");
        let fcitx5_backend = include_str!("../linux/fcitx5/src/entropyuniversalsymbols.cpp");

        for symbol in ["←", "↑", "→", "↓", "↔"] {
            assert!(
                ibus_backend.contains(symbol),
                "IBus backend is missing {symbol}"
            );
            assert!(
                fcitx5_backend.contains(symbol),
                "Fcitx5 backend is missing {symbol}"
            );
        }
    }
}
