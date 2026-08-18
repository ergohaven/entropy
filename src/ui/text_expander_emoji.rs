use super::*;

const EMOJI_GROUP_ICONS: [&str; 10] = ["★", "😀", "👋", "🐻", "🍎", "🚗", "⚽", "💡", "♥", "⚑"];
const EMOJI_GROUP_KEYS: [&str; 10] = [
    "text_expander.emoji_group_all",
    "text_expander.emoji_group_smileys",
    "text_expander.emoji_group_people",
    "text_expander.emoji_group_animals",
    "text_expander.emoji_group_food",
    "text_expander.emoji_group_travel",
    "text_expander.emoji_group_activities",
    "text_expander.emoji_group_objects",
    "text_expander.emoji_group_symbols",
    "text_expander.emoji_group_flags",
];

pub(super) fn show_text_expander_emoji_popup(
    ui: &mut egui::Ui,
    popup_id: egui::Id,
    anchor: &egui::Response,
    search: &mut String,
    selected_group: &mut usize,
    lang: crate::i18n::Language,
    scale: f32,
) -> Option<&'static str> {
    let popup_width = 344.0 * scale;
    let result = crate::ui_style::popup_below_widget_with_width(
        ui,
        popup_id,
        anchor,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        popup_width,
        |ui| {
            ui.set_min_width(popup_width);
            ui.set_max_width(popup_width);
            ui.spacing_mut().item_spacing = egui::vec2(6.0 * scale, 6.0 * scale);

            crate::ui_style::modern_text_field_sized(
                ui,
                popup_id.with("search"),
                search,
                popup_width,
                32.0 * scale,
                crate::i18n::tr_catalog(lang, "text_expander.emoji_search_hint"),
                64,
                egui::Align::Min,
            );

            let group_labels = EMOJI_GROUP_ICONS
                .iter()
                .map(|label| (*label).to_owned())
                .collect::<Vec<_>>();
            if let Some(group) = crate::ui_style::settings_segmented_control(
                ui,
                popup_id.with("groups"),
                &group_labels,
                (*selected_group).min(group_labels.len() - 1),
                egui::vec2(popup_width, 34.0 * scale),
            ) {
                *selected_group = group;
            }

            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    lang,
                    EMOJI_GROUP_KEYS[(*selected_group).min(EMOJI_GROUP_KEYS.len() - 1)],
                ))
                .size(11.0 * scale)
                .color(crate::ui_style::muted_text(ui.visuals().dark_mode)),
            );

            let query = search.trim().to_lowercase();
            let group = emoji_group(*selected_group);
            let matches = emojis::iter()
                .filter(|emoji| group.is_none_or(|group| emoji.group() == group))
                .filter(|emoji| emoji_matches(emoji, &query))
                .collect::<Vec<_>>();
            let mut picked = None;
            egui::ScrollArea::vertical()
                .id_salt(popup_id.with("results"))
                .max_height(244.0 * scale)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_min_width(popup_width);
                    if matches.is_empty() {
                        ui.add_sized(
                            egui::vec2(popup_width, 44.0 * scale),
                            egui::Label::new(
                                RichText::new(crate::i18n::tr_catalog(
                                    lang,
                                    "text_expander.emoji_no_results",
                                ))
                                .size(12.0 * scale)
                                .color(crate::ui_style::muted_text(ui.visuals().dark_mode)),
                            )
                            .halign(egui::Align::Center),
                        );
                        return;
                    }

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(5.0 * scale, 5.0 * scale);
                        for emoji in matches {
                            let response = crate::ui_style::modern_button_with_font(
                                ui,
                                emoji.as_str(),
                                egui::vec2(37.0 * scale, 37.0 * scale),
                                24.0 * scale,
                                true,
                            )
                            .on_hover_text(emoji.name());
                            if response.clicked() {
                                picked = Some(emoji.as_str());
                            }
                        }
                    });
                });
            picked
        },
    )
    .flatten();

    if result.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    result
}

fn emoji_group(index: usize) -> Option<emojis::Group> {
    match index {
        0 => None,
        1 => Some(emojis::Group::SmileysAndEmotion),
        2 => Some(emojis::Group::PeopleAndBody),
        3 => Some(emojis::Group::AnimalsAndNature),
        4 => Some(emojis::Group::FoodAndDrink),
        5 => Some(emojis::Group::TravelAndPlaces),
        6 => Some(emojis::Group::Activities),
        7 => Some(emojis::Group::Objects),
        8 => Some(emojis::Group::Symbols),
        9 => Some(emojis::Group::Flags),
        _ => None,
    }
}

fn emoji_matches(emoji: &emojis::Emoji, query: &str) -> bool {
    query.is_empty()
        || emoji.as_str().contains(query)
        || emoji.name().to_lowercase().contains(query)
        || emoji
            .shortcodes()
            .any(|shortcode| shortcode.to_lowercase().contains(query))
}

pub(super) fn insert_emoji_at_char_range(
    text: &mut String,
    start: usize,
    end: usize,
    emoji: &str,
) -> usize {
    let char_count = text.chars().count();
    let start = start.min(char_count);
    let end = end.max(start).min(char_count);
    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    text.replace_range(start_byte..end_byte, emoji);
    start + emoji.chars().count()
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_after_unicode_cursor_without_splitting_utf8() {
        let mut text = "Привет".to_owned();
        let cursor = insert_emoji_at_char_range(&mut text, 6, 6, " 👋");

        assert_eq!(text, "Привет 👋");
        assert_eq!(cursor, 8);
    }

    #[test]
    fn replaces_selected_characters() {
        let mut text = "hello world".to_owned();
        let cursor = insert_emoji_at_char_range(&mut text, 6, 11, "🌍");

        assert_eq!(text, "hello 🌍");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn search_matches_names_and_shortcodes() {
        let rocket = emojis::get("🚀").unwrap();

        assert!(emoji_matches(rocket, "rocket"));
        assert!(!emoji_matches(rocket, "banana"));
    }
}
