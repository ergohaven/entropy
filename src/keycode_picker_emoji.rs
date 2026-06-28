use super::*;

impl KeycodePicker {
    pub(super) fn show_vial_emoji(&mut self, ui: &mut egui::Ui) {
        let scale = responsive_picker_element_scale(ui.ctx());
        let dark = ui.visuals().dark_mode;

        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_emoji_palette"))
                .size(11.0 * scale)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(6.0 * scale);

        ui.horizontal_wrapped(|ui| {
            let search_width = (ui.available_width() * 0.42).clamp(260.0, 380.0);
            crate::ui_style::modern_text_field(
                ui,
                ui.make_persistent_id("emoji_search_query"),
                &mut self.emoji_search_query,
                search_width,
                tr_picker(self.language, "key_picker.emoji_search_placeholder"),
                48,
                egui::Align::Min,
            );

            if picker_button(
                ui,
                tr_picker(self.language, "key_picker.emoji_clear"),
                Vec2::new(72.0 * scale, 32.0 * scale),
                !self.emoji_search_query.is_empty(),
                false,
            )
            .clicked()
            {
                self.emoji_search_query.clear();
            }
        });

        ui.add_space(8.0 * scale);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(tr_picker(self.language, "key_picker.emoji_skin_tone"))
                    .size(11.0 * scale)
                    .color(Color32::from_gray(150)),
            );
            for tone in crate::emoji_catalog::EmojiSkinTone::ALL {
                let label = emoji_skin_tone_label(self.language, tone);
                let width = (label.chars().count() as f32 * 7.0 + 26.0).clamp(58.0, 108.0) * scale;
                if picker_button(
                    ui,
                    label,
                    Vec2::new(width, 30.0 * scale),
                    true,
                    self.emoji_skin_tone == tone,
                )
                .clicked()
                {
                    self.emoji_skin_tone = tone;
                }
            }
        });

        ui.add_space(8.0 * scale);
        ui.horizontal_wrapped(|ui| {
            let all_active = self.emoji_category.is_none();
            if picker_button(
                ui,
                tr_picker(self.language, "key_picker.emoji_category_all"),
                Vec2::new(58.0 * scale, 30.0 * scale),
                true,
                all_active,
            )
            .clicked()
            {
                self.emoji_category = None;
            }

            for category in crate::emoji_catalog::emoji_categories() {
                let label = emoji_category_label(self.language, *category);
                let width = (label.chars().count() as f32 * 7.0 + 26.0).clamp(70.0, 112.0) * scale;
                if picker_button(
                    ui,
                    label,
                    Vec2::new(width, 30.0 * scale),
                    true,
                    self.emoji_category == Some(*category),
                )
                .clicked()
                {
                    self.emoji_category = Some(*category);
                }
            }
        });

        ui.add_space(12.0 * scale);
        self.show_emoji_selection_bar(ui, scale, dark);
        ui.add_space(12.0 * scale);

        let results =
            crate::emoji_catalog::filter_emoji(&self.emoji_search_query, self.emoji_category);
        if results.is_empty() {
            ui.add_sized(
                Vec2::new(ui.available_width(), 44.0 * scale),
                egui::Label::new(
                    RichText::new(tr_picker(self.language, "key_picker.emoji_no_results"))
                        .size(12.0 * scale)
                        .color(crate::ui_style::muted_text(dark)),
                )
                .halign(egui::Align::Center),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(5.0 * scale, 5.0 * scale);
                for entry in results {
                    let active = self.emoji_selected == Some(entry);
                    let resp = emoji_cell_button(ui, entry.emoji, active, scale)
                        .on_hover_text(crate::i18n::tr_text(self.language, entry.name));
                    if resp.clicked() {
                        self.emoji_selected = Some(entry);
                    }
                }
            });
        }
    }

    fn show_emoji_selection_bar(&mut self, ui: &mut egui::Ui, scale: f32, dark: bool) {
        let Some(entry) = self.emoji_selected else {
            return;
        };
        let output_preview = emoji_output_preview(self.language, entry, self.emoji_skin_tone);

        let height = 58.0 * scale;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::hover(),
        );
        ui.painter().rect(
            rect,
            9.0,
            crate::ui_style::surface_fill(dark),
            crate::ui_style::modal_outline_stroke(dark),
            egui::StrokeKind::Inside,
        );

        let emoji_rect = egui::Rect::from_min_size(
            rect.min + Vec2::new(14.0, 7.0) * scale,
            Vec2::splat(44.0 * scale),
        );
        ui.painter().text(
            emoji_rect.center(),
            egui::Align2::CENTER_CENTER,
            entry.emoji,
            egui::FontId::proportional(28.0 * scale),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            egui::pos2(rect.min.x + 68.0 * scale, rect.center().y - 8.0 * scale),
            egui::Align2::LEFT_CENTER,
            entry.name,
            egui::FontId::proportional(13.0 * scale),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            egui::pos2(rect.min.x + 68.0 * scale, rect.center().y + 10.0 * scale),
            egui::Align2::LEFT_CENTER,
            emoji_category_label(self.language, entry.category),
            egui::FontId::proportional(11.0 * scale),
            crate::ui_style::muted_text(dark),
        );

        ui.painter().text(
            egui::pos2(rect.right() - 18.0 * scale, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            format!(
                "{}: {}",
                tr_picker(self.language, "key_picker.emoji_output"),
                output_preview
            ),
            egui::FontId::proportional(12.0 * scale),
            crate::ui_style::muted_text(dark),
        );
    }
}

