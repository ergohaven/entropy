//! Compile repository-owned TOML legend profiles into a validated static catalog.
//! No profile is discovered or parsed from the user's machine at runtime.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

#[path = "key_legends/english.rs"]
mod english;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileProfile {
    schema_version: u32,
    id: String,
    family_id: String,
    variant_id: String,
    variant_name_en: String,
    variant_name_ru: String,
    default_variant: bool,
    name_en: String,
    name_ru: String,
    short_name: String,
    #[serde(default)]
    legacy: LegacyAliases,
    keys: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAliases {
    english_first: Option<String>,
    secondary_first: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Pair,
    Symbol,
    Number,
}

impl Kind {
    fn rust_name(self) -> &'static str {
        match self {
            Self::Pair => "Pair",
            Self::Symbol => "Symbol",
            Self::Number => "Number",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    kind: Kind,
    normal: String,
    shift: Option<String>,
}

fn text(value: &str, max: usize, context: &str) {
    assert!(
        !value.trim().is_empty()
            && value.chars().count() <= max
            && !value.chars().any(char::is_control),
        "{context}: expected nonempty display text, at most {max} characters and no controls"
    );
}

fn identifier(value: &str, context: &str) {
    assert!(
        !value.is_empty()
            && value.len() <= 64
            && value.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'),
        "{context}: expected a stable lowercase ASCII identifier"
    );
}

pub fn generate() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let directory = root.join("key_legends");
    println!("cargo:rerun-if-changed=key_legends");
    println!("cargo:rerun-if-changed=src/key_legends_build.rs");
    println!("cargo:rerun-if-changed=src/key_legends/english.rs");
    let mut paths: Vec<_> = std::fs::read_dir(&directory)
        .expect("key_legends directory is missing")
        .map(|entry| entry.expect("cannot enumerate key_legends").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    let mut identifiers = BTreeSet::from(["en".to_owned(), "english".to_owned()]);
    let mut parsed = Vec::new();
    for path in paths {
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_file(),
            "{}: profiles must be regular repository files",
            path.display()
        );
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            bytes.len() <= 256 * 1024,
            "{}: profile is too large",
            path.display()
        );
        let source = std::str::from_utf8(&bytes).expect("legend profiles must be UTF-8");
        let profile: FileProfile =
            toml::from_str(source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(
            profile.schema_version,
            2,
            "{}: unsupported schema",
            path.display()
        );
        identifier(&profile.id, "profile id");
        assert_eq!(
            path.file_stem().and_then(|s| s.to_str()),
            Some(profile.id.as_str()),
            "profile filename must equal its stable id"
        );
        assert!(
            identifiers.insert(profile.id.clone()),
            "duplicate or reserved profile id: {}",
            profile.id
        );
        identifier(&profile.family_id, "family_id");
        identifier(&profile.variant_id, "variant_id");
        assert!(
            !matches!(profile.family_id.as_str(), "en" | "english"),
            "English is the fixed primary layout, not a secondary family"
        );
        text(&profile.variant_name_en, 80, "variant_name_en");
        text(&profile.variant_name_ru, 80, "variant_name_ru");
        text(&profile.name_en, 80, "name_en");
        text(&profile.name_ru, 80, "name_ru");
        text(&profile.short_name, 12, "short_name");
        for alias in [
            &profile.legacy.english_first,
            &profile.legacy.secondary_first,
        ]
        .into_iter()
        .flatten()
        {
            identifier(alias, "legacy alias");
            assert!(
                identifiers.insert(alias.clone()),
                "duplicate or reserved legacy alias: {alias}"
            );
        }
        assert!(
            !profile.keys.is_empty() && profile.keys.len() <= 256,
            "{}: expected 1..256 key entries",
            path.display()
        );
        let mut entries = BTreeSet::new();
        for (token, entry) in profile.keys.iter() {
            assert!(
                token.len() == 6
                    && token.starts_with("0x")
                    && token.as_bytes()[2..]
                        .iter()
                        .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b)),
                "{}: usage must be written exactly as 0x0004 (four uppercase hex digits)",
                token
            );
            let usage = u16::from_str_radix(&token[2..], 16).unwrap();
            assert!((0x0004..=0x00A4).contains(&usage), "{token}: expected a basic Keyboard/Keypad HID usage, not an action or modifier wrapper");
            text(&entry.normal, 64, token);
            if let Some(shift) = &entry.shift {
                text(shift, 64, token);
            }
            match entry.kind {
                Kind::Pair => {}
                Kind::Symbol => {
                    assert!(
                        english::symbols(usage).is_some(),
                        "{token}: symbol style needs an English symbol position"
                    );
                    assert!(
                        entry.shift.is_some(),
                        "{token}: symbol style requires shift"
                    );
                }
                Kind::Number => {
                    assert!(
                        (0x001E..=0x0027).contains(&usage),
                        "{token}: number style is only for the number row"
                    );
                    assert!(
                        entry.shift.is_some(),
                        "{token}: number style requires shift"
                    );
                }
            }
            assert!(entries.insert(usage), "duplicate HID usage: {token}");
        }
        parsed.push((profile.id.clone(), profile, path));
    }
    // Check IDs and legacy aliases across the complete catalog before emitting it.
    parsed.sort_by(|a, b| {
        (&a.1.family_id, !a.1.default_variant, &a.1.variant_id, &a.0).cmp(&(
            &b.1.family_id,
            !b.1.default_variant,
            &b.1.variant_id,
            &b.0,
        ))
    });
    let mut families = BTreeMap::new();
    let mut variants = BTreeSet::new();
    for (_, profile, _) in &parsed {
        let family = families.entry(profile.family_id.as_str()).or_insert((
            profile.name_en.as_str(),
            profile.name_ru.as_str(),
            profile.short_name.as_str(),
            0usize,
        ));
        assert_eq!(
            (family.0, family.1, family.2),
            (
                profile.name_en.as_str(),
                profile.name_ru.as_str(),
                profile.short_name.as_str()
            ),
            "{}: family names and short_name must agree across variants",
            profile.family_id
        );
        family.3 += usize::from(profile.default_variant);
        assert!(
            variants.insert((profile.family_id.as_str(), profile.variant_id.as_str())),
            "{}/{}: duplicate variant",
            profile.family_id,
            profile.variant_id
        );
    }
    for (id, family) in families {
        assert_eq!(
            family.3, 1,
            "{id}: each secondary family requires exactly one default variant"
        );
    }
    let mut output = String::from("// Generated from repository key_legends/*.toml; do not edit.\nstatic PROFILES: &[Profile] = &[\n");
    for (_, profile, path) in &parsed {
        let legacy = |value: &Option<String>| {
            value
                .as_ref()
                .map(|s| format!("Some({s:?})"))
                .unwrap_or_else(|| "None".into())
        };
        writeln!(output, "    Profile {{ id: {:?}, family_id: {:?}, variant_id: {:?}, variant_name_en: {:?}, variant_name_ru: {:?}, default_variant: {}, name_en: {:?}, name_ru: {:?}, short_name: {:?}, legacy_english_first: {}, legacy_secondary_first: {}, entries: &[", profile.id, profile.family_id, profile.variant_id, profile.variant_name_en, profile.variant_name_ru, profile.default_variant, profile.name_en, profile.name_ru, profile.short_name, legacy(&profile.legacy.english_first), legacy(&profile.legacy.secondary_first)).unwrap();
        let mut entries: Vec<_> = profile
            .keys
            .iter()
            .map(|(token, entry)| (u16::from_str_radix(&token[2..], 16).unwrap(), entry))
            .collect();
        entries.sort_by_key(|entry| entry.0);
        for (usage, entry) in entries {
            let shift = entry
                .shift
                .as_ref()
                .map(|s| format!("Some({s:?})"))
                .unwrap_or_else(|| "None".into());
            writeln!(output, "        Entry {{ usage: 0x{usage:04X}, kind: Kind::{}, normal: {:?}, shift: {} }},", entry.kind.rust_name(), entry.normal, shift).unwrap();
        }
        writeln!(output, "    ] }},").unwrap();
        let relative = path.strip_prefix(&root).unwrap();
        println!("cargo:rerun-if-changed={}", relative.display());
    }
    output.push_str("];\n");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(Path::new(&out).join("key_legends_catalog.rs"), output).unwrap();
}
