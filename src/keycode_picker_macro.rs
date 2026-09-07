use super::*;

const SS_QMK_PREFIX: u8 = 1;
const SS_TAP_CODE: u8 = 1;
const SS_DOWN_CODE: u8 = 2;
const SS_UP_CODE: u8 = 3;
const SS_DELAY_CODE: u8 = 4;
const VIAL_MACRO_EXT_TAP: u8 = 5;
const VIAL_MACRO_EXT_DOWN: u8 = 6;
const VIAL_MACRO_EXT_UP: u8 = 7;

fn macro_settings_total_rows(action_count: usize) -> usize {
    // Slot, name, description, all configured actions, and one trailing
    // selector that creates the next action in place.
    4 + action_count
}

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
    pub(super) fn assign_macro_slot(&mut self, slot: u8) {
        if (slot as usize) >= self.macro_count {
            return;
        }
        self.result = Some((0x7700 + slot as u16).into());
        self.macro_inline_selected = Some(slot);
        self.open = false;
    }

    pub(super) fn show_macro_slot_grid(
        &mut self,
        ui: &mut egui::Ui,
        selected_macro: u8,
        grid_id: &'static str,
    ) -> Option<u8> {
        let slot_count = self.macro_count.min(u8::MAX as usize + 1);
        if slot_count == 0 {
            return None;
        }

        let columns = ((ui.available_width() + 4.0) / 52.0)
            .floor()
            .clamp(4.0, 16.0) as usize;
        let mut picked = None;
        egui::Frame::NONE.show(ui, |ui| {
            let rows = slot_count.div_ceil(columns);
            let slot_scroll_height =
                (rows.min(2) as f32 * 43.0 + 4.0) * responsive_picker_element_scale(ui.ctx());
            ui.set_max_height(slot_scroll_height);
            egui::ScrollArea::vertical()
                .max_height(slot_scroll_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new(grid_id)
                        .num_columns(columns)
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
                                    picked = Some(i);
                                }
                                if (i as usize + 1).is_multiple_of(columns) {
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
        picked
    }

    pub(crate) fn show_macro_settings_editor(
        &mut self,
        ui: &mut egui::Ui,
        raw_n: u8,
        grid_id: &'static str,
    ) -> u8 {
        let slot_count = self.macro_count.min(u8::MAX as usize + 1);
        if slot_count == 0 {
            crate::ui_style::modal_empty_state(
                ui,
                crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.no_macro_slots_available_on_this_keyboard",
                ),
                None,
            );
            return 254;
        }

        let mut selected_macro = if (raw_n as usize) < slot_count {
            raw_n
        } else {
            0
        };
        let n = selected_macro as usize;
        self.ensure_macro_meta_len(n);

        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let content_width = metrics.settings_content_width();
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.settings_control_width();
        let control_height = metrics.settings_control_height();
        let control_font_size = metrics.settings_control_font_size();
        let action_control_width = metrics.value(300.0);
        let language = self.language;
        let custom_pairs = self.custom_keycode_pairs();
        let original_actions = self.macro_actions[n].clone();
        let mut edited_actions = original_actions.clone();
        let original_name = self.macro_names[n].clone();
        let original_description = self.macro_descriptions[n].clone();
        let mut edited_name = original_name.clone();
        let mut edited_description = original_description.clone();
        let mut picked_slot = None;
        let mut remove_idx = None;
        let mut move_up = None;
        let mut move_down = None;
        let mut undo_applied = false;

        if let Some(notice) = self.macro_ext_keycodes_notice(language) {
            crate::ui_style::modal_hint(ui, notice);
            ui.add_space(metrics.value(4.0));
        }

        let total_rows = macro_settings_total_rows(edited_actions.len());
        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(metrics.value(4.0)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let list = crate::app::allocate_adaptive_settings_list_viewport(
                    ui,
                    "macro_settings",
                    metrics,
                    total_rows,
                    metrics.value(94.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;

                    for row_idx in list.first_visible_row..list.last_visible_row {
                        match row_idx {
                            0 => {
                                let labels = (0..slot_count)
                                    .map(|idx| {
                                        let name = self
                                            .macro_names
                                            .get(idx)
                                            .map(|name| name.trim())
                                            .filter(|name| !name.is_empty());
                                        match name {
                                            Some(name) => format!("M{idx}: {name}"),
                                            None => format!("M{idx}"),
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(language, "macro_editor.picker_item"),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.select_macro_slot",
                                    )),
                                    control_width,
                                    |ui| {
                                        let dropdown_id =
                                            ui.make_persistent_id(("macro_settings_slot", grid_id));
                                        let (_, picked) =
                                            crate::ui_style::modern_dropdown_select_sized(
                                                ui,
                                                dropdown_id,
                                                &labels,
                                                n,
                                                control_width,
                                                control_height,
                                                control_font_size,
                                            );
                                        picked_slot = picked.map(|idx| idx as u8);
                                    },
                                );
                            }
                            1 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(language, "macro_editor.macro_name"),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.local_name_for_this_macro",
                                    )),
                                    control_width,
                                    |ui| {
                                        let response = crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id(("macro_name", grid_id, n)),
                                            &mut edited_name,
                                            control_width,
                                            control_height,
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.macro_name",
                                            ),
                                            MACRO_NAME_CHAR_LIMIT,
                                            egui::Align::Center,
                                        );
                                        if response.changed() {
                                            limit_chars(&mut edited_name, MACRO_NAME_CHAR_LIMIT);
                                        }
                                    },
                                );
                            }
                            2 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.macro_description",
                                    ),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "macro_editor.optional_description_for_this_macro",
                                    )),
                                    control_width,
                                    |ui| {
                                        let response = crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id((
                                                "macro_description",
                                                grid_id,
                                                n,
                                            )),
                                            &mut edited_description,
                                            control_width,
                                            control_height,
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.macro_description",
                                            ),
                                            MACRO_DESCRIPTION_CHAR_LIMIT,
                                            egui::Align::Min,
                                        );
                                        if response.changed() {
                                            limit_chars(
                                                &mut edited_description,
                                                MACRO_DESCRIPTION_CHAR_LIMIT,
                                            );
                                        }
                                    },
                                );
                            }
                            action_row => {
                                let action_idx = action_row - 3;
                                if action_idx == edited_actions.len() {
                                    let mut added_action = None;
                                    crate::ui_style::settings_list_row_with_tooltip(
                                        ui,
                                        row_content_width,
                                        row_height,
                                        crate::i18n::tr_catalog(language, "macro_editor.actions"),
                                        true,
                                        (!list.suppress_tooltips).then_some(
                                            crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.add_actions_below",
                                            ),
                                        ),
                                        action_control_width,
                                        |ui| {
                                            ui.spacing_mut().item_spacing.x = metrics.value(4.0);
                                            let add_width =
                                                (action_control_width - metrics.value(16.0)) / 5.0;
                                            let add_size = egui::vec2(add_width, control_height);
                                            for (label_key, tooltip_key, action) in [
                                                (
                                                    "macro_editor.plus_text",
                                                    "macro_editor.type_characters",
                                                    MacroAction::Text(String::new()),
                                                ),
                                                (
                                                    "macro_editor.plus_tap",
                                                    "macro_editor.press_and_release_a_key",
                                                    MacroAction::Tap(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_down",
                                                    "macro_editor.hold_a_key",
                                                    MacroAction::Down(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_up",
                                                    "macro_editor.release_a_key",
                                                    MacroAction::Up(0x04),
                                                ),
                                                (
                                                    "macro_editor.plus_delay",
                                                    "macro_editor.pause_in_milliseconds",
                                                    MacroAction::Delay(100),
                                                ),
                                            ] {
                                                if crate::ui_style::modern_button_with_font(
                                                    ui,
                                                    crate::i18n::tr_catalog(language, label_key),
                                                    add_size,
                                                    metrics.value(10.5),
                                                    true,
                                                )
                                                .on_hover_text(crate::i18n::tr_catalog(
                                                    language,
                                                    tooltip_key,
                                                ))
                                                .clicked()
                                                {
                                                    added_action = Some(action);
                                                }
                                            }
                                        },
                                    );
                                    if let Some(action) = added_action {
                                        let opens_key_picker = matches!(
                                            action,
                                            MacroAction::Tap(_)
                                                | MacroAction::Down(_)
                                                | MacroAction::Up(_)
                                        );
                                        edited_actions.push(action);
                                        if opens_key_picker {
                                            self.macro_key_pick =
                                                Some((n, edited_actions.len() - 1));
                                        }
                                    }
                                    continue;
                                }

                                let action_count = edited_actions.len();
                                let action = &mut edited_actions[action_idx];
                                let (label, tooltip) = match action {
                                    MacroAction::Text(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.text"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.types_text_characters_one_by_one",
                                        ),
                                    ),
                                    MacroAction::Tap(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.tap"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.press_and_release_a_key",
                                        ),
                                    ),
                                    MacroAction::Down(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.down"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.press_a_key_hold_until_up",
                                        ),
                                    ),
                                    MacroAction::Up(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.up"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.release_a_previously_pressed_key",
                                        ),
                                    ),
                                    MacroAction::Delay(_) => (
                                        crate::i18n::tr_catalog(language, "macro_editor.delay"),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.wait_before_next_action",
                                        ),
                                    ),
                                    MacroAction::Raw(_) => {
                                        ("Raw", "Raw macro bytes preserved for compatibility")
                                    }
                                };

                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    label,
                                    true,
                                    (!list.suppress_tooltips).then_some(tooltip),
                                    action_control_width,
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = metrics.value(4.0);
                                        let small_size = metrics.size(26.0, control_height / scale);
                                        let up_response = crate::ui_style::modern_button_with_font(
                                            ui,
                                            "↑",
                                            small_size,
                                            control_font_size,
                                            action_idx > 0,
                                        )
                                        .on_hover_text(crate::i18n::tr_catalog(
                                            language,
                                            "macro_editor.move_up",
                                        ));
                                        if up_response.clicked() && action_idx > 0 {
                                            move_up = Some(action_idx);
                                        }
                                        let down_response =
                                            crate::ui_style::modern_button_with_font(
                                                ui,
                                                "↓",
                                                small_size,
                                                control_font_size,
                                                action_idx + 1 < action_count,
                                            )
                                            .on_hover_text(crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.move_down",
                                            ));
                                        if down_response.clicked() && action_idx + 1 < action_count
                                        {
                                            move_down = Some(action_idx);
                                        }

                                        let editor_width = metrics.value(200.0);
                                        match action {
                                            MacroAction::Text(text) => {
                                                crate::ui_style::modern_text_field_sized(
                                                    ui,
                                                    ui.make_persistent_id((
                                                        "macro_text_action",
                                                        grid_id,
                                                        n,
                                                        action_idx,
                                                    )),
                                                    text,
                                                    editor_width,
                                                    control_height,
                                                    crate::i18n::tr_catalog(
                                                        language,
                                                        "macro_editor.type_text_here",
                                                    ),
                                                    256,
                                                    egui::Align::Min,
                                                )
                                                .on_hover_text(crate::i18n::tr_catalog(
                                                    language,
                                                    "macro_editor.characters_to_type_when_this_macro_runs",
                                                ));
                                            }
                                            MacroAction::Tap(keycode)
                                            | MacroAction::Down(keycode)
                                            | MacroAction::Up(keycode) => {
                                                let button_label =
                                                    keycode_label_with_names_and_layout(
                                                        *keycode,
                                                        &custom_pairs,
                                                        &self.layer_names,
                                                        self.key_legend_layout,
                                                    );
                                                let response =
                                                    crate::ui_style::modern_button_with_font(
                                                        ui,
                                                        &button_label,
                                                        egui::vec2(editor_width, control_height),
                                                        control_font_size,
                                                        true,
                                                    )
                                                    .on_hover_text(tooltip);
                                                if response.clicked() {
                                                    self.macro_key_pick = Some((n, action_idx));
                                                }
                                            }
                                            MacroAction::Delay(ms) => {
                                                let mut value = ms.to_string();
                                                if crate::ui_style::modern_text_field_sized(
                                                    ui,
                                                    ui.make_persistent_id((
                                                        "macro_delay",
                                                        grid_id,
                                                        n,
                                                        action_idx,
                                                    )),
                                                    &mut value,
                                                    metrics.value(82.0),
                                                    control_height,
                                                    "",
                                                    5,
                                                    egui::Align::Center,
                                                )
                                                .on_hover_text(crate::i18n::tr_catalog(
                                                    language,
                                                    "macro_editor.delay_is_in_milliseconds",
                                                ))
                                                .changed()
                                                {
                                                    if let Ok(parsed) = value.parse::<u16>() {
                                                        *ms = parsed;
                                                    }
                                                }
                                            }
                                            MacroAction::Raw(bytes) => {
                                                ui.add_sized(
                                                    egui::vec2(
                                                        editor_width,
                                                        control_height,
                                                    ),
                                                    egui::Label::new(
                                                        RichText::new(format_raw_macro_bytes(bytes))
                                                            .size(control_font_size)
                                                            .color(crate::ui_style::muted_text(
                                                                ui.visuals().dark_mode,
                                                            )),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(
                                                    "Entropy cannot edit these macro bytes yet, but will save them unchanged",
                                                );
                                            }
                                        }

                                        let remove_response =
                                            crate::ui_style::modern_button_with_font(
                                                ui,
                                                "×",
                                                small_size,
                                                control_font_size,
                                                true,
                                            )
                                            .on_hover_text(crate::i18n::tr_catalog(
                                                language,
                                                "macro_editor.remove_this_action",
                                            ));
                                        if remove_response.clicked() {
                                            remove_idx = Some(action_idx);
                                        }
                                    },
                                );
                            }
                        }
                    }
                });

                if list.has_scrollbar {
                    crate::ui_style::paint_floating_scrollbar_handle(
                        ui,
                        list.track_rect,
                        list.handle_height,
                        list.scroll_ratio,
                        list.track_hovered,
                    );
                }

                let action_size = crate::ui_style::modal_action_button_size() * scale;
                let action_gap = metrics.value(8.0);
                let actions_rect = crate::app::fixed_settings_action_bar_rect(
                    list.viewport,
                    metrics,
                    action_size,
                    2,
                    action_gap,
                );
                crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = action_gap;
                        let can_clear = !edited_actions.is_empty()
                            || !edited_name.trim().is_empty()
                            || !edited_description.trim().is_empty();
                        let clear_response = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(language, "alt_repeat_editor.clear"),
                            action_size,
                            control_font_size,
                            can_clear,
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            language,
                            "key_picker_text.remove_all_actions_from_this_macro",
                        ));
                        if clear_response.clicked() && can_clear {
                            edited_actions.clear();
                            edited_name.clear();
                            edited_description.clear();
                        }

                        let undo_position = self
                            .macro_undo_stack
                            .iter()
                            .rposition(|(macro_idx, _)| *macro_idx == n);
                        let undo_response = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(language, "alt_repeat_editor.undo"),
                            action_size,
                            control_font_size,
                            undo_position.is_some(),
                        )
                        .on_hover_text(crate::i18n::tr_catalog(
                            language,
                            "key_picker_text.undo_last_change",
                        ));
                        if undo_response.clicked() {
                            if let Some(position) = undo_position {
                                let (_, previous) = self.macro_undo_stack.remove(position);
                                edited_actions = previous;
                                undo_applied = true;
                            }
                        }
                    });
                });
            },
        );

        if let Some(idx) = remove_idx {
            if idx < edited_actions.len() {
                edited_actions.remove(idx);
                if self
                    .macro_key_pick
                    .is_some_and(|(macro_idx, action_idx)| macro_idx == n && action_idx >= idx)
                {
                    self.macro_key_pick = None;
                }
            }
        }
        if let Some(idx) = move_up {
            if idx > 0 && idx < edited_actions.len() {
                edited_actions.swap(idx, idx - 1);
            }
        }
        if let Some(idx) = move_down {
            if idx + 1 < edited_actions.len() {
                edited_actions.swap(idx, idx + 1);
            }
        }

        if edited_actions != original_actions {
            if !undo_applied {
                self.macro_undo_stack.push((n, original_actions));
                if self.macro_undo_stack.len() > 64 {
                    self.macro_undo_stack.remove(0);
                }
            }
            self.macro_actions[n] = edited_actions;
            self.encode_macro(n);
            self.mark_macros_dirty();
        }
        if edited_name != original_name || edited_description != original_description {
            self.macro_names[n] = edited_name;
            self.macro_descriptions[n] = edited_description;
            self.macro_metadata_dirty = true;
        }
        if let Some(slot) = picked_slot {
            selected_macro = slot;
            self.macro_inline_selected = Some(slot);
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

    pub(super) fn macro_description(&self, n: usize) -> Option<String> {
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
        if self.macro_count == 0 {
            self.macro_inline_selected = None;
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.no_macro_slots_available_on_this_keyboard",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return;
        }

        let Some(selected) = self.macro_inline_selected else {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "macro_editor.picker_intro",
                ))
                .size(11.0)
                .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "macro_editor.picker_item"),
                Self::picker_key_size(ui.ctx()),
                true,
                false,
            )
            .clicked()
            {
                self.macro_inline_selected = Some(0);
            }
            return;
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
        if let Some(picked) = self.show_macro_slot_grid(ui, selected, "macro_grid_picker") {
            self.assign_macro_slot(picked);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_editor_keeps_one_trailing_action_selector_row() {
        assert_eq!(macro_settings_total_rows(0), 4);
        assert_eq!(macro_settings_total_rows(3), 7);
    }

    #[test]
    fn explicit_slot_pick_assigns_macro_without_marking_contents_dirty() {
        let mut picker = KeycodePicker {
            open: true,
            macro_count: 8,
            ..Default::default()
        };

        picker.assign_macro_slot(4);

        assert_eq!(
            picker.result.map(|binding| binding.vial_keycode()),
            Some(0x7704)
        );
        assert_eq!(picker.macro_inline_selected, Some(4));
        assert!(!picker.open);
        assert!(!picker.macros_dirty);
    }

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
