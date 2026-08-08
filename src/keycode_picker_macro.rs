use super::*;

const SS_QMK_PREFIX: u8 = 1;
const SS_TAP_CODE: u8 = 1;
const SS_DOWN_CODE: u8 = 2;
const SS_UP_CODE: u8 = 3;
const SS_DELAY_CODE: u8 = 4;
const VIAL_MACRO_EXT_TAP: u8 = 5;
const VIAL_MACRO_EXT_DOWN: u8 = 6;
const VIAL_MACRO_EXT_UP: u8 = 7;

fn append_macro_key_action(encoded: &mut Vec<u8>, basic_opcode: u8, ext_opcode: u8, keycode: u16) {
    encoded.push(SS_QMK_PREFIX);
    if keycode < 0x0100 {
        encoded.push(basic_opcode);
        encoded.push(keycode as u8);
        return;
    }

    encoded.push(ext_opcode);
    let wire_keycode = if keycode & 0x00FF == 0 {
        0xFF00 | (keycode >> 8)
    } else {
        keycode
    };
    encoded.extend_from_slice(&wire_keycode.to_le_bytes());
}

pub(crate) fn decode_macro_actions(bytes: &[u8]) -> Vec<MacroAction> {
    let mut actions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == SS_QMK_PREFIX {
            if i + 1 >= bytes.len() {
                actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                break;
            }

            match bytes[i + 1] {
                SS_TAP_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Tap(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_DOWN_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Down(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_UP_CODE => {
                    if i + 2 < bytes.len() {
                        let keycode = bytes[i + 2];
                        if is_send_string_keycode(keycode) {
                            actions.push(MacroAction::Up(keycode as u16));
                        } else {
                            actions.push(MacroAction::Raw(bytes[i..i + 3].to_vec()));
                        }
                        i += 3;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                SS_DELAY_CODE => {
                    if i + 3 < bytes.len() && bytes[i + 2] != 0 && bytes[i + 3] != 0 {
                        let ms = (bytes[i + 2] as u16 - 1) + (bytes[i + 3] as u16 - 1) * 255;
                        actions.push(MacroAction::Delay(ms));
                        i += 4;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                VIAL_MACRO_EXT_TAP..=VIAL_MACRO_EXT_UP => {
                    if i + 3 < bytes.len() {
                        let mut keycode = u16::from_le_bytes([bytes[i + 2], bytes[i + 3]]);
                        // Matches Vial GUI's workaround for QMK decode_keycode():
                        // values whose low byte is zero are serialized as FFxx.
                        if keycode > 0xFF00 {
                            keycode = (keycode & 0xFF) << 8;
                        }
                        match bytes[i + 1] {
                            VIAL_MACRO_EXT_TAP => actions.push(MacroAction::Tap(keycode)),
                            VIAL_MACRO_EXT_DOWN => actions.push(MacroAction::Down(keycode)),
                            VIAL_MACRO_EXT_UP => actions.push(MacroAction::Up(keycode)),
                            _ => {}
                        }
                        i += 4;
                    } else {
                        actions.push(MacroAction::Raw(bytes[i..].to_vec()));
                        break;
                    }
                }
                _ => {
                    let end = (i + 2).min(bytes.len());
                    actions.push(MacroAction::Raw(bytes[i..end].to_vec()));
                    i = end;
                }
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != SS_QMK_PREFIX {
                i += 1;
            }
            push_macro_text_or_raw(&mut actions, &bytes[start..i]);
        }
    }
    actions
}

fn is_send_string_keycode(keycode: u8) -> bool {
    matches!(keycode, 0x04..=0xC0 | 0xCD..=0xE7)
}

pub(crate) fn encode_macro_actions(actions: &[MacroAction]) -> Vec<u8> {
    let mut encoded = Vec::new();
    for action in actions {
        match action {
            MacroAction::Text(s) => encoded.extend_from_slice(s.as_bytes()),
            MacroAction::Tap(kc) => {
                append_macro_key_action(&mut encoded, SS_TAP_CODE, VIAL_MACRO_EXT_TAP, *kc);
            }
            MacroAction::Down(kc) => {
                append_macro_key_action(&mut encoded, SS_DOWN_CODE, VIAL_MACRO_EXT_DOWN, *kc);
            }
            MacroAction::Up(kc) => {
                append_macro_key_action(&mut encoded, SS_UP_CODE, VIAL_MACRO_EXT_UP, *kc);
            }
            MacroAction::Delay(ms) => {
                let hi = (*ms / 255 + 1) as u8;
                let lo = (*ms % 255 + 1) as u8;
                encoded.push(SS_QMK_PREFIX);
                encoded.push(SS_DELAY_CODE);
                encoded.push(lo);
                encoded.push(hi);
            }
            MacroAction::Raw(bytes) => encoded.extend_from_slice(bytes),
        }
    }
    encoded
}

fn push_macro_text_or_raw(actions: &mut Vec<MacroAction>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        actions.push(MacroAction::Text(s.to_string()));
    } else {
        actions.push(MacroAction::Raw(bytes.to_vec()));
    }
}

fn format_raw_macro_bytes(bytes: &[u8]) -> String {
    let mut label = bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > 8 {
        label.push_str(" ...");
    }
    if label.is_empty() {
        "empty".to_string()
    } else {
        label
    }
}

fn limit_chars(value: &mut String, limit: usize) {
    let limited: String = value.chars().take(limit).collect();
    if limited.len() != value.len() {
        *value = limited;
    }
}

impl KeycodePicker {
    fn show_macro_editor_contents(
        &mut self,
        ui: &mut egui::Ui,
        raw_n: u8,
        grid_id: &'static str,
        _add_action_id: &'static str,
        _footer_text: &'static str,
    ) -> u8 {
        let slot_count = self.macro_count.min(u8::MAX as usize + 1);
        let mut selected_macro = if slot_count > 0 && (raw_n as usize) < slot_count {
            raw_n
        } else {
            0
        };
        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.choose_macro",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        if slot_count == 0 {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.no_macro_slots_available_on_this_keyboard",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return 254;
        }
        if let Some(notice) = self.macro_ext_keycodes_notice(self.language) {
            ui.label(
                RichText::new(notice)
                    .size(11.0)
                    .color(Color32::from_rgb(180, 120, 40)),
            );
            ui.add_space(4.0);
        }
        egui::Frame::NONE.show(ui, |ui| {
            let slot_scroll_height = 86.0 * responsive_picker_element_scale(ui.ctx());
            ui.set_max_height(slot_scroll_height);
            egui::ScrollArea::vertical()
                .max_height(slot_scroll_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(grid_id)
                        .num_columns(16)
                        .spacing([4.0, 4.0])
                        .show(ui, |ui| {
                            for i in 0..slot_count {
                                let i = i as u8;
                                let is_active = i == selected_macro;
                                let has_content = self.macro_has_content(i as usize);
                                let display_name = self.macro_display_name(i as usize);
                                let description = self.macro_description(i as usize);
                                let id_text = format!("M{}", i);
                                let mut resp = picker_slot_button(
                                    ui,
                                    &id_text,
                                    &display_name,
                                    is_active,
                                    has_content,
                                );
                                if let Some(description) = description {
                                    resp = resp
                                        .on_hover_text(format!("{display_name}\n{description}"));
                                } else if display_name != id_text {
                                    resp = resp.on_hover_text(display_name.clone());
                                }
                                if resp.clicked() {
                                    self.ensure_macro_meta_len(i as usize);
                                    selected_macro = i;
                                }
                                if (i + 1).is_multiple_of(16) {
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
        ui.add_space(crate::ui_style::modal_space_sm());

        if selected_macro == 254 {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.select_a_macro_above_to_edit",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return selected_macro;
        }

        let n = selected_macro as usize;
        self.ensure_macro_meta_len(n);

        let scale = responsive_picker_element_scale(ui.ctx());
        let macro_font_size = 14.0 * scale;
        let custom_pairs = self.custom_keycode_pairs();
        ui.add_space(4.0 * scale);
        let language = self.language;
        let mut macro_metadata_changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0 * scale;
            if let Some(name) = self.macro_names.get_mut(n) {
                let resp = crate::ui_style::modern_text_field_sized(
                    ui,
                    ui.make_persistent_id(("macro_name", grid_id, n)),
                    name,
                    124.0 * scale,
                    32.0 * scale,
                    crate::i18n::tr_catalog(language, "macro_editor.macro_name"),
                    MACRO_NAME_CHAR_LIMIT,
                    egui::Align::Center,
                );
                if resp.changed() {
                    limit_chars(name, MACRO_NAME_CHAR_LIMIT);
                    macro_metadata_changed = true;
                }
            }

            if let Some(description) = self.macro_descriptions.get_mut(n) {
                let description_w = ui.available_width().max(180.0 * scale).min(360.0 * scale);
                let resp = crate::ui_style::modern_text_field_sized(
                    ui,
                    ui.make_persistent_id(("macro_description", grid_id, n)),
                    description,
                    description_w,
                    32.0 * scale,
                    crate::i18n::tr_catalog(language, "macro_editor.macro_description"),
                    MACRO_DESCRIPTION_CHAR_LIMIT,
                    egui::Align::Min,
                )
                .on_hover_text(crate::i18n::tr_catalog(
                    language,
                    "macro_editor.optional_description_for_this_macro",
                ));
                if resp.changed() {
                    limit_chars(description, MACRO_DESCRIPTION_CHAR_LIMIT);
                    macro_metadata_changed = true;
                }
            }
        });
        ui.add_space(6.0);

        let mut remove_idx = None;
        let mut move_up: Option<usize> = None;
        let mut move_down: Option<usize> = None;
        let mut macro_changed = false;
        let avail_w = ui.available_width();
        {
            let action_count = self.macro_actions[n].len();
            for (i, action) in self.macro_actions[n].iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let arrow_size = picker_scaled_size(ui.ctx(), 28.0, 28.0);
                    let up_resp = picker_button(ui, "↑", arrow_size, i > 0, false).on_hover_text(
                        crate::i18n::tr_catalog(self.language, "macro_editor.move_up"),
                    );
                    let down_resp = picker_button(ui, "↓", arrow_size, i + 1 < action_count, false)
                        .on_hover_text(crate::i18n::tr_catalog(
                            self.language,
                            "macro_editor.move_down",
                        ));
                    if up_resp.clicked() && i > 0 {
                        move_up = Some(i);
                    }
                    if down_resp.clicked() && i + 1 < action_count {
                        move_down = Some(i);
                    }

                    let (type_label, type_color, tooltip) = match action {
                        MacroAction::Text(_) => (
                            crate::i18n::tr_catalog(self.language, "macro_editor.text"),
                            crate::ui_style::accent(),
                            crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.types_text_characters_one_by_one",
                            ),
                        ),
                        MacroAction::Tap(_) => (
                            crate::i18n::tr_catalog(self.language, "macro_editor.tap"),
                            crate::ui_style::accent(),
                            crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.press_and_release_a_key",
                            ),
                        ),
                        MacroAction::Down(_) => (
                            crate::i18n::tr_catalog(self.language, "macro_editor.down"),
                            Color32::from_rgb(200, 150, 50),
                            crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.press_a_key_hold_until_up",
                            ),
                        ),
                        MacroAction::Up(_) => (
                            crate::i18n::tr_catalog(self.language, "macro_editor.up"),
                            Color32::from_rgb(132, 150, 178),
                            crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.release_a_previously_pressed_key",
                            ),
                        ),
                        MacroAction::Delay(_) => (
                            crate::i18n::tr_catalog(self.language, "macro_editor.delay"),
                            Color32::from_gray(150),
                            crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.wait_before_next_action",
                            ),
                        ),
                        MacroAction::Raw(_) => (
                            "Raw",
                            Color32::from_rgb(190, 110, 90),
                            "Raw macro bytes preserved for compatibility",
                        ),
                    };
                    ui.allocate_ui(picker_scaled_size(ui.ctx(), 55.0, 30.0), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(type_label)
                                    .size(macro_font_size)
                                    .color(type_color)
                                    .strong(),
                            )
                            .sense(egui::Sense::hover()),
                        )
                        .on_hover_text(tooltip);
                    });

                    match action {
                        MacroAction::Text(text) => {
                            let text_w = (avail_w - 220.0 * scale).max(150.0 * scale);
                            if crate::ui_style::modern_text_field_sized(
                                ui,
                                ui.make_persistent_id(("macro_text_action", grid_id, n, i)),
                                text,
                                text_w,
                                32.0 * scale,
                                crate::i18n::tr_catalog(
                                    self.language,
                                    "macro_editor.type_text_here",
                                ),
                                256,
                                egui::Align::Min,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.characters_to_type_when_this_macro_runs",
                            ))
                            .changed()
                            {
                                macro_changed = true;
                            }
                        }
                        MacroAction::Tap(kc) => {
                            let label = keycode_label_with_names_and_layout(
                                *kc ,
                                &custom_pairs,
                                &self.layer_names,
                                self.key_legend_layout,
                            );
                            if picker_button(
                                ui,
                                &label,
                                picker_scaled_size(ui.ctx(), 100.0, 30.0),
                                true,
                                false,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.click_to_change_key_press_and_release_this_key",
                            ))
                            .clicked()
                            {
                                self.macro_key_pick = Some((n, i));
                            }
                        }
                        MacroAction::Down(kc) => {
                            let label = keycode_label_with_names_and_layout(
                                *kc ,
                                &custom_pairs,
                                &self.layer_names,
                                self.key_legend_layout,
                            );
                            if picker_button(
                                ui,
                                &label,
                                picker_scaled_size(ui.ctx(), 100.0, 30.0),
                                true,
                                false,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.click_to_change_key_holds_down_until_up",
                            ))
                            .clicked()
                            {
                                self.macro_key_pick = Some((n, i));
                            }
                        }
                        MacroAction::Up(kc) => {
                            let label = keycode_label_with_names_and_layout(
                                *kc ,
                                &custom_pairs,
                                &self.layer_names,
                                self.key_legend_layout,
                            );
                            if picker_button(
                                ui,
                                &label,
                                picker_scaled_size(ui.ctx(), 100.0, 30.0),
                                true,
                                false,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.click_to_change_key_releases_this_key",
                            ))
                            .clicked()
                            {
                                self.macro_key_pick = Some((n, i));
                            }
                        }
                        MacroAction::Delay(ms) => {
                            let mut ms_str = ms.to_string();
                            if crate::ui_style::modern_text_field_sized(
                                ui,
                                ui.make_persistent_id(("macro_delay", grid_id, n, i)),
                                &mut ms_str,
                                80.0 * scale,
                                32.0 * scale,
                                "",
                                5,
                                egui::Align::Center,
                            )
                            .on_hover_text(crate::i18n::tr_catalog(
                                self.language,
                                "macro_editor.delay_is_in_milliseconds",
                            ))
                            .changed()
                            {
                                if let Ok(v) = ms_str.parse::<u16>() {
                                    if *ms != v {
                                        *ms = v;
                                        macro_changed = true;
                                    }
                                }
                            }
                        }
                        MacroAction::Raw(bytes) => {
                            let label = format_raw_macro_bytes(bytes);
                            ui.allocate_ui(picker_scaled_size(ui.ctx(), 170.0, 30.0), |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(label)
                                            .size(macro_font_size)
                                            .color(Color32::from_gray(160)),
                                    )
                                    .sense(egui::Sense::hover()),
                                )
                                .on_hover_text(
                                    "Entropy cannot edit these macro bytes yet, but will save them unchanged",
                                );
                            });
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if picker_button(
                            ui,
                            "✕",
                            picker_scaled_size(ui.ctx(), 30.0, 30.0),
                            true,
                            false,
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            self.language,
                            "macro_editor.remove_this_action",
                        ))
                        .clicked()
                        {
                            remove_idx = Some(i);
                        }
                    });
                });
                ui.add_space(2.0);
            }
        }
        if let Some(idx) = remove_idx {
            if idx < self.macro_actions[n].len() {
                self.macro_undo_stack
                    .push((n, self.macro_actions[n].clone()));
                self.macro_actions[n].remove(idx);
                macro_changed = true;
                if let Some((mn, ai)) = self.macro_key_pick {
                    if mn == n && ai >= idx {
                        self.macro_key_pick = None;
                    }
                }
            }
        }
        if let Some(idx) = move_up {
            if idx > 0 {
                self.macro_actions[n].swap(idx, idx - 1);
                macro_changed = true;
            }
        }
        if let Some(idx) = move_down {
            if idx + 1 < self.macro_actions[n].len() {
                self.macro_actions[n].swap(idx, idx + 1);
                macro_changed = true;
            }
        }

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.plus_text"),
                picker_scaled_size(ui.ctx(), 72.0, 30.0),
                true,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.type_characters",
            ))
            .clicked()
            {
                self.macro_actions[n].push(MacroAction::Text(String::new()));
                macro_changed = true;
            }
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.plus_tap"),
                picker_scaled_size(ui.ctx(), 66.0, 30.0),
                true,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.press_and_release_a_key",
            ))
            .clicked()
            {
                self.macro_actions[n].push(MacroAction::Tap(0x04));
                macro_changed = true;
                self.macro_key_pick = Some((n, self.macro_actions[n].len() - 1));
            }
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.plus_down"),
                picker_scaled_size(ui.ctx(), 80.0, 30.0),
                true,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.hold_a_key",
            ))
            .clicked()
            {
                self.macro_actions[n].push(MacroAction::Down(0x04));
                macro_changed = true;
                self.macro_key_pick = Some((n, self.macro_actions[n].len() - 1));
            }
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.plus_up"),
                picker_scaled_size(ui.ctx(), 64.0, 30.0),
                true,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.release_a_key",
            ))
            .clicked()
            {
                self.macro_actions[n].push(MacroAction::Up(0x04));
                macro_changed = true;
                self.macro_key_pick = Some((n, self.macro_actions[n].len() - 1));
            }
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.plus_delay"),
                picker_scaled_size(ui.ctx(), 82.0, 30.0),
                true,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "macro_editor.pause_in_milliseconds",
            ))
            .clicked()
            {
                self.macro_actions[n].push(MacroAction::Delay(100));
                macro_changed = true;
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_clear_macro = self.macro_has_content(n)
                || self
                    .macro_names
                    .get(n)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
                || self
                    .macro_descriptions
                    .get(n)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "key_picker_text.clear_all"),
                picker_scaled_size(ui.ctx(), 86.0, 30.0),
                can_clear_macro,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.remove_all_actions_from_this_macro",
            ))
            .clicked()
            {
                self.macro_undo_stack
                    .push((n, self.macro_actions[n].clone()));
                self.macro_actions[n].clear();
                if n < self.macro_texts.len() {
                    self.macro_texts[n].clear();
                }
                if n < self.macro_names.len() {
                    self.macro_names[n].clear();
                }
                if n < self.macro_descriptions.len() {
                    self.macro_descriptions[n].clear();
                }
                macro_changed = true;
                macro_metadata_changed = true;
            }
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "key_picker_text.undo_undo"),
                picker_scaled_size(ui.ctx(), 78.0, 30.0),
                !self.macro_undo_stack.is_empty(),
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.undo_last_change",
            ))
            .clicked()
            {
                if let Some((idx, prev)) = self.macro_undo_stack.pop() {
                    if idx < self.macro_actions.len() {
                        self.macro_actions[idx] = prev;
                        if idx == n {
                            macro_changed = true;
                        } else {
                            self.encode_macro(idx);
                            self.mark_macros_dirty();
                        }
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if picker_button(
                    ui,
                    picker_ok_label(self.language),
                    picker_scaled_size(ui.ctx(), 72.0, 30.0),
                    true,
                    false,
                )
                .clicked()
                {
                    self.result = Some((0x7700 + n as u16).into());
                    self.open = false;
                }
            });
        });

        if macro_changed {
            self.encode_macro(n);
            self.mark_macros_dirty();
        }
        if macro_metadata_changed {
            self.macro_metadata_dirty = true;
        }

        selected_macro
    }

    fn macro_has_content(&self, n: usize) -> bool {
        self.macro_actions
            .get(n)
            .map(|a| !a.is_empty())
            .unwrap_or(false)
            || self
                .macro_texts
                .get(n)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
    }

    fn ensure_macro_meta_len(&mut self, n: usize) {
        while self.macro_texts.len() <= n {
            self.macro_texts.push(Vec::new());
        }
        while self.macro_names.len() <= n {
            self.macro_names.push(String::new());
        }
        while self.macro_descriptions.len() <= n {
            self.macro_descriptions.push(String::new());
        }
        while self.macro_actions.len() <= n {
            self.macro_actions.push(vec![]);
        }
    }

    pub(super) fn macro_display_name(&self, n: usize) -> String {
        match self.macro_names.get(n) {
            Some(name) if !name.trim().is_empty() => name.clone(),
            _ => format!("M{}", n),
        }
    }

    fn macro_description(&self, n: usize) -> Option<String> {
        self.macro_descriptions
            .get(n)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub(super) fn encode_macro(&mut self, n: usize) -> bool {
        while self.macro_texts.len() <= n {
            self.macro_texts.push(Vec::new());
        }
        while self.macro_actions.len() <= n {
            self.macro_actions.push(vec![]);
        }
        let encoded = encode_macro_actions(&self.macro_actions[n]);
        let changed = self.macro_texts[n] != encoded;
        if changed {
            self.macro_texts[n] = encoded;
        }
        changed
    }

    pub(super) fn show_vial_macros(&mut self, ui: &mut egui::Ui) {
        let previous = self.macro_inline_selected.unwrap_or(0);
        let selected = self.show_macro_editor_contents(
            ui,
            previous,
            "macro_grid_inline",
            "add_action_inline",
            "Saved to device when you close the keycode picker",
        );
        self.macro_inline_selected = Some(selected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_basic_macro_tap_as_vial_short_action() {
        let mut picker = KeycodePicker {
            macro_actions: vec![vec![MacroAction::Tap(0x0006)]],
            ..Default::default()
        };

        picker.encode_macro(0);

        assert_eq!(picker.macro_texts[0].as_slice(), &[0x01, 0x01, 0x06]);
    }

    #[test]
    fn encodes_modified_macro_tap_as_vial_extended_action() {
        let mut picker = KeycodePicker {
            macro_actions: vec![vec![MacroAction::Tap(0x0106)]],
            ..Default::default()
        };

        picker.encode_macro(0);

        assert_eq!(picker.macro_texts[0].as_slice(), &[0x01, 0x05, 0x06, 0x01]);
    }

    #[test]
    fn encodes_zero_low_byte_extended_keycode_with_vial_escape() {
        let mut encoded = Vec::new();

        append_macro_key_action(&mut encoded, SS_TAP_CODE, VIAL_MACRO_EXT_TAP, 0xA000);

        assert_eq!(encoded, [0x01, 0x05, 0xA0, 0xFF]);
    }

    #[test]
    fn preserves_raw_macro_bytes_that_cannot_be_decoded() {
        let raw_macro = [
            0xEF,
            SS_QMK_PREFIX,
            0x99,
            0xAA,
            SS_QMK_PREFIX,
            SS_DELAY_CODE,
            0x10,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert!(matches!(actions[0], MacroAction::Raw(_)));
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }

    #[test]
    fn preserves_unsupported_send_string_keycodes_as_raw() {
        let raw_macro = [
            SS_QMK_PREFIX,
            SS_DOWN_CODE,
            0xEF,
            SS_QMK_PREFIX,
            SS_UP_CODE,
            0xEF,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert_eq!(
            actions,
            vec![
                MacroAction::Raw(vec![SS_QMK_PREFIX, SS_DOWN_CODE, 0xEF]),
                MacroAction::Raw(vec![SS_QMK_PREFIX, SS_UP_CODE, 0xEF]),
            ]
        );
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }

    #[test]
    fn decodes_supported_send_string_modifiers() {
        let raw_macro = [
            SS_QMK_PREFIX,
            SS_DOWN_CODE,
            0xE0,
            SS_QMK_PREFIX,
            SS_UP_CODE,
            0xE0,
        ];

        let actions = decode_macro_actions(&raw_macro);

        assert_eq!(
            actions,
            vec![MacroAction::Down(0xE0), MacroAction::Up(0xE0)]
        );
        assert_eq!(encode_macro_actions(&actions), raw_macro);
    }
}
