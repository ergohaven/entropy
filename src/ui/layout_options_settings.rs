use super::*;

impl EntropyApp {
    pub(super) fn draw_layout_options_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let row_content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let dropdown_width = metrics.value(220.0);
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);

        let dark = ui.visuals().dark_mode;
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.hid_device.is_some()
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };
        let options = self
            .layout
            .as_ref()
            .map(|layout| layout.layout_options.clone())
            .unwrap_or_default();
        let display_option_indices: Vec<usize> = options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| (!Self::is_encoder_layout_option(option)).then_some(idx))
            .collect();

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::DisplayPresetsTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::DisplayPresetsDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if display_option_indices.is_empty() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::DisplayPresetsUnavailable),
                        None,
                    );
                    return;
                }

                if !hid_ready || self.layout_options_value.is_none() {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::DisplayPresetsConnect),
                        None,
                    );
                    return;
                }

                let total_rows = display_option_indices.len();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "layout_options",
                    metrics,
                    total_rows,
                    0.0,
                );
                let values = Self::unpack_layout_option_values(
                    &options,
                    self.layout_options_value.unwrap_or(0),
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for display_row_idx in list.first_visible_row..list.last_visible_row {
                        let Some(&row_idx) = display_option_indices.get(display_row_idx) else {
                            continue;
                        };
                        let option = &options[row_idx];
                        let translated_option_label =
                            crate::i18n::tr_text(self.app_settings.language, &option.label);
                        if option.choices.is_empty() {
                            let mut enabled = values.get(row_idx).copied().unwrap_or(0) != 0;
                            crate::ui_style::settings_list_row_with_tooltip(
                                ui,
                                row_content_width,
                                row_height,
                                translated_option_label.as_str(),
                                true,
                                Some(crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "auto_shift_settings.toggle_firmware_layout_display_option",
                                )),
                                switch_width,
                                |ui| {
                                    let resp = crate::ui_style::settings_switch_sized_stable(
                                        ui,
                                        ("layout_options", row_idx),
                                        &mut enabled,
                                        switch_size,
                                    );
                                    if resp.changed() {
                                        self.set_layout_option_value(row_idx, u32::from(enabled));
                                    }
                                },
                            );
                        } else {
                            let selected_idx = values
                                .get(row_idx)
                                .copied()
                                .unwrap_or(0)
                                .min(option.choices.len().saturating_sub(1) as u32)
                                as usize;
                            let selected_raw_text = option
                                .choices
                                .get(selected_idx)
                                .map(|s| s.as_str())
                                .unwrap_or("Unknown");
                            let selected_text = Self::display_preset_choice_label(
                                self.app_settings.language,
                                selected_raw_text,
                            );
                            let translated_label = translated_option_label.as_str();
                            let tooltip = if matches!(
                                self.app_settings.language,
                                crate::i18n::Language::Russian
                            ) {
                                format!("Выбрать пресет прошивки для {translated_label}")
                            } else {
                                format!("Choose firmware preset for {}", option.label)
                            };
                            crate::ui_style::settings_list_row_with_tooltip(
                                ui,
                                row_content_width,
                                row_height,
                                translated_label,
                                true,
                                Some(&tooltip),
                                dropdown_width,
                                |ui| {
                                    let dropdown_id =
                                        ui.make_persistent_id(("layout_option_dropdown", row_idx));
                                    let dropdown_resp =
                                        crate::ui_style::modern_dropdown_button_sized(
                                            ui,
                                            dropdown_id,
                                            &selected_text,
                                            ui.visuals().text_color(),
                                            dropdown_width,
                                            metrics.settings_control_height(),
                                            metrics.settings_control_font_size(),
                                        );

                                    egui::popup_below_widget(
                                        ui,
                                        dropdown_id,
                                        &dropdown_resp,
                                        egui::PopupCloseBehavior::CloseOnClickOutside,
                                        |ui| {
                                            ui.set_min_width(dropdown_width);
                                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                            egui::ScrollArea::vertical()
                                                .id_salt(("layout_option_dropdown_scroll", row_idx))
                                                .max_height(metrics.value(142.0))
                                                .auto_shrink([false, true])
                                                .show(ui, |ui| {
                                                    for (choice_idx, label) in
                                                        option.choices.iter().enumerate()
                                                    {
                                                        let selected = choice_idx == selected_idx;
                                                        let (option_rect, option_resp) = ui
                                                            .allocate_exact_size(
                                                                metrics.size(220.0, 28.0),
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
                                                        let display_label =
                                                            Self::display_preset_choice_label(
                                                                self.app_settings.language,
                                                                label,
                                                            );
                                                        ui.painter().text(
                                                            egui::pos2(
                                                                option_rect.left()
                                                                    + metrics.value(10.0),
                                                                option_rect.center().y,
                                                            ),
                                                            egui::Align2::LEFT_CENTER,
                                                            display_label,
                                                            FontId::proportional(
                                                                metrics.value(12.0),
                                                            ),
                                                            if selected {
                                                                ui.visuals().text_color()
                                                            } else {
                                                                app_muted_text(dark)
                                                            },
                                                        );
                                                        if option_resp.clicked() {
                                                            self.set_layout_option_value(
                                                                row_idx,
                                                                choice_idx as u32,
                                                            );
                                                            ui.memory_mut(|m| m.close_popup());
                                                        }
                                                    }
                                                });
                                        },
                                    );
                                },
                            );
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
            });
        });
    }

    fn set_layout_option_value(&mut self, option_idx: usize, value: u32) {
        let Some(options) = self
            .layout
            .as_ref()
            .map(|layout| layout.layout_options.clone())
        else {
            return;
        };
        if option_idx >= options.len() {
            return;
        }
        let mut values =
            Self::unpack_layout_option_values(&options, self.layout_options_value.unwrap_or(0));
        if let Some(slot) = values.get_mut(option_idx) {
            *slot = value;
        }
        let packed = Self::pack_layout_option_values(&options, &values);
        self.layout_options_value = Some(packed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(option) = options.get(option_idx) {
            if Self::is_display_preset_layout_option(option) {
                let selected_label = option
                    .choices
                    .get(value as usize)
                    .map(|choice| choice.as_str())
                    .unwrap_or("");
                if Self::display_preset_needs_entropy(selected_label) {
                    self.save_display_preset_restore(packed);
                } else {
                    self.clear_display_preset_restore();
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(hid) = &self.hid_device {
            if let Err(e) = hid.set_layout_options(packed) {
                self.status_msg = format!("Failed to save layout option: {e}");
                log::warn!("set_layout_options failed: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.sync_qmk_hid_host_bridges();
    }
}
