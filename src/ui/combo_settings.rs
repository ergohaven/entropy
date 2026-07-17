use super::*;

impl EntropyApp {
    pub(super) fn draw_combo_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        content_rect: egui::Rect,
    ) -> bool {
        self.handle_combo_editor_input(ctx, false);
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let mut combo_keycap_hovered = false;

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                let scale = responsive_settings_editor_scale(ui.ctx());
                ui.add_space(18.0 * scale);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::ComboTitle))
                        .size(18.0 * scale)
                        .strong(),
                );
                ui.add_space(6.0 * scale);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::ComboDescription))
                        .size(13.0 * scale)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(18.0 * scale);
                combo_keycap_hovered = self.draw_combo_editor_content(ui, false);
            });
        });
        combo_keycap_hovered
    }

    pub(super) fn push_combo_undo(&mut self) {
        self.combo_undo_stack.push(ComboUndoSnapshot {
            entries: self.combo_entries.clone(),
            names: self.combo_names.clone(),
            colors: self.combo_colors.clone(),
            term: self.combo_term,
            selected: self.selected_combo,
            visible_count: self.combo_visible_count,
        });
        if self.combo_undo_stack.len() > 64 {
            self.combo_undo_stack.remove(0);
        }
    }

    fn open_combo_key_picker(&mut self, combo_idx: usize, field: ComboPickField) {
        self.combo_pick_target = Some((combo_idx, field));
        self.keycode_picker.layer_names = self.layer_names.clone();
        self.keycode_picker
            .open_full_key_picker(crate::keycode_picker::KeycodeTab::Basic);
    }

    fn handle_combo_editor_input(&mut self, ctx: &egui::Context, allow_close: bool) -> bool {
        if !self.keycode_picker.open
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && allow_close
        {
            return true;
        }
        false
    }

    fn draw_combo_editor_content(&mut self, ui: &mut egui::Ui, show_intro: bool) -> bool {
        let dark = ui.visuals().dark_mode;
        if show_intro {
            crate::ui_style::modal_hint(
                ui,
                crate::i18n::tr(
                    self.app_settings.language,
                    crate::i18n::Key::ComboDescription,
                ),
            );
        }

        if self.firmware != FirmwareProtocol::Vial {
            crate::ui_style::modal_empty_state(
                ui,
                "Dynamic combos are not supported for this firmware",
                None,
            );
            return false;
        }

        if self.combo_entries.is_empty() {
            crate::ui_style::modal_empty_state(
                ui,
                "This device does not report any dynamic combo slots",
                None,
            );
            return false;
        }

        self.selected_combo = self
            .selected_combo
            .min(self.combo_entries.len().saturating_sub(1));
        self.combo_names
            .resize(self.combo_entries.len(), String::new());
        normalize_combo_colors(&mut self.combo_colors, self.combo_entries.len());
        self.combo_visible_count = self.combo_entries.len().max(1);

        let combo_idx = self.selected_combo;
        let page_center_x = ui.max_rect().center().x;
        let combo_undo_snapshot = ComboUndoSnapshot {
            entries: self.combo_entries.clone(),
            names: self.combo_names.clone(),
            colors: self.combo_colors.clone(),
            term: self.combo_term,
            selected: self.selected_combo,
            visible_count: self.combo_visible_count,
        };
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let content_width = metrics.settings_content_width();
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.settings_control_width();
        let control_height = metrics.settings_control_height();
        let control_font_size = metrics.settings_control_font_size();
        let input_keys_control_width = metrics.value(228.0);
        let input_keys_row_height = row_height.max(metrics.value(62.0));
        let input_key_size = metrics.size(54.0, 54.0);
        let timeout_control_width = metrics.value(118.0);
        let custom_pairs = self
            .layout
            .as_ref()
            .map(|l| l.custom_keycodes.clone())
            .unwrap_or_default();
        let custom = custom_pairs.as_slice();
        let mut combo_keycap_hovered = false;
        let selected_combo_empty = self
            .combo_entries
            .get(combo_idx)
            .map(|entry| entry.keys.iter().all(|&k| k == 0) && entry.output == 0)
            .unwrap_or(true)
            && self
                .combo_names
                .get(combo_idx)
                .map(|name| name.trim().is_empty())
                .unwrap_or(true);
        let selected_text = match self.combo_names.get(combo_idx) {
            Some(name) if !name.trim().is_empty() => format!("C{}: {}", combo_idx, name.trim()),
            _ => format!("C{}", combo_idx),
        };
        let selected_text_color = if selected_combo_empty {
            app_inactive_entry_text(dark)
        } else {
            ui.visuals().text_color()
        };
        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(4.0 * scale),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_content_width,
                    row_height,
                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.entry"),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "combo_editor.select_combo_slot",
                    )),
                    control_width,
                    |ui| {
                        let dropdown_id = ui.make_persistent_id("combo_entry_dropdown");
                        let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
                            ui,
                            dropdown_id,
                            selected_text.as_str(),
                            selected_text_color,
                            control_width,
                            control_height,
                            control_font_size,
                        );
                        ui.style_mut().visuals.window_stroke =
                            crate::ui_style::modal_outline_stroke(dark);
                        ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                        crate::ui_style::popup_below_widget(
                            ui,
                            dropdown_id,
                            &dropdown_resp,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(control_width);
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                egui::ScrollArea::vertical()
                                    .id_salt("combo_entry_dropdown_scroll")
                                    .max_height(142.0 * scale)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for entry_idx in 0..self.combo_entries.len() {
                                            let empty = self
                                                .combo_entries
                                                .get(entry_idx)
                                                .map(|entry| {
                                                    entry.keys.iter().all(|&k| k == 0)
                                                        && entry.output == 0
                                                })
                                                .unwrap_or(true)
                                                && self
                                                    .combo_names
                                                    .get(entry_idx)
                                                    .map(|name| name.trim().is_empty())
                                                    .unwrap_or(true);
                                            let option_text = match self.combo_names.get(entry_idx)
                                            {
                                                Some(name) if !name.trim().is_empty() => {
                                                    format!("C{}: {}", entry_idx, name.trim())
                                                }
                                                _ => format!("C{}", entry_idx),
                                            };
                                            let selected = entry_idx == self.selected_combo;
                                            let (option_rect, option_resp) = ui
                                                .allocate_exact_size(
                                                    Vec2::new(control_width, 28.0 * scale),
                                                    Sense::click(),
                                                );
                                            if option_resp.hovered() {
                                                ui.ctx().set_cursor_icon(
                                                    egui::CursorIcon::PointingHand,
                                                );
                                            }
                                            let option_fill = if selected {
                                                if dark {
                                                    Color32::from_rgb(58, 58, 61)
                                                } else {
                                                    Color32::from_rgb(236, 236, 238)
                                                }
                                            } else if option_resp.hovered() {
                                                crate::ui_style::hover_fill(dark)
                                            } else {
                                                Color32::TRANSPARENT
                                            };
                                            ui.painter().rect_filled(option_rect, 7.0, option_fill);
                                            ui.painter().text(
                                                egui::pos2(
                                                    option_rect.left() + 10.0,
                                                    option_rect.center().y,
                                                ),
                                                egui::Align2::LEFT_CENTER,
                                                option_text,
                                                FontId::proportional(12.0 * scale),
                                                if selected {
                                                    ui.visuals().text_color()
                                                } else if empty {
                                                    app_inactive_entry_text(dark)
                                                } else {
                                                    app_muted_text(dark)
                                                },
                                            );
                                            if option_resp.clicked() {
                                                self.selected_combo = entry_idx;
                                                egui::Popup::close_all(ui.ctx());
                                            }
                                        }
                                    });
                            },
                        );
                    },
                );

                let mut combo_name_changed = false;
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_content_width,
                    row_height,
                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.name"),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "combo_editor.local_name_for_this_combo_slot",
                    )),
                    control_width,
                    |ui| {
                        if let Some(name) = self.combo_names.get_mut(combo_idx) {
                            let resp = crate::ui_style::modern_text_field_sized(
                                ui,
                                egui::Id::new(("combo_name", combo_idx)),
                                name,
                                control_width,
                                control_height,
                                crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "alt_repeat_editor.name",
                                ),
                                12,
                                egui::Align::Center,
                            );
                            combo_name_changed = resp.changed();
                            resp.clone().on_hover_text(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "alt_repeat_editor.stored_locally_in_entropy",
                            ));
                        }
                    },
                );
                if combo_name_changed {
                    self.combo_undo_stack.push(combo_undo_snapshot.clone());
                    self.combo_names_dirty = true;
                }

                let selected_color = self
                    .combo_colors
                    .get(combo_idx)
                    .copied()
                    .unwrap_or(COMBO_NO_COLOR);
                let color_palette = combo_color_palette(self.combo_entries.len());
                let color_swatch_width = metrics.value(64.0);
                let color_swatch_size = metrics.size(64.0, 34.0);
                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_content_width,
                    row_height,
                    crate::i18n::tr_catalog(self.app_settings.language, "common.color"),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "combo_editor.local_color_for_combo_slot",
                    )),
                    color_swatch_width,
                    |ui| {
                        let popup_id = ui.make_persistent_id(("combo_color_picker", combo_idx));
                        let popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
                        let swatch_border = if popup_open {
                            app_accent()
                        } else if dark {
                            Color32::from_gray(95)
                        } else {
                            Color32::from_gray(185)
                        };
                        let (swatch_rect, swatch_resp) =
                            ui.allocate_exact_size(color_swatch_size, Sense::click());
                        if swatch_resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if swatch_resp.clicked() {
                            egui::Popup::toggle_id(ui.ctx(), popup_id);
                        }
                        paint_combo_color_picker_button(
                            ui,
                            swatch_rect,
                            selected_color,
                            swatch_border,
                            dark,
                            scale,
                        );
                        swatch_resp
                            .clone()
                            .on_hover_text(combo_color_dropdown_label(
                                self.app_settings.language,
                                selected_color,
                                &color_palette,
                            ));
                        ui.style_mut().visuals.window_stroke =
                            crate::ui_style::modal_outline_stroke(dark);
                        ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                        crate::ui_style::popup_below_widget(
                            ui,
                            popup_id,
                            &swatch_resp,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                let cell = metrics.value(28.0);
                                let gap = metrics.value(6.0);
                                const COLS: usize = 5;
                                const VISIBLE_ROWS: usize = 5;
                                let picker_width = cell * COLS as f32 + gap * (COLS - 1) as f32;
                                let picker_height =
                                    cell * VISIBLE_ROWS as f32 + gap * (VISIBLE_ROWS - 1) as f32;
                                ui.set_min_width(picker_width);
                                ui.spacing_mut().item_spacing = Vec2::new(gap, gap);
                                egui::ScrollArea::vertical()
                                    .id_salt(("combo_color_picker_scroll", combo_idx))
                                    .max_height(picker_height)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        let mut options =
                                            Vec::with_capacity(color_palette.len() + 1);
                                        options.push(COMBO_NO_COLOR);
                                        options.extend(color_palette.iter().copied());
                                        for row in options.chunks(COLS) {
                                            ui.horizontal(|ui| {
                                                for color_value in row.iter().copied() {
                                                    let selected = color_value == selected_color;
                                                    let used_by_other = color_value
                                                        != COMBO_NO_COLOR
                                                        && self
                                                            .combo_colors
                                                            .iter()
                                                            .enumerate()
                                                            .any(|(idx, value)| {
                                                                idx != combo_idx
                                                                    && *value == color_value
                                                            });
                                                    let enabled = selected || !used_by_other;
                                                    let (cell_rect, cell_resp) = ui
                                                        .allocate_exact_size(
                                                            Vec2::splat(cell),
                                                            Sense::click(),
                                                        );
                                                    if enabled && cell_resp.hovered() {
                                                        ui.ctx().set_cursor_icon(
                                                            egui::CursorIcon::PointingHand,
                                                        );
                                                    }
                                                    paint_combo_color_picker_cell(
                                                        ui,
                                                        cell_rect,
                                                        color_value,
                                                        selected,
                                                        enabled,
                                                        dark,
                                                        scale,
                                                    );
                                                    let option_label = combo_color_dropdown_label(
                                                        self.app_settings.language,
                                                        color_value,
                                                        &color_palette,
                                                    );
                                                    if enabled {
                                                        cell_resp
                                                            .clone()
                                                            .on_hover_text(option_label);
                                                    } else {
                                                        cell_resp.clone().on_hover_text(
                                                            crate::i18n::tr_catalog(
                                                                self.app_settings.language,
                                                                "combo_editor.color_used",
                                                            ),
                                                        );
                                                    }
                                                    if enabled
                                                        && cell_resp.clicked()
                                                        && self.combo_colors.get(combo_idx).copied()
                                                            != Some(color_value)
                                                    {
                                                        self.combo_undo_stack
                                                            .push(combo_undo_snapshot.clone());
                                                        if let Some(slot_color) =
                                                            self.combo_colors.get_mut(combo_idx)
                                                        {
                                                            *slot_color = color_value;
                                                        }
                                                        self.combo_colors_dirty = true;
                                                        egui::Popup::close_all(ui.ctx());
                                                    }
                                                }
                                            });
                                        }
                                    });
                            },
                        );
                    },
                );

                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_content_width,
                    input_keys_row_height,
                    crate::i18n::tr_catalog(self.app_settings.language, "combo_editor.input_keys"),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "combo_editor.keys_that_must_be_pressed_together",
                    )),
                    input_keys_control_width,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0 * scale;
                        for key_idx in 0..4 {
                            let value = self.combo_entries[combo_idx].keys[key_idx];
                            let button_label = if value == 0 {
                                String::new()
                            } else {
                                keycode_label_with_macro_names(
                                    value,
                                    custom,
                                    &self.layer_names,
                                    &self.keycode_picker.macro_names,
                                    &self.keycode_picker.tap_dance_names,
                                    self.app_settings.key_legend_layout,
                                )
                            };
                            let hover_label = button_label.replace('\n', " ");
                            let resp = crate::ui_style::modern_keycap_button(
                                ui,
                                button_label.as_str(),
                                input_key_size,
                                true,
                            );
                            if !hover_label.is_empty() {
                                resp.clone().on_hover_text(hover_label.as_str());
                            }
                            combo_keycap_hovered |= resp.hovered();
                            if resp.clicked_by(egui::PointerButton::Primary) {
                                self.open_combo_key_picker(
                                    combo_idx,
                                    ComboPickField::Trigger(key_idx),
                                );
                            }
                            if resp.clicked_by(egui::PointerButton::Secondary) {
                                self.secondary_click_handled = true;
                                if value != 0 {
                                    self.push_combo_undo();
                                    self.combo_entries[combo_idx].keys[key_idx] = 0;
                                    self.combo_dirty = true;
                                }
                            }
                        }
                    },
                );

                crate::ui_style::settings_list_row_with_tooltip(
                    ui,
                    row_content_width,
                    input_keys_row_height,
                    crate::i18n::tr_catalog(self.app_settings.language, "combo_editor.output_key"),
                    true,
                    Some(crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "combo_editor.keycode_sent_when_the_combo_activates",
                    )),
                    input_key_size.x,
                    |ui| {
                        let value = self.combo_entries[combo_idx].output;
                        let button_label = if value == 0 {
                            String::new()
                        } else {
                            keycode_label_with_macro_names(
                                value,
                                custom,
                                &self.layer_names,
                                &self.keycode_picker.macro_names,
                                &self.keycode_picker.tap_dance_names,
                                self.app_settings.key_legend_layout,
                            )
                        };
                        let hover_label = button_label.replace('\n', " ");
                        let resp = crate::ui_style::modern_keycap_button(
                            ui,
                            button_label.as_str(),
                            input_key_size,
                            true,
                        );
                        if !hover_label.is_empty() {
                            resp.clone().on_hover_text(hover_label.as_str());
                        }
                        combo_keycap_hovered |= resp.hovered();
                        if resp.clicked_by(egui::PointerButton::Primary) {
                            self.open_combo_key_picker(combo_idx, ComboPickField::Output);
                        }
                        if resp.clicked_by(egui::PointerButton::Secondary) {
                            self.secondary_click_handled = true;
                            if value != 0 {
                                self.push_combo_undo();
                                self.combo_entries[combo_idx].output = 0;
                                self.combo_dirty = true;
                            }
                        }
                    },
                );

                if let Some(current_combo_term) = self.combo_term {
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        row_content_width,
                        row_height,
                        crate::i18n::tr_catalog(self.app_settings.language, "common.timeout"),
                        true,
                        Some(crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "combo_editor.maximum_time_between_combo_key_presses",
                        )),
                        timeout_control_width,
                        |ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let edit_id = egui::Id::new("combo_term");
                                    let mut combo_term_text = ui.ctx().data_mut(|d| {
                                        d.get_temp::<String>(edit_id)
                                            .unwrap_or_else(|| current_combo_term.to_string())
                                    });
                                    if combo_term_text.parse::<u16>().ok()
                                        != Some(current_combo_term)
                                        && !ui.memory(|m| m.has_focus(edit_id))
                                    {
                                        combo_term_text = current_combo_term.to_string();
                                    }
                                    let resp = crate::ui_style::modern_text_field_sized(
                                        ui,
                                        edit_id,
                                        &mut combo_term_text,
                                        70.0 * scale,
                                        control_height,
                                        "",
                                        4,
                                        egui::Align::RIGHT,
                                    );
                                    let resp = settings_field_unit_tooltip(
                                        resp,
                                        self.app_settings.language,
                                        false,
                                        SettingsFieldUnit::Milliseconds,
                                    );
                                    if resp.changed() {
                                        let filtered: String = combo_term_text
                                            .chars()
                                            .filter(|c| c.is_ascii_digit())
                                            .take(4)
                                            .collect();
                                        if filtered != combo_term_text {
                                            combo_term_text = filtered;
                                        }
                                    }
                                    let commit = resp.lost_focus()
                                        || (resp.has_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                    if commit {
                                        match combo_term_text.trim().parse::<u16>() {
                                            Ok(parsed) => {
                                                let next_combo_term = parsed.max(1);
                                                if next_combo_term != current_combo_term {
                                                    self.combo_undo_stack
                                                        .push(combo_undo_snapshot.clone());
                                                    self.combo_term = Some(next_combo_term);
                                                    self.combo_term_dirty = true;
                                                }
                                                combo_term_text = next_combo_term.to_string();
                                            }
                                            Err(_) => {
                                                combo_term_text = current_combo_term.to_string();
                                            }
                                        }
                                    }
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(edit_id, combo_term_text);
                                    });
                                    if self.combo_undo_stack.len() > 64 {
                                        self.combo_undo_stack.remove(0);
                                    }
                                },
                            );
                        },
                    );
                }
            },
        );

        ui.add_space(14.0 * scale);
        let action_size = crate::ui_style::modal_action_button_size() * scale;
        let action_width = action_size.x * 2.0 + 8.0 * scale;
        let action_rect = egui::Rect::from_min_size(
            egui::pos2(page_center_x - action_width / 2.0, ui.cursor().min.y),
            Vec2::new(action_width, action_size.y),
        );
        crate::ui_style::allocate_ui_at_rect(ui, action_rect, |ui| {
            ui.set_min_size(action_rect.size());
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0 * scale;
                let clear_enabled = combo_idx < self.combo_entries.len()
                    && (self.combo_entries[combo_idx].keys.iter().any(|&k| k != 0)
                        || self.combo_entries[combo_idx].output != 0
                        || self
                            .combo_names
                            .get(combo_idx)
                            .map(|s| !s.trim().is_empty())
                            .unwrap_or(false)
                        || self.combo_colors.get(combo_idx).copied() != Some(COMBO_NO_COLOR));
                let clear_resp = crate::ui_style::modern_button_with_font(
                    ui,
                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.clear"),
                    action_size,
                    control_font_size,
                    clear_enabled,
                );
                if clear_resp.clicked() && clear_enabled {
                    self.push_combo_undo();
                    self.combo_entries[combo_idx] = ComboEntry::default();
                    if let Some(name) = self.combo_names.get_mut(combo_idx) {
                        name.clear();
                    }
                    if let Some(color) = self.combo_colors.get_mut(combo_idx) {
                        *color = COMBO_NO_COLOR;
                    }
                    self.combo_dirty = true;
                    self.combo_names_dirty = true;
                    self.combo_colors_dirty = true;
                }
                let undo_enabled = !self.combo_undo_stack.is_empty();
                let undo_resp = crate::ui_style::modern_button_with_font(
                    ui,
                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.undo"),
                    action_size,
                    control_font_size,
                    undo_enabled,
                );
                if undo_resp.clicked() && undo_enabled {
                    if let Some(snapshot) = self.combo_undo_stack.pop() {
                        self.combo_entries = snapshot.entries;
                        self.combo_names = snapshot.names;
                        self.combo_colors = snapshot.colors;
                        self.combo_term = snapshot.term;
                        self.combo_visible_count = snapshot
                            .visible_count
                            .clamp(1, self.combo_entries.len().max(1));
                        self.selected_combo = snapshot
                            .selected
                            .min(self.combo_visible_count.saturating_sub(1));
                        self.combo_dirty = true;
                        self.combo_names_dirty = true;
                        self.combo_colors_dirty = true;
                        self.combo_term_dirty = true;
                    }
                }
            });
        });
        ui.allocate_space(Vec2::new(1.0, action_size.y));
        combo_keycap_hovered
    }
}

