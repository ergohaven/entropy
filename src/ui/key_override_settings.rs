use super::*;

impl EntropyApp {
    pub(super) fn draw_key_override_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        const KEY_OVERRIDE_PAGE_WIDTH: f32 = 548.0;
        const KEY_OVERRIDE_TITLE_Y_OFFSET: f32 = 18.0;
        const KEY_OVERRIDE_DESC_GAP: f32 = 6.0;
        const KEY_OVERRIDE_BLOCK_TOP_GAP: f32 = 18.0;

        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let page_width = KEY_OVERRIDE_PAGE_WIDTH * scale;

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(KEY_OVERRIDE_TITLE_Y_OFFSET * scale);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::KeyOverridesTitle))
                        .size(18.0 * scale)
                        .strong(),
                );
                ui.add_space(KEY_OVERRIDE_DESC_GAP * scale);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::KeyOverridesDescription,
                    ))
                    .size(13.0 * scale)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(KEY_OVERRIDE_BLOCK_TOP_GAP * scale);
                let editor_height = (content_rect.height()
                    - KEY_OVERRIDE_TITLE_Y_OFFSET * scale
                    - KEY_OVERRIDE_DESC_GAP * scale
                    - KEY_OVERRIDE_BLOCK_TOP_GAP * scale
                    - 64.0 * scale)
                    .max(360.0 * scale);
                ui.allocate_ui_with_layout(
                    egui::vec2(page_width, editor_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.draw_key_override_editor_content(ui, true);
                    },
                );
            });
        });
    }

    pub(super) fn push_key_override_undo(&mut self) {
        self.key_override_undo_stack.push((
            self.key_override_entries.clone(),
            self.key_override_names.clone(),
            self.selected_key_override,
            self.key_override_visible_count,
        ));
        if self.key_override_undo_stack.len() > 64 {
            self.key_override_undo_stack.remove(0);
        }
    }

    fn write_all_key_overrides(&mut self) -> bool {
        let mut saved = true;
        for idx in 0..self.key_override_entries.len() {
            saved &= self.write_key_override(idx);
        }
        saved
    }

    fn key_override_entry_exists(entry: &KeyOverrideEntry) -> bool {
        entry.trigger != 0
            || entry.replacement != 0
            || entry.layers != 0
            || entry.trigger_mods != 0
            || entry.negative_mod_mask != 0
            || entry.suppressed_mods != 0
            || entry.options.activation_trigger_down
            || entry.options.activation_required_mod_down
            || entry.options.activation_negative_mod_up
            || entry.options.one_mod
            || entry.options.no_reregister_trigger
            || entry.options.no_unregister_on_other_key_down
    }

    pub(super) fn normalize_key_override_entry(entry: &mut KeyOverrideEntry) {
        entry.options.enabled = Self::key_override_entry_exists(entry);
    }

    pub(super) fn write_key_override(&mut self, idx: usize) -> bool {
        let Some(entry) = self.key_override_entries.get_mut(idx) else {
            return true;
        };
        Self::normalize_key_override_entry(entry);
        let entry = entry.clone();
        if self.hid_write_task_active() {
            self.key_override_dirty = true;
            return false;
        }
        let Some(hid) = &self.hid_device else {
            self.key_override_dirty = true;
            return false;
        };
        if let Err(e) = hid.set_key_override(
            idx as u8,
            entry.trigger,
            entry.replacement,
            entry.layers,
            entry.trigger_mods,
            entry.negative_mod_mask,
            entry.suppressed_mods,
            entry.options.bits(),
        ) {
            self.status_msg = format!("Failed to save Key Override {}: {}", idx + 1, e);
            log::warn!("set_key_override({idx}) failed: {e}");
            self.key_override_dirty = true;
            false
        } else {
            true
        }
    }

    pub(super) fn flush_pending_key_override_writes(&mut self) {
        if self.key_override_dirty
            && !self.hid_write_task_active()
            && self.write_all_key_overrides()
        {
            self.key_override_dirty = false;
        }
    }

    fn open_key_override_picker(&mut self, target: KeyOverridePickField) {
        let allow_mod_key = matches!(target, KeyOverridePickField::Replacement);
        self.key_override_pick_target = Some(target);
        self.keycode_picker
            .open_regular_key_picker_with_mod_key(allow_mod_key);
    }

    fn key_override_mod_mask_summary(language: crate::i18n::Language, mask: u8) -> String {
        match mask.count_ones() {
            0 => crate::i18n::tr_catalog(language, "key_override_editor.none").to_string(),
            8 => crate::i18n::tr_catalog(language, "key_override_editor.all_mods").to_string(),
            count if matches!(language, crate::i18n::Language::Russian) => {
                format!("{} модиф.", count)
            }
            count => format!("{} mods", count),
        }
    }

    fn key_override_layers_summary(language: crate::i18n::Language, layers: u16) -> String {
        match layers.count_ones() {
            0 => crate::i18n::tr_catalog(language, "key_override_editor.no_layers").to_string(),
            16 => crate::i18n::tr_catalog(language, "key_override_editor.all_layers").to_string(),
            count if matches!(language, crate::i18n::Language::Russian) => {
                format!("{} слоёв", count)
            }
            count => format!("{} layers", count),
        }
    }

    fn draw_key_override_layers_modern(
        ui: &mut egui::Ui,
        layers: &mut u16,
        language: crate::i18n::Language,
    ) -> bool {
        let mut changed = false;
        let dark = ui.visuals().dark_mode;

        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let popup_width = metrics.value(244.0);
        let layer_button_size = metrics.size(52.0, 30.0);
        let button_gap = metrics.value(6.0);
        let layer_row_width = layer_button_size.x * 4.0 + button_gap * 3.0;
        let layer_row_inset = ((popup_width - layer_row_width) * 0.5).max(0.0);

        ui.set_min_width(popup_width);
        ui.spacing_mut().item_spacing = Vec2::new(button_gap, button_gap);
        for row in 0..4 {
            ui.horizontal(|ui| {
                ui.add_space(layer_row_inset);
                for col in 0..4 {
                    let idx = row * 4 + col;
                    let bit = 1u16 << idx;
                    let selected = (*layers & bit) != 0;
                    let label = idx.to_string();
                    let (rect, resp) = ui.allocate_exact_size(layer_button_size, Sense::click());
                    let active = resp.is_pointer_button_down_on();
                    let hovered = resp.hovered();
                    let fill = if selected {
                        if hovered || active {
                            if dark {
                                Color32::from_rgb(72, 64, 68)
                            } else {
                                Color32::from_rgb(242, 226, 230)
                            }
                        } else if dark {
                            Color32::from_rgb(62, 56, 59)
                        } else {
                            Color32::from_rgb(248, 236, 239)
                        }
                    } else if active {
                        if dark {
                            Color32::from_rgb(56, 56, 59)
                        } else {
                            Color32::from_rgb(232, 232, 235)
                        }
                    } else if hovered {
                        crate::ui_style::hover_fill(dark)
                    } else {
                        crate::ui_style::surface_fill(dark)
                    };
                    let stroke = if selected {
                        Stroke::new(metrics.value(1.35), app_accent())
                    } else {
                        crate::ui_style::modal_outline_stroke(dark)
                    };
                    ui.painter().rect(
                        rect,
                        metrics.value(9.0),
                        fill,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(metrics.value(12.5)),
                        if selected {
                            ui.visuals().text_color()
                        } else {
                            app_muted_text(dark).gamma_multiply(if dark { 0.62 } else { 1.10 })
                        },
                    );
                    if selected {
                        ui.painter().circle_filled(
                            egui::pos2(
                                rect.right() - metrics.value(8.0),
                                rect.top() + metrics.value(8.0),
                            ),
                            metrics.value(2.4),
                            app_accent(),
                        );
                    }
                    if hovered {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.clicked() {
                        if selected {
                            *layers &= !bit;
                        } else {
                            *layers |= bit;
                        }
                        changed = true;
                    }
                }
            });
        }

        ui.add_space(metrics.value(4.0));
        let action_width = metrics.value(112.0) * 2.0 + button_gap;
        let action_inset = ((popup_width - action_width) * 0.5).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(action_inset);
            let all_resp = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(language, "key_override_editor.enable_all"),
                metrics.size(112.0, 30.0),
                true,
            );
            if all_resp.clicked() && *layers != u16::MAX {
                *layers = u16::MAX;
                changed = true;
            }
            let none_resp = crate::ui_style::modern_button(
                ui,
                crate::i18n::tr_catalog(language, "key_override_editor.disable_all"),
                metrics.size(112.0, 30.0),
                true,
            );
            if none_resp.clicked() && *layers != 0 {
                *layers = 0;
                changed = true;
            }
        });
        changed
    }

    fn draw_key_override_mod_mask_modern(
        ui: &mut egui::Ui,
        mask: &mut u8,
        language: crate::i18n::Language,
    ) -> bool {
        let mut changed = false;
        let gui = crate::keycode::gui_mod_name();
        let labels = if matches!(language, crate::i18n::Language::Russian) {
            [
                "Левый Ctrl".to_string(),
                "Левый Shift".to_string(),
                "Левый Alt".to_string(),
                format!("Левый {}", gui),
                "Правый Ctrl".to_string(),
                "Правый Shift".to_string(),
                "Правый Alt".to_string(),
                format!("Правый {}", gui),
            ]
        } else {
            [
                "Left Ctrl".to_string(),
                "Left Shift".to_string(),
                "Left Alt".to_string(),
                format!("Left {}", gui),
                "Right Ctrl".to_string(),
                "Right Shift".to_string(),
                "Right Alt".to_string(),
                format!("Right {}", gui),
            ]
        };

        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let popup_width = metrics.value(244.0);
        let row_height = metrics.value(34.0);
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);
        ui.set_min_width(popup_width);
        ui.spacing_mut().item_spacing.y = 0.0;
        for (idx, label) in labels.iter().enumerate() {
            let bit = 1u8 << idx;
            let mut checked = (*mask & bit) != 0;
            crate::ui_style::settings_list_row_with_tooltip(
                ui,
                popup_width,
                row_height,
                label.as_str(),
                true,
                None,
                switch_width,
                |ui| {
                    let resp =
                        crate::ui_style::settings_switch_sized(ui, &mut checked, switch_size);
                    if resp.changed() {
                        if checked {
                            *mask |= bit;
                        } else {
                            *mask &= !bit;
                        }
                        changed = true;
                    }
                },
            );
        }
        changed
    }

    fn draw_key_override_editor_content(&mut self, ui: &mut egui::Ui, _two_column: bool) {
        let dark = ui.visuals().dark_mode;
        if self.firmware != FirmwareProtocol::Vial {
            crate::ui_style::modal_empty_state(
                ui,
                "Key Overrides are not supported for this firmware",
                None,
            );
            return;
        }

        if self.key_override_entries.is_empty() {
            crate::ui_style::modal_empty_state(
                ui,
                "This device does not report any Key Override slots",
                None,
            );
            return;
        }

        self.selected_key_override = self
            .selected_key_override
            .min(self.key_override_entries.len().saturating_sub(1));
        self.key_override_names
            .resize(self.key_override_entries.len(), String::new());
        self.key_override_visible_count = self.key_override_entries.len().max(1);

        let idx = self.selected_key_override;
        let current = self.key_override_entries[idx].clone();
        let mut edited = current.clone();
        let page_center_x = ui.max_rect().center().x;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let scale = metrics.scale;
        let content_width = metrics.settings_content_width();
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let control_width = metrics.settings_control_width();
        let control_height = metrics.settings_control_height();
        let control_font_size = metrics.settings_control_font_size();
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);
        const ROW_COUNT: usize = 14;

        let custom = self
            .layout
            .as_ref()
            .map(|l| l.custom_keycodes.as_slice())
            .unwrap_or(&[]);
        let selected_override_empty = self
            .key_override_entries
            .get(idx)
            .map(|entry| !Self::key_override_entry_exists(entry))
            .unwrap_or(true)
            && self
                .key_override_names
                .get(idx)
                .map(|name| name.trim().is_empty())
                .unwrap_or(true);
        let selected_override_text = match self.key_override_names.get(idx) {
            Some(name) if !name.trim().is_empty() => format!("KO{}: {}", idx, name.trim()),
            _ => format!("KO{}", idx),
        };
        let selected_override_text_color = if selected_override_empty {
            app_inactive_entry_text(dark)
        } else {
            ui.visuals().text_color()
        };
        let trigger_label = if edited.trigger == 0 {
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "key_override_editor.pick_trigger",
            )
            .to_string()
        } else {
            keycode_label_with_macro_names(
                edited.trigger,
                custom,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )
            .replace('\n', " ")
        };
        let replacement_label = if edited.replacement == 0 {
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "key_override_editor.pick_replacement",
            )
            .to_string()
        } else {
            keycode_label_with_macro_names(
                edited.replacement,
                custom,
                &self.layer_names,
                &self.keycode_picker.macro_names,
                &self.keycode_picker.tap_dance_names,
                self.app_settings.key_legend_layout,
            )
            .replace('\n', " ")
        };
        let trigger_tip = keycode_tooltip_with_macro_names(
            edited.trigger,
            custom,
            &self.layer_names,
            &self.keycode_picker.macro_names,
            &self.keycode_picker.macro_descriptions,
            &self.keycode_picker.tap_dance_names,
        );
        let replacement_tip = keycode_tooltip_with_macro_names(
            edited.replacement,
            custom,
            &self.layer_names,
            &self.keycode_picker.macro_names,
            &self.keycode_picker.macro_descriptions,
            &self.keycode_picker.tap_dance_names,
        );

        crate::ui_style::modal_content(
            ui,
            crate::ui_style::ModalLayout::new(content_width).with_top_padding(4.0 * scale),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "key_override_settings",
                    metrics,
                    ROW_COUNT,
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
                                                crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.entry"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.select_key_override_slot")),
                                                control_width,
                                                |ui| {
                                                    let dropdown_id = ui.make_persistent_id("key_override_entry_dropdown");
                                                    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
                                                        ui,
                                                        dropdown_id,
                                                        selected_override_text.as_str(),
                                                        selected_override_text_color,
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
                                                                .id_salt("key_override_entry_dropdown_scroll")
                                                                 .max_height(142.0 * scale)
                                                                .auto_shrink([false, true])
                                                                .show(ui, |ui| {
                                                                    for entry_idx in 0..self.key_override_entries.len() {
                                                                        let empty = self.key_override_entries.get(entry_idx)
                                                                            .map(|entry| !Self::key_override_entry_exists(entry))
                                                                            .unwrap_or(true)
                                                                            && self.key_override_names.get(entry_idx)
                                                                                .map(|name| name.trim().is_empty())
                                                                                .unwrap_or(true);
                                                                        let option_text = match self.key_override_names.get(entry_idx) {
                                                                            Some(name) if !name.trim().is_empty() => format!("KO{}: {}", entry_idx, name.trim()),
                                                                            _ => format!("KO{}", entry_idx),
                                                                        };
                                                                        let selected = entry_idx == self.selected_key_override;
                                                                        let (option_rect, option_resp) = ui.allocate_exact_size(
                                                                            Vec2::new(control_width, 28.0 * scale),
                                                                            Sense::click(),
                                                                        );
                                                                        if option_resp.hovered() {
                                                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                                                        }
                                                                        let option_fill = if selected {
                                                                            if dark { Color32::from_rgb(58, 58, 61) } else { Color32::from_rgb(236, 236, 238) }
                                                                        } else if option_resp.hovered() {
                                                                            crate::ui_style::hover_fill(dark)
                                                                        } else {
                                                                            Color32::TRANSPARENT
                                                                        };
                                                                        ui.painter().rect_filled(option_rect, 7.0, option_fill);
                                                                        ui.painter().text(
                                                                            egui::pos2(option_rect.left() + 10.0, option_rect.center().y),
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
                                                                            self.selected_key_override = entry_idx;
                                                                            egui::Popup::close_all(ui.ctx());
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
                                                crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.name"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.local_name_for_this_key_override_slot")),
                                                control_width,
                                                |ui| {
                                                    if let Some(name) = self.key_override_names.get_mut(idx) {
                                                        let resp = crate::ui_style::modern_text_field_sized(
                                                            ui,
                                                            crate::ui_style::ModernTextField::new(
                                                                egui::Id::new(("key_override_name", idx)),
                                                                name,
                                                                control_width,
                                                                32.0 * scale,
                                                                crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.name"),
                                                                12,
                                                                egui::Align::Center,
                                                            ),
                                                        );
                                                        name_changed = resp.changed();
                                                        resp.clone().on_hover_text(crate::i18n::tr_catalog(self.app_settings.language, "alt_repeat_editor.stored_locally_in_entropy"));
                                                    }
                                                },
                                            );
                                            if name_changed {
                                                save_key_override_names(&self.key_override_names, &self.current_device_name);
                                            }
                                        }
                                        2 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.trigger"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.original_key_that_can_be_overridden")),
                                                control_width,
                                                |ui| {
                                                    let resp = crate::ui_style::modern_button_with_font(ui, trigger_label.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() {
                                                        self.open_key_override_picker(KeyOverridePickField::Trigger);
                                                    }
                                                    resp.on_hover_text(crate::i18n::tr_text(self.app_settings.language, &trigger_tip));
                                                },
                                            );
                                        }
                                        3 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.replacement"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.keycode_sent_while_override_conditions_match")),
                                                control_width,
                                                |ui| {
                                                    let resp = crate::ui_style::modern_button_with_font(ui, replacement_label.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() {
                                                        self.open_key_override_picker(KeyOverridePickField::Replacement);
                                                    }
                                                    resp.on_hover_text(crate::i18n::tr_text(self.app_settings.language, &replacement_tip));
                                                },
                                            );
                                        }
                                        4 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.suppressed_mods"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.modifiers_hidden_while_the_replacement_is_active")),
                                                control_width,
                                                |ui| {
                                                    let popup_id = ui.make_persistent_id(("ko_suppressed_mods_popup", idx));
                                                    let summary = Self::key_override_mod_mask_summary(self.app_settings.language, edited.suppressed_mods);
                                                    let resp = crate::ui_style::modern_button_with_font(ui, summary.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() { egui::Popup::toggle_id(ui.ctx(), popup_id); }
                                                    ui.style_mut().visuals.window_stroke = crate::ui_style::modal_outline_stroke(dark);
                                                    ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                                                    crate::ui_style::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                                        Self::draw_key_override_mod_mask_modern(ui, &mut edited.suppressed_mods, self.app_settings.language);
                                                    });
                                                },
                                            );
                                        }
                                        5 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.trigger_mods"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.modifiers_required_for_this_override")),
                                                control_width,
                                                |ui| {
                                                    let popup_id = ui.make_persistent_id(("ko_trigger_mods_popup", idx));
                                                    let summary = Self::key_override_mod_mask_summary(self.app_settings.language, edited.trigger_mods);
                                                    let resp = crate::ui_style::modern_button_with_font(ui, summary.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() { egui::Popup::toggle_id(ui.ctx(), popup_id); }
                                                    ui.style_mut().visuals.window_stroke = crate::ui_style::modal_outline_stroke(dark);
                                                    ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                                                    crate::ui_style::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                                        Self::draw_key_override_mod_mask_modern(ui, &mut edited.trigger_mods, self.app_settings.language);
                                                    });
                                                },
                                            );
                                        }
                                        6 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.negative_mods"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.modifiers_that_block_this_override")),
                                                control_width,
                                                |ui| {
                                                    let popup_id = ui.make_persistent_id(("ko_negative_mods_popup", idx));
                                                    let summary = Self::key_override_mod_mask_summary(self.app_settings.language, edited.negative_mod_mask);
                                                    let resp = crate::ui_style::modern_button_with_font(ui, summary.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() { egui::Popup::toggle_id(ui.ctx(), popup_id); }
                                                    ui.style_mut().visuals.window_stroke = crate::ui_style::modal_outline_stroke(dark);
                                                    ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                                                    crate::ui_style::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                                        Self::draw_key_override_mod_mask_modern(ui, &mut edited.negative_mod_mask, self.app_settings.language);
                                                    });
                                                },
                                            );
                                        }
                                        7 => {
                                            crate::ui_style::settings_list_row_with_tooltip(
                                                ui,
                                                row_content_width,
                                                row_height,
                                                crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.enable_on_layers"),
                                                true,
                                                Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.layers_where_this_override_can_activate")),
                                                control_width,
                                                |ui| {
                                                    let popup_id = ui.make_persistent_id(("ko_layers_popup", idx));
                                                    let summary = Self::key_override_layers_summary(self.app_settings.language, edited.layers);
                                                    let resp = crate::ui_style::modern_button_with_font(ui, summary.as_str(), Vec2::new(control_width, control_height), control_font_size, true);
                                                    if resp.clicked() { egui::Popup::toggle_id(ui.ctx(), popup_id); }
                                                    ui.style_mut().visuals.window_stroke = crate::ui_style::modal_outline_stroke(dark);
                                                    ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                                                    crate::ui_style::popup_below_widget(ui, popup_id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                                                        Self::draw_key_override_layers_modern(ui, &mut edited.layers, self.app_settings.language);
                                                    });
                                                },
                                            );
                                        }
                                        8 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.trigger_press"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.activate_when_the_trigger_key_is_pressed")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "trigger_press"), &mut edited.options.activation_trigger_down, switch_size); }),
                                        9 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.required_mod_press"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.activate_when_a_required_modifier_is_pressed")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "required_mod_press"), &mut edited.options.activation_required_mod_down, switch_size); }),
                                        10 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.blocked_mod_release"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.activate_when_a_blocking_modifier_is_released")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "blocked_mod_release"), &mut edited.options.activation_negative_mod_up, switch_size); }),
                                        11 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.any_one_mod"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.any_one_trigger_modifier_is_enough")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "any_one_mod"), &mut edited.options.one_mod, switch_size); }),
                                        12 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.no_re_send"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.do_not_resend_the_trigger_after_override_ends")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "no_re_send"), &mut edited.options.no_reregister_trigger, switch_size); }),
                                        13 => crate::ui_style::settings_list_row_with_tooltip(ui, row_content_width, row_height, crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.stay_active"), true, Some(crate::i18n::tr_catalog(self.app_settings.language, "key_override_editor.stay_active_when_another_key_is_pressed")), switch_width, |ui| { crate::ui_style::settings_switch_sized_stable(ui, ("key_override_settings", "stay_active"), &mut edited.options.no_unregister_on_other_key_down, switch_size); }),
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

                let action_size = crate::ui_style::modal_action_button_size() * scale;
                let action_width = action_size.x * 2.0 + 8.0 * scale;
                let action_top = list.viewport.bottom() + 14.0 * scale;
                let action_rect = egui::Rect::from_min_size(
                    egui::pos2(page_center_x - action_width / 2.0, action_top),
                    Vec2::new(action_width, action_size.y),
                );
                crate::ui_style::allocate_ui_at_rect(ui, action_rect, |ui| {
                    ui.set_min_size(action_rect.size());
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0 * scale;
                        let clear_enabled =
                            Self::key_override_entry_exists(&self.key_override_entries[idx])
                                || self
                                    .key_override_names
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
                            control_font_size,
                            clear_enabled,
                        );
                        if clear_resp.clicked() && clear_enabled {
                            self.push_key_override_undo();
                            self.key_override_entries[idx] = KeyOverrideEntry::default();
                            if let Some(name) = self.key_override_names.get_mut(idx) {
                                name.clear();
                            }
                            save_key_override_names(
                                &self.key_override_names,
                                &self.current_device_name,
                            );
                            self.write_key_override(idx);
                        }

                        let undo_enabled = !self.key_override_undo_stack.is_empty();
                        let undo_resp = crate::ui_style::modern_button_with_font(
                            ui,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "alt_repeat_editor.undo",
                            ),
                            action_size,
                            control_font_size,
                            undo_enabled,
                        );
                        if undo_resp.clicked() && undo_enabled {
                            if let Some((entries, names, selected, visible_count)) =
                                self.key_override_undo_stack.pop()
                            {
                                self.key_override_entries = entries;
                                self.key_override_names = names;
                                self.key_override_visible_count =
                                    visible_count.clamp(1, self.key_override_entries.len().max(1));
                                self.selected_key_override =
                                    selected.min(self.key_override_visible_count.saturating_sub(1));
                                save_key_override_names(
                                    &self.key_override_names,
                                    &self.current_device_name,
                                );
                                self.write_all_key_overrides();
                            }
                        }
                    });
                });
                let reserve_bottom = action_rect.bottom() - list.viewport.bottom();
                ui.allocate_space(Vec2::new(1.0, reserve_bottom.max(action_size.y)));
            },
        );

        Self::normalize_key_override_entry(&mut edited);
        if edited != current {
            self.push_key_override_undo();
            self.key_override_entries[idx] = edited;
            self.write_key_override(idx);
        }
    }
}
