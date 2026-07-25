use super::*;

#[derive(Clone, Copy)]
struct RgbModalLayout {
    content_width: f32,
    top_padding: f32,
    row_height: f32,
    color_row_height: f32,
}

impl RgbModalLayout {
    fn responsive(ctx: &egui::Context) -> Self {
        let metrics = crate::ui_style::ResponsiveMetrics::from_ctx(ctx);
        Self {
            content_width: metrics.settings_content_width(),
            top_padding: metrics.value(4.0),
            row_height: metrics.settings_row_height(),
            color_row_height: metrics.settings_row_height(),
        }
    }

    fn modal_layout(self) -> crate::ui_style::ModalLayout {
        crate::ui_style::ModalLayout::new(self.content_width).with_top_padding(self.top_padding)
    }
}

fn rgb_effect_options(state: &RgbSettingsState) -> Vec<(u16, &'static str)> {
    match state.kind {
        RgbSupportKind::QmkRgblight => QMK_RGBLIGHT_EFFECTS.to_vec(),
        RgbSupportKind::VialRgb => VIALRGB_EFFECTS
            .iter()
            .copied()
            .filter(|(id, _)| {
                *id == 0
                    || state.supported_effects.is_empty()
                    || state.supported_effects.contains(id)
            })
            .collect(),
        RgbSupportKind::None => vec![],
    }
}

fn rgb_effect_supports_color(kind: RgbSupportKind, effect: u16) -> bool {
    match kind {
        RgbSupportKind::QmkRgblight => matches!(effect, 1..=5 | 15..=36),
        RgbSupportKind::VialRgb => effect != 0,
        RgbSupportKind::None => false,
    }
}

fn rgb_effect_supports_speed(kind: RgbSupportKind, effect: u16) -> bool {
    match kind {
        RgbSupportKind::QmkRgblight => matches!(effect, 2..=36),
        RgbSupportKind::VialRgb => !matches!(effect, 0..=5),
        RgbSupportKind::None => false,
    }
}

