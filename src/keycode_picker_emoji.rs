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
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.emoji_assignment_hint"))
                .size(10.0 * scale)
                .color(crate::ui_style::muted_text(dark)),
        );
        if self.emoji_assignment_error {
            ui.add_space(3.0 * scale);
            ui.label(
                RichText::new(tr_picker(
                    self.language,
                    "key_picker.emoji_no_free_macro_slot",
                ))
                .size(10.0 * scale)
                .color(Color32::from_rgb(200, 110, 90)),
            );
        }
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
        for (label_key, sections) in emoji_presentation_sections() {
            let entries =
                crate::emoji_catalog::filter_emoji_sections(&self.emoji_search_query, sections);
            if entries.is_empty() {
                continue;
            }

            has_results = true;
            ui.label(
                RichText::new(tr_picker(self.language, label_key))
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
            let selected_tone = if active {
                self.emoji_skin_tone
            } else {
                crate::emoji_catalog::EmojiSkinTone::Default
            };
            self.emoji_selected = Some(entry);
            self.emoji_skin_tone = selected_tone;
            self.assign_emoji(entry, selected_tone);
        }

        if resp.clicked_by(egui::PointerButton::Secondary)
            && ui.input(|input| input.modifiers.ctrl)
            && entry.supports_skin_tone
        {
            self.emoji_assignment_error = false;
            if !active {
                self.emoji_selected = Some(entry);
                self.emoji_skin_tone = crate::emoji_catalog::EmojiSkinTone::Default;
            }
            self.emoji_skin_tone = self.emoji_skin_tone.next();
        }
    }

    fn assign_emoji(
        &mut self,
        entry: &'static crate::emoji_catalog::EmojiEntry,
        tone: crate::emoji_catalog::EmojiSkinTone,
    ) {
        let emoji = crate::emoji_catalog::emoji_sequence(entry, tone);
        let Some(actions) = emoji_macro_actions(&emoji) else {
            self.emoji_assignment_error = true;
            return;
        };
        let encoded_len = super::keycode_picker_macro::encode_macro_actions(&actions).len();
        let Some((slot, write_macro)) = self.emoji_macro_slot(&emoji, encoded_len) else {
            self.emoji_assignment_error = true;
            return;
        };

        if write_macro {
            self.ensure_macro_meta_len(slot);
            self.macro_actions[slot] = actions;
            self.encode_macro(slot);
            self.macros_dirty = true;
        }
        self.emoji_assignment_error = false;
        self.result = Some(0x7700 + slot as u16);
        self.open = false;
    }

    fn emoji_macro_slot(&self, emoji: &str, encoded_len: usize) -> Option<(usize, bool)> {
        if let Some(slot) = (0..self.macro_count).find(|slot| {
            self.macro_actions
                .get(*slot)
                .and_then(|actions| decode_emoji_macro(actions))
                .as_deref()
                == Some(emoji)
        }) {
            return Some((slot, false));
        }

        (0..self.macro_count.min(u8::MAX as usize + 1))
            .find(|slot| {
                self.macro_actions
                    .get(*slot)
                    .map(|actions| actions.is_empty())
                    .unwrap_or(true)
                    && self
                        .macro_texts
                        .get(*slot)
                        .map(|bytes| bytes.is_empty())
                        .unwrap_or(true)
                    && self.macro_buffer_fits(*slot, encoded_len)
            })
            .map(|slot| (slot, true))
    }

    fn macro_buffer_fits(&self, replaced_slot: usize, replacement_len: usize) -> bool {
        let Some(capacity) = self.macro_buffer_size else {
            return false;
        };
        let slot_count = self.macro_texts.len().max(self.macro_count);
        let required = (0..slot_count)
            .map(|slot| {
                if slot == replaced_slot {
                    replacement_len
                } else {
                    self.macro_texts.get(slot).map(Vec::len).unwrap_or(0)
                }
            })
            .sum::<usize>()
            + slot_count;
        required <= capacity
    }
}

const KC_LGUI: u16 = 0x00e3;

fn emoji_macro_actions(emoji: &str) -> Option<Vec<MacroAction>> {
    let payload = crate::host_text_transport::encode_text_payload(emoji)?;
    let mut actions = Vec::with_capacity(payload.len() + 3);
    actions.push(MacroAction::Down(KC_LGUI));
    actions.push(MacroAction::Tap(crate::host_text_transport::KC_F20));
    actions.push(MacroAction::Up(KC_LGUI));
    actions.extend(payload.into_iter().map(MacroAction::Tap));
    Some(actions)
}

