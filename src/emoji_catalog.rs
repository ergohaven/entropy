#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmojiCategory {
    Smileys,
    People,
    Nature,
    Food,
    Travel,
    Activities,
    Objects,
    Symbols,
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

impl EmojiCategory {
    pub const ALL: [EmojiCategory; 8] = [
        EmojiCategory::Smileys,
        EmojiCategory::People,
        EmojiCategory::Nature,
        EmojiCategory::Food,
        EmojiCategory::Travel,
        EmojiCategory::Activities,
        EmojiCategory::Objects,
        EmojiCategory::Symbols,
    ];
}

impl EmojiSkinTone {
    pub const ALL: [EmojiSkinTone; 6] = [
        EmojiSkinTone::Default,
        EmojiSkinTone::Light,
        EmojiSkinTone::MediumLight,
        EmojiSkinTone::Medium,
        EmojiSkinTone::MediumDark,
        EmojiSkinTone::Dark,
    ];

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmojiEntry {
    pub emoji: &'static str,
    pub name: &'static str,
    pub category: EmojiCategory,
    pub keywords: &'static [&'static str],
    pub supports_skin_tone: bool,
}

macro_rules! emoji_entry {
    ($emoji:literal, $name:literal, $category:ident, [$($keyword:literal),* $(,)?]) => {
        EmojiEntry {
            emoji: $emoji,
            name: $name,
            category: EmojiCategory::$category,
            keywords: &[$($keyword),*],
            supports_skin_tone: false,
        }
    };
}

macro_rules! skin_emoji_entry {
    ($emoji:literal, $name:literal, $category:ident, [$($keyword:literal),* $(,)?]) => {
        EmojiEntry {
            emoji: $emoji,
            name: $name,
            category: EmojiCategory::$category,
            keywords: &[$($keyword),*],
            supports_skin_tone: true,
        }
    };
}

