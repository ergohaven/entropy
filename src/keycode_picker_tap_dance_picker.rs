use super::*;

impl KeycodePicker {
    pub(super) fn show_td_key_picker(&mut self, ctx: &egui::Context, td_idx: usize, field: u8) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.td_mod_key_pick.is_some() {
                self.td_mod_key_pick = None;
            } else {
                self.td_key_pick = None;
            }
            return;
        }

        let pending_mod_key = self.td_mod_key_pick;
        if let Some((pending_td_idx, pending_field, base)) = pending_mod_key {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        if !modifiers.any() {
                            if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                                if self.is_tap_dance_regular_key(qmk) {
                                    self.set_tap_dance_field(
                                        pending_td_idx,
                                        pending_field,
                                        base | qmk,
                                    );
                                    self.td_mod_key_pick = None;
                                    self.td_key_pick = None;
                                }
                            }
                        }
                    }
                }
            });
        } else {
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        if let Some(qmk) = egui_key_to_qmk(*key, *modifiers) {
                            let is_basic_key = qmk > 0 && qmk < 0x0100;
                            let is_regular_key = (0x0100..0x2000).contains(&qmk)
                                && self.is_tap_dance_regular_key(qmk & 0x00FF);
                            if is_basic_key || is_regular_key {
                                self.set_tap_dance_field(td_idx, field, qmk);
                                self.td_key_pick = None;
                            }
                        }
                    }
                }
            });
        }

        let field_name = match field {
            0 => crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_tap"),
            1 => crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_hold"),
            2 => crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_double_tap"),
            3 => crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_tap_plus_hold"),
            _ => "?",
        };
        let helper_text = match field {
            0 => crate::i18n::tr_catalog(
                self.language,
                "key_picker_text.best_for_normal_keys_navigation_media_and_special_actions",
            ),
            1 => crate::i18n::tr_catalog(self.language, "key_picker_text.hold_actions_are_limited_to_left_right_modifiers_and_layers"),
            2 => crate::i18n::tr_catalog(self.language, "key_picker_text.best_for_a_second_tap_action_usually_another_normal_key_or_command"),
            3 => crate::i18n::tr_catalog(self.language, "key_picker_text.tap_then_hold_actions_are_limited_to_left_right_modifiers_and_layers"),
            _ => tr_picker(self.language, "key_picker.press_key_or_click_cancel"),
        };
        let mut still_open = true;
        let popup_size = key_picker_popup_size(ctx);
        let window_title = if let Some((_pending_td_idx, _pending_field, base)) = pending_mod_key {
            format!(
                "{}: {}",
                tr_picker(self.language, "key_picker.pick_modifier_combo_title"),
                picker_mod_key_label(base)
            )
        } else {
            crate::i18n::tr_catalog_format(
                self.language,
                "key_picker.pick_key_for",
                &[("field", field_name)],
            )
        };
        crate::ui_style::centered_modal_window(
            ctx,
            window_title.as_str(),
            self.popup_state.id(PopupKey::TdKeyPickWindow),
            &mut still_open,
            popup_size,
        )
        .show(ctx, |ui| {
            apply_picker_button_visuals(ui);
            self.show_popup_view_mode_header(ui);
            crate::ui_style::modal_intro(
                ui,
                tr_picker(self.language, "key_picker.press_key_or_click_cancel"),
            );
            crate::ui_style::modal_hint(
                ui,
                if pending_mod_key.is_some() {
                    tr_picker(self.language, "key_picker.pending_mod_hint")
                } else {
                    helper_text
                },
            );
            ui.add_space(crate::ui_style::modal_space_xs());
            if pending_mod_key.is_some() {
                if picker_button(
                    ui,
                    tr_picker(self.language, "key_picker.cancel"),
                    crate::ui_style::modal_action_button_size(),
                    true,
                    false,
                )
                .clicked()
                {
                    self.td_mod_key_pick = None;
                }
            } else if picker_button(
                ui,
                tr_picker(self.language, "key_picker.none_clear"),
                crate::ui_style::modal_action_button_size(),
                true,
                false,
            )
            .clicked()
            {
                self.set_tap_dance_field(td_idx, field, 0);
                self.td_key_pick = None;
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .max_height(key_picker_popup_scroll_height(popup_size))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some((pending_td_idx, pending_field, base)) = pending_mod_key {
                        let key_choices = self.tap_dance_regular_key_choices();
                        if let Some(value) = self.show_popup_key_choice_view(ui, key_choices, false)
                        {
                            self.set_tap_dance_field(pending_td_idx, pending_field, base | value);
                            self.td_mod_key_pick = None;
                            self.td_key_pick = None;
                        }
                    } else if matches!(field, 1 | 3) {
                        self.show_tap_dance_hold_picker_content(ui, td_idx, field);
                    } else {
                        let key_choices: Vec<&'static crate::keycode::Keycode> = KEYCODES
                            .iter()
                            .filter(|kc| is_8bit_tap_key_choice(kc) && !kc.name.starts_with("RGB_"))
                            .collect();
                        match self.popup_view_mode {
                            PickerViewMode::Layout => {
                                if let Some(value) =
                                    self.show_popup_key_choice_view(ui, key_choices, true)
                                {
                                    self.set_tap_dance_field(td_idx, field, value);
                                    self.td_key_pick = None;
                                }
                            }
                            PickerViewMode::List => {
                                if let Some(value) = show_grouped_popup_key_buttons(
                                    ui,
                                    key_choices,
                                    &self.layer_names,
                                    true,
                                    self.language,
                                    self.key_legend_layout,
                                ) {
                                    self.set_tap_dance_field(td_idx, field, value);
                                    self.td_key_pick = None;
                                }
                                self.show_tap_dance_mod_key_section(ui, td_idx, field);
                                self.show_tap_dance_universal_symbol_sections(ui, td_idx, field);
                                self.show_tap_dance_layer_section(ui, td_idx, field);
                                self.show_tap_dance_custom_keycode_section(ui, td_idx, field);
                                self.show_tap_dance_macro_section(ui, td_idx, field);
                            }
                        }
                    }
                });
        });
        if !still_open {
            self.td_mod_key_pick = None;
            self.td_key_pick = None;
        }
    }

    fn show_tap_dance_hold_picker_content(&mut self, ui: &mut egui::Ui, td_idx: usize, field: u8) {
        if self.popup_view_mode == PickerViewMode::Layout {
            let key_choices = self.tap_dance_regular_key_choices();
            if let Some(value) = self.show_popup_key_choice_view(ui, key_choices, false) {
                self.set_tap_dance_field(td_idx, field, value);
                self.td_key_pick = None;
            }
            return;
        }

        ui.label(
            RichText::new(tr_picker(
                self.language,
                "key_picker.section_plain_modifiers",
            ))
            .size(11.0)
            .color(Color32::from_gray(150))
            .strong(),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let plain_modifiers = [
                ("Ctrl".to_owned(), 0x00E0u16, 0x00E4u16, "Ctrl".to_owned()),
                ("Shift".to_owned(), 0x00E1u16, 0x00E5u16, "Shift".to_owned()),
                ("Alt".to_owned(), 0x00E2u16, 0x00E6u16, "Alt".to_owned()),
                (
                    gui_label(false).to_string(),
                    0x00E3u16,
                    0x00E7u16,
                    gui_mod_name().to_string(),
                ),
            ];
            for (label, left_value, right_value, mod_name) in plain_modifiers {
                let resp =
                    picker_keycap_button(ui, &label, Self::picker_key_size(ui.ctx()), true, false)
                        .on_hover_text(crate::i18n::tr_text(
                            self.language,
                            &plain_modifier_tooltip(&mod_name),
                        ));
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.set_tap_dance_field(td_idx, field, left_value);
                    self.td_key_pick = None;
                }
                if resp.clicked_by(egui::PointerButton::Secondary) {
                    self.set_tap_dance_field(td_idx, field, right_value);
                    self.td_key_pick = None;
                }
            }
        });
        ui.add_space(crate::ui_style::modal_space_sm());

        let key_choices = self.tap_dance_regular_key_choices();
        if let Some(value) = show_grouped_popup_key_buttons(
            ui,
            key_choices,
            &self.layer_names,
            false,
            self.language,
            self.key_legend_layout,
        ) {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }

        self.show_tap_dance_mod_key_section(ui, td_idx, field);
        self.show_tap_dance_universal_symbol_sections(ui, td_idx, field);
        self.show_tap_dance_layer_section(ui, td_idx, field);
        self.show_tap_dance_custom_keycode_section(ui, td_idx, field);
    }

    fn show_tap_dance_custom_keycode_section(
        &mut self,
        ui: &mut egui::Ui,
        td_idx: usize,
        field: u8,
    ) {
        if !Self::tap_dance_custom_keycodes_allowed_in_field(field) {
            return;
        }
        if let Some(value) = self.show_custom_keycode_choice_section(ui) {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }
    }

    fn show_tap_dance_macro_section(&mut self, ui: &mut egui::Ui, td_idx: usize, field: u8) {
        if !Self::tap_dance_macros_allowed_in_field(field)
            || !self.supports_macro
            || self.macro_count == 0
        {
            return;
        }

        let choices: Vec<(u16, String, String)> = (0..self.macro_count.min(256))
            .map(|idx| {
                let id = format!("M{}", idx);
                let display_name = self.macro_display_name(idx);
                let tooltip = if display_name == id {
                    format!("{id} — macro")
                } else {
                    format!("{id} — macro {display_name}")
                };
                (0x7700 + idx as u16, display_name, tooltip)
            })
            .collect();

        if let Some(value) =
            show_grouped_popup_choice_buttons(ui, vec![("Macros", choices)], self.language)
        {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }
    }

    fn show_tap_dance_layer_section(&mut self, ui: &mut egui::Ui, td_idx: usize, field: u8) {
        let Some(kind) = Self::tap_dance_layer_key_pick_kind(field) else {
            return;
        };
        let layer_choices = self.layer_action_key_choices(kind);
        if let Some(value) =
            show_grouped_popup_choice_buttons(ui, vec![("Layers", layer_choices)], self.language)
        {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }
    }

    fn show_tap_dance_universal_symbol_sections(
        &mut self,
        ui: &mut egui::Ui,
        td_idx: usize,
        field: u8,
    ) {
        let language = self.language;
        if let Some(value) = show_universal_symbol_section(
            ui,
            language,
            "key_picker.section_universal_symbols",
            UNIVERSAL_MAIN_SYMBOL_ORDER,
            true,
        ) {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }
        ui.add_space(crate::ui_style::modal_space_sm());
        if let Some(value) = show_universal_symbol_section(
            ui,
            language,
            "key_picker.section_extra_universal_symbols",
            UNIVERSAL_EXTRA_SYMBOL_ORDER,
            false,
        ) {
            self.set_tap_dance_field(td_idx, field, value);
            self.td_key_pick = None;
        }
        ui.add_space(crate::ui_style::modal_space_sm());
    }

    fn show_tap_dance_mod_key_section(&mut self, ui: &mut egui::Ui, td_idx: usize, field: u8) {
        ui.label(
            RichText::new(tr_picker(self.language, "key_picker.section_mod_key"))
                .size(11.0)
                .color(Color32::from_gray(150))
                .strong(),
        );
        ui.add_space(4.0);
        let shortcuts: Vec<(String, u16, u16, String)> = vec![
            (picker_mod_key_label(0x0100), 0x0100, 0x1100, "Ctrl".into()),
            (picker_mod_key_label(0x0200), 0x0200, 0x1200, "Shift".into()),
            (picker_mod_key_label(0x0400), 0x0400, 0x1400, "Alt".into()),
            (
                picker_mod_key_label(0x0800),
                0x0800,
                0x1800,
                gui_mod_name().to_string(),
            ),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, left_base, right_base, mod_name) in &shortcuts {
                let resp = ui
                    .add_sized(Self::picker_key_size(ui.ctx()), egui::Button::new(""))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                Self::paint_compact_picker_label(ui, &resp, label);
                if resp.clicked_by(egui::PointerButton::Primary) {
                    self.td_mod_key_pick = Some((td_idx, field, *left_base));
                }
                if resp.clicked_by(egui::PointerButton::Secondary) {
                    self.td_mod_key_pick = Some((td_idx, field, *right_base));
                }
                resp.on_hover_text(crate::i18n::tr_text(
                    self.language,
                    &mod_combo_tooltip(mod_name, true),
                ));
            }
        });
        ui.add_space(crate::ui_style::modal_space_sm());
    }

    fn tap_dance_regular_key_choices(&self) -> Vec<&'static crate::keycode::Keycode> {
        KEYCODES
            .iter()
            .filter(|kc| {
                is_8bit_tap_key_choice(kc)
                    && !matches!(kc.category, KeycodeCategory::Modifier)
                    && !kc.name.starts_with("RGB_")
            })
            .collect()
    }

    fn is_tap_dance_regular_key(&self, value: u16) -> bool {
        self.tap_dance_regular_key_choices()
            .iter()
            .any(|kc| kc.value == value)
    }

    fn tap_dance_layer_key_pick_kind(field: u8) -> Option<MacroKeyPickKind> {
        match field {
            0 | 2 => Some(MacroKeyPickKind::Tap),
            1 | 3 => Some(MacroKeyPickKind::Down),
            _ => None,
        }
    }

    fn tap_dance_custom_keycodes_allowed_in_field(field: u8) -> bool {
        matches!(field, 0..=3)
    }

    fn tap_dance_macros_allowed_in_field(field: u8) -> bool {
        matches!(field, 0 | 2)
    }

    fn set_tap_dance_field(&mut self, n: usize, field: u8, value: u16) {
        if n >= self.tap_dance_entries.len() {
            return;
        }
        let value = crate::keycode::normalize_output_symbol_keycode(value);
        let current = match self.tap_dance_entries.get(n) {
            Some(td) => match field {
                0 => td.on_tap,
                1 => td.on_hold,
                2 => td.on_double_tap,
                3 => td.on_tap_hold,
                _ => return,
            },
            None => return,
        };
        if current == value {
            return;
        }
        self.push_tap_dance_undo(n);
        if let Some(td) = self.tap_dance_entries.get_mut(n) {
            match field {
                0 => td.on_tap = value,
                1 => td.on_hold = value,
                2 => td.on_double_tap = value,
                3 => td.on_tap_hold = value,
                _ => {}
            }
        }
        self.tap_dance_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_dance_custom_keycodes_are_available_for_all_fields() {
        for field in 0..=3 {
            assert!(
                KeycodePicker::tap_dance_custom_keycodes_allowed_in_field(field),
                "field {field} should allow custom keycodes"
            );
        }
    }

    #[test]
    fn tap_dance_macros_remain_tap_actions_only() {
        assert!(KeycodePicker::tap_dance_macros_allowed_in_field(0));
        assert!(!KeycodePicker::tap_dance_macros_allowed_in_field(1));
        assert!(KeycodePicker::tap_dance_macros_allowed_in_field(2));
        assert!(!KeycodePicker::tap_dance_macros_allowed_in_field(3));
    }
}
