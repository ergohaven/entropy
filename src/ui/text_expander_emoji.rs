use super::*;
use unicode_segmentation::UnicodeSegmentation;

const EMOJI_GROUP_ICONS: [&str; 9] = ["★", "😀", "👋", "🐻", "🍎", "🚗", "⚽", "💡", "♥"];
const EMOJI_GROUP_KEYS: [&str; 9] = [
    "text_expander.emoji_group_all",
    "text_expander.emoji_group_smileys",
    "text_expander.emoji_group_people",
    "text_expander.emoji_group_animals",
    "text_expander.emoji_group_food",
    "text_expander.emoji_group_travel",
    "text_expander.emoji_group_activities",
    "text_expander.emoji_group_objects",
    "text_expander.emoji_group_symbols",
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
            let columns = 8usize;
            let row_height = 42.0 * scale;
            let row_count = matches.len().div_ceil(columns);
            egui::ScrollArea::vertical()
                .id_salt(popup_id.with("results"))
                .max_height(244.0 * scale)
                .auto_shrink([false, true])
                .show_rows(ui, row_height, row_count, |ui, visible_rows| {
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

                    for row in visible_rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(5.0 * scale, 0.0);
                            let start = row * columns;
                            let end = (start + columns).min(matches.len());
                            for emoji in &matches[start..end] {
                                let response = color_emoji_button(ui, emoji.as_str(), scale)
                                    .on_hover_text(emoji.name());
                                if response.clicked() {
                                    picked = Some(emoji.as_str());
                                }
                            }
                        });
                    }
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
        _ => None,
    }
}

pub(super) fn color_emoji_button(ui: &mut egui::Ui, emoji: &str, scale: f32) -> egui::Response {
    let size = egui::vec2(37.0 * scale, 37.0 * scale);
    color_emoji_button_sized(ui, emoji, size)
}

pub(super) fn color_emoji_button_sized(
    ui: &mut egui::Ui,
    emoji: &str,
    size: egui::Vec2,
) -> egui::Response {
    let icon_size = size.x.min(size.y);
    let response = crate::ui_style::modern_button_with_font(ui, "", size, icon_size * 0.65, true);
    if let Some(texture) = twemoji_texture(ui.ctx(), emoji) {
        ui.painter().image(
            texture.id(),
            response.rect.shrink(icon_size * 0.135),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().text(
            response.rect.center(),
            egui::Align2::CENTER_CENTER,
            emoji,
            egui::FontId::proportional(icon_size * 0.65),
            ui.visuals().text_color(),
        );
    }
    response
}

pub(super) fn monochrome_emoji_picker_button(
    ui: &mut egui::Ui,
    size: egui::Vec2,
) -> egui::Response {
    crate::ui_style::modern_button_with_font(ui, "☺", size, size.x.min(size.y) * 0.65, true)
}

fn emoji_reservation_format(ui: &egui::Ui, character: char, font_size: f32) -> egui::TextFormat {
    let base_font = egui::FontId::proportional(font_size);
    let (glyph_width, row_height) = ui.fonts_mut(|fonts| {
        (
            fonts.glyph_width(&base_font, character),
            fonts.row_height(&base_font),
        )
    });
    let target_width = row_height.max(font_size);
    let scale = if glyph_width > f32::EPSILON {
        (target_width / glyph_width).max(1.0)
    } else {
        1.0
    };

    egui::TextFormat {
        font_id: egui::FontId::proportional(font_size * scale),
        line_height: Some(row_height),
        color: egui::Color32::TRANSPARENT,
        ..Default::default()
    }
}

fn emoji_image_rect(glyph_rect: egui::Rect, field_height: f32) -> egui::Rect {
    let available_side = glyph_rect
        .width()
        .min(glyph_rect.height())
        .min((field_height - 6.0).max(1.0));
    let image_side = (available_side - 1.0).max(1.0);
    egui::Rect::from_center_size(glyph_rect.center(), egui::Vec2::splat(image_side))
}

pub(super) fn color_emoji_text_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    width: f32,
    height: f32,
    hint: &str,
    char_limit: usize,
) -> egui::Response {
    let font_size = 12.5 * (height / 32.0).clamp(1.0, 1.3);
    let text_color = ui.visuals().text_color();
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_width;
        job.break_on_newline = false;
        job.halign = egui::Align::Min;

        for grapheme in buffer.as_str().graphemes(true) {
            if emoji_has_twemoji(grapheme) {
                for (idx, character) in grapheme.chars().enumerate() {
                    let format = if idx == 0 {
                        emoji_reservation_format(ui, character, font_size)
                    } else {
                        egui::TextFormat {
                            font_id: egui::FontId::proportional(0.1),
                            line_height: Some(font_size),
                            color: egui::Color32::TRANSPARENT,
                            ..Default::default()
                        }
                    };
                    job.append(&character.to_string(), 0.0, format);
                }
            } else {
                job.append(
                    grapheme,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(font_size),
                        color: text_color,
                        ..Default::default()
                    },
                );
            }
        }

        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };
    let output = crate::ui_style::modern_text_field_sized_with_layouter(
        ui,
        id,
        text,
        width,
        height,
        hint,
        char_limit,
        egui::Align::Min,
        &mut layouter,
    );

    let painter = ui.painter().with_clip_rect(output.text_clip_rect);
    for (range, emoji) in emoji_render_spans(output.galley.job.text.as_str()) {
        let Some(glyph_rect) = galley_char_range_rect(&output.galley, output.galley_pos, range)
        else {
            continue;
        };
        let Some(texture) = twemoji_texture(ui.ctx(), emoji) else {
            continue;
        };
        let image_rect = emoji_image_rect(glyph_rect, height);
        painter.image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    output.response.response
}

