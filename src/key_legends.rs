//! Repository-owned keyboard legends shared by every Entropy display surface.
//! The build script validates TOML and embeds the catalog; no runtime files,
//! operating-system input state or firmware settings are read or changed here.
mod english;

use crate::i18n::{self, Language};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug)]
enum Kind {
    Pair,
    Symbol,
    Number,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    usage: u16,
    kind: Kind,
    normal: &'static str,
    shift: Option<&'static str>,
}

#[derive(Debug)]
struct Profile {
    id: &'static str,
    family_id: &'static str,
    variant_id: &'static str,
    variant_name_en: &'static str,
    variant_name_ru: &'static str,
    default_variant: bool,
    name_en: &'static str,
    name_ru: &'static str,
    short_name: &'static str,
    legacy_english_first: Option<&'static str>,
    legacy_secondary_first: Option<&'static str>,
    entries: &'static [Entry],
}

include!(concat!(env!("OUT_DIR"), "/key_legends_catalog.rs"));

impl Profile {
    fn entry(&self, usage: u16) -> Option<&Entry> {
        self.entries
            .binary_search_by_key(&usage, |entry| entry.usage)
            .ok()
            .map(|index| &self.entries[index])
    }

    fn name(&self, language: Language) -> &'static str {
        match language {
            Language::English => self.name_en,
            Language::Russian => self.name_ru,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegendOrder {
    #[default]
    EnglishFirst,
    SecondaryFirst,
}

/// Stable catalog selection, independent of the interface and input languages.
/// The ID is resolved to compiled storage, so existing copyable UI state and
/// picker cache keys do not need a second selection or a mutable registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyLegendLayout {
    id: &'static str,
    order: LegendOrder,
}

impl Default for KeyLegendLayout {
    fn default() -> Self {
        Self::English
    }
}

#[allow(non_upper_case_globals)]
impl KeyLegendLayout {
    pub const English: Self = Self {
        id: "en",
        order: LegendOrder::EnglishFirst,
    };
    // Compatibility names for existing internal callers and regression fixtures.
    // Additional profiles are discovered from TOML; they never need constants.
    pub const Russian: Self = Self {
        id: "ru",
        order: LegendOrder::EnglishFirst,
    };
    pub const RussianPrimary: Self = Self {
        id: "ru",
        order: LegendOrder::SecondaryFirst,
    };

    fn profile(self) -> Option<&'static Profile> {
        PROFILES.iter().find(|profile| profile.id == self.id)
    }

    pub fn all() -> Vec<Self> {
        std::iter::once(Self::English)
            .chain(PROFILES.iter().map(|profile| Self {
                id: profile.id,
                order: LegendOrder::EnglishFirst,
            }))
            .collect()
    }

    /// One entry per secondary family, represented by its declared default.
    /// English is the None option; no second persistent selection is introduced.
    pub fn secondary_layouts() -> Vec<Self> {
        std::iter::once(Self::English)
            .chain(
                PROFILES
                    .iter()
                    .filter(|profile| profile.default_variant)
                    .map(|profile| Self {
                        id: profile.id,
                        order: LegendOrder::EnglishFirst,
                    }),
            )
            .collect()
    }

    pub fn same_family(self, other: Self) -> bool {
        self.profile().map(|profile| profile.family_id)
            == other.profile().map(|profile| profile.family_id)
    }

    pub fn select_secondary(self, target: Self) -> Self {
        if self.same_family(target) {
            self
        } else {
            self.select_profile(target)
        }
    }

    pub fn variants(self) -> Vec<Self> {
        let Some(family) = self.profile().map(|profile| profile.family_id) else {
            return Vec::new();
        };
        PROFILES
            .iter()
            .filter(|profile| profile.family_id == family)
            .map(|profile| Self {
                id: profile.id,
                order: self.order,
            })
            .collect()
    }