pub const EMOJI_CATALOG: &[EmojiEntry] = &[
    emoji_entry!("😀", "Grinning face", Smileys, ["happy", "smile"]),
    emoji_entry!(
        "😃",
        "Grinning face with big eyes",
        Smileys,
        ["happy", "smile"]
    ),
    emoji_entry!(
        "😄",
        "Grinning face with smiling eyes",
        Smileys,
        ["happy", "smile"]
    ),
    emoji_entry!("😁", "Beaming face", Smileys, ["happy", "grin"]),
    emoji_entry!("😆", "Squinting face", Smileys, ["laugh", "happy"]),
    emoji_entry!(
        "😅",
        "Grinning face with sweat",
        Smileys,
        ["relief", "laugh"]
    ),
    emoji_entry!("😂", "Face with tears of joy", Smileys, ["laugh", "tears"]),
    emoji_entry!("😉", "Winking face", Smileys, ["wink", "smile"]),
    emoji_entry!(
        "😊",
        "Smiling face with smiling eyes",
        Smileys,
        ["smile", "blush"]
    ),
    emoji_entry!(
        "😍",
        "Smiling face with heart eyes",
        Smileys,
        ["love", "heart"]
    ),
    skin_emoji_entry!("👋", "Waving hand", People, ["hello", "bye"]),
    skin_emoji_entry!("👍", "Thumbs up", People, ["yes", "approve"]),
    skin_emoji_entry!("👎", "Thumbs down", People, ["no", "disapprove"]),
    skin_emoji_entry!("🙏", "Folded hands", People, ["please", "thanks"]),
    skin_emoji_entry!("👏", "Clapping hands", People, ["applause", "clap"]),
    skin_emoji_entry!("💪", "Flexed biceps", People, ["strong", "power"]),
    emoji_entry!("👀", "Eyes", People, ["look", "watch"]),
    emoji_entry!("🐶", "Dog face", Nature, ["pet", "animal"]),
    emoji_entry!("🐱", "Cat face", Nature, ["pet", "animal"]),
    emoji_entry!("🐻", "Bear", Nature, ["animal", "wild"]),
    emoji_entry!("🐼", "Panda", Nature, ["animal", "wild"]),
    emoji_entry!("🐸", "Frog", Nature, ["animal", "green"]),
    emoji_entry!("🌱", "Seedling", Nature, ["plant", "grow"]),
    emoji_entry!("🌲", "Evergreen tree", Nature, ["tree", "forest"]),
    emoji_entry!("🌵", "Cactus", Nature, ["plant", "desert"]),
    emoji_entry!("🌻", "Sunflower", Nature, ["flower", "sun"]),
    emoji_entry!("🌙", "Crescent moon", Nature, ["night", "moon"]),
    emoji_entry!("⭐", "Star", Nature, ["favorite", "night"]),
    emoji_entry!("🍎", "Red apple", Food, ["fruit", "apple"]),
    emoji_entry!("🍌", "Banana", Food, ["fruit", "banana"]),
    emoji_entry!("🍓", "Strawberry", Food, ["fruit", "berry"]),
    emoji_entry!("🍒", "Cherries", Food, ["fruit", "cherry"]),
    emoji_entry!("🍕", "Pizza", Food, ["food", "slice"]),
    emoji_entry!("🍔", "Hamburger", Food, ["food", "burger"]),
    emoji_entry!("🍣", "Sushi", Food, ["food", "fish"]),
    emoji_entry!("🍜", "Steaming bowl", Food, ["noodles", "ramen"]),
    emoji_entry!("🍰", "Shortcake", Food, ["dessert", "cake"]),
    emoji_entry!("☕", "Hot beverage", Food, ["coffee", "tea"]),
    emoji_entry!("🚗", "Car", Travel, ["auto", "drive"]),
    emoji_entry!("🚕", "Taxi", Travel, ["cab", "drive"]),
    emoji_entry!("🚌", "Bus", Travel, ["transport", "public"]),
    emoji_entry!("🚆", "Train", Travel, ["rail", "transport"]),
    emoji_entry!("✈️", "Airplane", Travel, ["flight", "travel"]),
    emoji_entry!("🚀", "Rocket", Travel, ["launch", "space"]),
    emoji_entry!("⛵", "Sailboat", Travel, ["boat", "sea"]),
    emoji_entry!("🏠", "House", Travel, ["home", "building"]),
    emoji_entry!("🏢", "Office building", Travel, ["work", "building"]),
    emoji_entry!("🏝️", "Desert island", Travel, ["island", "vacation"]),
    emoji_entry!("⚽", "Soccer ball", Activities, ["sport", "football"]),
    emoji_entry!("🏀", "Basketball", Activities, ["sport", "ball"]),
    emoji_entry!("🎾", "Tennis", Activities, ["sport", "ball"]),
    emoji_entry!("🎮", "Video game", Activities, ["game", "controller"]),
    emoji_entry!("🎲", "Game die", Activities, ["dice", "random"]),
    emoji_entry!("🎯", "Bullseye", Activities, ["target", "goal"]),
    emoji_entry!("🎧", "Headphones", Activities, ["music", "audio"]),
    emoji_entry!("🎹", "Musical keyboard", Activities, ["music", "piano"]),
    emoji_entry!("🎨", "Artist palette", Activities, ["art", "paint"]),
    emoji_entry!("🎬", "Clapper board", Activities, ["movie", "video"]),
    emoji_entry!("📚", "Books", Activities, ["read", "study"]),
    emoji_entry!("💡", "Light bulb", Objects, ["idea", "light"]),
    emoji_entry!("🔑", "Key", Objects, ["lock", "secret"]),
    emoji_entry!("🔒", "Locked", Objects, ["secure", "lock"]),
    emoji_entry!("🔔", "Bell", Objects, ["alert", "notification"]),
    emoji_entry!("📌", "Pushpin", Objects, ["pin", "note"]),
    emoji_entry!("✂️", "Scissors", Objects, ["cut", "tool"]),
    emoji_entry!("🖊️", "Pen", Objects, ["write", "edit"]),
    emoji_entry!("📱", "Mobile phone", Objects, ["phone", "device"]),
    emoji_entry!("💻", "Laptop", Objects, ["computer", "device"]),
    emoji_entry!("🖥️", "Desktop computer", Objects, ["computer", "screen"]),
    emoji_entry!("📦", "Package", Objects, ["box", "ship"]),
    emoji_entry!("❤️", "Red heart", Symbols, ["heart", "love"]),
    emoji_entry!("💙", "Blue heart", Symbols, ["heart", "love"]),
    emoji_entry!("💚", "Green heart", Symbols, ["heart", "love"]),
    emoji_entry!("💛", "Yellow heart", Symbols, ["heart", "love"]),
    emoji_entry!("💜", "Purple heart", Symbols, ["heart", "love"]),
    emoji_entry!("✅", "Check mark button", Symbols, ["check", "done"]),
    emoji_entry!("❌", "Cross mark", Symbols, ["x", "close"]),
    emoji_entry!("⚠️", "Warning", Symbols, ["alert", "caution"]),
    emoji_entry!("➡️", "Right arrow", Symbols, ["arrow", "right"]),
    emoji_entry!("⬇️", "Down arrow", Symbols, ["arrow", "down"]),
    emoji_entry!("⬆️", "Up arrow", Symbols, ["arrow", "up"]),
    emoji_entry!("♻️", "Recycling symbol", Symbols, ["recycle", "green"]),
];

