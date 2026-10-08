use super::*;

pub(super) const MAIN_MENU_BATTERY_RESERVED_H: f32 = 34.0;

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

fn main_menu_reserves_battery_status_space(info: Option<&DeviceAboutInfo>) -> bool {
    info.map(|info| info.supports_battery_halves)
        .unwrap_or(false)
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

fn layer_name_hover_is_available(user_action_busy: bool, selected_layer_ready: bool) -> bool {
    !user_action_busy && selected_layer_ready
}

fn layer_name_edit_is_available(hover_available: bool, background_layer_active: bool) -> bool {
    hover_available && !background_layer_active
}

fn application_layout_after_step(current: usize, count: usize, step: i32) -> usize {
    if count == 0 {
        return 0;
    }
    if step < 0 {
        current.saturating_sub(1)
    } else if step > 0 {
        (current + 1).min(count - 1)
    } else {
        current.min(count - 1)
    }
}

impl EntropyApp {
    fn draw_application_layout_switcher(
        &mut self,
        ui: &mut egui::Ui,
        center_x: f32,
        center_y: f32,
    ) {
        let options = self.application_layout_editor_options();
        if options.is_empty() {
            return;
        }
        let current_id = self
            .application_layout_settings()
            .map(|settings| settings.active_layout_id.clone())
            .unwrap_or_else(|| {
                crate::application_layouts::DEFAULT_APPLICATION_LAYOUT_ID.to_owned()
            });
        let current_index = options
            .iter()
            .position(|(id, _)| id == &current_id)
            .unwrap_or(0);
        let current_name = options[current_index].1.clone();
        let visible_name: String = current_name.chars().take(14).collect();
        let selector_width = 200.0;
        let selector_height = 34.0;
        let selector_rect = egui::Rect::from_center_size(
            egui::pos2(center_x, center_y),
            egui::vec2(140.0, selector_height),
        );
        let left_center = egui::pos2(center_x - 86.0, center_y - 1.0);
        let right_center = egui::pos2(center_x + 86.0, center_y - 1.0);
        let dropdown_id = ui.make_persistent_id("layout_page_application_selector");
        let response = ui.allocate_rect(selector_rect, Sense::click());
        let left_rect = egui::Rect::from_center_size(left_center, egui::vec2(28.0, 34.0));
        let right_rect = egui::Rect::from_center_size(right_center, egui::vec2(28.0, 34.0));
        let left_response = ui.allocate_rect(left_rect, Sense::click());
        let right_response = ui.allocate_rect(right_rect, Sense::click());

        if left_response.clicked() {
            let index = application_layout_after_step(current_index, options.len(), -1);
            if index != current_index {
                self.activate_application_layout(&options[index].0);
            }
        }
        if right_response.clicked() {
            let index = application_layout_after_step(current_index, options.len(), 1);
            if index != current_index {
                self.activate_application_layout(&options[index].0);
            }
        }
        if response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), dropdown_id);
        }
        if response.hovered() || left_response.hovered() || right_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let text_color = if self.dark_mode {
            Color32::from_gray(245)
        } else {
            Color32::from_gray(60)
        };
        let disabled = if self.dark_mode {
            Color32::from_gray(60)
        } else {
            Color32::from_gray(200)
        };
        let arrow_color = |hovered| {
            if hovered {
                app_accent()
            } else if self.dark_mode {
                Color32::from_gray(140)
            } else {
                Color32::from_gray(120)
            }
        };
        let name_size = if visible_name.chars().count() > 11 {
            18.0
        } else if visible_name.chars().count() > 8 {
            21.0
        } else {
            26.0
        };
        ui.painter().text(
            egui::pos2(center_x, center_y),
            egui::Align2::CENTER_CENTER,
            visible_name,
            FontId::proportional(name_size),
            text_color,
        );
        ui.painter().text(
            left_center,
            egui::Align2::CENTER_CENTER,
            "‹",
            FontId::proportional(34.7),
            if current_index == 0 {
                disabled
            } else {
                arrow_color(left_response.hovered())
            },
        );
        ui.painter().text(
            right_center,
            egui::Align2::CENTER_CENTER,
            "›",
            FontId::proportional(34.7),
            if current_index + 1 >= options.len() {
                disabled
            } else {
                arrow_color(right_response.hovered())
            },
        );

        crate::ui_style::popup_below_widget(
            ui,
            dropdown_id,
            &response,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(selector_width);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                for (id, name) in &options {
                    if ui.selectable_label(id == &current_id, name).clicked() {
                        self.activate_application_layout(id);
                        egui::Popup::close_id(ui.ctx(), dropdown_id);
                    }
                }
            },
        );
    }

    fn main_menu_battery_status(&self) -> MainMenuBatteryStatus {
        main_menu_battery_status(
            self.device_about_info
                .as_ref()
                .and_then(|info| info.battery_halves),
        )
    }

    pub(super) fn main_menu_reserves_battery_status_space(&self) -> bool {
        main_menu_reserves_battery_status_space(self.device_about_info.as_ref())
    }

    fn draw_main_menu_battery_status(&self, ui: &mut egui::Ui, center_x: f32, layer_center_y: f32) {
        let status = self.main_menu_battery_status();
        if status == MainMenuBatteryStatus::None {
            return;
        }

        let lang = self.app_settings.language;
        let text_color = app_muted_text(self.dark_mode);
        let font = FontId::proportional(13.0);
        let center_y = layer_center_y + 42.0;
        let paint_value = |x: f32, value: u8| {
            let painter = ui.painter();
            let value_text = about_device_ui::battery_percent_text(lang, Some(value));
            let galley = painter.layout_no_wrap(value_text, font.clone(), text_color);
            let icon_gap = 5.0;
            let content_width = crate::ui_style::BATTERY_ICON_WIDTH + icon_gap + galley.size().x;
            let content_left = x - content_width * 0.5;
            crate::ui_style::paint_battery_icon(
                painter,
                egui::pos2(
                    content_left + crate::ui_style::BATTERY_ICON_WIDTH * 0.5,
                    center_y,
                ),
                value,
                text_color,
            );
            painter.galley(
                egui::pos2(
                    content_left + crate::ui_style::BATTERY_ICON_WIDTH + icon_gap,
                    center_y - galley.size().y * 0.5,
                ),
                galley,
                text_color,
            );
        };

        match status {
            MainMenuBatteryStatus::None => {}
            MainMenuBatteryStatus::Single(value) => paint_value(center_x, value),
            MainMenuBatteryStatus::Split { left, right } => {
                let value_offset = 44.0;
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
            let layer_count = if self.application_layout_editor_active {
                crate::application_layouts::APPLICATION_LAYOUT_LAYER_COUNT
            } else {
                self.layer_count
            };
            let selected = self.selected_layer;
            let editor_layer_names = self
                .application_layout_editor_active
                .then(|| self.application_layout_editor_layer_names());
            // raw_name — чистое имя без префикса, хранится в layer_names
            let raw_name = editor_layer_names
                .as_ref()
                .and_then(|names| names.get(selected))
                .or_else(|| self.layer_names.get(selected))
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
                let commit = resp.lost_focus()
                    || ui.input(|inp| inp.key_pressed(egui::Key::Enter))
                    || ui.input(|inp| inp.viewport().focused == Some(false));
                let cancel = ui.input(|inp| inp.key_pressed(egui::Key::Escape));
                if cancel {
                    ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
                if commit || cancel {
                    if !cancel {
                        let proposed_name = self.editing_layer_text.trim().to_string();
                        if proposed_name.is_empty() {
                            self.editing_layer_text = raw_name.clone();
                        } else {
                            let new_name = proposed_name;
                            if self.application_layout_editor_active {
                                if let Some(layout_id) = self.editing_layer_layout_id.clone() {
                                    self.rename_application_layout_layer(
                                        &layout_id, selected, new_name,
                                    );
                                }
                            } else {
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
                                        if let Err(e) = dev.set_qmk_setting_string(
                                            200 + selected as u16,
                                            &new_name,
                                        ) {
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
                    }
                    self.editing_layer = None;
                    self.editing_layer_text.clear();
                    self.editing_layer_focus_requested = false;
                    self.editing_layer_layout_id = None;
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
                #[cfg(not(target_arch = "wasm32"))]
                let layer_name_hover_available = self.application_layout_editor_active
                    || layer_name_hover_is_available(
                        self.hid_user_action_busy(),
                        self.deferred_device_load.layer_status(selected).ready(),
                    );
                #[cfg(not(target_arch = "wasm32"))]
                let layer_name_edit_available = self.application_layout_editor_active
                    || layer_name_edit_is_available(
                        layer_name_hover_available,
                        self.vial_hid_background_layer_active(),
                    );
                #[cfg(target_arch = "wasm32")]
                let layer_name_hover_available = true;
                #[cfg(target_arch = "wasm32")]
                let layer_name_edit_available = layer_name_hover_available;
                if name_r.hovered() && layer_name_hover_available {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if name_r.clicked() && layer_name_edit_available && !self.editing_layout_visibility
                {
                    self.editing_layer = Some(selected);
                    self.editing_layer_text = raw_name.clone();
                    self.editing_layer_layout_id = self
                        .application_layout_editor_active
                        .then(|| {
                            self.application_layout_settings()
                                .map(|settings| settings.active_layout_id.clone())
                        })
                        .flatten();
                }

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

                self.draw_layout_bottom_hints(
                    ui,
                    center_x,
                    name_r.hovered() && layer_name_hover_available,
                );
            }

            self.draw_main_menu_battery_status(ui, center_x, mid_y);
            if self.application_layout_editor_active {
                self.draw_application_layout_switcher(ui, center_x, mid_y + 50.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{DeferredDeviceLoadState, DeferredLoadStatus, DeviceAboutInfo, EntropyApp};

    use super::{
        application_layout_after_step, layer_after_wheel, layer_name_edit_is_available,
        layer_name_hover_is_available, main_menu_battery_status,
        main_menu_reserves_battery_status_space, MainMenuBatteryStatus,
    };

    #[test]
    fn clicking_main_menu_layer_name_starts_renaming() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        app.layer_count = 2;
        app.layer_names = vec!["Base".into(), "Fn".into()];
        app.deferred_device_load = DeferredDeviceLoadState::complete(2);
        app.deferred_device_load
            .set_layer_status(1, DeferredLoadStatus::NotLoaded);
        let pos = egui::pos2(550.0, 50.0);
        let frame = |app: &mut EntropyApp, events| {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
            );
        };
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.editing_layer, Some(0));
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(app.editing_layer, Some(0));
        frame(&mut app, vec![egui::Event::Text("New".into())]);
        assert_eq!(app.editing_layer, Some(0));
        assert!(app.editing_layer_text.contains("New"));
    }

    #[test]
    fn layer_name_hover_requires_selected_layer_but_not_all_layers() {
        assert!(!layer_name_hover_is_available(true, true));
        assert!(layer_name_hover_is_available(false, true));
        assert!(!layer_name_hover_is_available(false, false));
        assert!(!layer_name_hover_is_available(true, false));
    }

    #[test]
    fn background_hid_reads_do_not_flicker_rename_hover_or_start_a_write() {
        let ready_hover = layer_name_hover_is_available(false, true);
        assert!(ready_hover);
        assert!(layer_name_edit_is_available(ready_hover, false));
        assert!(!layer_name_edit_is_available(ready_hover, true));
        // The hover state remains true in both frames while the serialized HID
        // reader alternates between active requests and short idle intervals.
        assert!(layer_name_hover_is_available(false, true));
    }

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
    fn battery_capability_reserves_space_before_values_arrive() {
        let mut info = DeviceAboutInfo {
            supports_battery_halves: true,
            ..Default::default()
        };

        assert!(main_menu_reserves_battery_status_space(Some(&info)));
        info.battery_halves = Some(crate::hid::BatteryHalves {
            left: Some(84),
            right: Some(81),
        });
        assert!(main_menu_reserves_battery_status_space(Some(&info)));
        assert!(!main_menu_reserves_battery_status_space(None));
    }

    #[test]
    fn wheel_event_moves_exactly_one_layer_without_wrapping() {
        assert_eq!(layer_after_wheel(0, 16, -120.0), 1);
        assert_eq!(layer_after_wheel(1, 16, 120.0), 0);
        assert_eq!(layer_after_wheel(14, 16, -120.0), 15);
        assert_eq!(layer_after_wheel(15, 16, -120.0), 15);
        assert_eq!(layer_after_wheel(0, 16, 120.0), 0);
    }

    #[test]
    fn application_layout_arrows_do_not_wrap() {
        assert_eq!(application_layout_after_step(0, 3, -1), 0);
        assert_eq!(application_layout_after_step(0, 3, 1), 1);
        assert_eq!(application_layout_after_step(2, 3, 1), 2);
        assert_eq!(application_layout_after_step(2, 3, -1), 1);
        assert_eq!(application_layout_after_step(0, 0, 1), 0);
    }

    #[test]
    fn wheel_magnitude_does_not_skip_layers() {
        assert_eq!(layer_after_wheel(2, 8, -10_000.0), 3);
        assert_eq!(layer_after_wheel(2, 8, 10_000.0), 1);
    }
}
