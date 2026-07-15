use super::*;

impl EntropyApp {
    fn touchpad_numeric_value(&self, qsid: u16) -> u16 {
        match qsid {
            121 => self.touchpad_settings.sniper_sens as u16,
            122 => self.touchpad_settings.scroll_sens as u16,
            123 => self.touchpad_settings.text_sens as u16,
            _ => 0,
        }
    }

    fn set_touchpad_numeric_value(&mut self, qsid: u16, value: u16) {
        match qsid {
            121 => self.touchpad_settings.sniper_sens = value.min(u8::MAX as u16) as u8,
            122 => self.touchpad_settings.scroll_sens = value.min(u8::MAX as u16) as u8,
            123 => self.touchpad_settings.text_sens = value.min(u8::MAX as u16) as u8,
            _ => {}
        }
    }

    fn write_touchpad_numeric_setting(
        &mut self,
        qsid: u16,
        old_value: u16,
        value: u16,
        display_label: &str,
    ) {
        let value = value.clamp(1, 255) as u8;
        self.queue_touchpad_setting_write(
            display_label.to_owned(),
            qsid,
            1,
            old_value,
            value as u16,
        );
    }

    fn write_touchpad_select_setting(
        &mut self,
        qsid: u16,
        old_value: u16,
        value: u8,
        display_label: &str,
    ) {
        self.queue_touchpad_setting_write(
            display_label.to_owned(),
            qsid,
            1,
            old_value,
            value as u16,
        );
    }

    fn write_touchpad_bool_setting(
        &mut self,
        qsid: u16,
        old_value: bool,
        value: bool,
        display_label: &str,
    ) {
        self.queue_touchpad_setting_write(
            display_label.to_owned(),
            qsid,
            1,
            u16::from(old_value),
            u16::from(value),
        );
    }

    fn write_touchpad_bits(&mut self, old_value: u8, display_label: &str) {
        self.queue_touchpad_setting_write(
            display_label.to_owned(),
            124,
            1,
            old_value as u16,
            self.touchpad_settings.bits as u16,
        );
    }