pub fn emoji_categories() -> &'static [EmojiCategory] {
    &EmojiCategory::ALL
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

pub fn filter_emoji(query: &str, category: Option<EmojiCategory>) -> Vec<&'static EmojiEntry> {
    let query = query.trim().to_lowercase();
    EMOJI_CATALOG
        .iter()
        .filter(|entry| category.is_none_or(|category| entry.category == category))
        .filter(|entry| query.is_empty() || entry_matches_query(entry, &query))
        .collect()
}

fn entry_matches_query(entry: &EmojiEntry, query: &str) -> bool {
    entry.emoji.contains(query)
        || entry.name.to_lowercase().contains(query)
        || entry
            .keywords
            .iter()
            .any(|keyword| keyword.to_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_broad_emoji_coverage() {
        assert!(EMOJI_CATALOG.len() >= 80);
        for category in [
            EmojiCategory::Smileys,
            EmojiCategory::People,
            EmojiCategory::Nature,
            EmojiCategory::Food,
            EmojiCategory::Travel,
            EmojiCategory::Activities,
            EmojiCategory::Objects,
            EmojiCategory::Symbols,
        ] {
            assert!(
                emoji_categories().contains(&category),
                "missing category {category:?}"
            );
            assert!(
                EMOJI_CATALOG.iter().any(|entry| entry.category == category),
                "catalog has no entries for {category:?}"
            );
        }
    }

    #[test]
    fn search_matches_name_keywords_and_glyph() {
        let heart_results = filter_emoji("heart", None);
        assert!(heart_results.iter().any(|entry| entry.emoji == "❤️"));

        let laugh_results = filter_emoji("laugh", None);
        assert!(laugh_results.iter().any(|entry| entry.emoji == "😂"));

        let glyph_results = filter_emoji("🚀", None);
        assert_eq!(
            glyph_results.first().map(|entry| entry.name),
            Some("Rocket")
        );
    }

    #[test]
    fn category_filter_limits_results() {
        let results = filter_emoji("heart", Some(EmojiCategory::Symbols));

        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|entry| entry.category == EmojiCategory::Symbols));
    }

    #[test]
    fn skin_tone_modifier_applies_only_to_supported_emoji() {
        let thumbs_up = EMOJI_CATALOG
            .iter()
            .find(|entry| entry.name == "Thumbs up")
            .unwrap();
        assert_eq!(emoji_sequence(thumbs_up, EmojiSkinTone::Medium), "👍🏽");

        let rocket = EMOJI_CATALOG
            .iter()
            .find(|entry| entry.name == "Rocket")
            .unwrap();
        assert_eq!(emoji_sequence(rocket, EmojiSkinTone::Medium), "🚀");
    }
}
