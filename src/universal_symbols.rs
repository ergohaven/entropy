use rmk_types::action::{Action, KeyAction};

pub(crate) const USER_TOGGLE: u8 = 0x80;
pub(crate) const USER_SYNC: u8 = 0x81;
pub(crate) const USER_SET_ENGLISH: u8 = 0x82;
pub(crate) const USER_SET_RUSSIAN: u8 = 0x83;
pub(crate) const USER_TOGGLE_MACOS: u8 = 0x84;
pub(crate) const USER_SYMBOL_START: u8 = 0x90;
pub(crate) const USER_RUSSIAN_LETTER_START: u8 = 0xB0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UniversalSymbolControl {
    pub(crate) user_id: u8,
    pub(crate) label: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UniversalSymbol {
    pub(crate) user_id: u8,
    pub(crate) symbol: char,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UniversalRussianLetter {
    pub(crate) user_id: u8,
    pub(crate) letter: char,
}

pub(crate) const CONTROLS: &[UniversalSymbolControl] = &[
    UniversalSymbolControl {
        user_id: USER_TOGGLE,
        label: "Lang\nToggle",
        name: "Universal Symbols: toggle English/Russian layout",
    },
    UniversalSymbolControl {
        user_id: USER_SYNC,
        label: "Lang\nSync",
        name: "Universal Symbols: sync firmware layout state without changing the OS",
    },
    UniversalSymbolControl {
        user_id: USER_SET_ENGLISH,
        label: "Lang\nEN",
        name: "Universal Symbols: switch to English",
    },
    UniversalSymbolControl {
        user_id: USER_SET_RUSSIAN,
        label: "Lang\nRU",
        name: "Universal Symbols: switch to Russian",
    },
    UniversalSymbolControl {
        user_id: USER_TOGGLE_MACOS,
        label: "PC /\nmacOS",
        name: "Universal Symbols: toggle PC/macOS Russian-layout mappings",
    },
];

pub(crate) const SYMBOLS: &[UniversalSymbol] = &[
    symbol(0, '.', "Full stop"),
    symbol(1, ',', "Comma"),
    symbol(2, ';', "Semicolon"),
    symbol(3, ':', "Colon"),
    symbol(4, '!', "Exclamation mark"),
    symbol(5, '?', "Question mark"),
    symbol(6, '/', "Slash"),
    symbol(7, '`', "Grave accent"),
    symbol(8, '~', "Tilde"),
    symbol(9, '\'', "Apostrophe"),
    symbol(10, '"', "Quotation mark"),
    symbol(11, '(', "Left parenthesis"),
    symbol(12, ')', "Right parenthesis"),
    symbol(13, '[', "Left bracket"),
    symbol(14, ']', "Right bracket"),
    symbol(15, '{', "Left brace"),
    symbol(16, '}', "Right brace"),
    symbol(17, '<', "Less-than sign"),
    symbol(18, '>', "Greater-than sign"),
    symbol(19, '-', "Hyphen-minus"),
    symbol(20, '+', "Plus sign"),
    symbol(21, '*', "Asterisk"),
    symbol(22, '=', "Equals sign"),
    symbol(23, '#', "Number sign"),
    symbol(24, '@', "At sign"),
    symbol(25, '$', "Dollar sign"),
    symbol(26, '%', "Percent sign"),
    symbol(27, '^', "Caret"),
    symbol(28, '&', "Ampersand"),
    symbol(29, '|', "Vertical bar"),
    symbol(30, '\\', "Backslash"),
    symbol(31, '_', "Underscore"),
];

pub(crate) const RUSSIAN_LETTERS: &[UniversalRussianLetter] = &[
    russian_letter(0, 'х'),
    russian_letter(1, 'б'),
    russian_letter(2, 'ю'),
    russian_letter(3, 'ъ'),
];

const fn symbol(offset: u8, symbol: char, _name: &'static str) -> UniversalSymbol {
    UniversalSymbol {
        user_id: USER_SYMBOL_START + offset,
        symbol,
    }
}

const fn russian_letter(offset: u8, letter: char) -> UniversalRussianLetter {
    UniversalRussianLetter {
        user_id: USER_RUSSIAN_LETTER_START + offset,
        letter,
    }
}

pub(crate) fn binding(user_id: u8) -> crate::keyboard::KeyBinding {
    KeyAction::Single(Action::User(user_id)).into()
}

pub(crate) fn user_id(action: KeyAction) -> Option<u8> {
    let KeyAction::Single(Action::User(user_id)) = action else {
        return None;
    };
    CONTROLS
        .iter()
        .any(|control| control.user_id == user_id)
        .then_some(user_id)
        .or_else(|| {
            SYMBOLS
                .iter()
                .any(|symbol| symbol.user_id == user_id)
                .then_some(user_id)
        })
        .or_else(|| {
            RUSSIAN_LETTERS
                .iter()
                .any(|letter| letter.user_id == user_id)
                .then_some(user_id)
        })
}

pub(crate) fn russian_letter_user_id(action: KeyAction) -> Option<u8> {
    let KeyAction::Single(Action::User(user_id)) = action else {
        return None;
    };
    RUSSIAN_LETTERS
        .iter()
        .any(|letter| letter.user_id == user_id)
        .then_some(user_id)
}

pub(crate) fn label(action: KeyAction) -> Option<String> {
    let user_id = user_id(action)?;
    CONTROLS
        .iter()
        .find(|control| control.user_id == user_id)
        .map(|control| control.label.to_owned())
        .or_else(|| {
            SYMBOLS
                .iter()
                .find(|symbol| symbol.user_id == user_id)
                .map(|symbol| symbol.symbol.to_string())
        })
        .or_else(|| {
            RUSSIAN_LETTERS
                .iter()
                .find(|letter| letter.user_id == user_id)
                .map(|letter| letter.letter.to_string())
        })
}

pub(crate) fn tooltip(action: KeyAction) -> Option<String> {
    let user_id = user_id(action)?;
    CONTROLS
        .iter()
        .find(|control| control.user_id == user_id)
        .map(|control| control.name.to_owned())
        .or_else(|| {
            SYMBOLS
                .iter()
                .find(|symbol| symbol.user_id == user_id)
                .map(|symbol| {
                    format!(
                        "Universal Symbols: firmware types {} in English and Russian layouts",
                        symbol.symbol
                    )
                })
        })
        .or_else(|| {
            RUSSIAN_LETTERS
                .iter()
                .find(|letter| letter.user_id == user_id)
                .map(|letter| {
                    format!(
                        "Universal Symbols: firmware types {} in the Russian layout; hold Shift for uppercase",
                        letter.letter
                    )
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_table_has_stable_unique_ids_and_only_ascii_punctuation() {
        assert_eq!(SYMBOLS.len(), 32);
        let ids = SYMBOLS
            .iter()
            .map(|entry| entry.user_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, (0x90..0xB0).collect::<Vec<_>>());
        assert!(SYMBOLS
            .iter()
            .all(|entry| entry.symbol.is_ascii_punctuation()));
    }

    #[test]
    fn russian_letter_table_has_stable_ids() {
        assert_eq!(RUSSIAN_LETTERS.len(), 4);
        assert_eq!(
            RUSSIAN_LETTERS
                .iter()
                .map(|entry| (entry.user_id, entry.letter))
                .collect::<Vec<_>>(),
            vec![(0xB0, 'х'), (0xB1, 'б'), (0xB2, 'ю'), (0xB3, 'ъ')]
        );
    }

    #[test]
    fn labels_decode_firmware_user_actions() {
        assert_eq!(
            label(binding(0x90).rmk_action().unwrap()).as_deref(),
            Some(".")
        );
        assert_eq!(
            label(binding(USER_SYNC).rmk_action().unwrap()).as_deref(),
            Some("Lang\nSync")
        );
        assert_eq!(
            label(binding(USER_RUSSIAN_LETTER_START).rmk_action().unwrap()).as_deref(),
            Some("х")
        );
        assert!(label(KeyAction::Single(Action::User(0x70))).is_none());
    }
}
