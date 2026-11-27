//! Shared US-English symbol positions used by legend generation and rendering.
//! These are displayed keycap legends, not an operating-system input engine.

pub const SYMBOLS: &[(u16, &str, &str)] = &[
    (0x001E, "1", "!"),
    (0x001F, "2", "@"),
    (0x0020, "3", "#"),
    (0x0021, "4", "$"),
    (0x0022, "5", "%"),
    (0x0023, "6", "^"),
    (0x0024, "7", "&"),
    (0x0025, "8", "*"),
    (0x0026, "9", "("),
    (0x0027, "0", ")"),
    (0x002D, "-", "_"),
    (0x002E, "=", "+"),
    (0x002F, "[", "{"),
    (0x0030, "]", "}"),
    (0x0031, "\\", "|"),
    (0x0032, "#", "~"),
    (0x0033, ";", ":"),
    (0x0034, "'", "\""),
    (0x0035, "`", "~"),
    (0x0036, ",", "<"),
    (0x0037, ".", ">"),
    (0x0038, "/", "?"),
    (0x0064, "\\", "|"),
];

pub fn symbols(usage: u16) -> Option<(&'static str, &'static str)> {
    SYMBOLS
        .iter()
        .find(|entry| entry.0 == usage)
        .map(|entry| (entry.1, entry.2))
}