fn emoji_cell_button(ui: &mut egui::Ui, emoji: &str, active: bool, scale: f32) -> egui::Response {
    let size = Vec2::splat(48.0 * scale);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let dark = ui.visuals().dark_mode;
    let fill = if active {
        crate::ui_style::accent()
    } else if resp.hovered() {
        crate::ui_style::hover_fill(dark)
    } else {
        crate::ui_style::surface_fill(dark)
    };
    ui.painter().rect(
        rect,
        8.0,
        fill,
        crate::ui_style::modal_outline_stroke(dark),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        emoji,
        egui::FontId::proportional(25.0 * scale),
        if active {
            Color32::WHITE
        } else {
            ui.visuals().text_color()
        },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn emoji_category_label(
    language: crate::i18n::Language,
    category: crate::emoji_catalog::EmojiCategory,
) -> &'static str {
    let key = match category {
        crate::emoji_catalog::EmojiCategory::Smileys => "key_picker.emoji_category_smileys",
        crate::emoji_catalog::EmojiCategory::People => "key_picker.emoji_category_people",
        crate::emoji_catalog::EmojiCategory::Nature => "key_picker.emoji_category_nature",
        crate::emoji_catalog::EmojiCategory::Food => "key_picker.emoji_category_food",
        crate::emoji_catalog::EmojiCategory::Travel => "key_picker.emoji_category_travel",
        crate::emoji_catalog::EmojiCategory::Activities => "key_picker.emoji_category_activities",
        crate::emoji_catalog::EmojiCategory::Objects => "key_picker.emoji_category_objects",
        crate::emoji_catalog::EmojiCategory::Symbols => "key_picker.emoji_category_symbols",
    };
    tr_picker(language, key)
}

fn emoji_skin_tone_label(
    language: crate::i18n::Language,
    tone: crate::emoji_catalog::EmojiSkinTone,
) -> &'static str {
    let key = match tone {
        crate::emoji_catalog::EmojiSkinTone::Default => "key_picker.emoji_skin_default",
        crate::emoji_catalog::EmojiSkinTone::Light => "key_picker.emoji_skin_light",
        crate::emoji_catalog::EmojiSkinTone::MediumLight => "key_picker.emoji_skin_medium_light",
        crate::emoji_catalog::EmojiSkinTone::Medium => "key_picker.emoji_skin_medium",
        crate::emoji_catalog::EmojiSkinTone::MediumDark => "key_picker.emoji_skin_medium_dark",
        crate::emoji_catalog::EmojiSkinTone::Dark => "key_picker.emoji_skin_dark",
    };
    tr_picker(language, key)
}

fn emoji_output_preview(
    language: crate::i18n::Language,
    entry: &crate::emoji_catalog::EmojiEntry,
    tone: crate::emoji_catalog::EmojiSkinTone,
) -> String {
    let output_sequence = crate::emoji_catalog::emoji_sequence(entry, tone);
    if entry.supports_skin_tone && tone != crate::emoji_catalog::EmojiSkinTone::Default {
        format!(
            "{} ({})",
            entry.emoji,
            emoji_skin_tone_label(language, tone)
        )
    } else {
        output_sequence
    }
}