    pub(super) fn draw_touchpad_select_control(
        ui: &mut egui::Ui,
        dark: bool,
        dropdown_id: egui::Id,
        selected_idx: usize,
        variants: &[String],
        width: f32,
    ) -> (egui::Response, Option<usize>) {
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let selected_text = variants
            .get(selected_idx)
            .map(|s| s.as_str())
            .unwrap_or("Unknown");
        let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
            ui,
            dropdown_id,
            selected_text,
            ui.visuals().text_color(),
            width,
            metrics.settings_control_height(),
            metrics.settings_control_font_size(),
        );
        let mut picked = None;
        crate::ui_style::popup_below_widget(
            ui,
            dropdown_id,
            &dropdown_resp,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(width);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                egui::ScrollArea::vertical()
                    .id_salt(("touchpad_select_scroll", dropdown_id))
                    .max_height(metrics.value(142.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (idx, label) in variants.iter().enumerate() {
                            let selected = idx == selected_idx;
                            let (option_rect, option_resp) = ui.allocate_exact_size(
                                Vec2::new(width, metrics.value(28.0)),
                                Sense::click(),
                            );
                            if option_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
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
                                    option_rect.left() + metrics.value(10.0),
                                    option_rect.center().y,
                                ),
                                egui::Align2::LEFT_CENTER,
                                label,
                                FontId::proportional(metrics.value(12.0)),
                                if selected {
                                    ui.visuals().text_color()
                                } else {
                                    app_muted_text(dark)
                                },
                            );
                            if option_resp.clicked() {
                                picked = Some(idx);
                                egui::Popup::close_all(ui.ctx());
                            }
                        }
                    });
            },
        );
        (dropdown_resp, picked)
    }

    fn draw_touchpad_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        metrics: crate::ui_style::ResponsiveMetrics,
        suppress_tooltips: bool,
    ) {
        let content_width = metrics.settings_row_content_width();
        let row_height = metrics.settings_row_height();
        let field_width = metrics.value(86.0);
        let switch_width = metrics.value(46.0);
        let switch_size = metrics.size(46.0, 24.0);
        let dropdown_width = metrics.value(120.0);
        let status_width = self.settings_write_status_width(metrics);
        let dark = ui.visuals().dark_mode;

        for row_idx in row_range {
            match row_idx {
                0 => {
                    let label = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "touchpad_settings.dpi",
                    );
                    let variants = self.touchpad_settings.dpi_variants.clone();
                    let selected_idx =
                        (self.touchpad_settings.dpi as usize).min(variants.len().saturating_sub(1));
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        label,
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.touchpad_pointer_resolution_in_dots_per_inch",
                            ))
                        },
                        dropdown_width + status_width,
                        |ui| {
                            self.draw_settings_write_status(ui, 120, metrics);
                            if variants.is_empty() {
                                let current = self.touchpad_settings.dpi;
                                let edit_id = egui::Id::new(("touchpad_edit", 120u16));
                                let mut text = ui.ctx().data_mut(|d| {
                                    d.get_temp::<String>(edit_id)
                                        .unwrap_or_else(|| current.to_string())
                                });
                                if text.parse::<u16>().ok() != Some(current)
                                    && !ui.memory(|m| m.has_focus(edit_id))
                                {
                                    text = current.to_string();
                                }
                                let resp = crate::ui_style::modern_text_field_sized(
                                    ui,
                                    edit_id,
                                    &mut text,
                                    field_width,
                                    metrics.settings_control_height(),
                                    "",
                                    5,
                                    egui::Align::Center,
                                );
                                let commit = resp.lost_focus()
                                    || (resp.has_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                if commit {
                                    match text.trim().parse::<u16>() {
                                        Ok(value) => {
                                            let value = value.clamp(100, 1000);
                                            if value != current {
                                                self.touchpad_settings.dpi = value;
                                                self.queue_touchpad_setting_write(
                                                    label.to_owned(),
                                                    120,
                                                    2,
                                                    current,
                                                    value,
                                                );
                                            }
                                            text = value.to_string();
                                        }
                                        Err(_) => text = current.to_string(),
                                    }
                                }
                                ui.ctx().data_mut(|d| d.insert_temp(edit_id, text));
                            } else {
                                let dropdown_id = ui.make_persistent_id("touchpad_dpi_dropdown");
                                let (_, picked) = Self::draw_touchpad_select_control(
                                    ui,
                                    dark,
                                    dropdown_id,
                                    selected_idx,
                                    &variants,
                                    dropdown_width,
                                );
                                if let Some(picked) = picked {
                                    let old_value = self.touchpad_settings.dpi;
                                    self.touchpad_settings.dpi = picked as u16;
                                    self.write_touchpad_select_setting(
                                        120,
                                        old_value,
                                        picked as u8,
                                        label,
                                    );
                                }
                            }
                        },
                    );
                }
                1..=3 => {
                    const SENS_MIN: u16 = 1;
                    const SENS_MAX: u16 = 32;
                    let slider_width = metrics.value(124.0);
                    let value_width = metrics.value(34.0);
                    let slider_control_width = slider_width + value_width + metrics.value(8.0);
                    let slider_size = [slider_width, metrics.value(20.0)];
                    let (qsid, label, tooltip) = match row_idx {
                        1 => (
                            121,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.sniper_sens",
                            ),
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.sniper_divisor_lower_is_faster_higher_is_more_precise",
                            ),
                        ),
                        2 => (
                            122,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.scroll_sens",
                            ),
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.scroll_divisor_lower_is_faster_higher_is_smoother",
                            ),
                        ),
                        3 => (
                            123,
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.text_sens",
                            ),
                            crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.text_mode_divisor_lower_is_faster_higher_is_slower",
                            ),
                        ),
                        _ => unreachable!(),
                    };
                    let current = self.touchpad_numeric_value(qsid).clamp(SENS_MIN, SENS_MAX);
                    let mut value = current as f32;
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        label,
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(tooltip)
                        },
                        slider_control_width + status_width,
                        |ui| {
                            self.draw_settings_write_status(ui, qsid, metrics);
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let slider_fill = if dark {
                                Color32::from_rgb(92, 92, 96)
                            } else {
                                Color32::from_rgb(190, 184, 182)
                            };
                            ui.visuals_mut().selection.bg_fill = slider_fill;
                            ui.visuals_mut().widgets.active.bg_fill = slider_fill;
                            ui.visuals_mut().widgets.active.weak_bg_fill = slider_fill;
                            ui.visuals_mut().widgets.hovered.bg_stroke =
                                Stroke::new(1.0_f32, slider_fill);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_sized(
                                        [value_width, row_height],
                                        egui::Label::new(
                                            RichText::new(format!("{}", value.round() as u8))
                                                .size(metrics.value(12.0))
                                                .color(if dark {
                                                    Color32::from_gray(230)
                                                } else {
                                                    Color32::from_gray(55)
                                                }),
                                        )
                                        .halign(egui::Align::RIGHT),
                                    );
                                    ui.spacing_mut().slider_width = slider_width;
                                    let slider = egui::Slider::new(
                                        &mut value,
                                        SENS_MIN as f32..=SENS_MAX as f32,
                                    )
                                    .step_by(1.0)
                                    .show_value(false)
                                    .trailing_fill(true);
                                    let resp = ui.add_sized(slider_size, slider);
                                    if resp.changed() {
                                        let new_value =
                                            value.round().clamp(SENS_MIN as f32, SENS_MAX as f32)
                                                as u16;
                                        if new_value != current {
                                            self.set_touchpad_numeric_value(qsid, new_value);
                                            self.write_touchpad_numeric_setting(
                                                qsid, current, new_value, label,
                                            );
                                        }
                                    }
                                },
                            );
                        },
                    );
                }
                4..=6 => {
                    let (bit, label, tooltip) = match row_idx {
                        4 => (0, crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.invert_scroll"), crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.reverse_the_touchpad_scroll_direction")),
                        5 => (
                            1,
                            crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.acceleration"),
                            crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.use_firmware_pointer_acceleration_for_touchpad_movement"),
                        ),
                        6 => (
                            2,
                            crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.sticky_mode"),
                            crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.keep_the_selected_touchpad_mode_active_until_another_mode_is_selected"),
                        ),
                        _ => unreachable!(),
                    };
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        label,
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(tooltip)
                        },
                        switch_width + status_width,
                        |ui| {
                            self.draw_settings_write_status(ui, 124, metrics);
                            let old_bits = self.touchpad_settings.bits;
                            let mut value = self.touchpad_settings.bit(bit);
                            let resp = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                ("touchpad_settings", bit),
                                &mut value,
                                switch_size,
                            );
                            if resp.changed() {
                                self.touchpad_settings.set_bit(bit, value);
                                self.write_touchpad_bits(old_bits, label);
                            }
                        },
                    );
                }
                7 if self.touchpad_settings.auto_layer_enable_supported => {
                    let label = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "touchpad_settings.auto_layer_enable",
                    );
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        label,
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(self.app_settings.language, "touchpad_settings.automatically_switch_to_the_selected_layer_while_the_touchpad_is_activ"))
                        },
                        switch_width + status_width,
                        |ui| {
                            self.draw_settings_write_status(ui, 142, metrics);
                            let mut value = self.touchpad_settings.auto_layer_enable;
                            let resp = crate::ui_style::settings_switch_sized_stable(
                                ui,
                                "touchpad_settings_auto_layer_enable",
                                &mut value,
                                switch_size,
                            );
                            if resp.changed() {
                                let old_value = self.touchpad_settings.auto_layer_enable;
                                self.touchpad_settings.auto_layer_enable = value;
                                self.write_touchpad_bool_setting(142, old_value, value, label);
                            }
                        },
                    );
                }
                8 if self.touchpad_settings.auto_layer_supported() => {
                    let label = crate::i18n::tr_catalog(
                        self.app_settings.language,
                        "touchpad_settings.auto_layer",
                    );
                    let variants = self.touchpad_settings.auto_layer_variants.clone();
                    let selected_idx = (self.touchpad_settings.auto_layer as usize)
                        .min(variants.len().saturating_sub(1));
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        label,
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "touchpad_settings.layer_selected_automatically_while_the_touchpad_is_active",
                            ))
                        },
                        dropdown_width + status_width,
                        |ui| {
                            self.draw_settings_write_status(ui, 143, metrics);
                            let dropdown_id = ui.make_persistent_id("touchpad_auto_layer_dropdown");
                            let (_, picked) = Self::draw_touchpad_select_control(
                                ui,
                                dark,
                                dropdown_id,
                                selected_idx,
                                &variants,
                                dropdown_width,
                            );
                            if let Some(picked) = picked {
                                let old_value = self.touchpad_settings.auto_layer;
                                self.touchpad_settings.auto_layer = picked as u8;
                                self.write_touchpad_select_setting(
                                    143,
                                    old_value as u16,
                                    picked as u8,
                                    label,
                                );
                            }
                        },
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn draw_touchpad_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.qmk_setting_transport_available()
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::TouchpadTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::TouchpadDescription))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.touchpad_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::TouchpadUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::TouchpadEnableHint)),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::TouchpadConnect),
                        None,
                    );
                    return;
                }

                let total_rows = self.touchpad_settings.row_count();
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "touchpad_settings",
                    metrics,
                    total_rows,
                    0.0,
                );
                crate::ui_style::allocate_ui_at_rect(ui, list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_touchpad_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        metrics,
                        list.suppress_tooltips,
                    );
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
}
