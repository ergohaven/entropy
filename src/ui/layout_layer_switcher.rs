use super::*;

pub(super) const MAIN_MENU_BATTERY_RESERVED_H: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainMenuBatteryStatus {
    None,
    Single(u8),
    Split { left: u8, right: u8 },
}

fn main_menu_battery_status(battery: Option<crate::hid::BatteryHalves>) -> MainMenuBatteryStatus {
    match battery {
        Some(crate::hid::BatteryHalves {
            left: Some(left),
            right: Some(right),
        }) => MainMenuBatteryStatus::Split { left, right },
        Some(crate::hid::BatteryHalves {
            left: Some(value),
            right: None,
        })
        | Some(crate::hid::BatteryHalves {
            left: None,
            right: Some(value),
        }) => MainMenuBatteryStatus::Single(value),
        _ => MainMenuBatteryStatus::None,
    }
}

fn layer_after_wheel(selected: usize, layer_count: usize, wheel_delta: f32) -> usize {
    if layer_count == 0 {
        return 0;
    }
    if wheel_delta < 0.0 {
        (selected + 1).min(layer_count - 1)
    } else if wheel_delta > 0.0 {
        selected.saturating_sub(1)
    } else {
        selected.min(layer_count - 1)
    }
}

impl EntropyApp {
    fn main_menu_battery_status(&self) -> MainMenuBatteryStatus {
        main_menu_battery_status(
            self.device_about_info
                .as_ref()
                .and_then(|info| info.battery_halves),
        )
    }

    pub(super) fn main_menu_has_battery_status(&self) -> bool {
        self.main_menu_battery_status() != MainMenuBatteryStatus::None
    }

    fn draw_main_menu_battery_status(&self, ui: &mut egui::Ui, center_x: f32, layer_center_y: f32) {
        let status = self.main_menu_battery_status();
        if status == MainMenuBatteryStatus::None {
            return;
        }

        let lang = self.app_settings.language;
        let text_color = app_muted_text(self.dark_mode);
        let font = FontId::proportional(13.0);
        let center_y = layer_center_y + 36.0;
        let paint_value = |x: f32, value: u8| {
            ui.painter().text(
                egui::pos2(x, center_y),
                egui::Align2::CENTER_CENTER,
                about_device_ui::battery_percent_text(lang, Some(value)),
                font.clone(),
                text_color,
            );
        };

        match status {
            MainMenuBatteryStatus::None => {}
            MainMenuBatteryStatus::Single(value) => paint_value(center_x, value),
            MainMenuBatteryStatus::Split { left, right } => {
                let value_offset = 38.0;
                paint_value(center_x - value_offset, left);
                paint_value(center_x + value_offset, right);
                ui.painter().line_segment(
                    [
                        egui::pos2(center_x, center_y - 8.0),
                        egui::pos2(center_x, center_y + 8.0),
                    ],
                    top_menu_divider_stroke(self.dark_mode),
                );
            }
        }
    }

