use super::*;

const STICKY_LAYOUT_KEYBOARD_MARGIN: f32 = 1.0_f32;
const STICKY_LAYOUT_BASE_KEY_H: f32 = 44.0_f32;
const STICKY_LAYOUT_BASE_ENCODER_R: f32 = 24.0_f32;

fn sticky_layout_should_draw_key(
    visibility_mode: StickyLayoutVisibilityMode,
    is_pressed: bool,
) -> bool {
    matches!(
        visibility_mode,
        StickyLayoutVisibilityMode::LayoutAndPresses
    ) || is_pressed
}

type EncoderGroup = (u8, egui::Rect, Option<(usize, u16)>, Option<(usize, u16)>);

fn sticky_rect_text_scale(rect: egui::Rect) -> f32 {
    (rect.width().min(rect.height()) / STICKY_LAYOUT_BASE_KEY_H).clamp(0.52, 2.4)
}

fn sticky_label_fit_scale(label: &str, font_size: f32, available: f32) -> f32 {
    let longest = label
        .split(['\n', '/'])
        .map(|part| part.trim().chars().count())
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    (available.max(4.0) / (longest * font_size.max(1.0) * 0.58)).clamp(0.48, 1.0)
}

fn sticky_encoder_text_scale(radius: f32) -> f32 {
    (radius / STICKY_LAYOUT_BASE_ENCODER_R).clamp(0.56, 2.2)
}

fn sticky_encoder_font_size(
    label: &str,
    radius: f32,
    short_label_size: f32,
    long_label_size: f32,
) -> f32 {
    let base = if label.chars().count() > 9 {
        long_label_size
    } else {
        short_label_size
    };
    let scaled = base * sticky_encoder_text_scale(radius);
    scaled * sticky_label_fit_scale(label, scaled, radius * 1.45)
}

fn sticky_press_font_size(label: &str, rect: egui::Rect) -> f32 {
    let base = if label.chars().count() > 8 { 7.2 } else { 8.2 };
    let scaled = base * (rect.height() / 18.0).clamp(0.56, 2.2);
    scaled * sticky_label_fit_scale(label, scaled, rect.width() - 6.0)
}

fn draw_sticky_encoder_arrow(
    painter: &egui::Painter,
    center: egui::Pos2,
    encoder_radius: f32,
    top: bool,
    color: Color32,
) {
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
    painter.add(egui::Shape::line(points.clone(), Stroke::new(1.7_f32, color)));
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
}

fn sticky_compact_label(label: &str, max_chars: usize) -> String {
    let label = label.trim();
    let count = label.chars().count();
    if count <= max_chars {
        return label.to_string();
    }
    let keep = max_chars.saturating_sub(1).max(1);
    format!("{}…", label.chars().take(keep).collect::<String>())
}

fn sticky_key_label_sizes(label: &str, rect: egui::Rect) -> (Option<f32>, f32) {
    let (top, bottom) = key_label_font_sizes(label);
    let rect_scale = sticky_rect_text_scale(rect);
    let base = bottom.max(top.unwrap_or(bottom)) * rect_scale;
    let fit_scale = sticky_label_fit_scale(label, base, rect.width() - 8.0);
    let scale = rect_scale * fit_scale;
    (top.map(|size| size * scale), bottom * scale)
}