    pub fn variant_id(self) -> Option<&'static str> {
        self.profile().map(|profile| profile.variant_id)
    }

    pub fn select_variant(self, target: Self) -> Self {
        if self.is_multilingual() && self.same_family(target) {
            self.select_profile(target)
        } else {
            self
        }
    }

    pub fn secondary_label(self, language: Language) -> String {
        self.profile()
            .map(|profile| capitalized(profile.name(language)))
            .unwrap_or_else(|| i18n::tr_catalog(language, "ui.key_legends_none").to_owned())
    }

    pub fn variant_label(self, language: Language) -> String {
        self.profile()
            .map(|profile| match language {
                Language::English => profile.variant_name_en,
                Language::Russian => profile.variant_name_ru,
            })
            .unwrap_or_else(|| i18n::tr_catalog(language, "ui.key_legends_none"))
            .to_owned()
    }

    pub fn all_with_orders() -> Vec<Self> {
        let mut selections = vec![Self::English];
        for selection in Self::all().into_iter().skip(1) {
            selections.extend([selection, selection.toggled_order()]);
        }
        selections
    }

    pub fn is_multilingual(self) -> bool {
        self.profile().is_some()
    }

    pub fn same_profile(self, other: Self) -> bool {
        self.id == other.id
    }

    pub fn select_profile(self, target: Self) -> Self {
        if target.is_multilingual() {
            Self {
                id: target.id,
                order: self.order,
            }
        } else {
            Self::English
        }
    }

    pub fn toggled_order(self) -> Self {
        if !self.is_multilingual() {
            return Self::English;
        }
        Self {
            id: self.id,
            order: match self.order {
                LegendOrder::EnglishFirst => LegendOrder::SecondaryFirst,
                LegendOrder::SecondaryFirst => LegendOrder::EnglishFirst,
            },
        }
    }

    fn profile_label(self, profile: &Profile, language: Language) -> String {
        if profile.default_variant {
            profile.name(language).to_owned()
        } else {
            i18n::tr_catalog_format(
                language,
                "ui.key_legends_variant_name",
                &[
                    ("layout", profile.name(language)),
                    ("variant", &self.variant_label(language)),
                ],
            )
        }
    }

    pub fn label(self, language: Language) -> String {
        let english = i18n::tr_catalog(language, "ui.key_legends_english");
        let Some(profile) = self.profile() else {
            return english.to_owned();
        };
        i18n::tr_catalog_format(
            language,
            "ui.key_legends_combined",
            &[
                ("english", english),
                ("layout", &self.profile_label(profile, language)),
            ],
        )
    }

    fn short_names(self, profile: &Profile) -> (&str, &str) {
        match self.order {
            LegendOrder::EnglishFirst => ("EN", profile.short_name),
            LegendOrder::SecondaryFirst => (profile.short_name, "EN"),
        }
    }

    pub fn order_label(self, language: Language) -> Option<String> {
        let profile = self.profile()?;
        let (first, second) = self.short_names(profile);
        Some(i18n::tr_catalog_format(
            language,
            "main_menu.key_legends_order",
            &[("first", first), ("second", second)],
        ))
    }

    pub fn export_label(self, language: Language) -> String {
        let Some(profile) = self.profile() else {
            // Preserve the export's language-independent English-only label.
            return "English".to_owned();
        };
        let name = capitalized(&self.profile_label(profile, language));
        let (first, _) = self.short_names(profile);
        i18n::tr_catalog_format(
            language,
            "ui.key_legends_export_order",
            &[("layout", &name), ("first", first)],
        )
    }

    fn from_id(id: &str, order: LegendOrder) -> Option<Self> {
        if id == "en" || id == "english" {
            return Some(Self::English);
        }
        PROFILES
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| Self {
                id: profile.id,
                order,
            })
    }

    fn from_legacy(value: &str) -> Option<Self> {
        if value == "english" {
            return Some(Self::English);
        }
        PROFILES
            .iter()
            .find_map(|profile| {
                let order = if profile.legacy_english_first == Some(value) {
                    LegendOrder::EnglishFirst
                } else if profile.legacy_secondary_first == Some(value) {
                    LegendOrder::SecondaryFirst
                } else {
                    return None;
                };
                Some(Self {
                    id: profile.id,
                    order,
                })
            })
            .or_else(|| Self::from_id(value, LegendOrder::EnglishFirst))
    }
}

impl Serialize for KeyLegendLayout {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("KeyLegendLayout", 2)?;
        state.serialize_field("layout", self.id)?;
        state.serialize_field("order", &self.order)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for KeyLegendLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Selection {
            layout: String,
            #[serde(default)]
            order: LegendOrder,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Saved {
            Legacy(String),
            Selection(Selection),
        }
        let saved = Saved::deserialize(deserializer)?;
        let (id, selection) = match saved {
            Saved::Legacy(id) => {
                let selection = Self::from_legacy(&id);
                (id, selection)
            }
            Saved::Selection(value) => {
                let selection = Self::from_id(&value.layout, value.order);
                (value.layout, selection)
            }
        };
        Ok(selection.unwrap_or_else(|| {
            // A profile removed from a later build must not invalidate all other
            // application settings. Never infer Russian from an unknown ID.
            log::warn!("Unavailable key-legend profile {id:?}; showing English");
            Self::English
        }))
    }
}

