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

pub const SMART_SYMBOLS: &[SmartSymbol] = &[
    // F13..F24
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
    SmartSymbol {
        trigger_keycode: KC_F13 + 8,
        symbol: '#',
        name: "Number sign",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 9,
        symbol: '@',
        name: "At sign",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 10,
        symbol: '№',
        name: "Numero sign",
    },
    SmartSymbol {
        trigger_keycode: KC_F13 + 11,
        symbol: '₽',
        name: "Ruble sign",
    },
    // Shift+F13..F24
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
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 8),
        symbol: '=',
        name: "Equals sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 9),
        symbol: '?',
        name: "Question mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 10),
        symbol: '|',
        name: "Vertical bar",
    },
    SmartSymbol {
        trigger_keycode: MOD_SHIFT | (KC_F13 + 11),
        symbol: '\\',
        name: "Backslash",
    },
    // Ctrl+F13..F24
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
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 8),
        symbol: '≠',
        name: "Not equal sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 9),
        symbol: '≈',
        name: "Almost equal sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 10),
        symbol: '✓',
        name: "Check mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | (KC_F13 + 11),
        symbol: '§',
        name: "Section sign",
    },
    // Alt+F13..F24
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
        symbol: '←',
        name: "Leftwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 8),
        symbol: '↑',
        name: "Upwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 9),
        symbol: '→',
        name: "Rightwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 10),
        symbol: '↓',
        name: "Downwards arrow",
    },
    SmartSymbol {
        trigger_keycode: MOD_ALT | (KC_F13 + 11),
        symbol: '↔',
        name: "Left right arrow",
    },
    // Ctrl+Alt+F13..F19
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
        name: "Cyrillic hard sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_ALT | (KC_F13 + 6),
        symbol: 'ё',
        name: "Cyrillic yo",
    },
    // Ctrl+Alt+Shift+F13..F19
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
    // Ctrl+Shift+F13..F24
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
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 8),
        symbol: '”',
        name: "Right double quotation mark",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 9),
        symbol: '™',
        name: "Trade mark sign",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 10),
        symbol: '~',
        name: "Tilde",
    },
    SmartSymbol {
        trigger_keycode: MOD_CTRL | MOD_SHIFT | (KC_F13 + 11),
        symbol: '_',
        name: "Underscore",
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
    fn arrow_symbols_use_unused_alt_function_key_slots() {
        let expected = [
            (MOD_ALT | (KC_F13 + 7), '←'),
            (MOD_ALT | (KC_F13 + 8), '↑'),
            (MOD_ALT | (KC_F13 + 9), '→'),
            (MOD_ALT | (KC_F13 + 10), '↓'),
            (MOD_ALT | (KC_F13 + 11), '↔'),
        ];

        for (keycode, symbol) in expected {
            assert_eq!(
                smart_symbol_for_keycode(keycode).map(|smart_symbol| smart_symbol.symbol),
                Some(symbol)
            );
        }
    }
}