fn draw_sticky_key_label(
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    dark: bool,
    rotation: f32,
    dimmed: bool,
) {
    let rect_scale = sticky_rect_text_scale(rect);
    let clip_rect = rect.shrink2(egui::vec2(4.0, 3.0) * rect_scale);
    let (top, bottom) = if label.contains('\n') {
        let mut parts = label.splitn(2, '\n');
        let t = parts.next().unwrap_or("");
        let b = parts.next().unwrap_or(label);
        (Some(t), b)
    } else if let Some(pos) = label.find('/') {
        (Some(&label[..pos]), &label[pos + 1..])
    } else {
        (None, label)
    };
    let (top_size, bottom_size) = sticky_key_label_sizes(label, rect);
    let (top_color, bottom_color) = if dimmed {
        if dark {
            (Color32::from_rgb(45, 45, 50), Color32::from_rgb(62, 56, 56))
        } else {
            (
                Color32::from_rgb(215, 215, 220),
                Color32::from_rgb(200, 200, 208),
            )
        }
    } else if dark {
        (
            Color32::from_rgb(130, 130, 145),
            Color32::from_rgb(239, 233, 232),
        )
    } else {
        (
            Color32::from_rgb(130, 130, 150),
            Color32::from_rgb(26, 26, 30),
        )
    };

    let clipped = painter.with_clip_rect(clip_rect);
    let lines: Vec<&str> = label.split('\n').collect();
    if let [top_str, middle, bottom_str] = lines.as_slice() {
        let center = rect.center();
        let is_symbol_line = |line: &str| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && trimmed.chars().count() <= 3
                && trimmed
                    .chars()
                    .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
        };
        let top_font = top_size.unwrap_or(8.0);
        let middle_font = if is_symbol_line(middle) {
            10.0 * rect_scale
        } else {
            9.0 * rect_scale
        } * sticky_label_fit_scale(middle, 9.8 * rect_scale, rect.width() - 8.0);
        let bottom_font = bottom_size;
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -12.0 * rect_scale, rotation),
            top_str,
            FontId::proportional(top_font),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -rect_scale, rotation),
            middle,
            FontId::proportional(middle_font),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, 10.5 * rect_scale, rotation),
            bottom_str,
            FontId::proportional(bottom_font),
            bottom_color,
            rotation,
        );
        return;
    }

    if let Some(top_str) = top {
        let center = rect.center();
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, -7.0 * rect_scale, rotation),
            top_str,
            FontId::proportional(top_size.unwrap_or(9.0)),
            top_color,
            rotation,
        );
        paint_centered_text_rotated(
            &clipped,
            center + rotated_offset(0.0, 6.0 * rect_scale, rotation),
            bottom,
            FontId::proportional(bottom_size),
            bottom_color,
            rotation,
        );
    } else {
        let font_size = if bottom == "↵" {
            (16.0 * rect_scale).min(bottom_size + 4.0 * rect_scale)
        } else {
            bottom_size
        };
        paint_centered_text_rotated(
            &clipped,
            rect.center(),
            bottom,
            FontId::proportional(font_size),
            bottom_color,
            rotation,
        );
    }
}

fn preview_layout_geometry(
    ctx: &egui::Context,
    layout: &KeyboardLayout,
    viewport: egui::Rect,
    ui_scale: f32,
) -> LayoutGeometry {
    layout_geometry_with_reserved(
        ctx,
        layout,
        viewport,
        ui_scale,
        2.0,
        2.0,
        6.0,
        Some(f32::INFINITY),
    )
}

