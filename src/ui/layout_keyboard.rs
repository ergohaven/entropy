use super::*;

impl EntropyApp {
    pub(super) fn draw_layout_keyboard_canvas(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
        avail: Vec2,
        offset_x: f32,
        offset_y: f32,
        unit: f32,
        padding: f32,
        layout_h: f32,
    ) {
        // Pass 1: allocate
        let key_rects: Vec<(usize, egui::Rect)> = layout
            .keys
            .iter()
            .enumerate()
            .filter_map(|(ki, key)| {
                if !Self::layout_condition_visible(
                    layout,
                    key.layout_condition,
                    self.layout_options_value,
                ) {
                    return None;
                }
                let geometry = LayoutGeometry {
                    offset_x,
                    offset_y,
                    unit,
                    padding,
                    layout_h,
                };
                let rect = layout_physical_key_rect(key, geometry);
                Some((ki, rect))
            })
            .collect();
        let encoder_rects: Vec<(usize, egui::Rect)> = layout
            .encoders
            .iter()
            .enumerate()
            .filter_map(|(ei, encoder)| {
                if !Self::layout_condition_visible(
                    layout,
                    encoder.layout_condition,
                    self.layout_options_value,
                ) {
                    return None;
                }
                let geometry = LayoutGeometry {
                    offset_x,
                    offset_y,
                    unit,
                    padding,
                    layout_h,
                };
                let rect = layout_physical_encoder_rect(encoder, geometry);
                Some((ei, rect))
            })
            .collect();
        let keyboard_target_rect = key_rects
            .iter()
            .map(|(_, rect)| *rect)
            .chain(encoder_rects.iter().map(|(_, rect)| *rect))
            .reduce(|acc, rect| acc.union(rect));
        if let Some(rect) = keyboard_target_rect {
            self.register_tour_target(TourTarget::KeyboardArea, rect.expand(10.0));
        }
        self.register_tour_target(
            TourTarget::BottomHints,
            egui::Rect::from_center_size(
                egui::pos2(ui.max_rect().center().x, ui.max_rect().bottom() - 34.0),
                Vec2::new(ui.max_rect().width().min(560.0), 52.0),
            ),
        );
        let mut encoder_groups: Vec<(u8, egui::Rect, Option<(usize, u16)>, Option<(usize, u16)>)> =
            Vec::new();
        for (ei, rect) in &encoder_rects {
            let encoder = &layout.encoders[*ei];
            if !self
                .encoder_visibility
                .get(encoder.encoder_idx as usize)
                .copied()
                .unwrap_or(true)
            {
                continue;
            }
            let kc = layout.get_encoder_keycode(self.selected_layer, *ei);
            if let Some((_, group_rect, ccw, cw)) = encoder_groups
                .iter_mut()
                .find(|(idx, _, _, _)| *idx == encoder.encoder_idx)
            {
                *group_rect = group_rect.union(*rect);
                if encoder.direction == 0 {
                    *ccw = Some((*ei, kc));
                } else {
                    *cw = Some((*ei, kc));
                }
            } else {
                encoder_groups.push((
                    encoder.encoder_idx,
                    *rect,
                    if encoder.direction == 0 {
                        Some((*ei, kc))
                    } else {
                        None
                    },
                    if encoder.direction == 0 {
                        None
                    } else {
                        Some((*ei, kc))
                    },
                ));
            }
        }
        let mut encoder_press_rects: Vec<(usize, egui::Rect)> = Vec::new();
        for (_, group_rect, _, _) in &encoder_groups {
            let center = group_rect.center();
            let radius = group_rect.width().min(group_rect.height()) * 0.5;
            let mut best_key: Option<(usize, f32)> = None;
            for (ki, key_rect) in &key_rects {
                if encoder_press_rects
                    .iter()
                    .any(|(assigned_ki, _)| assigned_ki == ki)
                {
                    continue;
                }
                let dist = key_rect.center().distance(center);
                if dist > radius * 0.38 {
                    continue;
                }
                match best_key {
                    Some((_, best_dist)) if dist >= best_dist => {}
                    _ => best_key = Some((*ki, dist)),
                }
            }
            if let Some((ki, _)) = best_key {
                let press_rect = egui::Rect::from_center_size(
                    center,
                    Vec2::new(
                        (radius * 0.88).min(group_rect.width() * 0.44),
                        (radius * 0.48).min(group_rect.height() * 0.22),
                    ),
                );
                encoder_press_rects.push((ki, press_rect));
            }
        }
        let mut rects: Vec<(usize, egui::Rect, egui::Response)> =
            Vec::with_capacity(layout.keys.len());
        for (ki, rect) in &key_rects {
            let response_rect = encoder_press_rects
                .iter()
                .find(|(press_ki, _)| press_ki == ki)
                .map(|(_, press_rect)| *press_rect)
                .unwrap_or(*rect);
            let response = ui.allocate_rect(response_rect, Sense::click());
            rects.push((*ki, response_rect, response));
        }

        // Reset hover_layer each frame — will be set again if a layer key is hovered
        let prev_hover = self.hover_layer;
        self.hover_layer = None;

        // Pass 2: hover + clicks + tooltips
        let mut hovered_key: Option<usize> = None;
        for (ki, _, response) in &mut rects {
            if response.hovered() {
                hovered_key = Some(*ki);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.selected_key = Some((self.selected_layer, *ki));
                self.keycode_picker.open = true;
                self.keycode_picker.result = None;
                self.keycode_picker.search_query.clear();
                self.keycode_picker.layer_names = self.layer_names.clone();
                self.keycode_picker.vial_quantum_pending_mod = None;
                self.keycode_picker.vial_quantum_pending_mt = None;
                self.keycode_picker.vial_layer_pending = None;
                // Reset all editor states so picker opens normally
                self.keycode_picker.tap_dance_editor_open = None;
                self.keycode_picker.selected_tab = crate::keycode_picker::KeycodeTab::Basic;
            }

            // Right-click actions: layer jump/retarget, modifier side swap, editors/settings.
            if response.secondary_clicked() {
                let ctrl_held = ui.input(|i| i.modifiers.ctrl);
                let kc = layout.get_keycode(self.selected_layer, *ki);
                self.handle_secondary_target(ctrl_held, kc, Some(*ki), None);
                if self.secondary_click_handled {
                    continue;
                }
            }

            // Tooltip — for layer keys show mini layout preview
            let kc = layout.get_keycode(self.selected_layer, *ki);
            // MO/TG/TO/OSL/TT/DF range and LT; OSM also lives in 0x52xx
            // but is deliberately excluded by vial_layer_target().
            let preview_layer: Option<usize> = vial_layer_target(kc);

            if let Some(preview_layer_idx) = preview_layer {
                if response.hovered() {
                    hovered_key = Some(*ki); // keep hovered_key for layer keys too
                    if self.app_settings.layer_hover_preview {
                        self.hover_layer = Some(preview_layer_idx);
                    } else {
                        let tip = keycode_tooltip_with_macro_names(
                            kc,
                            &layout.custom_keycodes,
                            &self.layer_names,
                            &self.keycode_picker.macro_names,
                            &self.keycode_picker.tap_dance_names,
                        );
                        *response = response
                            .clone()
                            .on_hover_text(crate::i18n::tr_text(self.app_settings.language, &tip));
                    }
                }
                if response.secondary_clicked() && preview_layer_idx != self.selected_layer {
                    // Right-click: jump to that layer
                    self.jump_back_stack.push(self.selected_layer);
                    self.selected_layer = preview_layer_idx;
                    self.hover_layer = None;
                    self.secondary_click_handled = true;
                }
            } else if response.hovered() {
                let tip = keycode_tooltip_with_macro_names(
                    kc,
                    &layout.custom_keycodes,
                    &self.layer_names,
                    &self.keycode_picker.macro_names,
                    &self.keycode_picker.tap_dance_names,
                );
                *response = response
                    .clone()
                    .on_hover_text(crate::i18n::tr_text(self.app_settings.language, &tip));
            }
        }

        // Animate hover_layer_progress
        let target_progress = if self.hover_layer.is_some() {
            1.0f32
        } else {
            0.0f32
        };
        let speed = 4.0f32;
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.hover_layer_progress +=
            (target_progress - self.hover_layer_progress) * (speed * dt).min(1.0);
        if (self.hover_layer_progress - target_progress).abs() > 0.01 {
            ctx.request_repaint();
        }

        // Pass 3: paint
        let painter = ui.painter();
        let mut hovered_encoder = false;
        let mut hovered_encoder_keycode = None;
        let hover_target = self
            .hover_layer
            .unwrap_or(prev_hover.unwrap_or(self.selected_layer));
        let hover_alpha = self.hover_layer_progress;
        let dark = self.dark_mode;
        // Use hover layer for logic (TRNS resolution etc) when mostly visible
        let layer = if hover_alpha > 0.5 {
            hover_target
        } else {
            self.selected_layer
        };
        let layer_led_color_idx = if self.layer_led_settings.supported {
            self.layer_led_settings
                .layer_colors
                .get(layer)
                .map(|color| color.value)
                .filter(|color_idx| !matches!(color_idx, 0 | 1))
        } else {
            None
        };
        // Off and White should keep the standard neutral outline/fill so disabled/uncolored
        // layers do not look artificially tinted.
        let layer_led_outline = layer_led_color_idx.map(layer_led_outline_color);
        let layer_led_hover_fill =
            layer_led_color_idx.map(|color_idx| layer_led_hover_fill(color_idx, dark));
        let combo_key_colors = combo_key_colors_for_layer(
            layout,
            layer,
            self.combo_entries.as_slice(),
            self.combo_colors.as_slice(),
        );
        let emphasize_selected_combo = matches!(self.settings_tab, SettingsTab::Combo);
        for (ki, rect, _) in &rects {
            let key = &layout.keys[*ki];
            let is_selected = self.selected_key == Some((layer, *ki));
            let is_hovered = hovered_key == Some(*ki);
            // Accent: #5B68DF indigo
            let bg = if is_selected {
                app_accent()
            } else if is_hovered {
                layer_led_hover_fill.unwrap_or_else(|| crate::ui_style::hover_fill(dark))
            } else {
                if dark {
                    Color32::from_rgb(48, 48, 52)
                } else {
                    Color32::from_rgb(255, 255, 255)
                }
            };

            let press_rect_override = encoder_press_rects
                .iter()
                .find(|(press_ki, _)| *press_ki == *ki)
                .map(|(_, press_rect)| *press_rect);
            let draw_rect = press_rect_override.unwrap_or(*rect);

            let is_hovering = hover_alpha > 0.05;

            if press_rect_override.is_some() {
                continue;
            }

            let kc = layout.get_keycode(layer, *ki);

            if kc == 0x0001 {
                paint_layout_keycap(
                    painter,
                    draw_rect,
                    key.rotation,
                    bg,
                    Stroke::new(
                        1.0,
                        layer_led_outline.unwrap_or_else(|| {
                            if dark {
                                Color32::from_rgb(54, 54, 58)
                            } else {
                                Color32::from_rgb(230, 230, 233)
                            }
                        }),
                    ),
                );
                if !is_hovering {
                    let fallback_kc = (0..layer)
                        .rev()
                        .map(|l| layout.get_keycode(l, *ki))
                        .find(|&k| k != 0x0001)
                        .unwrap_or(0x0000);
                    let label = if fallback_kc == 0x0000 || fallback_kc == 0x0001 {
                        String::new()
                    } else {
                        keycode_label_with_macro_names(
                            fallback_kc,
                            &layout.custom_keycodes,
                            &self.layer_names,
                            &self.keycode_picker.macro_names,
                            &self.keycode_picker.tap_dance_names,
                            self.app_settings.key_legend_layout,
                        )
                    };
                    let label = number_row_shifted_label(
                        label,
                        self.app_settings.show_shifted_number_symbols,
                        self.app_settings.key_legend_layout,
                    );
                    draw_key_label_dimmed(
                        &painter,
                        draw_rect,
                        &label,
                        dark,
                        key.rotation.to_radians(),
                    );
                }
            } else if kc == 0x0000 {
                let border = layer_led_outline.unwrap_or_else(|| {
                    if dark {
                        Color32::from_rgb(54, 54, 58)
                    } else {
                        Color32::from_rgb(230, 230, 233)
                    }
                });
                paint_layout_keycap(
                    painter,
                    draw_rect,
                    key.rotation,
                    bg,
                    Stroke::new(1.0, border),
                );
            } else {
                let border = layer_led_outline.unwrap_or_else(|| {
                    if dark {
                        Color32::from_rgb(54, 54, 58)
                    } else {
                        Color32::from_rgb(230, 230, 233)
                    }
                });
                paint_layout_keycap(
                    painter,
                    draw_rect,
                    key.rotation,
                    bg,
                    Stroke::new(1.0, border),
                );
                let label = number_row_shifted_label(
                    keycode_label_with_macro_names(
                        kc,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                        self.app_settings.key_legend_layout,
                    ),
                    self.app_settings.show_shifted_number_symbols,
                    self.app_settings.key_legend_layout,
                );
                draw_key_label(&painter, draw_rect, &label, dark, key.rotation.to_radians());
            }

            if let Some(colors) = combo_key_colors.get(*ki) {
                if !colors.is_empty() {
                    if emphasize_selected_combo {
                        if let Some((_, selected_color)) = colors
                            .iter()
                            .find(|(combo_idx, _)| *combo_idx == self.selected_combo)
                        {
                            paint_layout_keycap(
                                painter,
                                draw_rect.shrink(1.5),
                                key.rotation,
                                Color32::TRANSPARENT,
                                Stroke::new(2.0, selected_color.gamma_multiply(0.95)),
                            );
                        }
                    }
                    paint_combo_color_markers(painter, draw_rect, colors, dark);
                }
            }
        }

        let encoder_custom_keycodes = layout.custom_keycodes.clone();
        let encoder_layer_names = self.layer_names.clone();
        let encoder_macro_names = self.keycode_picker.macro_names.clone();
        let encoder_tap_dance_names = self.keycode_picker.tap_dance_names.clone();
        let encoder_key_legend_layout = self.app_settings.key_legend_layout;
        let encoder_value_label = |kc: u16| -> String {
            keycode_label_with_macro_names(
                kc,
                &encoder_custom_keycodes,
                &encoder_layer_names,
                &encoder_macro_names,
                &encoder_tap_dance_names,
                encoder_key_legend_layout,
            )
            .replace('\n', " ")
        };
        let encoder_label = |layer_idx: usize, visual_idx: usize, kc: u16| -> (String, bool) {
            match kc {
                0x0000 => (String::new(), false),
                0x0001 => {
                    let fallback = (0..layer_idx)
                        .rev()
                        .map(|fallback_layer| {
                            layout.get_encoder_keycode(fallback_layer, visual_idx)
                        })
                        .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001));
                    match fallback {
                        Some(fallback_kc) => (encoder_value_label(fallback_kc), true),
                        None => ("▽".to_string(), false),
                    }
                }
                value => (encoder_value_label(value), false),
            }
        };
        let encoder_dim_text_color = if dark {
            Color32::from_rgb(62, 56, 56)
        } else {
            Color32::from_rgb(200, 200, 208)
        };

        let draw_encoder_arrow = |painter: &egui::Painter,
                                  center: egui::Pos2,
                                  encoder_radius: f32,
                                  top: bool,
                                  color: Color32| {
            let (start_deg, end_deg) = if top {
                (240.0_f32, 300.0_f32)
            } else {
                (120.0_f32, 60.0_f32)
            };
            let r = encoder_radius * 1.22;
            let mut points = Vec::new();
            for step in 0..=12 {
                let t = step as f32 / 12.0;
                let deg = start_deg + (end_deg - start_deg) * t;
                let rad = deg.to_radians();
                points.push(egui::pos2(
                    center.x + rad.cos() * r,
                    center.y + rad.sin() * r,
                ));
            }
            painter.add(egui::Shape::line(points.clone(), Stroke::new(1.7, color)));
            if points.len() >= 2 {
                let end = points[points.len() - 1];
                let prev = points[points.len() - 2];
                let dir = egui::vec2(end.x - prev.x, end.y - prev.y).normalized();
                let left = egui::vec2(-dir.y, dir.x);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        end,
                        egui::pos2(
                            end.x - dir.x * 3.6 + left.x * 2.4,
                            end.y - dir.y * 3.6 + left.y * 2.4,
                        ),
                        egui::pos2(
                            end.x - dir.x * 3.6 - left.x * 2.4,
                            end.y - dir.y * 3.6 - left.y * 2.4,
                        ),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        };

        const ENCODER_HOVER_SCALE: f32 = 1.5;
        let encoder_hover_enlarge = self.app_settings.encoder_hover_enlarge;
        for (_encoder_idx, rect, ccw, cw) in &encoder_groups {
            let center = rect.center();
            let base_radius = rect.width().min(rect.height()) * LAYOUT_ENCODER_RADIUS_FACTOR;
            let hover_radius = base_radius * ENCODER_HOVER_SCALE;
            let interactive_radius = if encoder_hover_enlarge {
                hover_radius
            } else {
                base_radius
            };
            let circle_bounds = egui::Rect::from_center_size(
                center,
                egui::vec2(interactive_radius * 2.0, interactive_radius * 2.0),
            );
            let press_slot = encoder_press_rects
                .iter()
                .find(|(_, press_rect)| press_rect.center().distance(center) < 1.0)
                .map(|(press_ki, press_rect)| (*press_ki, *press_rect));
            let (top_rect, middle_rect, bottom_rect) = if let Some((_, press_rect)) = press_slot {
                let divider_gap = base_radius * 0.06;
                let top_divider_y = press_rect.top() - divider_gap;
                let bottom_divider_y = press_rect.bottom() + divider_gap;
                (
                    egui::Rect::from_min_max(
                        circle_bounds.min,
                        egui::pos2(circle_bounds.max.x, top_divider_y),
                    ),
                    Some(egui::Rect::from_min_max(
                        egui::pos2(circle_bounds.min.x, top_divider_y),
                        egui::pos2(circle_bounds.max.x, bottom_divider_y),
                    )),
                    egui::Rect::from_min_max(
                        egui::pos2(circle_bounds.min.x, bottom_divider_y),
                        circle_bounds.max,
                    ),
                )
            } else {
                (
                    egui::Rect::from_min_max(
                        circle_bounds.min,
                        egui::pos2(circle_bounds.max.x, center.y),
                    ),
                    None,
                    egui::Rect::from_min_max(
                        egui::pos2(circle_bounds.min.x, center.y),
                        circle_bounds.max,
                    ),
                )
            };
            let top_resp = ui.allocate_rect(top_rect, Sense::click());
            let middle_resp =
                middle_rect.map(|middle_rect| ui.allocate_rect(middle_rect, Sense::click()));
            let bottom_resp = ui.allocate_rect(bottom_rect, Sense::click());
            let encoder_hovered = top_resp.hovered()
                || middle_resp.as_ref().map(|r| r.hovered()).unwrap_or(false)
                || bottom_resp.hovered();
            let radius = if encoder_hovered && encoder_hover_enlarge {
                hover_radius
            } else {
                base_radius
            };
            let font_scale = if encoder_hovered && encoder_hover_enlarge {
                ENCODER_HOVER_SCALE
            } else {
                1.0
            };
            if encoder_hovered {
                hovered_encoder = true;
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let ctrl_held = ui.input(|i| i.modifiers.ctrl);
            if top_resp.hovered() {
                if let Some((_, kc)) = cw {
                    hovered_encoder_keycode = Some(*kc);
                    let tip = keycode_tooltip_with_macro_names(
                        *kc,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                    );
                    let _ = top_resp
                        .clone()
                        .on_hover_text(crate::i18n::tr_text(self.app_settings.language, &tip));
                }
            }
            if top_resp.secondary_clicked() {
                if let Some((visual_idx, kc)) = cw {
                    self.handle_secondary_target(ctrl_held, *kc, None, Some(*visual_idx));
                }
            }
            if top_resp.clicked() {
                if let Some((visual_idx, _)) = cw {
                    self.selected_key = None;
                    self.selected_encoder = Some((layer, *visual_idx));
                    self.keycode_picker.open = true;
                    self.keycode_picker.result = None;
                }
            }
            if let (Some((press_ki, _)), Some(middle_resp)) = (press_slot, middle_resp.as_ref()) {
                if middle_resp.hovered() {
                    hovered_key = Some(press_ki);
                    let kc = layout.get_keycode(self.selected_layer, press_ki);
                    hovered_encoder_keycode = Some(kc);
                    let tip = keycode_tooltip_with_macro_names(
                        kc,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                    );
                    let _ = middle_resp
                        .clone()
                        .on_hover_text(crate::i18n::tr_text(self.app_settings.language, &tip));
                }
                if middle_resp.secondary_clicked() {
                    let kc = layout.get_keycode(self.selected_layer, press_ki);
                    self.handle_secondary_target(ctrl_held, kc, Some(press_ki), None);
                }
                if middle_resp.clicked() {
                    self.open_picker_for_target(Some(press_ki), None);
                    self.selected_encoder = None;
                }
            }
            if bottom_resp.hovered() {
                if let Some((_, kc)) = ccw {
                    hovered_encoder_keycode = Some(*kc);
                    let tip = keycode_tooltip_with_macro_names(
                        *kc,
                        &layout.custom_keycodes,
                        &self.layer_names,
                        &self.keycode_picker.macro_names,
                        &self.keycode_picker.tap_dance_names,
                    );
                    let _ = bottom_resp
                        .clone()
                        .on_hover_text(crate::i18n::tr_text(self.app_settings.language, &tip));
                }
            }
            if bottom_resp.secondary_clicked() {
                if let Some((visual_idx, kc)) = ccw {
                    self.handle_secondary_target(ctrl_held, *kc, None, Some(*visual_idx));
                }
            }
            if bottom_resp.clicked() {
                if let Some((visual_idx, _)) = ccw {
                    self.selected_key = None;
                    self.selected_encoder = Some((self.selected_layer, *visual_idx));
                    self.keycode_picker.open = true;
                    self.keycode_picker.result = None;
                }
            }

            let top_selected = cw
                .map(|(visual_idx, _)| Some((layer, visual_idx)) == self.selected_encoder)
                .unwrap_or(false);
            let bottom_selected = ccw
                .map(|(visual_idx, _)| Some((layer, visual_idx)) == self.selected_encoder)
                .unwrap_or(false);
            let middle_selected = press_slot
                .map(|(press_ki, _)| self.selected_key == Some((layer, press_ki)))
                .unwrap_or(false);
            let middle_hovered = middle_resp.as_ref().map(|r| r.hovered()).unwrap_or(false);
            let visuals = &ui.visuals().widgets;
            let fill_radius = radius + LAYOUT_ENCODER_FILL_EXTRA;
            let top_fill = if top_selected {
                visuals.active.bg_fill
            } else if top_resp.hovered() {
                visuals.hovered.bg_fill
            } else {
                visuals.inactive.bg_fill
            };
            let bottom_fill = if bottom_selected {
                visuals.active.bg_fill
            } else if bottom_resp.hovered() {
                visuals.hovered.bg_fill
            } else {
                visuals.inactive.bg_fill
            };
            let middle_fill = if middle_selected {
                visuals.active.bg_fill
            } else if middle_hovered {
                visuals.hovered.bg_fill
            } else {
                visuals.inactive.bg_fill
            };
            let outline = if top_selected || bottom_selected || middle_selected {
                visuals.active.bg_stroke
            } else if top_resp.hovered() || middle_hovered || bottom_resp.hovered() {
                visuals.hovered.bg_stroke
            } else {
                visuals.inactive.bg_stroke
            };

            let painter = ui.painter();
            painter.circle_filled(center, fill_radius, visuals.inactive.bg_fill);
            painter
                .with_clip_rect(top_rect)
                .circle_filled(center, fill_radius, top_fill);
            if let Some(middle_rect) = middle_rect {
                painter
                    .with_clip_rect(middle_rect)
                    .circle_filled(center, fill_radius, middle_fill);
            }
            painter
                .with_clip_rect(bottom_rect)
                .circle_filled(center, fill_radius, bottom_fill);
            painter.circle_stroke(center, radius, outline);

            let has_press_button = encoder_press_rects
                .iter()
                .any(|(_, press_rect)| press_rect.center().distance(center) < 1.0);
            let (top_label, top_dimmed) = cw
                .map(|(visual_idx, kc)| encoder_label(layer, visual_idx, kc))
                .unwrap_or_else(|| (String::new(), false));
            let (bottom_label, bottom_dimmed) = ccw
                .map(|(visual_idx, kc)| encoder_label(layer, visual_idx, kc))
                .unwrap_or_else(|| (String::new(), false));
            let top_font = if has_press_button {
                egui::FontId::proportional(
                    if top_label.chars().count() > 9 {
                        6.6
                    } else {
                        7.4
                    } * font_scale,
                )
            } else {
                egui::FontId::proportional(
                    if top_label.chars().count() > 9 {
                        8.5
                    } else {
                        9.5
                    } * font_scale,
                )
            };
            let bottom_font = if has_press_button {
                egui::FontId::proportional(
                    if bottom_label.chars().count() > 9 {
                        6.6
                    } else {
                        7.4
                    } * font_scale,
                )
            } else {
                egui::FontId::proportional(
                    if bottom_label.chars().count() > 9 {
                        8.5
                    } else {
                        9.5
                    } * font_scale,
                )
            };
            let top_label_y = center.y - radius * if has_press_button { 0.52 } else { 0.30 };
            let bottom_label_y = center.y + radius * if has_press_button { 0.52 } else { 0.30 };
            let top_text_color = if top_dimmed {
                encoder_dim_text_color
            } else if top_selected {
                visuals.active.fg_stroke.color
            } else if top_resp.hovered() {
                visuals.hovered.fg_stroke.color
            } else {
                visuals.inactive.fg_stroke.color
            };
            let bottom_text_color = if bottom_dimmed {
                encoder_dim_text_color
            } else if bottom_selected {
                visuals.active.fg_stroke.color
            } else if bottom_resp.hovered() {
                visuals.hovered.fg_stroke.color
            } else {
                visuals.inactive.fg_stroke.color
            };
            painter.text(
                egui::pos2(center.x, top_label_y),
                egui::Align2::CENTER_CENTER,
                top_label,
                top_font,
                top_text_color,
            );
            painter.text(
                egui::pos2(center.x, bottom_label_y),
                egui::Align2::CENTER_CENTER,
                bottom_label,
                bottom_font,
                bottom_text_color,
            );

            let arrow_color_top = outline.color;
            let arrow_color_bottom = outline.color;
            draw_encoder_arrow(painter, center, radius, true, arrow_color_top);
            draw_encoder_arrow(painter, center, radius, false, arrow_color_bottom);

            if let (Some((press_ki, _)), Some(middle_rect)) = (press_slot, middle_rect) {
                let top_divider_y = middle_rect.top();
                let bottom_divider_y = middle_rect.bottom();
                let divider_radius = (radius - 0.5).max(0.0);
                let top_divider_half_width = (divider_radius * divider_radius
                    - (top_divider_y - center.y) * (top_divider_y - center.y))
                    .max(0.0)
                    .sqrt();
                let bottom_divider_half_width = (divider_radius * divider_radius
                    - (bottom_divider_y - center.y) * (bottom_divider_y - center.y))
                    .max(0.0)
                    .sqrt();
                painter.line_segment(
                    [
                        egui::pos2(center.x - top_divider_half_width, top_divider_y),
                        egui::pos2(center.x + top_divider_half_width, top_divider_y),
                    ],
                    Stroke::new(1.0, outline.color),
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - bottom_divider_half_width, bottom_divider_y),
                        egui::pos2(center.x + bottom_divider_half_width, bottom_divider_y),
                    ],
                    Stroke::new(1.0, outline.color),
                );
                let press_text_rect = middle_rect.shrink2(egui::vec2(4.0, 2.0));

                let (press_label, press_dimmed) = {
                    let kc = layout.get_keycode(layer, press_ki);
                    if kc == 0x0001 {
                        let fallback_kc = (0..layer)
                            .rev()
                            .map(|l| layout.get_keycode(l, press_ki))
                            .find(|&k| !matches!(k, 0x0000 | 0x0001))
                            .unwrap_or(0x0000);
                        if fallback_kc == 0x0000 {
                            ("▽".to_string(), false)
                        } else {
                            (
                                keycode_label_with_macro_names(
                                    fallback_kc,
                                    &layout.custom_keycodes,
                                    &self.layer_names,
                                    &self.keycode_picker.macro_names,
                                    &self.keycode_picker.tap_dance_names,
                                    self.app_settings.key_legend_layout,
                                ),
                                true,
                            )
                        }
                    } else if kc == 0x0000 {
                        (String::new(), false)
                    } else {
                        (
                            keycode_label_with_macro_names(
                                kc,
                                &layout.custom_keycodes,
                                &self.layer_names,
                                &self.keycode_picker.macro_names,
                                &self.keycode_picker.tap_dance_names,
                                self.app_settings.key_legend_layout,
                            ),
                            false,
                        )
                    }
                };
                let press_label = press_label.replace('\n', " ");
                let text_color = if press_dimmed {
                    encoder_dim_text_color
                } else if middle_selected {
                    visuals.active.fg_stroke.color
                } else if middle_hovered {
                    visuals.hovered.fg_stroke.color
                } else {
                    visuals.inactive.fg_stroke.color
                };
                let press_font = FontId::proportional(
                    if press_label.chars().count() > 8 {
                        7.2
                    } else {
                        8.2
                    } * font_scale,
                );
                painter.with_clip_rect(press_text_rect).text(
                    press_text_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    press_label,
                    press_font,
                    text_color,
                );
            } else {
                let divider_half_width = (radius - 0.5).max(0.0);
                painter.line_segment(
                    [
                        egui::pos2(center.x - divider_half_width, center.y),
                        egui::pos2(center.x + divider_half_width, center.y),
                    ],
                    Stroke::new(1.0, outline.color),
                );
            }
        }

        self.prev_hovered_key = hovered_key;
        self.prev_hovered_encoder = hovered_encoder;
        self.prev_hovered_encoder_keycode = hovered_encoder_keycode;

        if layout_h > avail.y {
            ui.allocate_space(Vec2::new(0.0, (layout_h - avail.y).max(0.0)));
        }
    }
}

