use super::*;

impl KeycodePicker {
    pub(super) fn show_emoji_sections(&mut self, ui: &mut egui::Ui) {
        let scale = responsive_picker_element_scale(ui.ctx());
        let dark = ui.visuals().dark_mode;

        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_emoji"))
                .size(11.0 * scale)
                .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0 * scale);
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

            ui.add_space(8.0 * scale);
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
        let mut has_results = false;
        for section in crate::emoji_catalog::emoji_sections() {
            let entries =
                crate::emoji_catalog::filter_emoji_section(&self.emoji_search_query, *section);
            if entries.is_empty() {
                continue;
            }

            has_results = true;
            ui.label(
                RichText::new(emoji_section_label(self.language, *section))
                    .size(11.0 * scale)
                    .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0 * scale);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(5.0 * scale, 5.0 * scale);
                for entry in entries {
                    self.show_emoji_button(ui, entry, scale);
                }
            });
            ui.add_space(8.0 * scale);
        }

        if !has_results {
            ui.add_sized(
                Vec2::new(ui.available_width(), 44.0 * scale),
                egui::Label::new(
                    RichText::new(tr_picker(self.language, "key_picker.emoji_no_results"))
                        .size(12.0 * scale)
                        .color(crate::ui_style::muted_text(dark)),
                )
                .halign(egui::Align::Center),
            );
        }
    }

    fn show_emoji_button(
        &mut self,
        ui: &mut egui::Ui,
        entry: &'static crate::emoji_catalog::EmojiEntry,
        scale: f32,
    ) {
        let active = self.emoji_selected == Some(entry);
        let tone = if active {
            self.emoji_skin_tone
        } else {
            crate::emoji_catalog::EmojiSkinTone::Default
        };
        let emoji = crate::emoji_catalog::emoji_sequence(entry, tone);
        let resp = emoji_cell_button(ui, &emoji, active, scale).on_hover_text(emoji_hover_text(
            self.language,
            entry,
            tone,
        ));

        if resp.clicked_by(egui::PointerButton::Primary) {
            self.emoji_selected = Some(entry);
            self.emoji_skin_tone = crate::emoji_catalog::EmojiSkinTone::Default;
        }

        if resp.clicked_by(egui::PointerButton::Secondary)
            && ui.input(|input| input.modifiers.ctrl)
            && entry.supports_skin_tone
        {
            if !active {
                self.emoji_selected = Some(entry);
                self.emoji_skin_tone = crate::emoji_catalog::EmojiSkinTone::Default;
            }
            self.emoji_skin_tone = self.emoji_skin_tone.next();
        }
    }
}

fn emoji_cell_button(ui: &mut egui::Ui, emoji: &str, active: bool, scale: f32) -> egui::Response {
    let size = KeycodePicker::picker_key_size(ui.ctx());
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
        egui::FontId::new(
            26.0 * scale,
            egui::FontFamily::Name("emoji_preview".into()),
        ),
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

fn emoji_section_label(
    language: crate::i18n::Language,
    section: crate::emoji_catalog::EmojiSection,
) -> &'static str {
    let key = match section {
        crate::emoji_catalog::EmojiSection::SmileysAndEmotion => {
            "key_picker.emoji_section_smileys_emotion"
        }
        crate::emoji_catalog::EmojiSection::PeopleAndBody => "key_picker.emoji_section_people_body",
        crate::emoji_catalog::EmojiSection::AnimalsAndNature => {
            "key_picker.emoji_section_animals_nature"
        }
        crate::emoji_catalog::EmojiSection::FoodAndDrink => "key_picker.emoji_section_food_drink",
        crate::emoji_catalog::EmojiSection::TravelAndPlaces => {
            "key_picker.emoji_section_travel_places"
        }
        crate::emoji_catalog::EmojiSection::Activities => "key_picker.emoji_category_activities",
        crate::emoji_catalog::EmojiSection::Objects => "key_picker.emoji_category_objects",
        crate::emoji_catalog::EmojiSection::Symbols => "key_picker.emoji_category_symbols",
        crate::emoji_catalog::EmojiSection::Flags => "key_picker.emoji_section_flags",
    };
    tr_picker(language, key)
}

fn emoji_hover_text(
    language: crate::i18n::Language,
    entry: &crate::emoji_catalog::EmojiEntry,
    tone: crate::emoji_catalog::EmojiSkinTone,
) -> String {
    let mut text = crate::i18n::tr_text(language, entry.name).to_string();
    if entry.supports_skin_tone {
        text.push('\n');
        text.push_str(tr_picker(language, "key_picker.emoji_tone_cycle_hint"));
        text.push('\n');
        text.push_str(tr_picker(language, "key_picker.emoji_skin_tone"));
        text.push_str(": ");
        text.push_str(emoji_skin_tone_label(language, tone));
    }
    text
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