fn decode_emoji_macro(actions: &[MacroAction]) -> Option<String> {
    if !matches!(
        actions,
        [
            MacroAction::Down(KC_LGUI),
            MacroAction::Tap(crate::host_text_transport::KC_F20),
            MacroAction::Up(KC_LGUI),
            ..
        ]
    ) {
        return None;
    }

    let now = std::time::Instant::now();
    let mut decoder = crate::host_text_transport::HostTextTransportDecoder::default();
    if decoder.handle(crate::host_text_transport::START_TRIGGER_KEYCODE, true, now)
        != crate::host_text_transport::TransportOutcome::Started
    {
        return None;
    }

    let payload = &actions[3..];
    for (idx, action) in payload.iter().enumerate() {
        let MacroAction::Tap(keycode) = action else {
            return None;
        };
        match decoder.handle(*keycode, true, now) {
            crate::host_text_transport::TransportOutcome::Complete(text)
                if idx + 1 == payload.len() =>
            {
                return Some(text);
            }
            crate::host_text_transport::TransportOutcome::Consumed => {}
            _ => return None,
        }
    }
    None
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
        egui::FontId::new(26.0 * scale, egui::FontFamily::Name("emoji_preview".into())),
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

fn emoji_presentation_sections(
) -> [(&'static str, &'static [crate::emoji_catalog::EmojiSection]); 8] {
    use crate::emoji_catalog::EmojiSection;

    [
        (
            "key_picker.emoji_section_people",
            &[EmojiSection::SmileysAndEmotion, EmojiSection::PeopleAndBody],
        ),
        (
            "key_picker.emoji_section_animals_nature",
            &[EmojiSection::AnimalsAndNature],
        ),
        (
            "key_picker.emoji_section_food_drink",
            &[EmojiSection::FoodAndDrink],
        ),
        (
            "key_picker.emoji_section_travel_places",
            &[EmojiSection::TravelAndPlaces],
        ),
        (
            "key_picker.emoji_category_activities",
            &[EmojiSection::Activities],
        ),
        (
            "key_picker.emoji_category_objects",
            &[EmojiSection::Objects],
        ),
        (
            "key_picker.emoji_category_symbols",
            &[EmojiSection::Symbols],
        ),
        ("key_picker.emoji_section_flags", &[EmojiSection::Flags]),
    ]
}

fn emoji_hover_text(
    language: crate::i18n::Language,
    entry: &crate::emoji_catalog::EmojiEntry,
    tone: crate::emoji_catalog::EmojiSkinTone,
) -> String {
    let mut text = crate::i18n::tr_text(language, entry.name).to_string();
    text.push('\n');
    text.push_str(tr_picker(language, "key_picker.emoji_assign_hint"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_starts_with_combined_emoji_and_people_section() {
        let sections = emoji_presentation_sections();

        assert_eq!(sections[0].0, "key_picker.emoji_section_people");
        assert_eq!(
            sections[0].1,
            &[
                crate::emoji_catalog::EmojiSection::SmileysAndEmotion,
                crate::emoji_catalog::EmojiSection::PeopleAndBody,
            ]
        );
    }

    #[test]
    fn generated_macro_round_trips_complex_emoji() {
        for emoji in ["😀", "👍🏽", "👨‍👩‍👧‍👦", "🏳️‍🌈"] {
            let actions = emoji_macro_actions(emoji).unwrap();
            let encoded =
                crate::keycode_picker::keycode_picker_macro::encode_macro_actions(&actions);
            let decoded =
                crate::keycode_picker::keycode_picker_macro::decode_macro_actions(&encoded);

            assert_eq!(decode_emoji_macro(&decoded).as_deref(), Some(emoji));
            assert_eq!(encoded.len(), actions.len() * 3);
            assert!(encoded
                .chunks_exact(3)
                .all(|chunk| chunk[0] == 1 && (1..=3).contains(&chunk[1])));
        }
    }

    #[test]
    fn every_catalog_entry_fits_transport_payload() {
        for entry in crate::emoji_catalog::EMOJI_CATALOG {
            for tone in [
                crate::emoji_catalog::EmojiSkinTone::Default,
                crate::emoji_catalog::EmojiSkinTone::Dark,
            ] {
                let emoji = crate::emoji_catalog::emoji_sequence(entry, tone);
                assert!(
                    emoji_macro_actions(&emoji).is_some(),
                    "emoji payload is too large: {} ({})",
                    entry.name,
                    emoji
                );
            }
        }
    }

    #[test]
    fn assignment_uses_first_free_macro_slot() {
        let entry = crate::emoji_catalog::EMOJI_CATALOG
            .iter()
            .find(|entry| entry.emoji == "🚀")
            .unwrap();
        let mut picker = KeycodePicker::default();
        picker.macro_buffer_size = Some(8192);
        picker.open = true;
        picker.emoji_target_keycode = Some(0x0004);
        picker.macro_actions[0] = vec![MacroAction::Text("occupied".into())];
        picker.encode_macro(0);

        picker.assign_emoji(entry, crate::emoji_catalog::EmojiSkinTone::Default);

        assert_eq!(picker.result, Some(0x7701));
        assert_eq!(
            decode_emoji_macro(&picker.macro_actions[1]).as_deref(),
            Some("🚀")
        );
        assert!(picker.macros_dirty);
        assert!(!picker.open);
    }

    #[test]
    fn assignment_reuses_matching_generated_macro_slot() {
        let smile = crate::emoji_catalog::EMOJI_CATALOG
            .iter()
            .find(|entry| entry.emoji == "😀")
            .unwrap();
        let mut picker = KeycodePicker::default();
        picker.macro_buffer_size = Some(8192);
        picker.macro_actions[3] = emoji_macro_actions("😀").unwrap();
        picker.encode_macro(3);
        picker.emoji_target_keycode = Some(0x0004);

        picker.assign_emoji(smile, crate::emoji_catalog::EmojiSkinTone::Default);

        assert_eq!(picker.result, Some(0x7703));
        assert_eq!(
            decode_emoji_macro(&picker.macro_actions[3]).as_deref(),
            Some("😀")
        );
        assert!(!picker.macros_dirty);
    }

    #[test]
    fn reassignment_does_not_mutate_previous_generated_macro() {
        let smile = crate::emoji_catalog::EMOJI_CATALOG
            .iter()
            .find(|entry| entry.emoji == "😀")
            .unwrap();
        let mut picker = KeycodePicker::default();
        picker.macro_buffer_size = Some(8192);
        picker.macro_actions[3] = emoji_macro_actions("🚀").unwrap();
        picker.encode_macro(3);
        picker.emoji_target_keycode = Some(0x7703);

        picker.assign_emoji(smile, crate::emoji_catalog::EmojiSkinTone::Default);

        assert_eq!(picker.result, Some(0x7700));
        assert_eq!(
            decode_emoji_macro(&picker.macro_actions[0]).as_deref(),
            Some("😀")
        );
        assert_eq!(
            decode_emoji_macro(&picker.macro_actions[3]).as_deref(),
            Some("🚀")
        );
    }

    #[test]
    fn assignment_reports_when_no_macro_slot_is_free() {
        let entry = crate::emoji_catalog::EMOJI_CATALOG.first().unwrap();
        let mut picker = KeycodePicker::default();
        picker.macro_buffer_size = Some(8192);
        picker.emoji_target_keycode = Some(0x0004);
        for slot in 0..picker.macro_count {
            picker.macro_actions[slot] = vec![MacroAction::Text("occupied".into())];
            picker.encode_macro(slot);
        }

        picker.assign_emoji(entry, crate::emoji_catalog::EmojiSkinTone::Default);

        assert_eq!(picker.result, None);
        assert!(picker.emoji_assignment_error);
        assert!(!picker.macros_dirty);
    }

    #[test]
    fn assignment_reports_when_macro_buffer_has_no_space() {
        let entry = crate::emoji_catalog::EMOJI_CATALOG.first().unwrap();
        let mut picker = KeycodePicker::default();
        picker.macro_buffer_size = Some(picker.macro_count);
        picker.emoji_target_keycode = Some(0x0004);

        picker.assign_emoji(entry, crate::emoji_catalog::EmojiSkinTone::Default);

        assert_eq!(picker.result, None);
        assert!(picker.emoji_assignment_error);
    }
}