fn combo_key_colors_for_layer(
    layout: &KeyboardLayout,
    layer: usize,
    combos: &[ComboEntry],
    colors: &[u32],
) -> Vec<Vec<(usize, Color32)>> {
    let mut key_colors = vec![Vec::new(); layout.keys.len()];
    for (combo_idx, combo) in combos.iter().enumerate() {
        let triggers: Vec<u16> = combo.keys.iter().copied().filter(|&key| key != 0).collect();
        if triggers.is_empty() || combo.output == 0 {
            continue;
        }
        let color = combo_color32(
            colors
                .get(combo_idx)
                .copied()
                .unwrap_or_else(|| combo_default_color(combo_idx)),
        );
        for (key_idx, slot) in key_colors.iter_mut().enumerate() {
            let keycode = layout_combo_match_keycode(layout, layer, key_idx);
            if keycode != 0 && triggers.contains(&keycode) {
                slot.push((combo_idx, color));
            }
        }
    }
    key_colors
}

fn layout_combo_match_keycode(layout: &KeyboardLayout, layer: usize, key_idx: usize) -> u16 {
    let keycode = layout.get_keycode(layer, key_idx);
    if keycode != 0x0001 {
        return keycode;
    }

    (0..layer)
        .rev()
        .map(|fallback_layer| layout.get_keycode(fallback_layer, key_idx))
        .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001))
        .unwrap_or(0x0000)
}

fn paint_combo_color_markers(
    painter: &egui::Painter,
    rect: egui::Rect,
    colors: &[(usize, Color32)],
    dark: bool,
) {
    let marker_count = colors.len().min(4);
    if marker_count == 0 {
        return;
    }

    let radius = (rect.width().min(rect.height()) * 0.06).clamp(2.2, 4.0);
    let gap = radius * 0.85;
    let total_width = marker_count as f32 * radius * 2.0 + (marker_count - 1) as f32 * gap;
    let mut x = rect.center().x - total_width * 0.5 + radius;
    let y = rect.top() + radius + rect.height() * 0.08;
    let outline = if dark {
        Color32::from_rgb(38, 38, 40)
    } else {
        Color32::from_rgb(255, 255, 255)
    };

    for (_, color) in colors.iter().take(marker_count) {
        let center = egui::pos2(x, y);
        painter.circle_filled(center, radius + 1.0, outline);
        painter.circle_filled(center, radius, color.gamma_multiply(0.9));
        x += radius * 2.0 + gap;
    }
}