impl EntropyApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_sticky_layout_preview(
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        layer: usize,
        layer_names: &[String],
        macro_names: &[String],
        tap_dance_names: &[String],
        key_legend_layout: KeyLegendLayout,
        show_shifted_number_symbols: bool,
        layout_options_value: Option<u32>,
        encoder_visibility: &[bool],
        module_settings: &ModuleSettingsState,
        matrix_pressed: &[bool],
        pressed_key_layers: &[Option<usize>],
        visibility_mode: StickyLayoutVisibilityMode,
        ui_scale: f32,
        dark: bool,
        rect: egui::Rect,
    ) {
        let painter = ui.painter_at(rect);
        let keyboard_rect = rect.shrink(STICKY_LAYOUT_KEYBOARD_MARGIN);
        let geometry = preview_layout_geometry(ui.ctx(), layout, keyboard_rect, ui_scale);
        let outline = if dark {
            Color32::from_rgb(58, 58, 62)
        } else {
            Color32::from_rgb(225, 225, 229)
        };
        let key_fill = if dark {
            Color32::from_rgb(48, 48, 52)
        } else {
            Color32::from_rgb(255, 255, 255)
        };
        let key_rects: Vec<(usize, egui::Rect)> = layout
            .keys
            .iter()
            .enumerate()
            .filter(|&(_ki, key)| {
                Self::layout_condition_visible(layout, key.layout_condition, layout_options_value)
            })
            .map(|(ki, key)| (ki, layout_physical_key_rect(key, geometry)))
            .collect();

        let mut encoder_groups: Vec<EncoderGroup> = Vec::new();
        for (encoder_idx, encoder) in layout.encoders.iter().enumerate() {
            if !Self::layout_condition_visible(
                layout,
                encoder.layout_condition,
                layout_options_value,
            ) || !Self::module_settings_encoder_visible(
                module_settings,
                layout,
                encoder.encoder_idx,
            ) {
                continue;
            }
            if !encoder_visibility
                .get(encoder.encoder_idx as usize)
                .copied()
                .unwrap_or(true)
            {
                continue;
            }

            let encoder_rect = layout_physical_encoder_rect(encoder, geometry);
            let kc = layout.get_encoder_keycode(layer, encoder_idx);
            if let Some((_, group_rect, ccw, cw)) = encoder_groups
                .iter_mut()
                .find(|(idx, _, _, _)| *idx == encoder.encoder_idx)
            {
                *group_rect = group_rect.union(encoder_rect);
                if encoder.direction == 0 {
                    *ccw = Some((encoder_idx, kc));
                } else {
                    *cw = Some((encoder_idx, kc));
                }
            } else {
                encoder_groups.push((
                    encoder.encoder_idx,
                    encoder_rect,
                    (encoder.direction == 0).then_some((encoder_idx, kc)),
                    (encoder.direction != 0).then_some((encoder_idx, kc)),
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

        for (ki, key_rect) in &key_rects {
            if encoder_press_rects
                .iter()
                .any(|(press_ki, _)| press_ki == ki)
            {
                continue;
            }

            let key = &layout.keys[*ki];
            let matrix_idx = key.row as usize * layout.cols + key.col as usize;
            let is_pressed = layout_matrix_key_pressed(layout, matrix_pressed, key.row, key.col);
            if !sticky_layout_should_draw_key(visibility_mode, is_pressed) {
                continue;
            }
            let key_layer = if is_pressed {
                pressed_key_layers
                    .get(matrix_idx)
                    .and_then(|source_layer| *source_layer)
                    .filter(|source_layer| *source_layer < layout.layers.len())
                    .unwrap_or(layer)
            } else {
                layer
            };
            let kc = layout.get_keycode(key_layer, *ki);
            let is_transparent = kc == 0x0001;
            let fill = if is_pressed {
                app_hover_fill(dark)
            } else {
                key_fill
            };
            let stroke = if is_pressed { app_accent() } else { outline };
            paint_layout_keycap(
                &painter,
                *key_rect,
                key.rotation,
                fill,
                Stroke::new(1.0_f32, stroke),
            );

            if kc == 0x0000 {
                continue;
            }

            let label_kc = if is_transparent {
                (0..key_layer)
                    .rev()
                    .map(|fallback_layer| layout.get_keycode(fallback_layer, *ki))
                    .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001))
                    .unwrap_or(0x0000)
            } else {
                kc
            };
            if label_kc == 0x0000 {
                continue;
            }
            let label = number_row_shifted_label(
                keycode_label_with_macro_names(
                    label_kc,
                    &layout.custom_keycodes,
                    layer_names,
                    macro_names,
                    tap_dance_names,
                    key_legend_layout,
                ),
                show_shifted_number_symbols,
                key_legend_layout,
            );
            draw_sticky_key_label(
                &painter,
                *key_rect,
                &label,
                dark,
                key.rotation.to_radians(),
                is_transparent,
            );
        }

        let encoder_value_label = |kc: u16| -> String {
            keycode_label_with_macro_names(
                kc,
                &layout.custom_keycodes,
                layer_names,
                macro_names,
                tap_dance_names,
                key_legend_layout,
            )
            .replace('\n', " ")
        };
        let label_for = |encoder_target: Option<(usize, u16)>| -> (String, bool) {
            let Some((visual_idx, kc)) = encoder_target else {
                return (String::new(), false);
            };
            let (label, dimmed) = match kc {
                0x0000 => (String::new(), false),
                0x0001 => {
                    let fallback = (0..layer)
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
            };
            (sticky_compact_label(&label, 9), dimmed)
        };
        let text_color = if dark {
            Color32::from_gray(232)
        } else {
            Color32::from_gray(32)
        };
        let dim_text_color = if dark {
            Color32::from_rgb(62, 56, 56)
        } else {
            Color32::from_rgb(200, 200, 208)
        };

        for (_, group_rect, ccw, cw) in encoder_groups {
            let center = group_rect.center();
            let radius = (group_rect.width().min(group_rect.height())
                * LAYOUT_ENCODER_RADIUS_FACTOR)
                .max(14.0);
            let fill_radius = radius + LAYOUT_ENCODER_FILL_EXTRA;
            let press_slot = encoder_press_rects
                .iter()
                .find(|(_, press_rect)| press_rect.center().distance(center) < 1.0)
                .map(|(press_ki, press_rect)| (*press_ki, *press_rect));
            let press_is_pressed = press_slot
                .map(|(press_ki, _)| {
                    let key = &layout.keys[press_ki];
                    layout_matrix_key_pressed(layout, matrix_pressed, key.row, key.col)
                })
                .unwrap_or(false);
            if !sticky_layout_should_draw_key(visibility_mode, press_is_pressed) {
                continue;
            }

            let (top_rect, middle_rect, bottom_rect) = if let Some((_, press_rect)) = press_slot {
                let divider_gap = radius * 0.06;
                let top_divider_y = press_rect.top() - divider_gap;
                let bottom_divider_y = press_rect.bottom() + divider_gap;
                (
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - fill_radius, center.y - fill_radius),
                        egui::pos2(center.x + fill_radius, top_divider_y),
                    ),
                    Some(egui::Rect::from_min_max(
                        egui::pos2(center.x - fill_radius, top_divider_y),
                        egui::pos2(center.x + fill_radius, bottom_divider_y),
                    )),
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - fill_radius, bottom_divider_y),
                        egui::pos2(center.x + fill_radius, center.y + fill_radius),
                    ),
                )
            } else {
                (
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - fill_radius, center.y - fill_radius),
                        egui::pos2(center.x + fill_radius, center.y),
                    ),
                    None,
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - fill_radius, center.y),
                        egui::pos2(center.x + fill_radius, center.y + fill_radius),
                    ),
                )
            };

            painter.circle_filled(center, fill_radius, key_fill);
            painter
                .with_clip_rect(top_rect)
                .circle_filled(center, fill_radius, key_fill);
            if let Some(middle_rect) = middle_rect {
                let middle_fill = if press_is_pressed {
                    app_hover_fill(dark)
                } else {
                    key_fill
                };
                painter
                    .with_clip_rect(middle_rect)
                    .circle_filled(center, fill_radius, middle_fill);
            }
            painter
                .with_clip_rect(bottom_rect)
                .circle_filled(center, fill_radius, key_fill);
            painter.circle_stroke(center, radius, Stroke::new(1.0_f32, outline));

            let has_press_button = press_slot.is_some();
            let (top_label, top_dimmed) = label_for(cw);
            let (bottom_label, bottom_dimmed) = label_for(ccw);
            let top_font = if has_press_button {
                egui::FontId::proportional(sticky_encoder_font_size(&top_label, radius, 7.4, 6.6))
            } else {
                egui::FontId::proportional(sticky_encoder_font_size(&top_label, radius, 9.5, 8.5))
            };
            let bottom_font = if has_press_button {
                egui::FontId::proportional(sticky_encoder_font_size(
                    &bottom_label,
                    radius,
                    7.4,
                    6.6,
                ))
            } else {
                egui::FontId::proportional(sticky_encoder_font_size(
                    &bottom_label,
                    radius,
                    9.5,
                    8.5,
                ))
            };
            let top_label_y = center.y - radius * if has_press_button { 0.52 } else { 0.30 };
            let bottom_label_y = center.y + radius * if has_press_button { 0.52 } else { 0.30 };
            painter.text(
                egui::pos2(center.x, top_label_y),
                egui::Align2::CENTER_CENTER,
                top_label,
                top_font,
                if top_dimmed {
                    dim_text_color
                } else {
                    text_color
                },
            );
            painter.text(
                egui::pos2(center.x, bottom_label_y),
                egui::Align2::CENTER_CENTER,
                bottom_label,
                bottom_font,
                if bottom_dimmed {
                    dim_text_color
                } else {
                    text_color
                },
            );

            draw_sticky_encoder_arrow(&painter, center, radius, true, outline);
            draw_sticky_encoder_arrow(&painter, center, radius, false, outline);

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
                    Stroke::new(1.0_f32, outline),
                );
                painter.line_segment(
                    [
                        egui::pos2(center.x - bottom_divider_half_width, bottom_divider_y),
                        egui::pos2(center.x + bottom_divider_half_width, bottom_divider_y),
                    ],
                    Stroke::new(1.0_f32, outline),
                );

                let (press_label, press_dimmed) = {
                    let key = &layout.keys[press_ki];
                    let matrix_idx = key.row as usize * layout.cols + key.col as usize;
                    let press_layer = pressed_key_layers
                        .get(matrix_idx)
                        .and_then(|source_layer| *source_layer)
                        .filter(|source_layer| *source_layer < layout.layers.len())
                        .unwrap_or(layer);
                    let kc = layout.get_keycode(press_layer, press_ki);
                    if kc == 0x0001 {
                        let fallback_kc = (0..press_layer)
                            .rev()
                            .map(|fallback_layer| layout.get_keycode(fallback_layer, press_ki))
                            .find(|fallback| !matches!(*fallback, 0x0000 | 0x0001))
                            .unwrap_or(0x0000);
                        if fallback_kc == 0x0000 {
                            ("▽".to_string(), false)
                        } else {
                            (
                                keycode_label_with_macro_names(
                                    fallback_kc,
                                    &layout.custom_keycodes,
                                    layer_names,
                                    macro_names,
                                    tap_dance_names,
                                    key_legend_layout,
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
                                layer_names,
                                macro_names,
                                tap_dance_names,
                                key_legend_layout,
                            ),
                            false,
                        )
                    }
                };
                let press_label = press_label.replace('\n', " ");
                let press_label = sticky_compact_label(&press_label, 8);
                let press_text_rect = middle_rect.shrink2(egui::vec2(4.0, 2.0));
                let press_font =
                    FontId::proportional(sticky_press_font_size(&press_label, press_text_rect));
                painter.with_clip_rect(press_text_rect).text(
                    press_text_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    press_label,
                    press_font,
                    if press_dimmed {
                        dim_text_color
                    } else {
                        text_color
                    },
                );
            } else {
                let divider_half_width = (radius - 0.5).max(0.0);
                painter.line_segment(
                    [
                        egui::pos2(center.x - divider_half_width, center.y),
                        egui::pos2(center.x + divider_half_width, center.y),
                    ],
                    Stroke::new(1.0_f32, outline),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressed_only_mode_draws_only_pressed_keys() {
        assert!(sticky_layout_should_draw_key(
            StickyLayoutVisibilityMode::LayoutAndPresses,
            false,
        ));
        assert!(sticky_layout_should_draw_key(
            StickyLayoutVisibilityMode::LayoutAndPresses,
            true,
        ));
        assert!(!sticky_layout_should_draw_key(
            StickyLayoutVisibilityMode::PressedOnly,
            false,
        ));
        assert!(sticky_layout_should_draw_key(
            StickyLayoutVisibilityMode::PressedOnly,
            true,
        ));
    }
}