fn emoji_render_spans(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut char_index = 0;
    text.graphemes(true)
        .filter_map(|grapheme| {
            let start = char_index;
            char_index += grapheme.chars().count();
            emoji_has_twemoji(grapheme).then_some((start..char_index, grapheme))
        })
        .collect()
}

fn galley_char_range_rect(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    range: std::ops::Range<usize>,
) -> Option<egui::Rect> {
    if range.is_empty() || range.end > galley.job.text.chars().count() {
        return None;
    }

    let start = galley.pos_from_cursor(egui::text::CCursor::new(range.start));
    let end = galley.pos_from_cursor(egui::text::CCursor::new(range.end));
    let min_x = start.left().min(end.left());
    let max_x = start.left().max(end.left());
    (max_x > min_x).then(|| {
        egui::Rect::from_min_max(
            galley_pos + egui::vec2(min_x, start.top().min(end.top())),
            galley_pos + egui::vec2(max_x, start.bottom().max(end.bottom())),
        )
    })
}

fn emoji_has_twemoji(emoji: &str) -> bool {
    emojis::get(emoji).is_some() && twemoji_asset(emoji).is_some()
}

fn twemoji_asset(emoji: &str) -> Option<&'static twemoji_assets::png::PngTwemojiAsset> {
    twemoji_assets::png::PngTwemojiAsset::from_emoji(emoji).or_else(|| {
        emoji
            .contains('\u{fe0f}')
            .then(|| {
                emoji
                    .chars()
                    .filter(|character| *character != '\u{fe0f}')
                    .collect::<String>()
            })
            .and_then(|normalized| twemoji_assets::png::PngTwemojiAsset::from_emoji(&normalized))
    })
}

fn twemoji_texture(ctx: &egui::Context, emoji: &str) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(("text_expander_twemoji", emoji));
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return Some(texture);
    }

    let asset = twemoji_asset(emoji)?;
    let rgba = image::load_from_memory(asset.data.0).ok()?.into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let texture = ctx.load_texture(
        format!("text-expander-twemoji-{emoji}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    Some(texture)
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

    #[test]
    fn flags_do_not_have_a_picker_category() {
        assert_eq!(EMOJI_GROUP_ICONS.len(), 9);
        assert_eq!(EMOJI_GROUP_KEYS.len(), 9);
        assert!(EMOJI_GROUP_KEYS.iter().all(|key| !key.contains("flags")));
    }

    #[test]
    fn color_asset_exists_for_a_representative_emoji() {
        assert!(twemoji_assets::png::PngTwemojiAsset::from_emoji("🚀").is_some());
    }

    #[test]
    fn smiling_face_presentation_variant_uses_twemoji_asset() {
        assert!(twemoji_assets::png::PngTwemojiAsset::from_emoji("☺️").is_none());
        assert!(twemoji_asset("☺️").is_some());
    }

    #[test]
    fn inline_renderer_keeps_one_span_per_emoji_grapheme() {
        let spans = emoji_render_spans("A☺️B👨‍👩‍👧‍👦");

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, 1..3);
        assert_eq!(spans[0].1, "☺️");
        assert_eq!(spans[1].1, "👨‍👩‍👧‍👦");
    }

    #[test]
    fn color_emoji_text_field_renders_mixed_graphemes() {
        let ctx = egui::Context::default();
        let mut text = "Text ☺️ and 👨‍👩‍👧‍👦".to_owned();

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            color_emoji_text_field(
                ui,
                egui::Id::new("color-emoji-text-field-test"),
                &mut text,
                320.0,
                32.0,
                "",
                480,
            );
        });

        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(
                        &shape.shape,
                        egui::Shape::Mesh(mesh)
                            if mesh.texture_id != egui::TextureId::default()
                    )
                })
                .count(),
            2
        );
        assert_eq!(text, "Text ☺️ and 👨‍👩‍👧‍👦");
    }

    #[test]
    fn adjacent_color_emoji_keep_separate_square_slots() {
        let ctx = egui::Context::default();
        let mut text = "🚀☺️".to_owned();

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            color_emoji_text_field(
                ui,
                egui::Id::new("adjacent-color-emoji-test"),
                &mut text,
                120.0,
                32.0,
                "",
                32,
            );
        });
        let image_rects = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                    Some(egui::Rect::from_points(
                        &mesh
                            .vertices
                            .iter()
                            .map(|vertex| vertex.pos)
                            .collect::<Vec<_>>(),
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(image_rects.len(), 2);
        assert!(image_rects.iter().all(|rect| rect.width() >= 10.0));
        assert!(image_rects[0].right() <= image_rects[1].left() + 0.1);
    }

    #[test]
    fn emoji_picker_trigger_is_monochrome_text() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            monochrome_emoji_picker_button(ui, egui::vec2(32.0, 32.0));
        });

        assert!(!output.shapes.iter().any(|shape| {
            matches!(
                &shape.shape,
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default()
            )
        }));
    }
}
