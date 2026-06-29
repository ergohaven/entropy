#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmojiSection {
    SmileysAndEmotion,
    PeopleAndBody,
    AnimalsAndNature,
    FoodAndDrink,
    TravelAndPlaces,
    Activities,
    Objects,
    Symbols,
    Flags,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmojiSkinTone {
    #[default]
    Default,
    Light,
    MediumLight,
    Medium,
    MediumDark,
    Dark,
}

impl EmojiSection {
    pub const ALL: [EmojiSection; 9] = [
        EmojiSection::SmileysAndEmotion,
        EmojiSection::PeopleAndBody,
        EmojiSection::AnimalsAndNature,
        EmojiSection::FoodAndDrink,
        EmojiSection::TravelAndPlaces,
        EmojiSection::Activities,
        EmojiSection::Objects,
        EmojiSection::Symbols,
        EmojiSection::Flags,
    ];
}

impl EmojiSkinTone {
    pub fn modifier(self) -> Option<char> {
        match self {
            EmojiSkinTone::Default => None,
            EmojiSkinTone::Light => Some('\u{1F3FB}'),
            EmojiSkinTone::MediumLight => Some('\u{1F3FC}'),
            EmojiSkinTone::Medium => Some('\u{1F3FD}'),
            EmojiSkinTone::MediumDark => Some('\u{1F3FE}'),
            EmojiSkinTone::Dark => Some('\u{1F3FF}'),
        }
    }

    pub fn next(self) -> Self {
        match self {
            EmojiSkinTone::Default => EmojiSkinTone::Light,
            EmojiSkinTone::Light => EmojiSkinTone::MediumLight,
            EmojiSkinTone::MediumLight => EmojiSkinTone::Medium,
            EmojiSkinTone::Medium => EmojiSkinTone::MediumDark,
            EmojiSkinTone::MediumDark => EmojiSkinTone::Dark,
            EmojiSkinTone::Dark => EmojiSkinTone::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmojiEntry {
    pub emoji: &'static str,
    pub name: &'static str,
    pub section: EmojiSection,
    pub subgroup: &'static str,
    pub keywords: &'static [&'static str],
    pub supports_skin_tone: bool,
}

#[path = "emoji_catalog_data.rs"]
mod emoji_catalog_data;
pub use emoji_catalog_data::EMOJI_CATALOG;

pub fn emoji_sections() -> &'static [EmojiSection] {
    &EmojiSection::ALL
}

pub fn emoji_sequence(entry: &EmojiEntry, skin_tone: EmojiSkinTone) -> String {
    let mut sequence = entry.emoji.to_owned();
    if entry.supports_skin_tone {
        if let Some(modifier) = skin_tone.modifier() {
            sequence.push(modifier);
        }
    }
    sequence
}

pub fn filter_emoji(query: &str) -> Vec<&'static EmojiEntry> {
    let query = query.trim().to_lowercase();
    let mut results: Vec<&EmojiEntry> = EMOJI_CATALOG
        .iter()
        .filter(|entry| query.is_empty() || entry_matches_query(entry, &query))
        .collect();
    if !query.is_empty() {
        results.sort_by_key(|entry| match entry.emoji {
            emoji if emoji == query => 0,
            emoji if emoji.contains(query.as_str()) => 1,
            _ => 2,
        });
    }
    results
}

pub fn filter_emoji_section(query: &str, section: EmojiSection) -> Vec<&'static EmojiEntry> {
    filter_emoji(query)
        .into_iter()
        .filter(|entry| entry.section == section)
        .collect()
}

fn entry_matches_query(entry: &EmojiEntry, query: &str) -> bool {
    entry.emoji.contains(query)
        || entry.name.to_lowercase().contains(query)
        || entry.subgroup.replace('-', " ").contains(query)
        || entry.keywords.iter().any(|keyword| keyword.contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unicode_group_coverage() {
        assert!(EMOJI_CATALOG.len() >= 1_800);
        for section in emoji_sections() {
            assert!(
                EMOJI_CATALOG.iter().any(|entry| entry.section == *section),
                "catalog has no entries for {section:?}"
            );
        }
    }

    #[test]
    fn catalog_includes_unicode_emoji_beyond_the_demo_subset() {
        for (emoji, query) in [
            ("🫠", "melting"),
            ("🫨", "shaking"),
            ("🪿", "goose"),
            ("🩷", "pink heart"),
            ("🇺🇦", "ukraine"),
        ] {
            assert!(
                filter_emoji(query).iter().any(|entry| entry.emoji == emoji),
                "missing {emoji} for query {query}"
            );
        }
    }

    #[test]
    fn catalog_excludes_skin_tone_variant_rows() {
        assert!(!EMOJI_CATALOG.iter().any(|entry| {
            entry.name.contains("skin tone")
                || entry
                    .emoji
                    .chars()
                    .any(|char| (0x1F3FB..=0x1F3FF).contains(&(char as u32)))
        }));
    }

    #[test]
    fn search_matches_name_keywords_and_glyph() {
        let heart_results = filter_emoji("heart");
        assert!(heart_results.iter().any(|entry| entry.emoji == "❤️"));

        let laugh_results = filter_emoji("laugh");
        assert!(laugh_results.iter().any(|entry| entry.emoji == "😂"));

        let glyph_results = filter_emoji("🚀");
        assert_eq!(
            glyph_results.first().map(|entry| entry.name),
            Some("rocket")
        );
    }

    #[test]
    fn section_filter_uses_unicode_groups() {
        let smileys = filter_emoji_section("", EmojiSection::SmileysAndEmotion);
        assert!(smileys.iter().any(|entry| entry.emoji == "😂"));
        assert!(!smileys.iter().any(|entry| entry.emoji == "👍"));

        let people = filter_emoji_section("", EmojiSection::PeopleAndBody);
        assert!(people.iter().any(|entry| entry.emoji == "👍"));
        assert!(!people.iter().any(|entry| entry.emoji == "😂"));
    }

    #[test]
    fn section_order_starts_with_common_emoji() {
        assert_eq!(
            emoji_sections().first(),
            Some(&EmojiSection::SmileysAndEmotion)
        );
    }

    #[test]
    fn skin_tone_modifier_applies_only_to_supported_emoji() {
        let thumbs_up = EMOJI_CATALOG
            .iter()
            .find(|entry| entry.name == "thumbs up")
            .unwrap();
        assert_eq!(emoji_sequence(thumbs_up, EmojiSkinTone::Medium), "👍🏽");

        let rocket = EMOJI_CATALOG
            .iter()
            .find(|entry| entry.name == "rocket")
            .unwrap();
        assert_eq!(emoji_sequence(rocket, EmojiSkinTone::Medium), "🚀");
    }

    #[test]
    fn skin_tone_cycles_back_to_default() {
        let mut tone = EmojiSkinTone::Default;
        for _ in [
            EmojiSkinTone::Default,
            EmojiSkinTone::Light,
            EmojiSkinTone::MediumLight,
            EmojiSkinTone::Medium,
            EmojiSkinTone::MediumDark,
            EmojiSkinTone::Dark,
        ] {
            tone = tone.next();
        }
        assert_eq!(tone, EmojiSkinTone::Default);
    }
}