fn capitalized(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn ordered(english: &str, secondary: &str, selection: KeyLegendLayout, separator: &str) -> String {
    if english == secondary {
        english.to_owned()
    } else if selection.order == LegendOrder::SecondaryFirst {
        format!("{secondary}{separator}{english}")
    } else {
        format!("{english}{separator}{secondary}")
    }
}

/// Resolve an existing keycode caption through the chosen display profile.
/// Unmapped actions retain the existing English/custom/action label unchanged.
pub(crate) fn label(value: u16, fallback: &str, selection: KeyLegendLayout) -> Option<String> {
    let profile = selection.profile()?;
    // Preserve the existing explicit LSFT symbol path; do not reinterpret
    // compound actions, other modifiers or their emitted firmware keycodes.
    if value & 0xFF00 == 0x0200 {
        let usage = value & 0x00FF;
        let entry = profile.entry(usage)?;
        let (_, english_shift) = english::symbols(usage)?;
        return entry
            .shift
            .map(|shift| ordered(english_shift, shift, selection, "\n"));
    }
    let entry = profile.entry(value)?;
    Some(match entry.kind {
        Kind::Pair => ordered(fallback, entry.normal, selection, "\n"),
        Kind::Number => ordered(fallback, entry.normal, selection, "\n"),
        Kind::Symbol => {
            let (base, shift) = english::symbols(value)?;
            format!("{}\n{base}", ordered(shift, entry.normal, selection, "  "))
        }
    })
}

/// One shared number-row presentation for the canvas, picker and popups.
pub(crate) fn number_row_label(usage: u16, selection: KeyLegendLayout) -> Option<String> {
    if !(0x001E..=0x0027).contains(&usage) {
        return None;
    }
    let (base, shift) = english::symbols(usage)?;
    let entry = selection.profile().and_then(|profile| profile.entry(usage));
    let top = entry
        .and_then(|entry| entry.shift)
        .map(|secondary| ordered(shift, secondary, selection, "  "))
        .unwrap_or_else(|| shift.to_owned());
    let bottom = entry
        .map(|entry| ordered(base, entry.normal, selection, "  "))
        .unwrap_or_else(|| base.to_owned());
    Some(format!("{top}\n{bottom}"))
}

/// Read a plain number binding, without replacing the label of a tap/hold or
/// other composed action. This is presentation-only and emits no key events.
pub(crate) fn number_row_binding_label(
    binding: crate::keyboard::KeyBinding,
    selection: KeyLegendLayout,
) -> Option<String> {
    use crate::keyboard::KeyBinding;
    use rmk_types::action::{Action, KeyAction};
    use rmk_types::keycode::KeyCode;
    let usage = match binding {
        KeyBinding::Vial(value) => value,
        KeyBinding::Rmk(KeyAction::Single(Action::Key(KeyCode::Hid(key))))
        | KeyBinding::Rmk(KeyAction::Tap(Action::Key(KeyCode::Hid(key)))) => key as u16,
        KeyBinding::Rynk(action) => {
            return crate::rynk::native_v1::v1_label_action(action)
                .and_then(|action| number_row_binding_label(action.into(), selection));
        }
        _ => return None,
    };
    number_row_label(usage, selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_keep_profile_and_order() {
        for (saved, expected) in [
            ("english", KeyLegendLayout::English),
            ("russian", KeyLegendLayout::Russian),
            ("russian_primary", KeyLegendLayout::RussianPrimary),
        ] {
            let selection: KeyLegendLayout = serde_json::from_str(&format!("\"{saved}\"")).unwrap();
            assert_eq!(selection, expected);
            assert_eq!(
                serde_json::from_value::<KeyLegendLayout>(serde_json::to_value(selection).unwrap())
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn catalog_and_all_order_choices_are_data_driven() {
        assert_eq!(KeyLegendLayout::all().len(), PROFILES.len() + 1);
        assert_eq!(
            KeyLegendLayout::all_with_orders().len(),
            PROFILES.len() * 2 + 1
        );
        for profile in PROFILES {
            let selection =
                KeyLegendLayout::from_id(profile.id, LegendOrder::EnglishFirst).unwrap();
            assert_eq!(selection.toggled_order().toggled_order(), selection);
            assert!(selection.is_multilingual());
            assert!(selection.label(Language::English).contains(profile.name_en));
            assert!(selection
                .order_label(Language::English)
                .unwrap()
                .contains(profile.short_name));
        }
    }

    #[test]
    fn russian_labels_and_number_row_preserve_existing_presentation() {
        assert_eq!(
            label(0x0004, "A", KeyLegendLayout::Russian).as_deref(),
            Some("A\nФ")
        );
        assert_eq!(
            label(0x0004, "A", KeyLegendLayout::RussianPrimary).as_deref(),
            Some("Ф\nA")
        );
        assert_eq!(
            label(0x002F, "[", KeyLegendLayout::Russian).as_deref(),
            Some("{  Х\n[")
        );
        assert_eq!(
            label(0x021F, "@", KeyLegendLayout::Russian).as_deref(),
            Some("@\n\"")
        );
        assert_eq!(
            number_row_label(0x001F, KeyLegendLayout::Russian).as_deref(),
            Some("@  \"\n2")
        );
        assert_eq!(
            number_row_label(0x001F, KeyLegendLayout::RussianPrimary).as_deref(),
            Some("\"  @\n2")
        );
        assert!(label(0x00E0, "Ctrl", KeyLegendLayout::Russian).is_none());
    }

    #[test]
    fn profile_selection_preserves_order_and_plain_actions() {
        let selection = KeyLegendLayout::RussianPrimary;
        assert!(selection.same_profile(KeyLegendLayout::Russian));
        assert_eq!(
            selection.select_profile(KeyLegendLayout::Russian),
            selection
        );
        assert_eq!(
            selection.select_profile(KeyLegendLayout::English),
            KeyLegendLayout::English
        );
        assert_eq!(
            number_row_binding_label(crate::keyboard::KeyBinding::Vial(0x001F), selection),
            number_row_label(0x001F, selection)
        );
        assert!(
            number_row_binding_label(crate::keyboard::KeyBinding::Vial(0x211F), selection)
                .is_none()
        );
    }

    #[test]
    fn secondary_families_filter_variants_and_keep_one_stored_profile() {
        assert!(KeyLegendLayout::English.variants().is_empty());
        assert_eq!(
            KeyLegendLayout::English.select_variant(KeyLegendLayout::Russian),
            KeyLegendLayout::English
        );
        let choices = KeyLegendLayout::secondary_layouts();
        assert_eq!(
            choices
                .iter()
                .filter(|choice| choice.is_multilingual())
                .count(),
            PROFILES
                .iter()
                .filter(|profile| profile.default_variant)
                .count()
        );
        for family in choices
            .into_iter()
            .filter(|choice| choice.is_multilingual())
        {
            assert!(family.profile().unwrap().default_variant);
            let variants = family.variants();
            assert!(!variants.is_empty());
            for variant in variants {
                assert!(variant.same_family(family));
                let selected = family.toggled_order().select_variant(variant);
                assert_eq!(selected.id, variant.id);
                assert_eq!(selected.order, LegendOrder::SecondaryFirst);
                assert_eq!(selected.select_secondary(family), selected);
                let saved = serde_json::to_value(selected).unwrap();
                assert_eq!(saved.as_object().unwrap().len(), 2);
                assert_eq!(saved["layout"], variant.id);
                assert_eq!(
                    serde_json::from_value::<KeyLegendLayout>(saved).unwrap(),
                    selected
                );
            }
        }
        assert_eq!(
            KeyLegendLayout::Russian.variant_label(Language::English),
            "Standard"
        );
        assert_eq!(
            KeyLegendLayout::RussianPrimary.select_secondary(KeyLegendLayout::Russian),
            KeyLegendLayout::RussianPrimary
        );
    }

    #[test]
    fn unavailable_profile_falls_back_without_rejecting_settings() {
        let selection: KeyLegendLayout =
            serde_json::from_str(r#"{"layout":"missing-profile","order":"secondary_first"}"#)
                .unwrap();
        assert_eq!(selection, KeyLegendLayout::English);
    }
}