    pub(super) fn draw_layout_layer_switcher_and_hints(
        &mut self,
        ui: &mut egui::Ui,
        top_base_y: f32,
        main_tabs_h: f32,
        layer_bar_h: f32,
    ) {
        // ── Layer switcher ─────────────────────────────────────────────────
        {
            let layer_count = self.layer_count;
            let selected = self.selected_layer;
            // raw_name — чистое имя без префикса, хранится в layer_names
            let raw_name = self
                .layer_names
                .get(selected)
                .cloned()
                .unwrap_or_else(|| selected.to_string());
            let visible_raw_name: String = raw_name.chars().take(12).collect();
            // display_name — с префиксом для отображения
            let display_name = if !raw_name.is_empty() && raw_name != selected.to_string() {
                format!("{}. {}", selected, visible_raw_name)
            } else {
                visible_raw_name.clone()
            };
            let name = display_name;
            let center_x = ui.max_rect().center().x;
            let bar_y = top_base_y + main_tabs_h + 24.0;
            let mid_y = bar_y + layer_bar_h / 2.0;
            // Layer name / edit field
            let name_rect = egui::Rect::from_min_size(
                egui::pos2(center_x - 85.0, bar_y),
                Vec2::new(170.0, 52.0),
            );
            self.register_tour_target(
                TourTarget::LayerSwitcher,
                name_rect.expand2(Vec2::new(72.0, 8.0)),
            );

            let display_name_len = visible_raw_name.chars().count();
            let display_label_size = if display_name_len > 10 {
                26.0
            } else if display_name_len > 7 {
                31.0
            } else {
                39.0
            };
            let label_font = egui::FontId {
                size: display_label_size,
                family: egui::FontFamily::Proportional,
            };
            let text_color = if self.dark_mode {
                Color32::from_gray(245)
            } else {
                Color32::from_gray(60)
            };

            if self.editing_layer == Some(selected) {
                // Limit input to 12 chars
                if self.editing_layer_text.chars().count() > 12 {
                    let s: String = self.editing_layer_text.chars().take(12).collect();
                    self.editing_layer_text = s;
                }
                let editing_font = egui::FontId {
                    size: 39.0,
                    family: egui::FontFamily::Proportional,
                };
                let resp = ui.put(
                    name_rect,
                    egui::TextEdit::singleline(&mut self.editing_layer_text)
                        .font(editing_font)
                        .horizontal_align(egui::Align::Center)
                        .char_limit(12)
                        .frame(egui::Frame::NONE),
                );
                // Request focus only on the first frame so lost_focus() works correctly.
                if !self.editing_layer_focus_requested {
                    resp.request_focus();
                    self.editing_layer_focus_requested = true;
                }
                // Commit on Enter or lost focus (click outside); cancel on Escape.
                let commit = resp.lost_focus() || ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                let cancel = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
                if commit || cancel {
                    if commit {
                        let proposed_name = self.editing_layer_text.trim().to_string();
                        if proposed_name.is_empty() {
                            self.editing_layer_text = raw_name.clone();
                        } else {
                            let new_name = proposed_name;
                            while self.layer_names.len() <= selected {
                                self.layer_names.push(self.layer_names.len().to_string());
                            }
                            self.layer_names[selected] = new_name.clone();
                            #[cfg(not(target_arch = "wasm32"))]
                            save_layer_names(&self.layer_names, &self.current_device_name);
                            #[cfg(target_arch = "wasm32")]
                            save_layer_names(&self.layer_names, "default");
                            // Also write name back to the connected device
                            #[cfg(not(target_arch = "wasm32"))]
                            if self.firmware == FirmwareProtocol::Vial {
                                if let Some(dev) = &self.hid_device {
                                    if let Err(e) =
                                        dev.set_qmk_setting_string(200 + selected as u16, &new_name)
                                    {
                                        log::warn!(
                                            "Vial set_qmk_setting_string failed for layer {}: {}",
                                            selected,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    self.editing_layer = None;
                    self.editing_layer_focus_requested = false;
                }
            } else {
                // Fixed arrow positions based on max 7-char name width so
                // arrows never jump around as the layer name changes.
                // name_rect is 170px wide → half = 85px; gap keeps arrows clear.
                let fixed_half = 85.0_f32;
                let gap = 16.0_f32;
                let arrow_y = mid_y - 2.0;
                let left_center = egui::pos2(center_x - fixed_half - gap - 24.0, arrow_y);
                let right_center = egui::pos2(center_x + fixed_half + gap + 24.0, arrow_y);

                // Still measure actual text width for painting the name and edit icon.
                let text_w = ui.fonts_mut(|f| {
                    f.layout_no_wrap(name.clone(), label_font.clone(), text_color)
                        .size()
                        .x
                });

                // Allocate name FIRST — arrows are allocated last and win in egui's
                // hit-test order (last allocation = highest priority).
                let name_hit = egui::Rect::from_center_size(
                    egui::pos2(center_x, mid_y),
                    Vec2::new(text_w + 12.0, 52.0),
                );
                let name_r = ui.allocate_rect(name_hit, Sense::click());

                // Full layer switch zone from arrow to arrow for mouse wheel switching.
                // Keep click/hover hitboxes close to the actual arrow glyph size.
                let left_hit = egui::Rect::from_center_size(left_center, Vec2::new(28.0, 44.0));
                let right_hit = egui::Rect::from_center_size(right_center, Vec2::new(28.0, 44.0));
                let wheel_hit = egui::Rect::from_min_max(
                    egui::pos2(left_hit.left(), mid_y - 26.0),
                    egui::pos2(right_hit.right(), mid_y + 26.0),
                );
                let wheel_r = ui.allocate_rect(wheel_hit, Sense::hover());

                // Scroll wheel over the whole layer bar switches layers (down = next, up = prev)
                if wheel_r.hovered() {
                    // Use the raw wheel event once. The smoothed delta persists across
                    // repaint frames and used to race through every layer per notch.
                    let scroll = ui.input(|i| {
                        i.raw
                            .events
                            .iter()
                            .find_map(|event| match event {
                                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                                _ => None,
                            })
                            .unwrap_or(0.0)
                    });
                    let next_layer = layer_after_wheel(selected, layer_count, scroll);
                    if next_layer != selected {
                        self.selected_layer = next_layer;
                        self.jump_back_stack.clear();
                    }
                }

                // Allocate arrows LAST so they have click priority over the name rect.
                let left_r = ui.allocate_rect(left_hit, Sense::click());
                let right_r = ui.allocate_rect(right_hit, Sense::click());
                if left_r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if right_r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if left_r.clicked() && selected > 0 {
                    self.selected_layer = selected - 1;
                    self.jump_back_stack.clear();
                }
                if right_r.clicked() && selected + 1 < layer_count {
                    self.selected_layer = selected + 1;
                    self.jump_back_stack.clear();
                }
                if name_r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if name_r.clicked() {
                    self.editing_layer = Some(selected);
                    self.editing_layer_text = raw_name.clone();
                }

                self.draw_layer_actions_menu(ui, egui::pos2(right_center.x + 42.0, arrow_y));

                // Paint
                let dis = if self.dark_mode {
                    Color32::from_gray(60)
                } else {
                    Color32::from_gray(200)
                };
                let ac_l = if left_r.hovered() {
                    app_accent()
                } else if self.dark_mode {
                    Color32::from_gray(140)
                } else {
                    Color32::from_gray(120)
                };
                let ac_r = if right_r.hovered() {
                    app_accent()
                } else if self.dark_mode {
                    Color32::from_gray(140)
                } else {
                    Color32::from_gray(120)
                };
                ui.painter().text(
                    left_center,
                    egui::Align2::CENTER_CENTER,
                    "‹",
                    FontId::proportional(52.0),
                    if selected == 0 { dis } else { ac_l },
                );
                ui.painter().text(
                    right_center,
                    egui::Align2::CENTER_CENTER,
                    "›",
                    FontId::proportional(52.0),
                    if selected + 1 >= layer_count {
                        dis
                    } else {
                        ac_r
                    },
                );
                ui.painter().text(
                    egui::pos2(center_x, mid_y),
                    egui::Align2::CENTER_CENTER,
                    &name,
                    label_font,
                    text_color,
                );

                self.draw_layout_bottom_hints(ui, center_x, name_r.hovered());
            }

            self.draw_main_menu_battery_status(ui, center_x, mid_y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{layer_after_wheel, main_menu_battery_status, MainMenuBatteryStatus};

    #[test]
    fn split_batteries_keep_left_and_right_order() {
        assert_eq!(
            main_menu_battery_status(Some(crate::hid::BatteryHalves {
                left: Some(98),
                right: Some(95),
            })),
            MainMenuBatteryStatus::Split {
                left: 98,
                right: 95,
            }
        );
    }

    #[test]
    fn one_reported_battery_is_centered() {
        assert_eq!(
            main_menu_battery_status(Some(crate::hid::BatteryHalves {
                left: None,
                right: Some(95),
            })),
            MainMenuBatteryStatus::Single(95)
        );
    }

    #[test]
    fn missing_battery_values_hide_the_main_menu_status() {
        assert_eq!(
            main_menu_battery_status(Some(crate::hid::BatteryHalves::default())),
            MainMenuBatteryStatus::None
        );
        assert_eq!(main_menu_battery_status(None), MainMenuBatteryStatus::None);
    }

    #[test]
    fn wheel_event_moves_exactly_one_layer_without_wrapping() {
        assert_eq!(layer_after_wheel(0, 6, -120.0), 1);
        assert_eq!(layer_after_wheel(1, 6, 120.0), 0);
        assert_eq!(layer_after_wheel(5, 6, -120.0), 5);
        assert_eq!(layer_after_wheel(0, 6, 120.0), 0);
    }

    #[test]
    fn wheel_magnitude_does_not_skip_layers() {
        assert_eq!(layer_after_wheel(2, 8, -10_000.0), 3);
        assert_eq!(layer_after_wheel(2, 8, 10_000.0), 1);
    }
}
