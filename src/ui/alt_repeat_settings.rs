use super::*;

impl EntropyApp {
    pub(super) fn draw_alt_repeat_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let alt_repeat_page_width = metrics.settings_content_width();
        let alt_repeat_title_y_offset = metrics.value(30.0);
        let alt_repeat_desc_gap = metrics.value(28.0);
        let alt_repeat_block_top_gap = metrics.value(22.0);

        let dark = ui.visuals().dark_mode;
        let center_x = content_rect.center().x;
        let title_y = content_rect.top() + alt_repeat_title_y_offset;
        let desc_y = title_y + alt_repeat_desc_gap;
        let block_top = desc_y + alt_repeat_block_top_gap;
        let block_rect = egui::Rect::from_min_max(
            egui::pos2(center_x - alt_repeat_page_width / 2.0, block_top),
            egui::pos2(
                center_x + alt_repeat_page_width / 2.0,
                content_rect.bottom(),
            ),
        );

        ui.painter().text(
            egui::pos2(center_x, title_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr(self.app_settings.language, crate::i18n::Key::AltRepeatTitle),
            FontId::proportional(metrics.value(18.0)),
            ui.visuals().text_color(),
        );
        ui.painter().text(
            egui::pos2(center_x, desc_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr(
                self.app_settings.language,
                crate::i18n::Key::AltRepeatDescription,
            ),
            FontId::proportional(metrics.value(13.0)),
            app_muted_text(dark),
        );

        crate::ui_style::allocate_ui_at_rect(ui, block_rect, |ui| {
            self.draw_alt_repeat_editor_content(ui);
        });
    }

    fn push_alt_repeat_undo(&mut self) {
        self.alt_repeat_undo_stack.push((
            self.alt_repeat_entries.clone(),
            self.alt_repeat_names.clone(),
            self.selected_alt_repeat,
        ));
        if self.alt_repeat_undo_stack.len() > 64 {
            self.alt_repeat_undo_stack.remove(0);
        }
    }

    fn alt_repeat_entry_exists(entry: &AltRepeatKeyEntry) -> bool {
        entry.keycode != 0 || entry.alt_keycode != 0 || entry.allowed_mods != 0
    }

    pub(super) fn normalize_alt_repeat_entry(entry: &mut AltRepeatKeyEntry) {
        entry.options.enabled = Self::alt_repeat_entry_exists(entry);
    }

    pub(super) fn write_alt_repeat_entry(&mut self, idx: usize) {
        let Some(entry) = self.alt_repeat_entries.get_mut(idx) else {
            return;
        };
        Self::normalize_alt_repeat_entry(entry);
        let entry = entry.clone();
        let Some(hid) = &self.hid_device else {
            return;
        };
        if let Err(e) = hid.set_alt_repeat_key(
            idx as u8,
            entry.keycode,
            entry.alt_keycode,
            entry.allowed_mods,
            entry.options.bits(),
        ) {
            self.status_msg = format!("Failed to save Alt Repeat {}: {}", idx + 1, e);
            log::warn!("set_alt_repeat_key({idx}) failed: {e}");
        }
    }

    fn open_alt_repeat_picker(&mut self, target: AltRepeatPickField) {
        self.alt_repeat_pick_target = Some(target);
        self.keycode_picker
            .open_regular_key_picker_with_mod_key(true);
    }

    pub(super) fn open_alt_repeat_window_compact(&mut self) {
        self.selected_alt_repeat = 0;
        self.alt_repeat_visible_count = 1;
        self.settings_tab = SettingsTab::AltRepeat;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    fn draw_alt_repeat_editor_content(&mut self, ui: &mut egui::Ui) {
        let dark = ui.visuals().dark_mode;
        if self.alt_repeat_entries.is_empty() {
            crate::ui_style::modal_empty_state(
                ui,
                crate::i18n::tr(
                    self.app_settings.language,
                    crate::i18n::Key::AltRepeatUnavailable,
                ),
                None,
            );
            return;
        }

        if self.selected_alt_repeat >= self.alt_repeat_entries.len() {
            self.selected_alt_repeat = 0;
        }
        self.selected_alt_repeat = self
            .selected_alt_repeat
            .min(self.alt_repeat_entries.len().saturating_sub(1));
        self.alt_repeat_names
            .resize(self.alt_repeat_entries.len(), String::new());

        let idx = self.selected_alt_repeat;
        let current = self.alt_repeat_entries[idx].clone();
        let mut edited = current.clone();
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let content_width = metrics.settings_content_width();
        let row_height = metrics.settings_row_height();
        const TOTAL_ROWS: usize = 11;
        let row_content_width = metrics.settings_row_content_width();
        let control_width = metrics.settings_control_width();
        let mod_control_width = metrics.value(210.0);
        let switch_size = metrics.size(46.0, 24.0);
        let custom = self
            .layout
            .as_ref()
            .map(|l| l.custom_keycodes.as_slice())
            .unwrap_or(&[]);
        let last_key_label = if edited.keycode == 0 {
            crate::i18n::tr_catalog(self.app_settings.language, "key_picker_text.pick_key")
                .to_string()
        } else {
            keycode_label_with_macro_names(
                edited.keycode,
                custom,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )
            .replace("\n", " ")
        };
        let alt_key_label = if edited.alt_keycode == 0 {
            crate::i18n::tr_catalog(self.app_settings.language, "key_picker_text.pick_key")
                .to_string()
        } else {
            keycode_label_with_macro_names(
                edited.alt_keycode,
                custom,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )
            .replace("\n", " ")
        };
        let last_key_tip = keycode_tooltip_with_macro_names(
            edited.keycode,
            custom,
            &self.layer_names,
            &self.keycode_picker.macro_names,
            &self.keycode_picker.tap_dance_names,
        );
        let alt_key_tip = keycode_tooltip_with_macro_names(
            edited.alt_keycode,
            custom,
            &self.layer_names,
            &self.keycode_picker.macro_names,
            &self.keycode_picker.tap_dance_names,
        );
        let selected_empty = !Self::alt_repeat_entry_exists(&edited)
            && self
                .alt_repeat_names
                .get(idx)
                .map(|name| name.trim().is_empty())
                .unwrap_or(true);
        let selected_text = match self.alt_repeat_names.get(idx) {
            Some(name) if !name.trim().is_empty() => format!("AR{}: {}", idx, name.trim()),
            _ => format!("AR{}", idx),
        };
        let selected_text_color = if selected_empty {
            app_inactive_entry_text(dark)
        } else {
            ui.visuals().text_color()
        };
        let gui = crate::keycode::gui_mod_name();

        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(4.0),
            |ui| {
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "alt_repeat_settings",
                    metrics,
                    TOTAL_ROWS,
                    metrics.value(86.0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for row_idx in list.first_visible_row..list.last_visible_row {
                        match row_idx {
                            0 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.entry",
                                    ),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.select_alt_repeat_slot",
                                    )),
                                    control_width,
                                    |ui| {
                                        let dropdown_id =
                                            ui.make_persistent_id("alt_repeat_entry_dropdown");
                                        let dropdown_resp =
                                            crate::ui_style::modern_dropdown_button_sized(
                                                ui,
                                                dropdown_id,
                                                selected_text.as_str(),
                                                selected_text_color,
                                                control_width,
                                                metrics.settings_control_height(),
                                                metrics.settings_control_font_size(),
                                            );

                                        ui.style_mut().visuals.window_stroke =
                                            crate::ui_style::modal_outline_stroke(dark);
                                        ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                                        egui::popup_below_widget(
                                            ui,
                                            dropdown_id,
                                            &dropdown_resp,
                                            egui::PopupCloseBehavior::CloseOnClickOutside,
                                            |ui| {
                                                ui.set_min_width(control_width);
                                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                                egui::ScrollArea::vertical()
                                                    .id_salt("alt_repeat_entry_dropdown_scroll")
                                                    .max_height(metrics.value(142.0))
                                                    .auto_shrink([false, true])
                                                    .show(ui, |ui| {
                                                        for entry_idx in
                                                            0..self.alt_repeat_entries.len()
                                                        {
                                                            let empty = self
                                                                .alt_repeat_entries
                                                                .get(entry_idx)
                                                                .map(|entry| {
                                                                    !Self::alt_repeat_entry_exists(
                                                                        entry,
                                                                    )
                                                                })
                                                                .unwrap_or(true)
                                                                && self
                                                                    .alt_repeat_names
                                                                    .get(entry_idx)
                                                                    .map(|name| {
                                                                        name.trim().is_empty()
                                                                    })
                                                                    .unwrap_or(true);
                                                            let option_text = match self
                                                                .alt_repeat_names
                                                                .get(entry_idx)
                                                            {
                                                                Some(name)
                                                                    if !name.trim().is_empty() =>
                                                                {
                                                                    format!(
                                                                        "AR{}: {}",
                                                                        entry_idx,
                                                                        name.trim()
                                                                    )
                                                                }
                                                                _ => format!("AR{}", entry_idx),
                                                            };
                                                            let selected = entry_idx
                                                                == self.selected_alt_repeat;
                                                            let (option_rect, option_resp) = ui
                                                                .allocate_exact_size(
                                                                    metrics.size(168.0, 28.0),
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
                                                            ui.painter().rect_filled(
                                                                option_rect,
                                                                7.0,
                                                                option_fill,
                                                            );
                                                            ui.painter().text(
                                                                egui::pos2(
                                                                    option_rect.left()
                                                                        + metrics.value(10.0),
                                                                    option_rect.center().y,
                                                                ),
                                                                egui::Align2::LEFT_CENTER,
                                                                option_text,
                                                                FontId::proportional(
                                                                    metrics.value(12.0),
                                                                ),
                                                                if selected {
                                                                    ui.visuals().text_color()
                                                                } else if empty {
                                                                    app_inactive_entry_text(
                                                                        ui.visuals().dark_mode,
                                                                    )
                                                                } else {
                                                                    app_muted_text(
                                                                        ui.visuals().dark_mode,
                                                                    )
                                                                },
                                                            );
                                                            if option_resp.clicked() {
                                                                self.selected_alt_repeat =
                                                                    entry_idx;
                                                                ui.memory_mut(|m| m.close_popup());
                                                            }
                                                        }
                                                    });
                                            },
                                        );
                                    },
                                );
                            }
                            1 => {
                                let mut name_changed = false;
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.name",
                                    ),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.local_name_for_this_slot",
                                    )),
                                    control_width,
                                    |ui| {
                                        if let Some(name) = self.alt_repeat_names.get_mut(idx) {
                                            let resp = crate::ui_style::modern_text_field_sized(
                                                ui,
                                                egui::Id::new(("alt_repeat_name", idx)),
                                                name,
                                                control_width,
                                                metrics.settings_control_height(),
                                                crate::i18n::tr_catalog(
                                                    self.app_settings.language,
                                                    "alt_repeat_editor.name",
                                                ),
                                                12,
                                                egui::Align::Center,
                                            );
                                            name_changed = resp.changed();
                                            resp.clone().on_hover_text(crate::i18n::tr_catalog(
                                                self.app_settings.language,
                                                "alt_repeat_editor.stored_locally_in_entropy",
                                            ));
                                        }
                                    },
                                );
                                if name_changed {
                                    self.push_alt_repeat_undo();
                                    save_alt_repeat_names(
                                        &self.alt_repeat_names,
                                        &self.current_device_name,
                                    );
                                }
                            }
                            2 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.last_key"),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.key_that_triggers_alternate_repeat_behavior",
                                    )),
                                    control_width,
                                    |ui| {
                                        let resp = crate::ui_style::modern_button_with_font(
                                            ui,
                                            last_key_label.as_str(),
                                            metrics.size(168.0, 32.0),
                                            metrics.settings_control_font_size(),
                                            true,
                                        );
                                        if resp.clicked() {
                                            self.open_alt_repeat_picker(
                                                AltRepeatPickField::LastKey,
                                            );
                                        }
                                        resp.on_hover_text(crate::i18n::tr_text(
                                            self.app_settings.language,
                                            last_key_tip.trim_end_matches('.'),
                                        ));
                                    },
                                );
                            }
                            3 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.alt_key"),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.key_repeated_when_alternate_repeat_activates",
                                    )),
                                    control_width,
                                    |ui| {
                                        let resp = crate::ui_style::modern_button_with_font(
                                            ui,
                                            alt_key_label.as_str(),
                                            metrics.size(168.0, 32.0),
                                            metrics.settings_control_font_size(),
                                            true,
                                        );
                                        if resp.clicked() {
                                            self.open_alt_repeat_picker(AltRepeatPickField::AltKey);
                                        }
                                        resp.on_hover_text(crate::i18n::tr_text(
                                            self.app_settings.language,
                                            alt_key_tip.trim_end_matches('.'),
                                        ));
                                    },
                                );
                            }
                            4..=7 => {
                                let (row_label, left_bit, right_bit) = match row_idx {
                                    4 => (
                                        crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.ctrl_mods",
                                        )
                                        .to_string(),
                                        0,
                                        4,
                                    ),
                                    5 => (
                                        crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.shift_mods",
                                        )
                                        .to_string(),
                                        1,
                                        5,
                                    ),
                                    6 => (
                                        crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.alt_mods",
                                        )
                                        .to_string(),
                                        2,
                                        6,
                                    ),
                                    _ => (
                                        if matches!(
                                            self.app_settings.language,
                                            crate::i18n::Language::Russian
                                        ) {
                                            format!("{gui}-моды")
                                        } else {
                                            format!("{} mods", gui)
                                        },
                                        3,
                                        7,
                                    ),
                                };
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    row_label.as_str(),
                                    true,
                                    Some(match row_idx {
                                        4 => crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.allowed_ctrl_modifiers",
                                        ),
                                        5 => crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.allowed_shift_modifiers",
                                        ),
                                        6 => crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.allowed_alt_modifiers",
                                        ),
                                        _ => crate::i18n::tr_catalog(
                                            self.app_settings.language,
                                            "alt_repeat_editor.allowed_os_modifiers",
                                        ),
                                    }),
                                    mod_control_width,
                                    |ui| {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let mut right_checked =
                                                    (edited.allowed_mods & (1 << right_bit)) != 0;
                                                let right_resp =
                                                    crate::ui_style::settings_switch_sized(
                                                        ui,
                                                        &mut right_checked,
                                                        switch_size,
                                                    );
                                                if right_resp.changed() {
                                                    if right_checked {
                                                        edited.allowed_mods |= 1 << right_bit;
                                                    } else {
                                                        edited.allowed_mods &= !(1 << right_bit);
                                                    }
                                                }
                                                let right_label_resp = ui.label(
                                                    RichText::new("R")
                                                        .size(metrics.value(12.0))
                                                        .color(app_muted_text(
                                                            ui.visuals().dark_mode,
                                                        )),
                                                );
                                                if right_label_resp.hovered() {
                                                    ui.ctx()
                                                        .set_cursor_icon(egui::CursorIcon::Help);
                                                }
                                                right_label_resp.on_hover_text(
                                                    crate::i18n::tr_catalog(
                                                        self.app_settings.language,
                                                        "alt_repeat_editor.right_side_modifier",
                                                    ),
                                                );
                                                ui.add_space(metrics.value(10.0));
                                                let mut left_checked =
                                                    (edited.allowed_mods & (1 << left_bit)) != 0;
                                                let left_resp =
                                                    crate::ui_style::settings_switch_sized(
                                                        ui,
                                                        &mut left_checked,
                                                        switch_size,
                                                    );
                                                if left_resp.changed() {
                                                    if left_checked {
                                                        edited.allowed_mods |= 1 << left_bit;
                                                    } else {
                                                        edited.allowed_mods &= !(1 << left_bit);
                                                    }
                                                }
                                                let left_label_resp = ui.label(
                                                    RichText::new("L")
                                                        .size(metrics.value(12.0))
                                                        .color(app_muted_text(
                                                            ui.visuals().dark_mode,
                                                        )),
                                                );
                                                if left_label_resp.hovered() {
                                                    ui.ctx()
                                                        .set_cursor_icon(egui::CursorIcon::Help);
                                                }
                                                left_label_resp.on_hover_text(
                                                    crate::i18n::tr_catalog(
                                                        self.app_settings.language,
                                                        "alt_repeat_editor.left_side_modifier",
                                                    ),
                                                );
                                            },
                                        );
                                    },
                                );
                            }
                            8 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.default_alt_key",
                                    ),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.use_this_alt_key_by_default",
                                    )),
                                    metrics.value(46.0),
                                    |ui| {
                                        crate::ui_style::settings_switch_sized(
                                            ui,
                                            &mut edited.options.default_to_this_alt_key,
                                            switch_size,
                                        );
                                    },
                                );
                            }
                            9 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.bidirectional",
                                    ),
                                    true,
                                    Some(crate::i18n::tr_catalog(
                                        self.app_settings.language,
                                        "alt_repeat_editor.allow_both_keys_to_alternate_each_other",
                                    )),
                                    metrics.value(46.0),
                                    |ui| {
                                        crate::ui_style::settings_switch_sized(
                                            ui,
                                            &mut edited.options.bidirectional,
                                            switch_size,
                                        );
                                    },
                                );
                            }
                            10 => {
                                crate::ui_style::settings_list_row_with_tooltip(
                                    ui,
                                    row_content_width,
                                    row_height,
                                    crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.ignore_handedness"),
                                    true,
                                    Some(crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.treat_left_and_right_modifiers_as_equivalent")),
                                    metrics.value(46.0),
                                    |ui| {
                                        crate::ui_style::settings_switch_sized(
                                            ui,
                                            &mut edited.options.ignore_mod_handedness,
                                            switch_size,
                                        );
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

                let action_size = metrics.size(104.0, 32.0);
                let action_width = action_size.x * 2.0 + ui.spacing().item_spacing.x;
                let actions_rect = egui::Rect::from_min_size(
                    egui::pos2(list.viewport.left(), list.viewport.bottom() + 24.0),
                    egui::vec2(content_width, action_size.y),
                );
                crate::ui_style::allocate_ui_at_rect(ui, actions_rect, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(((content_width - action_width) / 2.0).max(0.0));
                        let clear_enabled =
                            Self::alt_repeat_entry_exists(&self.alt_repeat_entries[idx])
                                || self
                                    .alt_repeat_names
                                    .get(idx)
                                    .map(|s| !s.trim().is_empty())
                                    .unwrap_or(false);
                        let clear_resp = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "alt_repeat_editor.clear",
                            ),
                            action_size,
                            metrics.settings_control_font_size(),
                            clear_enabled,
                        );
                        if clear_resp.clicked() {
                            self.push_alt_repeat_undo();
                            self.alt_repeat_entries[idx] = AltRepeatKeyEntry::default();
                            if let Some(name) = self.alt_repeat_names.get_mut(idx) {
                                name.clear();
                            }
                            save_alt_repeat_names(
                                &self.alt_repeat_names,
                                &self.current_device_name,
                            );
                            self.write_alt_repeat_entry(idx);
                            edited = self.alt_repeat_entries[idx].clone();
                        }

                        let undo_enabled = !self.alt_repeat_undo_stack.is_empty();
                        let undo_resp = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "alt_repeat_editor.undo",
                            ),
                            action_size,
                            metrics.settings_control_font_size(),
                            undo_enabled,
                        );
                        if undo_resp.clicked() {
                            if let Some((entries, names, selected)) =
                                self.alt_repeat_undo_stack.pop()
                            {
                                self.alt_repeat_entries = entries;
                                self.alt_repeat_names = names;
                                self.selected_alt_repeat =
                                    selected.min(self.alt_repeat_entries.len().saturating_sub(1));
                                save_alt_repeat_names(
                                    &self.alt_repeat_names,
                                    &self.current_device_name,
                                );
                                for entry_idx in 0..self.alt_repeat_entries.len() {
                                    self.write_alt_repeat_entry(entry_idx);
                                }
                            }
                        }
                    });
                });
            },
        );

        Self::normalize_alt_repeat_entry(&mut edited);
        if edited != current {
            self.push_alt_repeat_undo();
            if let Some(slot) = self.alt_repeat_entries.get_mut(idx) {
                *slot = edited;
            }
            self.write_alt_repeat_entry(idx);
        }
    }
}
