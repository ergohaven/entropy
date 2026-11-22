use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use anyhow::{ensure, Context, Result};
use serde::Deserialize;

use crate::keycode::{printable_output_for_hid, shift_wrapped_base_keycode, KeyOutputLayout};

pub(crate) const EMPTY_LEGEND_SLOT: char = '\u{2007}';

pub(crate) const STANDARD_RUSSIAN: &str = "ru-standard";

const BUNDLED_LAYOUTS: &[&str] = &[
    include_str!("key_legends/ru-standard.toml"),
    include_str!("key_legends/ru-typewriter.toml"),
    include_str!("key_legends/fr-azerty-legacy.toml"),
];

static CATALOG: LazyLock<Vec<Layout>> = LazyLock::new(|| {
    load_catalog(BUNDLED_LAYOUTS).expect("bundled key legend catalog must be valid")
});

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum Glyph {
    Direct(String),
    Dead(DeadGlyph),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DeadGlyph {
    dead: String,
}

impl Glyph {
    fn text(&self) -> &str {
        match self {
            Self::Direct(text) | Self::Dead(DeadGlyph { dead: text }) => text,
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Direct(text) => text.clone(),
            Self::Dead(DeadGlyph { dead }) => format!("◌{dead}"),
        }
    }

    fn case_pair(&self, shifted: &Self) -> bool {
        let (Self::Direct(base), Self::Direct(shifted)) = (self, shifted) else {
            return false;
        };
        let mut base_chars = base.chars();
        let Some(letter) = base_chars.next().filter(|letter| letter.is_lowercase()) else {
            return false;
        };
        let mut uppercase = letter.to_uppercase();
        base_chars.next().is_none()
            && uppercase
                .next()
                .is_some_and(|upper| upper != letter && shifted.chars().eq(std::iter::once(upper)))
            && uppercase.next().is_none()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outputs {
    base: Option<Glyph>,
    shift: Option<Glyph>,
}

impl Outputs {
    fn folded_letter(&self) -> Option<&Glyph> {
        let shifted = self.shift.as_ref()?;
        self.base.as_ref()?.case_pair(shifted).then_some(shifted)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutFile {
    id: String,
    language: String,
    name: String,
    keys: BTreeMap<String, Outputs>,
}

#[derive(Debug)]
pub(crate) struct Layout {
    pub(crate) id: String,
    pub(crate) language: String,
    pub(crate) name: String,
    keys: BTreeMap<u16, Outputs>,
}

fn load_catalog(files: &[&str]) -> Result<Vec<Layout>> {
    let mut ids = BTreeSet::new();
    files
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let file: LayoutFile = toml::from_str(text)
                .with_context(|| format!("key legend catalog entry {index}"))?;
            ensure!(
                !file.id.is_empty()
                    && file
                        .id
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                    && !file.language.trim().is_empty()
                    && !file.name.trim().is_empty(),
                "invalid metadata for key legend layout {:?}",
                file.id
            );
            ensure!(
                ids.insert(file.id.clone()),
                "duplicate layout ID {}",
                file.id
            );
            let mut keys = BTreeMap::new();
            for (usage, outputs) in file.keys {
                ensure!(
                    usage.len() == 2
                        && usage
                            .bytes()
                            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
                    "{}: expected two lowercase hex digits for HID usage {usage:?}",
                    file.id
                );
                let hid = u16::from_str_radix(&usage, 16)
                    .with_context(|| format!("{}: invalid HID usage {usage:?}", file.id))?;
                ensure!(
                    printable_output_for_hid(hid, KeyOutputLayout::English).is_some(),
                    "{}: unsupported printable HID usage {usage}",
                    file.id
                );
                ensure!(
                    outputs.base.is_some() || outputs.shift.is_some(),
                    "{}: no outputs for {usage}",
                    file.id
                );
                for glyph in outputs.base.iter().chain(outputs.shift.iter()) {
                    let text = glyph.text();
                    ensure!(
                        !text.is_empty()
                            && text.trim() == text
                            && !text.chars().any(char::is_control),
                        "{}: invalid glyph at {usage}",
                        file.id
                    );
                }
                ensure!(
                    keys.insert(hid, outputs).is_none(),
                    "{}: duplicate HID usage {usage}",
                    file.id
                );
            }
            ensure!(!keys.is_empty(), "{}: empty key mapping", file.id);
            Ok(Layout {
                id: file.id,
                language: file.language,
                name: file.name,
                keys,
            })
        })
        .collect()
}

pub(crate) fn catalog() -> &'static [Layout] {
    &CATALOG
}

pub(crate) fn layout(id: &str) -> Option<&'static Layout> {
    catalog().iter().find(|variant| variant.id == id)
}

pub(crate) fn is_number_row(hid: u16) -> bool {
    (0x001E..=0x0027).contains(&hid)
}

fn joined(first: Option<&Glyph>, second: Option<&Glyph>, separator: &str) -> String {
    match (first, second) {
        (Some(a), Some(b)) if a != b => format!("{}{separator}{}", a.display(), b.display()),
        (Some(glyph), _) | (_, Some(glyph)) => glyph.display(),
        (None, None) => String::new(),
    }
}

pub(crate) fn caption(
    value: u16,
    layout: &Layout,
    secondary_first: bool,
    show_number_shift: bool,
) -> Option<String> {
    let explicit_shift = shift_wrapped_base_keycode(value);
    let hid = explicit_shift.unwrap_or(value);
    let secondary = layout.keys.get(&hid)?;
    let (base, shift) = printable_output_for_hid(hid, KeyOutputLayout::English)?;
    let english = Outputs {
        base: Some(Glyph::Direct(base.to_string())),
        shift: shift.map(|ch| Glyph::Direct(ch.to_string())),
    };
    let (first, second) = if secondary_first {
        (secondary, &english)
    } else {
        (&english, secondary)
    };

    if explicit_shift.is_some() {
        let label = joined(first.shift.as_ref(), second.shift.as_ref(), "\n");
        return (!label.is_empty()).then_some(label);
    }

    let mut first_letter = first.folded_letter();
    let mut second_letter = second.folded_letter();
    let first_display_base = first_letter.or(first.base.as_ref());
    let second_display_base = second_letter.or(second.base.as_ref());
    if (first.base == second.base) != (first_display_base == second_display_base) {
        // Case folding must preserve whether the base outputs are equal. For
        // example, i and dotless ı both uppercase to I.
        first_letter = None;
        second_letter = None;
    }
    if first_letter.is_some() && second_letter.is_some() {
        return Some(joined(first_letter, second_letter, "\n"));
    }
    if is_number_row(hid) && !show_number_shift {
        return Some(joined(first.base.as_ref(), second.base.as_ref(), "\n"));
    }

    // A case-folded letter shares the hint row with the other layout's
    // Shift symbol, as in the existing bilingual punctuation legends.
    if !is_number_row(hid) && first_letter.is_some() != second_letter.is_some() {
        let symbols = if first_letter.is_some() {
            second
        } else {
            first
        };
        if let (Some(base), Some(_)) = (&symbols.base, &symbols.shift) {
            let hint = joined(
                first_letter.or(first.shift.as_ref()),
                second_letter.or(second.shift.as_ref()),
                "  ",
            );
            return Some(format!("{hint}\n{}", base.display()));
        }
    }

    let first_shift = first.shift.as_ref().filter(|_| first_letter.is_none());
    let second_shift = second.shift.as_ref().filter(|_| second_letter.is_none());
    let shift_row = if first.base.is_some() && second.base.is_some() && first.base != second.base {
        // Leave a blank where a layout has no Shift output. Keep the other
        // hint on its side of the keycap.
        match (first_shift, second_shift) {
            (Some(glyph), None) => format!("{}  {EMPTY_LEGEND_SLOT}", glyph.display()),
            (None, Some(glyph)) => format!("{EMPTY_LEGEND_SLOT}  {}", glyph.display()),
            _ => joined(first_shift, second_shift, "  "),
        }
    } else {
        joined(first_shift, second_shift, "  ")
    };
    let base_row = joined(
        first_letter.or(first.base.as_ref()),
        second_letter.or(second.base.as_ref()),
        if shift_row.is_empty() { "\n" } else { "  " },
    );
    Some(match (shift_row.is_empty(), base_row.is_empty()) {
        (true, _) => base_row,
        (_, true) => shift_row,
        _ => format!("{shift_row}\n{base_row}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_captions_preserve_distinct_outputs_at_each_level() {
        let standard = layout(STANDARD_RUSSIAN).unwrap();
        let typewriter = layout("ru-typewriter").unwrap();
        let french = layout("fr-azerty-legacy").unwrap();
        assert_eq!(caption(0x04, standard, false, true).unwrap(), "A\nФ");
        assert_eq!(caption(0x04, standard, true, true).unwrap(), "Ф\nA");
        assert_eq!(caption(0x2e, standard, false, true).unwrap(), "+\n=");
        assert_eq!(caption(0x33, standard, false, true).unwrap(), ":  Ж\n;");
        assert_eq!(caption(0x38, standard, false, true).unwrap(), "?  ,\n/  .");
        assert_eq!(
            caption(0x1f, typewriter, false, true).unwrap(),
            "@  2\n2  -"
        );
        assert_eq!(caption(0x1f, typewriter, false, false).unwrap(), "2\n-");
        assert_eq!(caption(0x1f, french, false, true).unwrap(), "@  2\n2  é");
        assert_eq!(caption(0x04, french, false, true).unwrap(), "A\nQ");
        assert_eq!(caption(0x06, french, false, true).unwrap(), "C");
        assert_eq!(
            caption(0x35, french, false, true).unwrap(),
            "~  \u{2007}\n`  ²"
        );
        assert_eq!(
            caption(0x35, french, true, true).unwrap(),
            "\u{2007}  ~\n²  `"
        );
        assert_eq!(caption(0x10, french, false, true).unwrap(), "M  ?\n,");
        assert_eq!(caption(0x10, french, true, true).unwrap(), "?  M\n,");
        assert_eq!(caption(0x2f, french, false, true).unwrap(), "{  ◌¨\n[  ◌^");
        assert_eq!(caption(0x64, french, false, true).unwrap(), "|  >\n\\  <");
        for shift in [0x0200, 0x1200] {
            assert_eq!(
                caption(shift | 0x1f, typewriter, false, false).unwrap(),
                "@\n2"
            );
            assert_eq!(caption(shift | 0x35, french, false, false).unwrap(), "~");
        }
        assert!(caption(0x031f, typewriter, false, true).is_none());
    }

    #[test]
    fn case_folding_preserves_distinct_base_outputs_with_the_same_uppercase() {
        let file = r#"
id = "test"
language = "Test"
name = "Test"
[keys]
"0c" = { base = "ı", shift = "I" }
"04" = { base = "a", shift = "ä" }
"#;
        let layouts = load_catalog(&[file]).unwrap();
        let variant = &layouts[0];
        assert_eq!(caption(0x0c, variant, false, true).unwrap(), "I\ni  ı");
        assert_eq!(caption(0x04, variant, false, true).unwrap(), "A  ä\na");
    }

    #[test]
    fn catalog_rejects_duplicate_ids_duplicate_usages_and_invalid_glyphs() {
        let valid = BUNDLED_LAYOUTS[0];
        assert!(load_catalog(&[valid, valid]).is_err());
        assert!(load_catalog(&[&format!("{valid}\"04\" = {{ base = \"A\" }}\n")]).is_err());
        assert!(load_catalog(&[&format!("{valid}\"+4\" = {{ base = \"A\" }}\n")]).is_err());
        assert!(load_catalog(&[&format!("{valid}\"2a\" = {{ base = \"A\" }}\n")]).is_err());
        assert!(load_catalog(&[&format!("{valid}\"65\" = {{ base = \"\" }}\n")]).is_err());
        let unexpected_dead_field = r#""65" = { base = { dead = "^", typo = 1 } }"#;
        assert!(load_catalog(&[&format!("{valid}{unexpected_dead_field}\n")]).is_err());
    }

    #[test]
    fn glyphs_keep_dead_keys_and_nonordinary_case_pairs_distinct() {
        let direct = Glyph::Direct("^".into());
        let dead = Glyph::Dead(DeadGlyph { dead: "^".into() });
        assert_eq!(joined(Some(&direct), Some(&dead), "  "), "^  ◌^");
        assert!(!Glyph::Direct("ß".into()).case_pair(&Glyph::Direct("SS".into())));
    }
}
