use super::*;

#[derive(Clone, Copy)]
enum LayerLedRow {
    Brightness,
    Timeout,
    BtProfileColor(usize),
    LayerColor(usize),
}

#[derive(Clone, Copy)]
enum LayerLedColorTarget {
    BtProfile(usize),
    Layer(usize),
}

impl EntropyApp {
    pub(super) fn draw_layer_led_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
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

        ui.allocate_ui_at_rect(content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::LayerLedsTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(
                        lang,
                        crate::i18n::Key::LayerLedsDescription,
                    ))
                    .size(13.0)
                    .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.layer_led_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::LayerLedsUnavailable),
                        Some(crate::i18n::tr(lang, crate::i18n::Key::LayerLedsEnableHint)),
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::LayerLedsConnect),
                        None,
                    );
                    return;
                }

                let rows = self.layer_led_rows();
                let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx());
                let list = allocate_adaptive_settings_list_viewport(
                    ui,
                    "layer_led_settings",
                    metrics,
                    rows.len(),
                    0.0,
                );
                ui.allocate_ui_at_rect(list.content_rect, |ui| {
                    ui.set_clip_rect(list.viewport);
                    ui.set_min_size(list.content_rect.size());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.draw_layer_led_editor_content(
                        ui,
                        list.first_visible_row..list.last_visible_row,
                        list.row_content_width,
                        list.row_height,
                        list.suppress_tooltips,
                        &rows,
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

    fn layer_led_rows(&self) -> Vec<LayerLedRow> {
        let mut rows = Vec::new();
        if self.layer_led_settings.brightness.is_some() {
            rows.push(LayerLedRow::Brightness);
        }
        if self.layer_led_settings.timeout.is_some() {
            rows.push(LayerLedRow::Timeout);
        }
        rows.extend(
            (0..self.layer_led_settings.bt_profile_colors.len()).map(LayerLedRow::BtProfileColor),
        );
        rows.extend((0..self.layer_led_settings.layer_colors.len()).map(LayerLedRow::LayerColor));
        rows
    }

    fn draw_layer_led_editor_content(
        &mut self,
        ui: &mut egui::Ui,
        row_range: std::ops::Range<usize>,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
        rows: &[LayerLedRow],
    ) {
        let scale = (row_height / 54.0).clamp(1.0, 1.12);
        let slider_width = 168.0 * scale;
        let value_width = 42.0 * scale;
        let slider_size = [slider_width, 18.0 * scale];
        let slider_control_width = slider_width + value_width;
        let swatch_width = 64.0 * scale;
        let swatch_size = Vec2::new(64.0 * scale, 34.0 * scale);

        for row_idx in row_range {
            let Some(row) = rows.get(row_idx).copied() else {
                continue;
            };
            match row {
                LayerLedRow::Brightness => {
                    let Some(setting) = self.layer_led_settings.brightness.clone() else {
                        continue;
                    };
                    let brightness_max = setting.max.max(1) as f32;
                    let mut value = (setting.value as f32 / brightness_max * 100.0)
                        .round()
                        .clamp(0.0, 100.0);
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "advanced_settings.led_brightness",
                        ),
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                "advanced_settings.global_led_brightness_for_layer_color_lighting",
                            ))
                        },
                        slider_control_width,
                        |ui| {
                            let value_text = format!("{}%", value.round() as u8);
                            if self.draw_layer_led_slider_control(
                                ui,
                                scale,
                                row_height,
                                slider_width,
                                value_width,
                                slider_size,
                                &mut value,
                                0.0..=100.0,
                                value_text,
                                app_accent().gamma_multiply(0.5),
                                None,
                                suppress_tooltips,
                            ) {
                                let new_value = ((value / 100.0) * brightness_max)
                                    .round()
                                    .clamp(0.0, brightness_max)
                                    as u16;
                                if new_value != setting.value {
                                    if let Some(current) = &mut self.layer_led_settings.brightness {
                                        current.value = new_value;
                                    }
                                    self.write_layer_led_numeric(
                                        setting.qsid,
                                        setting.width,
                                        new_value,
                                        "Layer LED brightness",
                                    );
                                }
                            }
                        },
                    );
                }
                LayerLedRow::Timeout => {
                    let Some(setting) = self.layer_led_settings.timeout.clone() else {
                        continue;
                    };
                    let mut value = setting.value as f32;
                    let timeout_unit = self.layer_led_settings.timeout_unit;
                    let tooltip_key = match timeout_unit {
                        LayerLedTimeoutUnit::Seconds => "advanced_settings.seconds_before_leds_turn_off_automatically_0_disables_timeout",
                        LayerLedTimeoutUnit::Minutes => "advanced_settings.minutes_before_leds_turn_off_automatically_0_disables_timeout",
                    };
                    let unit = match timeout_unit {
                        LayerLedTimeoutUnit::Seconds => SettingsFieldUnit::Seconds,
                        LayerLedTimeoutUnit::Minutes => SettingsFieldUnit::Minutes,
                    };
                    let suffix = match timeout_unit {
                        LayerLedTimeoutUnit::Seconds => "s",
                        LayerLedTimeoutUnit::Minutes => "m",
                    };
                    crate::ui_style::settings_list_row_with_tooltip(
                        ui,
                        content_width,
                        row_height,
                        crate::i18n::tr_catalog(
                            self.app_settings.language,
                            "advanced_settings.led_timeout",
                        ),
                        true,
                        if suppress_tooltips {
                            None
                        } else {
                            Some(crate::i18n::tr_catalog(
                                self.app_settings.language,
                                tooltip_key,
                            ))
                        },
                        slider_control_width,
                        |ui| {
                            let value_text = if value.round() as u16 == 0 {
                                crate::i18n::tr_catalog(
                                    self.app_settings.language,
                                    "advanced_settings.off",
                                )
                                .to_string()
                            } else {
                                format!("{}{}", value.round() as u16, suffix)
                            };
                            if self.draw_layer_led_slider_control(
                                ui,
                                scale,
                                row_height,
                                slider_width,
                                value_width,
                                slider_size,
                                &mut value,
                                0.0..=setting.max.max(1) as f32,
                                value_text,
                                if ui.visuals().dark_mode {
                                    Color32::from_rgb(92, 92, 96)
                                } else {
                                    Color32::from_rgb(190, 184, 182)
                                },
                                Some(unit),
                                suppress_tooltips,
                            ) {
                                let new_value = value.round().clamp(0.0, setting.max as f32) as u16;
                                if new_value != setting.value {
                                    if let Some(current) = &mut self.layer_led_settings.timeout {
                                        current.value = new_value;
                                    }
                                    self.write_layer_led_numeric(
                                        setting.qsid,
                                        setting.width,
                                        new_value,
                                        "Layer LED timeout",
                                    );
                                }
                            }
                        },
                    );
                }
                LayerLedRow::BtProfileColor(profile) => {
                    let Some(setting) = self
                        .layer_led_settings
                        .bt_profile_colors
                        .get(profile)
                        .cloned()
                    else {
                        continue;
                    };
                    let profile_text = profile.to_string();
                    let label = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "advanced_settings.bt_profile_color",
                        &[("profile", profile_text.as_str())],
                    );
                    let tooltip = crate::i18n::tr_catalog_format(
                        self.app_settings.language,
                        "advanced_settings.led_palette_color_for_bluetooth_profile",
                        &[("profile", profile_text.as_str())],
                    );
                    self.draw_layer_led_color_row(
                        ui,
                        content_width,
                        row_height,
                        suppress_tooltips,
                        swatch_width,
                        swatch_size,
                        scale,
                        &label,
                        &tooltip,
                        LayerLedColorTarget::BtProfile(profile),
                        setting,
                    );
                }
                LayerLedRow::LayerColor(layer) => {
                    let Some(setting) = self.layer_led_settings.layer_colors.get(layer).cloned()
                    else {
                        continue;
                    };
                    let label = self.layer_led_layer_label(layer);
                    let tooltip =
                        if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                            format!("Цвет подсветки, когда активен слой {layer}")
                        } else {
                            format!("LED palette color used when layer {layer} is active")
                        };
                    self.draw_layer_led_color_row(
                        ui,
                        content_width,
                        row_height,
                        suppress_tooltips,
                        swatch_width,
                        swatch_size,
                        scale,
                        &label,
                        &tooltip,
                        LayerLedColorTarget::Layer(layer),
                        setting,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_layer_led_slider_control(
        &self,
        ui: &mut egui::Ui,
        scale: f32,
        row_height: f32,
        slider_width: f32,
        value_width: f32,
        slider_size: [f32; 2],
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        value_text: String,
        slider_fill: Color32,
        unit: Option<SettingsFieldUnit>,
        suppress_tooltips: bool,
    ) -> bool {
        ui.spacing_mut().item_spacing.x = 0.0;
        let dark = ui.visuals().dark_mode;
        ui.visuals_mut().selection.bg_fill = slider_fill;
        ui.visuals_mut().widgets.active.bg_fill = slider_fill;
        ui.visuals_mut().widgets.active.weak_bg_fill = slider_fill;
        ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1.0, slider_fill);
        let mut changed = false;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_sized(
                [value_width, row_height],
                egui::Label::new(RichText::new(value_text).size(12.0 * scale).color(if dark {
                    Color32::from_gray(230)
                } else {
                    Color32::from_gray(55)
                }))
                .halign(egui::Align::RIGHT),
            );
            ui.spacing_mut().slider_width = slider_width;
            let slider = egui::Slider::new(value, range)
                .step_by(1.0)
                .show_value(false)
                .trailing_fill(true);
            let resp = ui.add_sized(slider_size, slider);
            let resp = if let Some(unit) = unit {
                settings_field_unit_tooltip(
                    resp,
                    self.app_settings.language,
                    suppress_tooltips,
                    unit,
                )
            } else {
                resp
            };
            changed = resp.changed();
        });
        changed
    }

    fn layer_led_layer_label(&self, layer: usize) -> String {
        self.layer_names
            .get(layer)
            .map(|name| name.trim())
            .filter(|name| !name.is_empty() && *name != layer.to_string())
            .map(|name| {
                let visible: String = name.chars().take(22).collect();
                if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                    format!("Слой {layer}: {visible}")
                } else {
                    format!("Layer {layer}: {visible}")
                }
            })
            .unwrap_or_else(|| {
                if matches!(self.app_settings.language, crate::i18n::Language::Russian) {
                    format!("Цвет слоя {layer}")
                } else {
                    format!("Layer {layer} color")
                }
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_layer_led_color_row(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        row_height: f32,
        suppress_tooltips: bool,
        swatch_width: f32,
        swatch_size: Vec2,
        scale: f32,
        label: &str,
        tooltip: &str,
        target: LayerLedColorTarget,
        setting: LayerLedColorSetting,
    ) {
        let qsids = setting.all_qsids().collect::<Vec<_>>();
        let current = setting.value;
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
            swatch_width,
            |ui| {
                let dark = ui.visuals().dark_mode;
                let popup_id = match target {
                    LayerLedColorTarget::BtProfile(profile) => {
                        ui.make_persistent_id(("layer_led_bt_profile_color_picker", profile))
                    }
                    LayerLedColorTarget::Layer(layer) => {
                        ui.make_persistent_id(("layer_led_layer_color_picker", layer))
                    }
                };
                let popup_open = ui.memory(|m| m.is_popup_open(popup_id));
                let swatch_color = layer_led_palette_color(current);
                let swatch_border = if popup_open {
                    app_accent()
                } else if dark {
                    Color32::from_gray(95)
                } else {
                    Color32::from_gray(185)
                };
                let (swatch_rect, swatch_resp) =
                    ui.allocate_exact_size(swatch_size, Sense::click());
                if swatch_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if swatch_resp.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }
                ui.painter().rect(
                    swatch_rect,
                    9.0,
                    app_surface_fill(dark),
                    Stroke::new(1.0, swatch_border),
                    egui::StrokeKind::Inside,
                );
                ui.painter().rect(
                    swatch_rect.shrink(5.0 * scale),
                    6.0,
                    swatch_color,
                    Stroke::new(1.0, swatch_border.gamma_multiply(0.85)),
                    egui::StrokeKind::Inside,
                );
                if current == 0 {
                    ui.painter().line_segment(
                        [
                            swatch_rect.left_top() + egui::vec2(10.0 * scale, 10.0 * scale),
                            swatch_rect.right_bottom() - egui::vec2(10.0 * scale, 10.0 * scale),
                        ],
                        Stroke::new(1.2, app_muted_text(dark)),
                    );
                }
                swatch_resp.clone().on_hover_text(crate::i18n::tr_catalog(
                    self.app_settings.language,
                    layer_led_palette_name(current),
                ));

                ui.style_mut().visuals.window_stroke = crate::ui_style::modal_outline_stroke(dark);
                ui.style_mut().visuals.window_fill = app_surface_fill(dark);
                egui::popup_below_widget(
                    ui,
                    popup_id,
                    &swatch_resp,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        let cell = 28.0 * scale;
                        let gap = 6.0 * scale;
                        const COLS: usize = 5;
                        let picker_width = cell * COLS as f32 + gap * (COLS - 1) as f32;
                        ui.set_min_width(picker_width);
                        ui.spacing_mut().item_spacing = Vec2::new(gap, gap);
                        for row in 0..5 {
                            ui.horizontal(|ui| {
                                for col in 0..COLS {
                                    let color_idx = row * COLS + col;
                                    let Some(option_label) = LAYER_LED_PALETTE.get(color_idx)
                                    else {
                                        continue;
                                    };
                                    let color_idx_u8 = color_idx as u8;
                                    let selected = color_idx_u8 == current;
                                    let (cell_rect, cell_resp) =
                                        ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                                    if cell_resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    let outline = if selected {
                                        app_accent()
                                    } else if dark {
                                        Color32::from_rgb(72, 72, 76)
                                    } else {
                                        Color32::from_rgb(210, 210, 214)
                                    };
                                    ui.painter().rect(
                                        cell_rect,
                                        7.0,
                                        app_surface_fill(dark),
                                        Stroke::new(if selected { 1.6 } else { 1.0 }, outline),
                                        egui::StrokeKind::Inside,
                                    );
                                    ui.painter().rect(
                                        cell_rect.shrink(4.5 * scale),
                                        5.0,
                                        layer_led_palette_color(color_idx_u8),
                                        Stroke::NONE,
                                        egui::StrokeKind::Inside,
                                    );
                                    if color_idx == 0 {
                                        ui.painter().line_segment(
                                            [
                                                cell_rect.left_top()
                                                    + egui::vec2(8.0 * scale, 8.0 * scale),
                                                cell_rect.right_bottom()
                                                    - egui::vec2(8.0 * scale, 8.0 * scale),
                                            ],
                                            Stroke::new(1.1, app_muted_text(dark)),
                                        );
                                    }
                                    cell_resp.clone().on_hover_text(crate::i18n::tr_text(
                                        self.app_settings.language,
                                        option_label,
                                    ));
                                    if cell_resp.clicked() {
                                        match target {
                                            LayerLedColorTarget::BtProfile(profile) => {
                                                if let Some(current) = self
                                                    .layer_led_settings
                                                    .bt_profile_colors
                                                    .get_mut(profile)
                                                {
                                                    current.value = color_idx_u8;
                                                }
                                            }
                                            LayerLedColorTarget::Layer(layer) => {
                                                if let Some(current) = self
                                                    .layer_led_settings
                                                    .layer_colors
                                                    .get_mut(layer)
                                                {
                                                    current.value = color_idx_u8;
                                                }
                                            }
                                        }
                                        self.write_layer_led_color(&qsids, color_idx_u8);
                                        ui.memory_mut(|m| m.close_popup());
                                    }
                                }
                            });
                        }
                    },
                );
            },
        );
    }

    fn write_layer_led_color(&mut self, qsids: &[u16], value: u8) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        for qsid in qsids {
            if let Err(e) = hid.set_qmk_setting_u8(*qsid, value) {
                self.status_msg = format!("Failed to save Layer LED color: {}", e);
                log::warn!("set_qmk_setting_u8(layer_led qsid {qsid}) failed: {e}");
            }
        }
    }

    fn write_layer_led_numeric(&mut self, qsid: u16, width: u8, value: u16, label: &str) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = if width > 1 {
            hid.set_qmk_setting_u16(qsid, value)
        } else {
            hid.set_qmk_setting_u8(qsid, value.min(u8::MAX as u16) as u8)
        };
        if let Err(e) = result {
            self.status_msg = format!("Failed to save {label}: {}", e);
            log::warn!("set_qmk_setting(layer_led qsid {qsid}) failed: {e}");
        }
    }
}
