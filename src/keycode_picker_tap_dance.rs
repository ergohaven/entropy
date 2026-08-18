use super::*;

impl KeycodePicker {
    fn assign_tap_dance_slot(&mut self, slot: u8) {
        if (slot as usize) >= self.tap_dance_entries.len() {
            return;
        }
        self.result = Some((0x5700 + slot as u16).into());
        self.tap_dance_editor_open = Some(slot);
        self.open = false;
    }

    fn tap_dance_has_content(&self, n: usize) -> bool {
        self.tap_dance_entries.get(n).is_some_and(|td| {
            !td.on_tap.is_no()
                || !td.on_hold.is_no()
                || !td.on_double_tap.is_no()
                || !td.on_tap_hold.is_no()
                || td.tapping_term != 200
        })
    }

    fn show_tap_dance_slot_grid(
        &mut self,
        ui: &mut egui::Ui,
        selected: u8,
        grid_id: &'static str,
    ) -> Option<u8> {
        if self.tap_dance_entries.is_empty() {
            return None;
        }

        let columns = ((ui.available_width() + 4.0) / 52.0)
            .floor()
            .clamp(4.0, 16.0) as usize;
        let mut picked = None;
        egui::Frame::NONE.show(ui, |ui| {
            let rows = self.tap_dance_entries.len().div_ceil(columns);
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
                            for n in 0..self.tap_dance_entries.len() as u8 {
                                self.ensure_tap_dance_name_len(n as usize);
                                let is_active = n == selected;
                                let display_name = self.tap_dance_display_name(n as usize);
                                let id_text = format!("TD{}", n);
                                let mut resp = picker_slot_button(
                                    ui,
                                    &id_text,
                                    &display_name,
                                    is_active,
                                    self.tap_dance_has_content(n as usize),
                                );
                                if display_name != id_text {
                                    resp = resp.on_hover_text(display_name.clone());
                                }
                                if resp.clicked() {
                                    picked = Some(n);
                                }
                                if (n as usize + 1).is_multiple_of(columns) {
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
        picked
    }

    pub(crate) fn show_tap_dance_settings_editor(&mut self, ui: &mut egui::Ui) {
        if self.tap_dance_entries.is_empty() {
            self.tap_dance_editor_open = None;
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "tap_dance_editor.no_tap_dance_slots_available_on_this_keyboard",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return;
        }

        let selected = match self.tap_dance_editor_open {
            Some(n) if (n as usize) < self.tap_dance_entries.len() => n,
            _ => 0,
        };
        self.tap_dance_editor_open = Some(selected);
        self.ensure_tap_dance_name_len(selected as usize);

        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "tap_dance_editor.choose_tap_dance",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        if let Some(picked) = self.show_tap_dance_slot_grid(ui, selected, "tap_dance_grid_settings")
        {
            self.tap_dance_editor_open = Some(picked);
        }
        ui.add_space(crate::ui_style::modal_space_sm());

        let n = self.tap_dance_editor_open.unwrap_or(0) as usize;
        self.ensure_tap_dance_name_len(n);
        let scale = responsive_picker_element_scale(ui.ctx());
        let td_font_size = 14.0 * scale;
        let custom_pairs = self.custom_keycode_pairs();
        ui.add_space(4.0 * scale);
        let prev_name = self.tap_dance_names.get(n).cloned().unwrap_or_default();
        let mut edited_name = prev_name.clone();
        let resp = crate::ui_style::modern_text_field_sized(
            ui,
            ui.make_persistent_id(("tap_dance_name", n)),
            &mut edited_name,
            124.0 * scale,
            32.0 * scale,
            crate::i18n::tr_catalog(self.language, "tap_dance_editor.td_name"),
            7,
            egui::Align::Center,
        );
        if resp.changed() {
            let trimmed: String = edited_name.chars().take(7).collect();
            if trimmed != prev_name {
                self.push_tap_dance_undo(n);
                self.ensure_tap_dance_name_len(n);
                self.tap_dance_names[n] = trimmed;
                self.tap_dance_dirty = true;
            }
        }
        ui.add_space(8.0);

        let fields = [
            (
                crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_tap"),
                crate::i18n::tr_catalog(self.language, "key_picker_text.key_sent_on_single_tap"),
                0u8,
            ),
            (
                crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_hold"),
                crate::i18n::tr_catalog(self.language, "key_picker_text.key_sent_when_held"),
                1,
            ),
            (
                crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_double_tap"),
                crate::i18n::tr_catalog(self.language, "key_picker_text.key_sent_on_double_tap"),
                2,
            ),
            (
                crate::i18n::tr_catalog(self.language, "tap_dance_editor.on_tap_plus_hold"),
                crate::i18n::tr_catalog(self.language, "key_picker_text.key_sent_on_tap_then_hold"),
                3,
            ),
        ];

        egui::Grid::new("td_fields_inline")
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for (label, tooltip, field_id) in &fields {
                    ui.add(
                        egui::Label::new(RichText::new(*label).size(td_font_size).strong())
                            .sense(egui::Sense::hover()),
                    )
                    .on_hover_text(*tooltip);

                    let kc = match field_id {
                        0 => self.tap_dance_entries[n].on_tap,
                        1 => self.tap_dance_entries[n].on_hold,
                        2 => self.tap_dance_entries[n].on_double_tap,
                        3 => self.tap_dance_entries[n].on_tap_hold,
                        _ => Default::default(),
                    };
                    let kc_label = self.tap_dance_field_label(kc, &custom_pairs);
                    if picker_button(ui, &kc_label, Vec2::new(120.0, 30.0), true, false)
                        .on_hover_text(if kc.is_no() {
                            crate::i18n::tr_catalog(
                                self.language,
                                "tap_dance_editor.click_to_assign_a_key",
                            )
                            .to_string()
                        } else {
                            crate::i18n::tr_text(self.language, &crate::app::key_binding_tooltip_with_macro_names(
                                kc,
                                &custom_pairs,
                                &self.layer_names,
                                &self.macro_names,
                                &self.macro_descriptions,
                                &self.tap_dance_names,
                            ))
                        })
                        .clicked()
                    {
                        self.td_mod_key_pick = None;
                        self.td_key_pick = Some((n, *field_id));
                    }
                    ui.end_row();
                }

                ui.add(
                    egui::Label::new(
                        RichText::new(crate::i18n::tr_catalog(
                            self.language,
                            "tap_dance_editor.tapping_term",
                        ))
                        .size(td_font_size)
                        .strong(),
                    )
                    .sense(egui::Sense::hover()),
                )
                .on_hover_text(crate::i18n::tr_catalog(
                    self.language,
                    "tap_dance_editor.time_in_milliseconds_to_distinguish_tap_from_hold_default_200",
                ));
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let prev_term = self.tap_dance_entries[n].tapping_term;
                    let mut term_str = prev_term.to_string();
                    if crate::ui_style::modern_text_field_sized(
                        ui,
                        ui.make_persistent_id(("tap_dance_term", n)),
                        &mut term_str,
                        76.0 * scale,
                        32.0 * scale,
                        "",
                        5,
                        egui::Align::Center,
                    )
                    .on_hover_text(crate::i18n::tr_catalog(
                        self.language,
                        "tap_dance_editor.tapping_term_is_in_milliseconds",
                    ))
                    .changed()
                    {
                        if let Ok(v) = term_str.parse::<u16>() {
                            let v = v.clamp(10, 3000);
                            if v != prev_term {
                                self.push_tap_dance_undo(n);
                                self.tap_dance_entries[n].tapping_term = v;
                                self.tap_dance_dirty = true;
                            }
                        }
                    }
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_clear_tap_dance = self
                .tap_dance_entries
                .get(n)
                .map(|td| {
                    !td.on_tap.is_no()
                        || !td.on_hold.is_no()
                        || !td.on_double_tap.is_no()
                        || !td.on_tap_hold.is_no()
                        || td.tapping_term != 200
                })
                .unwrap_or(false)
                || self
                    .tap_dance_names
                    .get(n)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "key_picker_text.clear_all"),
                picker_scaled_size(ui.ctx(), 86.0, 30.0),
                can_clear_tap_dance,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "tap_dance_editor.clear_all_actions_for_this_tap_dance",
            ))
            .clicked()
            {
                self.push_tap_dance_undo(n);
                if let Some(td) = self.tap_dance_entries.get_mut(n) {
                    td.on_tap = Default::default();
                    td.on_hold = Default::default();
                    td.on_double_tap = Default::default();
                    td.on_tap_hold = Default::default();
                    td.tapping_term = 200;
                }
                if n < self.tap_dance_names.len() {
                    self.tap_dance_names[n].clear();
                }
                self.tap_dance_dirty = true;
            }
            let can_undo_current = self
                .tap_dance_undo_stack
                .iter()
                .any(|(idx, _, _)| *idx == n);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "key_picker_text.undo_undo"),
                picker_scaled_size(ui.ctx(), 78.0, 30.0),
                can_undo_current,
                false,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                self.language,
                "tap_dance_editor.undo_last_tap_dance_change",
            ))
            .clicked()
            {
                if let Some(pos) = self
                    .tap_dance_undo_stack
                    .iter()
                    .rposition(|(idx, _, _)| *idx == n)
                {
                    let (idx, prev, prev_name) = self.tap_dance_undo_stack.remove(pos);
                    if idx < self.tap_dance_entries.len() {
                        self.tap_dance_entries[idx] = prev;
                    }
                    self.ensure_tap_dance_name_len(idx);
                    if idx < self.tap_dance_names.len() {
                        self.tap_dance_names[idx] = prev_name;
                    }
                    self.tap_dance_editor_open = Some(idx as u8);
                    self.tap_dance_dirty = true;
                }
            }
        });
    }

    pub(super) fn show_vial_tap_dance(&mut self, ui: &mut egui::Ui) {
        if self.tap_dance_entries.is_empty() {
            self.tap_dance_editor_open = None;
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "tap_dance_editor.no_tap_dance_slots_available_on_this_keyboard",
                ))
                .size(16.0)
                .color(Color32::from_gray(140)),
            );
            return;
        }

        let Some(selected) = self.tap_dance_editor_open else {
            ui.label(
                RichText::new(crate::i18n::tr_catalog(
                    self.language,
                    "tap_dance_editor.picker_intro",
                ))
                .size(11.0)
                .color(Color32::from_gray(150)),
            );
            ui.add_space(4.0);
            if picker_button(
                ui,
                crate::i18n::tr_catalog(self.language, "tap_dance_editor.picker_item"),
                Self::picker_key_size(ui.ctx()),
                true,
                false,
            )
            .clicked()
            {
                self.tap_dance_editor_open = Some(0);
            }
            return;
        };

        ui.label(
            RichText::new(crate::i18n::tr_catalog(
                self.language,
                "tap_dance_editor.choose_tap_dance",
            ))
            .size(11.0)
            .color(Color32::from_gray(150)),
        );
        ui.add_space(4.0);
        if let Some(picked) = self.show_tap_dance_slot_grid(ui, selected, "tap_dance_grid_picker") {
            self.assign_tap_dance_slot(picked);
        }
    }

    pub(super) fn ensure_tap_dance_name_len(&mut self, n: usize) {
        while self.tap_dance_names.len() <= n {
            self.tap_dance_names.push(String::new());
        }
    }

    pub(super) fn tap_dance_display_name(&self, n: usize) -> String {
        match self.tap_dance_names.get(n) {
            Some(name) if !name.trim().is_empty() => name.clone(),
            _ => format!("TD{}", n),
        }
    }

    pub(super) fn tap_dance_field_label(
        &self,
        value: crate::keyboard::KeyBinding,
        custom_pairs: &[crate::keyboard::CustomKeycode],
    ) -> String {
        if value.is_no() {
            return "None".to_string();
        }
        crate::app::key_binding_label_with_macro_names(
            value,
            custom_pairs,
            &self.layer_names,
            &self.macro_names,
            &self.tap_dance_names,
            self.key_legend_layout,
        )
    }

    pub(super) fn push_tap_dance_undo(&mut self, n: usize) {
        self.ensure_tap_dance_name_len(n);
        if let Some(td) = self.tap_dance_entries.get(n).cloned() {
            let name = self.tap_dance_names.get(n).cloned().unwrap_or_default();
            self.tap_dance_undo_stack.push((n, td, name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_slot_pick_assigns_tap_dance_without_marking_contents_dirty() {
        let mut picker = KeycodePicker {
            open: true,
            tap_dance_entries: vec![TapDanceEntry::default(); 4],
            ..Default::default()
        };

        picker.assign_tap_dance_slot(3);

        assert_eq!(
            picker.result.map(|binding| binding.vial_keycode()),
            Some(0x5703)
        );
        assert_eq!(picker.tap_dance_editor_open, Some(3));
        assert!(!picker.open);
        assert!(!picker.tap_dance_dirty);
    }
}