fn combo_color_dropdown_label(
    lang: crate::i18n::Language,
    color_value: u32,
    palette: &[u32],
) -> String {
    if color_value == COMBO_NO_COLOR {
        crate::i18n::tr_catalog(lang, "combo_editor.no_color").to_owned()
    } else if let Some(idx) = palette.iter().position(|color| *color == color_value) {
        crate::i18n::tr_catalog_format(
            lang,
            "combo_editor.color_number",
            &[("number", &(idx + 1).to_string())],
        )
    } else {
        format!("#{color_value:06X}")
    }
}

fn paint_combo_color_picker_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    color_value: u32,
    border: Color32,
    dark: bool,
    scale: f32,
) {
    ui.painter().rect(
        rect,
        9.0,
        app_surface_fill(dark),
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    paint_combo_color_chip(
        ui,
        rect.shrink(5.0 * scale),
        color_value,
        false,
        Some(border.gamma_multiply(0.85)),
        dark,
        scale,
    );
}

fn paint_combo_color_picker_cell(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    color_value: u32,
    selected: bool,
    enabled: bool,
    dark: bool,
    scale: f32,
) {
    let outline = if selected {
        app_accent()
    } else if dark {
        Color32::from_rgb(72, 72, 76)
    } else {
        Color32::from_rgb(210, 210, 214)
    };
    ui.painter().rect(
        rect,
        7.0,
        app_surface_fill(dark),
        Stroke::new(if selected { 1.6_f32 } else { 1.0_f32 }, outline),
        egui::StrokeKind::Inside,
    );
    paint_combo_color_chip(
        ui,
        rect.shrink(4.5 * scale),
        color_value,
        !enabled,
        None,
        dark,
        scale,
    );
}

fn paint_combo_color_chip(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    color_value: u32,
    disabled: bool,
    stroke_color: Option<Color32>,
    dark: bool,
    scale: f32,
) {
    let stroke = stroke_color
        .map(|color| Stroke::new(1.0 * scale, color))
        .unwrap_or(Stroke::NONE);
    if color_value == COMBO_NO_COLOR {
        ui.painter().rect(
            rect,
            5.0,
            Color32::TRANSPARENT,
            stroke,
            egui::StrokeKind::Inside,
        );
        let slash = Stroke::new(1.2 * scale, app_muted_text(dark));
        ui.painter().line_segment(
            [
                rect.left_top() + egui::vec2(4.0 * scale, 4.0 * scale),
                rect.right_bottom() - egui::vec2(4.0 * scale, 4.0 * scale),
            ],
            slash,
        );
        return;
    }

    let mut color = combo_color32(color_value).gamma_multiply(0.92);
    if disabled {
        color = color.gamma_multiply(0.42);
    }
    ui.painter()
        .rect(rect, 5.0, color, stroke, egui::StrokeKind::Inside);
}
