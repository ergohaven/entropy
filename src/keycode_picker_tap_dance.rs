use super::*;

impl KeycodePicker {
    pub(super) fn assign_tap_dance_slot(&mut self, slot: u8) {
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

    pub(super) fn show_tap_dance_slot_grid(
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
            crate::ui_style::modal_empty_state(
                ui,
                crate::i18n::tr_catalog(
                    self.language,
                    "tap_dance_editor.no_tap_dance_slots_available_on_this_keyboard",
                ),
                None,
            );
            return;
        }

        let mut selected = match self.tap_dance_editor_open {
            Some(slot) if (slot as usize) < self.tap_dance_entries.len() => slot,
            _ => 0,
        };
        let n = selected as usize;
        self.ensure_tap_dance_name_len(n);

        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let content_width = metrics.settings_content_width();
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.settings_control_width();
        let control_height = metrics.settings_control_height();
        let control_font_size = metrics.settings_control_font_size();
        let keycap_size = metrics.size(54.0, 54.0);
        let language = self.language;
        let custom_pairs = self.custom_keycode_pairs();
        let original_entry = self.tap_dance_entries[n].clone();
        let original_name = self.tap_dance_names[n].clone();
        let mut edited_entry = original_entry.clone();
        let mut edited_name = original_name.clone();
        let mut picked_slot = None;
        let mut undo_applied = false;

        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(metrics.value(4.0)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let list = crate::app::allocate_adaptive_settings_list_viewport(
                    ui,
                    "tap_dance_settings",
                    metrics,
                    7,
                    metrics.value(54.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;

                    for row_idx in list.first_visible_row..list.last_visible_row {
                        match row_idx {
                            0 => {
                                let labels = (0..self.tap_dance_entries.len())
                                    .map(|idx| {
                                        let name = self
                                            .tap_dance_names
                                            .get(idx)
                                            .map(|name| name.trim())
                                            .filter(|name| !name.is_empty());
                                        match name {
                                            Some(name) => format!("TD{idx}: {name}"),
                                            None => format!("TD{idx}"),
                                        }
                                    })
                                    .collect::<Vec<_>>();
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        language,
                                        "tap_dance_editor.picker_item",
                                    ),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "tap_dance_editor.select_tap_dance_slot",
                                    )),
                                    control_width,
                                    |ui| {
                                        let dropdown_id =
                                            ui.make_persistent_id("tap_dance_settings_slot");
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
                                    crate::i18n::tr_catalog(language, "tap_dance_editor.td_name"),
                                    true,
                                    (!list.suppress_tooltips).then_some(crate::i18n::tr_catalog(
                                        language,
                                        "tap_dance_editor.local_name_for_this_tap_dance",
                                    )),
                                    control_width,
                                    |ui| {
                                        let response = crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id(("tap_dance_name", n)),
                                            &mut edited_name,
                                            control_width,
                                            control_height,
                                            crate::i18n::tr_catalog(
                                                language,
                                                "tap_dance_editor.td_name",
                                            ),
                                            7,
                                            egui::Align::Center,
                                        );
                                        if response.changed() {
                                            edited_name = edited_name.chars().take(7).collect();
                                        }
                                    },
                                );
                            }
                            2..=5 => {
                                let field_id = (row_idx - 2) as u8;
                                let (label, tooltip, value) = match field_id {
                                    0 => (
                                        crate::i18n::tr_catalog(
                                            language,
                                            "tap_dance_editor.on_tap",
                                        ),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "key_picker_text.key_sent_on_single_tap",
                                        ),
                                        edited_entry.on_tap,
                                    ),
                                    1 => (
                                        crate::i18n::tr_catalog(
                                            language,
                                            "tap_dance_editor.on_hold",
                                        ),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "key_picker_text.key_sent_when_held",
                                        ),
                                        edited_entry.on_hold,
                                    ),
                                    2 => (
                                        crate::i18n::tr_catalog(
                                            language,
                                            "tap_dance_editor.on_double_tap",
                                        ),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "key_picker_text.key_sent_on_double_tap",
                                        ),
                                        edited_entry.on_double_tap,
                                    ),
                                    _ => (
                                        crate::i18n::tr_catalog(
                                            language,
                                            "tap_dance_editor.on_tap_plus_hold",
                                        ),
                                        crate::i18n::tr_catalog(
                                            language,
                                            "key_picker_text.key_sent_on_tap_then_hold",
                                        ),
                                        edited_entry.on_tap_hold,
                                    ),
                                };
                                let keycap_label = self.tap_dance_field_label(value, &custom_pairs);
                                let hover_text = if value.is_no() {
                                    crate::i18n::tr_catalog(
                                        language,
                                        "tap_dance_editor.click_to_assign_a_key",
                                    )
                                    .to_owned()
                                } else {
                                    crate::i18n::tr_text(
                                        language,
                                        &crate::app::key_binding_tooltip_with_macro_names(
                                            value,
                                            &custom_pairs,
                                            &self.layer_names,
                                            &self.macro_names,
                                            &self.macro_descriptions,
                                            &self.tap_dance_names,
                                        ),
                                    )
                                };
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    label,
                                    true,
                                    (!list.suppress_tooltips).then_some(tooltip),
                                    keycap_size.x,
                                    |ui| {
                                        let response = crate::ui_style::modern_keycap_button(
                                            ui,
                                            &keycap_label,
                                            keycap_size,
                                            true,
                                        )
                                        .on_hover_text(hover_text);
                                        if response.clicked() {
                                            self.td_mod_key_pick = None;
                                            self.td_key_pick = Some((n, field_id));
                                        }
                                    },
                                );
                            }
                            6 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        language,
                                        "tap_dance_editor.tapping_term",
                                    ),
                                    true,
                                    (!list.suppress_tooltips).then_some(
                                        crate::i18n::tr_catalog(
                                            language,
                                            "tap_dance_editor.time_in_milliseconds_to_distinguish_tap_from_hold_default_200",
                                        ),
                                    ),
                                    metrics.value(82.0),
                                    |ui| {
                                        let mut value =
                                            edited_entry.tapping_term.to_string();
                                        if crate::ui_style::modern_text_field_sized(
                                            ui,
                                            ui.make_persistent_id((
                                                "tap_dance_term",
                                                n,
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
                                            "tap_dance_editor.tapping_term_is_in_milliseconds",
                                        ))
                                        .changed()
                                        {
                                            if let Ok(parsed) = value.parse::<u16>() {
                                                edited_entry.tapping_term =
                                                    parsed.clamp(10, 3000);
                                            }
                                        }
                                    },
                                );
                            }
                            _ => {}
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
            },
        );

        ui.add_space(metrics.value(14.0));
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = metrics.value(8.0);
            let action_size = crate::ui_style::modal_action_button_size() * scale;
            let can_clear = !edited_entry.on_tap.is_no()
                || !edited_entry.on_hold.is_no()
                || !edited_entry.on_double_tap.is_no()
                || !edited_entry.on_tap_hold.is_no()
                || edited_entry.tapping_term != 200
                || !edited_name.trim().is_empty();
            let clear_response = crate::ui_style::modern_button_with_font(
                ui,
                crate::i18n::tr_catalog(language, "alt_repeat_editor.clear"),
                action_size,
                control_font_size,
                can_clear,
            )
            .on_hover_text(crate::i18n::tr_catalog(
                language,
                "tap_dance_editor.clear_all_actions_for_this_tap_dance",
            ));
            if clear_response.clicked() && can_clear {
                edited_entry = TapDanceEntry {
                    tapping_term: 200,
                    ..Default::default()
                };
                edited_name.clear();
            }

            let undo_position = self
                .tap_dance_undo_stack
                .iter()
                .rposition(|(slot, _, _)| *slot == n);
            let undo_response = crate::ui_style::modern_button_with_font(
                ui,
                crate::i18n::tr_catalog(language, "alt_repeat_editor.undo"),
                action_size,
                control_font_size,
                undo_position.is_some(),
            )
            .on_hover_text(crate::i18n::tr_catalog(
                language,
                "tap_dance_editor.undo_last_tap_dance_change",
            ));
            if undo_response.clicked() {
                if let Some(position) = undo_position {
                    let (_, previous, previous_name) = self.tap_dance_undo_stack.remove(position);
                    edited_entry = previous;
                    edited_name = previous_name;
                    undo_applied = true;
                }
            }
        });

        if edited_entry != original_entry || edited_name != original_name {
            if !undo_applied {
                self.tap_dance_undo_stack
                    .push((n, original_entry, original_name));
                if self.tap_dance_undo_stack.len() > 64 {
                    self.tap_dance_undo_stack.remove(0);
                }
            }
            self.tap_dance_entries[n] = edited_entry;
            self.tap_dance_names[n] = edited_name;
            self.tap_dance_dirty = true;
        }
        if let Some(slot) = picked_slot {
            selected = slot;
            self.tap_dance_editor_open = Some(slot);
        }

        let _ = selected;
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