fn rgb_picker_contrast(color: impl Into<egui::Rgba>) -> Color32 {
    if color.into().intensity() < 0.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

fn compact_rgb_slider_1d(
    ui: &mut egui::Ui,
    value: &mut f32,
    color_at: impl Fn(f32) -> Color32,
) -> bool {
    let desired_size = Vec2::new(ui.spacing().slider_width, 18.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    if let Some(pos) = response.interact_pointer_pos() {
        let new_value = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        if (*value - new_value).abs() > f32::EPSILON {
            *value = new_value;
            changed = true;
        }
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let rect = rect.expand(visuals.expansion);
        let mut mesh = egui::epaint::Mesh::default();
        const N: u32 = 36;
        for i in 0..N {
            let t = i as f32 / (N - 1) as f32;
            let x = egui::lerp(rect.x_range(), t);
            mesh.colored_vertex(egui::pos2(x, rect.top()), color_at(t));
            mesh.colored_vertex(egui::pos2(x, rect.bottom()), color_at(t));
            if i + 1 < N {
                let idx = i * 2;
                mesh.add_triangle(idx, idx + 1, idx + 2);
                mesh.add_triangle(idx + 1, idx + 2, idx + 3);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));

        let x = egui::lerp(rect.x_range(), *value);
        let picked_color = color_at(*value);
        let stroke = Stroke::new(1.2_f32, rgb_picker_contrast(picked_color));
        let handle_rect = egui::Rect::from_center_size(
            egui::pos2(x, rect.center().y),
            Vec2::new(10.0, rect.height() + 6.0),
        );
        ui.painter().rect(
            handle_rect,
            3.0,
            picked_color,
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .rect_stroke(rect, 2.0, visuals.bg_stroke, egui::StrokeKind::Inside);
    }

    changed
}

fn compact_rgb_slider_2d(
    ui: &mut egui::Ui,
    x_value: &mut f32,
    y_value: &mut f32,
    color_at: impl Fn(f32, f32) -> Color32,
) -> bool {
    let desired_size = Vec2::splat(ui.spacing().slider_width);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    if let Some(pos) = response.interact_pointer_pos() {
        let new_x_value = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let new_y_value = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
        if (*x_value - new_x_value).abs() > f32::EPSILON {
            *x_value = new_x_value;
            changed = true;
        }
        if (*y_value - new_y_value).abs() > f32::EPSILON {
            *y_value = new_y_value;
            changed = true;
        }
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let rect = rect.expand(visuals.expansion);
        let mut mesh = egui::epaint::Mesh::default();
        const N: u32 = 36;
        for xi in 0..N {
            let xt = xi as f32 / (N - 1) as f32;
            let x = egui::lerp(rect.x_range(), xt);
            for yi in 0..N {
                let yt = yi as f32 / (N - 1) as f32;
                let y = egui::lerp(rect.y_range(), 1.0 - yt);
                mesh.colored_vertex(egui::pos2(x, y), color_at(xt, yt));
                if xi + 1 < N && yi + 1 < N {
                    let tl = yi + xi * N;
                    mesh.add_triangle(tl, tl + 1, tl + N);
                    mesh.add_triangle(tl + 1, tl + N, tl + N + 1);
                }
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));

        let x = egui::lerp(rect.x_range(), *x_value);
        let y = egui::lerp(rect.y_range(), 1.0 - *y_value);
        let picked_color = color_at(*x_value, *y_value);
        let stroke = Stroke::new(1.6_f32, rgb_picker_contrast(picked_color));
        ui.painter().circle_stroke(egui::pos2(x, y), 10.0, stroke);
        ui.painter()
            .rect_stroke(rect, 2.0, visuals.bg_stroke, egui::StrokeKind::Inside);
    }

    changed
}

fn compact_rgb_color_picker(ui: &mut egui::Ui, hsva: &mut egui::ecolor::Hsva) -> bool {
    let mut changed = false;
    let mut h = hsva.h.rem_euclid(1.0);
    let mut s = hsva.s.clamp(0.0, 1.0);
    let mut v = hsva.v.clamp(0.0, 1.0);

    changed |= compact_rgb_slider_2d(ui, &mut s, &mut v, |s, v| {
        egui::ecolor::Hsva { h, s, v, a: 1.0 }.into()
    });
    ui.add_space(6.0);
    changed |= compact_rgb_slider_1d(ui, &mut h, |h| {
        egui::ecolor::Hsva {
            h,
            s: 1.0,
            v: 1.0,
            a: 1.0,
        }
        .into()
    });

    if changed {
        hsva.h = h;
        hsva.s = s;
        hsva.v = v;
    }
    changed
}

impl EntropyApp {
    pub(super) fn draw_rgb_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        dark: bool,
    ) {
        let lang = self.app_settings.language;
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

        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::RgbTitle))
                        .size(18.0)
                        .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(crate::i18n::tr(lang, crate::i18n::Key::RgbDescription))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(24.0);

                if !self.rgb_settings.supported {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::RgbUnavailableTooltip),
                        None,
                    );
                    return;
                }

                if !hid_ready {
                    crate::ui_style::modal_empty_state(
                        ui,
                        crate::i18n::tr(lang, crate::i18n::Key::RgbConnect),
                        None,
                    );
                    return;
                }

                self.draw_rgb_editor_content(ui, dark, &RgbModalLayout::responsive(ui.ctx()));
            });
        });
    }

    fn set_rgb_effect(&mut self, effect: u16) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = match self.rgb_settings.kind {
            RgbSupportKind::QmkRgblight => {
                hid.set_qmk_rgblight_effect(effect.min(u8::MAX as u16) as u8)
            }
            RgbSupportKind::VialRgb => hid.set_vialrgb_mode(
                effect,
                self.rgb_settings.speed,
                self.rgb_settings.hue,
                self.rgb_settings.saturation,
                self.rgb_settings.brightness,
            ),
            RgbSupportKind::None => return,
        };
        match result {
            Ok(()) => {
                self.rgb_settings.effect = effect;
                if effect != 0 {
                    self.rgb_settings.last_enabled_effect = effect;
                }
                self.autosave_rgb_settings();
            }
            Err(e) => {
                self.status_msg = format!("Failed to update RGB effect: {}", e);
                log::warn!("set_rgb_effect failed: {e}");
            }
        }
    }

    fn set_rgb_brightness(&mut self, brightness: u8) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = match self.rgb_settings.kind {
            RgbSupportKind::QmkRgblight => hid.set_qmk_rgblight_brightness(brightness),
            RgbSupportKind::VialRgb => hid.set_vialrgb_mode(
                self.rgb_settings.effect,
                self.rgb_settings.speed,
                self.rgb_settings.hue,
                self.rgb_settings.saturation,
                brightness,
            ),
            RgbSupportKind::None => return,
        };
        match result {
            Ok(()) => {
                self.rgb_settings.brightness = brightness;
                self.autosave_rgb_settings();
            }
            Err(e) => {
                self.status_msg = format!("Failed to update RGB brightness: {}", e);
                log::warn!("set_rgb_brightness failed: {e}");
            }
        }
    }

    fn set_rgb_color(&mut self, hue: u8, saturation: u8) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = match self.rgb_settings.kind {
            RgbSupportKind::QmkRgblight => hid.set_qmk_rgblight_color(hue, saturation),
            RgbSupportKind::VialRgb => hid.set_vialrgb_mode(
                self.rgb_settings.effect,
                self.rgb_settings.speed,
                hue,
                saturation,
                self.rgb_settings.brightness,
            ),
            RgbSupportKind::None => return,
        };
        match result {
            Ok(()) => {
                self.rgb_settings.hue = hue;
                self.rgb_settings.saturation = saturation;
                self.autosave_rgb_settings();
            }
            Err(e) => {
                self.status_msg = format!("Failed to update RGB color: {}", e);
                log::warn!("set_rgb_color failed: {e}");
            }
        }
    }

    fn set_rgb_speed(&mut self, speed: u8) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        let result = match self.rgb_settings.kind {
            RgbSupportKind::QmkRgblight => hid.set_qmk_rgblight_effect_speed(speed),
            RgbSupportKind::VialRgb => hid.set_vialrgb_mode(
                self.rgb_settings.effect,
                speed,
                self.rgb_settings.hue,
                self.rgb_settings.saturation,
                self.rgb_settings.brightness,
            ),
            RgbSupportKind::None => return,
        };
        match result {
            Ok(()) => {
                self.rgb_settings.speed = speed;
                self.autosave_rgb_settings();
            }
            Err(e) => {
                self.status_msg = format!("Failed to update RGB speed: {}", e);
                log::warn!("set_rgb_speed failed: {e}");
            }
        }
    }

    fn autosave_rgb_settings(&mut self) {
        let Some(hid) = &self.hid_device else {
            return;
        };
        if let Err(e) = hid.save_rgb() {
            self.status_msg = format!("Failed to save RGB settings: {}", e);
            log::warn!("save_rgb failed: {e}");
        }
    }

    fn draw_rgb_editor_content(&mut self, ui: &mut egui::Ui, dark: bool, layout: &RgbModalLayout) {
        let options = rgb_effect_options(&self.rgb_settings);
        let mut enabled = self.rgb_settings.is_enabled();
        let mut selected_effect = self.rgb_settings.effect;
        let brightness_max = self.rgb_settings.max_brightness.max(1);
        let current_percent = ((self.rgb_settings.brightness as f32 / brightness_max as f32)
            * 100.0)
            .round()
            .clamp(0.0, 100.0);
        let mut brightness_percent = current_percent;
        let selected_effect_name = options
            .iter()
            .find(|(id, _)| *id == self.rgb_settings.effect)
            .map(|(_, name)| *name)
            .unwrap_or("Unknown");
        let speed_max = 255.0_f32;
        let mut speed_percent = ((self.rgb_settings.speed as f32 / speed_max) * 100.0)
            .round()
            .clamp(0.0, 100.0);
        let mut color_hsva = egui::ecolor::Hsva {
            h: self.rgb_settings.hue as f32 / 255.0,
            s: self.rgb_settings.saturation as f32 / 255.0,
            v: 1.0,
            a: 1.0,
        };

        crate::ui_style::modal_content(ui, layout.modal_layout(), |ui| {
            let content_width = layout.content_width;
            let scale = (layout.row_height / 54.0).clamp(1.0, 1.12);
            let rgb_value_width = 36.0 * scale;
            let rgb_slider_width = 160.0 * scale;
            let rgb_slider_size = egui::vec2(168.0 * scale, 24.0 * scale);
            let rgb_control_width = rgb_slider_size.x + rgb_value_width;
            let rgb_control_height = 32.0 * scale;
            let rgb_font_size = 12.5 * scale;

            crate::ui_style::settings_list_row(
                ui,
                content_width,
                layout.row_height,
                crate::i18n::tr_catalog(self.app_settings.language, "common.enable"),
                true,
                46.0 * scale,
                |ui| {
                    let enable_resp = crate::ui_style::settings_switch_sized(
                        ui,
                        &mut enabled,
                        egui::vec2(46.0 * scale, 24.0 * scale),
                    );
                    if enable_resp.changed() {
                        let next_effect = if enabled {
                            self.rgb_settings.effect_or_default()
                        } else {
                            if self.rgb_settings.effect != 0 {
                                self.rgb_settings.last_enabled_effect = self.rgb_settings.effect;
                            }
                            0
                        };
                        self.set_rgb_effect(next_effect);
                        selected_effect = self.rgb_settings.effect;
                    }
                },
            );

            crate::ui_style::settings_list_row(
                ui,
                content_width,
                layout.row_height,
                crate::i18n::tr_catalog(self.app_settings.language, "common.effect"),
                true,
                rgb_slider_size.x,
                |ui| {
                    let dropdown_id = ui.make_persistent_id("rgb_effect_dropdown");
                    let dropdown_resp = crate::ui_style::modern_dropdown_button_sized(
                        ui,
                        dropdown_id,
                        selected_effect_name,
                        ui.visuals().text_color(),
                        rgb_slider_size.x,
                        rgb_control_height,
                        rgb_font_size,
                    );

                    crate::ui_style::popup_below_widget(
                        ui,
                        dropdown_id,
                        &dropdown_resp,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(rgb_slider_size.x);
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                            egui::ScrollArea::vertical()
                                .id_salt("rgb_effect_dropdown_scroll")
                                .max_height(142.0 * scale)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for (id, label) in &options {
                                        let selected = *id == selected_effect;
                                        let (option_rect, option_resp) = ui.allocate_exact_size(
                                            Vec2::new(rgb_slider_size.x, 28.0 * scale),
                                            Sense::click(),
                                        );
                                        if option_resp.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
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
                                            *label,
                                            FontId::proportional(12.0 * scale),
                                            if selected {
                                                ui.visuals().text_color()
                                            } else {
                                                app_muted_text(dark)
                                            },
                                        );
                                        if option_resp.clicked() {
                                            selected_effect = *id;
                                            self.set_rgb_effect(selected_effect);
                                            egui::Popup::close_all(ui.ctx());
                                        }
                                    }
                                });
                        },
                    );
                },
            );

            let color_enabled = rgb_effect_supports_color(self.rgb_settings.kind, selected_effect);
            crate::ui_style::settings_list_row(
                ui,
                content_width,
                layout.color_row_height,
                crate::i18n::tr_catalog(self.app_settings.language, "common.color"),
                color_enabled,
                64.0 * scale,
                |ui| {
                    let popup_id = ui.make_persistent_id("rgb_color_popup");
                    let popup_hsva_id = popup_id.with("hsva");
                    let popup_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
                    let border = if dark {
                        Color32::from_gray(95)
                    } else {
                        Color32::from_gray(185)
                    };
                    let swatch_border = if color_enabled && popup_open {
                        app_accent()
                    } else {
                        border
                    };
                    let swatch_color: Color32 = color_hsva.into();
                    let swatch_sense = if color_enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    };
                    let (swatch_rect, swatch_resp) =
                        ui.allocate_exact_size(Vec2::new(64.0 * scale, 34.0 * scale), swatch_sense);
                    if color_enabled && swatch_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if color_enabled && swatch_resp.clicked() {
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(popup_hsva_id, color_hsva));
                        egui::Popup::toggle_id(ui.ctx(), popup_id);
                    }
                    ui.painter().rect(
                        swatch_rect,
                        9.0,
                        app_surface_fill(dark),
                        Stroke::new(1.0_f32, swatch_border),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().rect(
                        swatch_rect.shrink(5.0 * scale),
                        6.0,
                        if color_enabled {
                            swatch_color
                        } else {
                            swatch_color.gamma_multiply(0.45)
                        },
                        Stroke::new(1.0_f32, swatch_border.gamma_multiply(0.85)),
                        egui::StrokeKind::Inside,
                    );

                    if color_enabled {
                        let mut picked_hsva = ui
                            .ctx()
                            .data(|d| d.get_temp::<egui::ecolor::Hsva>(popup_hsva_id))
                            .unwrap_or(color_hsva);
                        crate::ui_style::popup_below_widget(
                            ui,
                            popup_id,
                            &swatch_resp,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.spacing_mut().slider_width = 136.0 * scale;
                                if compact_rgb_color_picker(ui, &mut picked_hsva) {
                                    let hue = (picked_hsva.h.rem_euclid(1.0) * 255.0)
                                        .round()
                                        .clamp(0.0, 255.0)
                                        as u8;
                                    let saturation = (picked_hsva.s.clamp(0.0, 1.0) * 255.0)
                                        .round()
                                        .clamp(0.0, 255.0)
                                        as u8;
                                    self.set_rgb_color(hue, saturation);
                                    color_hsva = picked_hsva;
                                    ui.ctx()
                                        .data_mut(|d| d.insert_temp(popup_hsva_id, picked_hsva));
                                }
                            },
                        );
                    }
                },
            );

            let speed_enabled = rgb_effect_supports_speed(self.rgb_settings.kind, selected_effect);
            let rgb_slider_fill: Color32 = Color32::from(color_hsva).gamma_multiply(0.5);
            crate::ui_style::settings_list_row(
                ui,
                content_width,
                layout.row_height,
                crate::i18n::tr_catalog(self.app_settings.language, "common.speed"),
                speed_enabled,
                rgb_control_width,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let value_color = if speed_enabled {
                        if dark {
                            Color32::from_gray(230)
                        } else {
                            Color32::from_gray(55)
                        }
                    } else {
                        app_muted_text(dark)
                    };
                    ui.visuals_mut().selection.bg_fill = if speed_enabled {
                        rgb_slider_fill
                    } else {
                        rgb_slider_fill.gamma_multiply(0.5)
                    };
                    ui.visuals_mut().widgets.active.bg_fill = rgb_slider_fill;
                    ui.visuals_mut().widgets.active.weak_bg_fill = rgb_slider_fill;
                    ui.visuals_mut().widgets.hovered.bg_stroke =
                        Stroke::new(1.0_f32, rgb_slider_fill);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized(
                            [rgb_value_width, layout.row_height],
                            egui::Label::new(
                                RichText::new(format!("{}%", speed_percent as u8))
                                    .size(12.0 * scale)
                                    .color(value_color),
                            )
                            .halign(egui::Align::RIGHT),
                        );
                        ui.add_enabled_ui(speed_enabled, |ui| {
                            ui.spacing_mut().slider_width = rgb_slider_width;
                            let slider = egui::Slider::new(&mut speed_percent, 0.0..=100.0)
                                .step_by(1.0)
                                .show_value(false)
                                .trailing_fill(true);
                            let resp = ui.add_sized(rgb_slider_size, slider);
                            if resp.changed() {
                                let raw_value = ((speed_percent / 100.0) * speed_max)
                                    .round()
                                    .clamp(0.0, speed_max)
                                    as u8;
                                self.set_rgb_speed(raw_value);
                            }
                        });
                    });
                },
            );

            let brightness_enabled = enabled;
            crate::ui_style::settings_list_row(
                ui,
                content_width,
                layout.row_height,
                crate::i18n::tr_catalog(self.app_settings.language, "common.brightness"),
                brightness_enabled,
                rgb_control_width,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let value_color = if brightness_enabled {
                        if dark {
                            Color32::from_gray(230)
                        } else {
                            Color32::from_gray(55)
                        }
                    } else {
                        app_muted_text(dark)
                    };
                    ui.visuals_mut().selection.bg_fill = if brightness_enabled {
                        rgb_slider_fill
                    } else {
                        rgb_slider_fill.gamma_multiply(0.5)
                    };
                    ui.visuals_mut().widgets.active.bg_fill = rgb_slider_fill;
                    ui.visuals_mut().widgets.active.weak_bg_fill = rgb_slider_fill;
                    ui.visuals_mut().widgets.hovered.bg_stroke =
                        Stroke::new(1.0_f32, rgb_slider_fill);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized(
                            [rgb_value_width, layout.row_height],
                            egui::Label::new(
                                RichText::new(format!("{}%", brightness_percent as u8))
                                    .size(12.0 * scale)
                                    .color(value_color),
                            )
                            .halign(egui::Align::RIGHT),
                        );
                        ui.add_enabled_ui(brightness_enabled, |ui| {
                            ui.spacing_mut().slider_width = rgb_slider_width;
                            let slider = egui::Slider::new(&mut brightness_percent, 0.0..=100.0)
                                .step_by(1.0)
                                .show_value(false)
                                .trailing_fill(true);
                            let resp = ui.add_sized(rgb_slider_size, slider);
                            if resp.changed() {
                                let raw_value = ((brightness_percent / 100.0)
                                    * brightness_max as f32)
                                    .round()
                                    .clamp(0.0, brightness_max as f32)
                                    as u8;
                                self.set_rgb_brightness(raw_value);
                            }
                        });
                    });
                },
            );
        });
    }
}
