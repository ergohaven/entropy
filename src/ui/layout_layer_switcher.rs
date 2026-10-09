use super::application_layout_runtime::app_layout_text;
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

fn layer_name_text_color(dark_mode: bool, hovered: bool) -> Color32 {
    if hovered {
        app_accent()
    } else if dark_mode {
        Color32::from_gray(245)
    } else {
        Color32::from_gray(60)
    }
}

impl EntropyApp {
    pub(super) fn show_main_menu_application_layout_switcher(&self) -> bool {
        self.application_layout_editor_active
            && self
                .application_layout_settings()
                .is_none_or(|settings| settings.automatic_switching_enabled)
    }

    fn draw_application_layout_switcher(
        &mut self,
        ui: &mut egui::Ui,
        center_x: f32,
        center_y: f32,
    ) -> bool {
        let options = self.application_layout_editor_options();
        if options.is_empty() {
            return false;
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
        let selector_width = 200.0;
        let selector_height = 34.0;
        let name_size = if current_name.chars().count() > 11 {
            18.0
        } else if current_name.chars().count() > 8 {
            21.0
        } else {
            26.0
        };
        let name_font = FontId::proportional(name_size);
        let name_galley = ui.fonts_mut(|fonts| {
            fonts.layout_no_wrap(current_name.clone(), name_font.clone(), Color32::WHITE)
        });
        let name_extent = name_galley.size();
        let chevron_half_width = 4.5;
        let chevron_x = center_x + name_extent.x / 2.0 + 9.0 + chevron_half_width;
        // The galley line box extends below the visible glyphs. Align to the
        // actual text mesh instead of its padded line-box bottom.
        let name_ink_bottom = center_y - name_extent.y / 2.0 + name_galley.mesh_bounds.max.y;
        let chevron_y = name_ink_bottom - 2.5;
        // Keep the name centered; fit the shared click target tightly around
        // both glyphs instead of reserving space for the longest possible name.
        let selector_rect = egui::Rect::from_min_max(
            egui::pos2(
                center_x - name_extent.x / 2.0 - 6.0,
                center_y - selector_height / 2.0,
            ),
            egui::pos2(
                chevron_x + chevron_half_width + 6.0,
                center_y + selector_height / 2.0,
            ),
        );
        let dropdown_id = ui.make_persistent_id("layout_page_application_selector");
        let response = ui.allocate_rect(selector_rect, Sense::click());
        let language = self.app_settings.language;
        if response.clicked() {
            egui::Popup::toggle_id(ui.ctx(), dropdown_id);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let text_color = if response.hovered() {
            app_accent()
        } else if self.dark_mode {
            Color32::from_gray(245)
        } else {
            Color32::from_gray(60)
        };
        let arrow_color = if response.hovered() {
            app_accent()
        } else {
            app_muted_text(self.dark_mode)
        };
        ui.painter().text(
            egui::pos2(center_x, center_y),
            egui::Align2::CENTER_CENTER,
            current_name,
            name_font,
            text_color,
        );
        crate::ui_style::paint_dropdown_chevron(
            ui.painter(),
            egui::pos2(chevron_x, chevron_y),
            arrow_color,
        );

        let groups = if egui::Popup::is_id_open(ui.ctx(), dropdown_id) {
            self.application_layout_editor_labeled_groups(&options, language)
        } else {
            Vec::new()
        };
        match crate::ui_style::modern_dropdown_grouped_options_with_action(
            ui,
            dropdown_id,
            &response,
            &options[0],
            &groups,
            &current_id,
            selector_width,
            12.5,
            Some(app_layout_text(language, "Настроить", "Configure")),
        ) {
            Some(crate::ui_style::GroupedDropdownChoice::Item(id)) => {
                self.activate_application_layout(&id);
            }
            Some(crate::ui_style::GroupedDropdownChoice::Action) => {
                self.open_application_layouts_page();
            }
            None => {}
        }
        response.hovered()
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
            let text_color = layer_name_text_color(self.dark_mode, false);
            let mut layer_name_hovered = None;

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
                let name_hovered = name_r.hovered() && layer_name_hover_available;
                if name_hovered {
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
                    layer_name_text_color(
                        self.dark_mode,
                        name_hovered && !self.editing_layout_visibility,
                    ),
                );

                layer_name_hovered = Some(name_hovered);
            }

            self.draw_main_menu_battery_status(ui, center_x, mid_y);
            let application_selector_hovered = if self.show_main_menu_application_layout_switcher()
            {
                self.draw_application_layout_switcher(ui, center_x, mid_y + 50.0)
            } else {
                false
            };
            if let Some(layer_name_hovered) = layer_name_hovered {
                self.draw_layout_bottom_hints(
                    ui,
                    center_x,
                    layer_name_hovered,
                    application_selector_hovered,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{
        DeferredDeviceLoadState, DeferredLoadStatus, DeviceAboutInfo, EntropyApp, MainMenuTab,
        SettingsTab,
    };

    use super::{
        app_layout_text, layer_after_wheel, layer_name_edit_is_available,
        layer_name_hover_is_available, layer_name_text_color, main_menu_battery_status,
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
    fn layer_name_uses_selector_accent_only_on_hover() {
        for dark_mode in [false, true] {
            assert_eq!(layer_name_text_color(dark_mode, true), super::app_accent());
            assert_ne!(
                layer_name_text_color(dark_mode, false),
                layer_name_text_color(dark_mode, true)
            );
        }
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
    fn main_menu_application_selector_follows_master_switch_without_disabling_settings() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let device_key = "offline-macropad-switch-test".to_owned();
        app.app_settings.application_layouts.insert(
            device_key.clone(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some(device_key.clone());
        app.application_layout_editor_active = app.application_layouts_supported();

        let render = |app: &mut EntropyApp| {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 0.0, 0.0, 52.0),
            );
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Default")
            })
        };

        assert!(app.show_main_menu_application_layout_switcher());
        assert!(render(&mut app), "enabled selector is visible");
        app.app_settings
            .application_layouts
            .get_mut(&device_key)
            .unwrap()
            .automatic_switching_enabled = false;
        assert!(
            app.application_layouts_supported(),
            "Advanced settings must remain accessible"
        );
        assert!(
            app.application_layout_editor_active,
            "layout editing stays available"
        );
        assert!(!app.show_main_menu_application_layout_switcher());
        assert!(!render(&mut app), "disabled selector must not be painted");
        app.app_settings
            .application_layouts
            .get_mut(&device_key)
            .unwrap()
            .automatic_switching_enabled = true;
        assert!(app.show_main_menu_application_layout_switcher());
        assert!(render(&mut app), "selector returns when enabled");
    }

    #[test]
    fn application_selector_shows_full_program_name_in_main_menu() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let device_key = "offline-macropad-full-program-name".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let id = settings.create_for_application_named(
            &crate::application_layouts::DetectedApplication {
                executable: "firefox".to_owned(),
                ..Default::default()
            },
            Some("Mozilla Firefox"),
            "",
        );
        settings.active_layout_id = id;
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| {
                app.draw_application_layout_switcher(ui, 450.0, 130.0);
            },
        );
        let name = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Mozilla Firefox" => {
                    assert!(shape.clip_rect.contains_rect(text.visual_bounding_rect()));
                    Some(text)
                }
                _ => None,
            })
            .expect("full name must be painted without the former 14-character cutoff");
        let chevron_left = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, .. } => Some(points[0].x.min(points[1].x)),
                _ => None,
            })
            .fold(f32::INFINITY, f32::min);
        let gap = chevron_left - name.visual_bounding_rect().right();
        assert!((8.0..=10.0).contains(&gap), "chevron gap: {gap}");
    }

    #[test]
    fn application_selector_has_one_down_arrow_beside_name() {
        let ctx = egui::Context::default();
        let mut app = EntropyApp::new_inert_for_test();
        let device_key = "offline-macropad-test".to_owned();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        for index in 0..12 {
            settings.create_for_application_named(
                &crate::application_layouts::DetectedApplication {
                    executable: format!("app-{index}"),
                    ..Default::default()
                },
                Some(&format!("Application {index}")),
                "",
            );
        }
        settings.create_for_application_named(
            &crate::application_layouts::DetectedApplication {
                executable: "firefox".to_owned(),
                ..Default::default()
            },
            Some("Firefox"),
            "",
        );
        app.app_settings
            .application_layouts
            .insert(device_key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(device_key);
        let selector_center = egui::pos2(450.0, 130.0);
        let frame = |app: &mut EntropyApp, events| {
            let mut popup_id = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    popup_id = Some(ui.make_persistent_id("layout_page_application_selector"));
                    app.draw_application_layout_switcher(ui, selector_center.x, selector_center.y);
                },
            );
            (popup_id.unwrap(), output)
        };
        let (popup_id, output) = frame(&mut app, vec![]);
        let name = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Default" => Some(text),
                _ => None,
            })
            .unwrap();
        let chevron = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, stroke } => Some((points, stroke)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(chevron.len(), 2, "one unfilled, two-stroke chevron");
        let chevron_left = chevron
            .iter()
            .flat_map(|(points, _)| points.iter())
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min);
        let gap = chevron_left - (name.pos.x + name.galley.size().x);
        assert!((8.0..=10.0).contains(&gap), "chevron gap: {gap}");
        assert!(chevron.iter().all(|(_, stroke)| stroke.width > 0.0));
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if matches!(text.galley.text(), "▾" | "‹" | "›"))));
        let chevron_tip = chevron
            .iter()
            .flat_map(|(points, _)| points.iter())
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let ink_bottom = name.visual_bounding_rect().bottom();
        assert!(
            (chevron_tip - ink_bottom).abs() < 1.0,
            "chevron tip {chevron_tip} versus visible text bottom {ink_bottom}"
        );
        let arrow_click = egui::pos2(chevron_left + 4.5, chevron_tip - 2.5);

        // The former left-arrow area no longer changes profiles or opens the menu.
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(364.0, 130.0)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(364.0, 130.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(!egui::Popup::is_id_open(&ctx, popup_id));

        // Hovering either half highlights both the program text and the arrow.
        for pos in [selector_center, arrow_click] {
            let (_, hover) = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            let text = hover
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Default" => Some(text),
                    _ => None,
                })
                .unwrap();
            assert_eq!(text.fallback_color, crate::ui_style::accent());
            let strokes = hover
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment { stroke, .. } => Some(stroke),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(strokes.len(), 2);
            assert!(strokes
                .iter()
                .all(|stroke| stroke.color == crate::ui_style::accent()));
        }

        // The arrow opens the dropdown through the existing selector response.
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(arrow_click),
                    egui::Event::PointerButton {
                        pos: arrow_click,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(egui::Popup::is_id_open(&ctx, popup_id));

        // Default stays at the first level; the programs are behind a category
        // that opens beside it on hover, as in Layer operations.
        let (_, popup) = frame(
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(50.0, 50.0))],
        );
        let category_pos = popup
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if matches!(text.galley.text(), "Other" | "Другие") => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("uncategorized applications appear as a submenu category");
        let browsers_pos = popup
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if matches!(text.galley.text(), "Browsers" | "Браузеры") =>
                {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .expect("known browser preset must appear as a category");
        assert!(popup.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Default"
                && shape.clip_rect.intersects(text.visual_bounding_rect())
        )));
        frame(&mut app, vec![egui::Event::PointerMoved(browsers_pos)]);
        let (_, browser_submenu) = frame(&mut app, vec![]);
        assert!(browser_submenu.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Firefox"
                && shape.clip_rect.intersects(text.visual_bounding_rect())
        )));
        let (category_row, child_menu) = ctx.data(|data| {
            (
                data.get_temp::<egui::Rect>(popup_id.with("category_row_rect"))
                    .expect("hovered category row"),
                data.get_temp::<egui::Rect>(popup_id.with("category_submenu_rect"))
                    .expect("open category submenu"),
            )
        });
        assert!(
            (child_menu.left() - category_row.right() - 12.0).abs() <= 1.0,
            "category-to-submenu gap should match Layer operations: {category_row:?} → {child_menu:?}"
        );
        frame(&mut app, vec![egui::Event::PointerMoved(category_pos)]);
        let (_, submenu) = frame(&mut app, vec![]);
        let visible_names = submenu
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text().starts_with("Application ")
                        && shape.clip_rect.intersects(text.visual_bounding_rect()) =>
                {
                    Some(text.galley.text().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            visible_names.len(),
            10,
            "ten programs must be visible in the category submenu: {visible_names:?}"
        );
        assert!(!visible_names.contains(&"Application 9".to_owned()));
        let choice = submenu
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Application 0" => {
                    Some(text.visual_bounding_rect().center())
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(choice),
                    egui::Event::PointerButton {
                        pos: choice,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            app.application_layout_settings()
                .unwrap()
                .active_layout()
                .unwrap()
                .name,
            "Application 0"
        );

        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(selector_center),
                    egui::Event::PointerButton {
                        pos: selector_center,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let (_, popup) = frame(&mut app, vec![]);
        let configure_label = app_layout_text(app.app_settings.language, "Настроить", "Configure");
        let label_center = |label: &str| {
            popup
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {label}"))
        };
        let configure = label_center(configure_label);
        assert!(configure.y > label_center("Default").y);
        assert!(
            configure.y
                > label_center(app_layout_text(
                    app.app_settings.language,
                    "Другие",
                    "Other",
                ))
                .y
        );
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(configure),
                    egui::Event::PointerButton {
                        pos: configure,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.settings_tab == SettingsTab::ApplicationLayouts);
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
        assert!(!app.application_layout_editor_active);
        assert_eq!(
            app.application_layout_settings()
                .unwrap()
                .active_layout()
                .unwrap()
                .name,
            "Application 0",
        );
    }

    #[test]
    fn application_selector_hover_uses_bottom_hint_instead_of_tooltip() {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.interaction.tooltip_delay = 0.0);
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.language = crate::i18n::Language::English;
        app.application_layout_editor_active = true;
        app.app_settings.application_layouts.insert(
            "offline-macropad".to_owned(),
            crate::application_layouts::DeviceApplicationLayouts::default(),
        );
        app.app_settings.last_application_layout_device_key = Some("offline-macropad".to_owned());
        let selector = egui::pos2(450.0, 146.0);
        let mut output = None;
        for time in [0.0, 1.0, 2.0] {
            output = Some(ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events: vec![egui::Event::PointerMoved(selector)],
                    ..Default::default()
                },
                |ui| app.draw_layout_layer_switcher_and_hints(ui, 6.0, 32.0, 68.0),
            ));
        }
        let output = output.unwrap();
        let hint = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "Select a layout or configure Autolayer" =>
                {
                    Some(text)
                }
                _ => None,
            })
            .expect("Autolayer hint in shared footer");
        assert!((hint.visual_bounding_rect().center().y - (650.0 - 36.0)).abs() < 2.0);
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text)
                if text.galley.text() == "Choose an application layout or open Autolayer settings"
        )));
        let away = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                events: vec![egui::Event::PointerMoved(egui::pos2(50.0, 300.0))],
                ..Default::default()
            },
            |ui| app.draw_layout_layer_switcher_and_hints(ui, 6.0, 32.0, 68.0),
        );
        assert!(!away.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text)
                if text.galley.text() == "Select a layout or configure Autolayer"
        )));
    }

    #[test]
    fn application_selector_groups_known_programs_and_other_layouts() {
        use crate::application_layouts::{
            ApplicationLayoutCategory as Category, DetectedApplication,
        };
        let mut app = EntropyApp::new_inert_for_test();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        for (executable, name) in [
            ("firefox", "Firefox"),
            ("code", "Code"),
            ("blender", "Blender"),
            ("obs", "OBS"),
            ("audacity", "Audacity"),
            ("Discord", "Discord"),
            ("custom-tool", "Custom Tool"),
        ] {
            settings.create_for_application_named(
                &DetectedApplication {
                    executable: executable.to_owned(),
                    ..Default::default()
                },
                Some(name),
                "",
            );
        }
        let key = "offline-macropad-test".to_owned();
        app.app_settings
            .application_layouts
            .insert(key.clone(), settings);
        app.app_settings.last_application_layout_device_key = Some(key);
        let groups =
            app.application_layout_editor_grouped_options(&app.application_layout_editor_options());
        let summary = groups
            .iter()
            .map(|(category, entries)| {
                (
                    category.as_str(),
                    entries
                        .iter()
                        .map(|(_, name)| name.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                (Category::Browsers.id(), vec!["Firefox"]),
                (Category::Development.id(), vec!["Code"]),
                (Category::Graphics.id(), vec!["Blender"]),
                (Category::Video.id(), vec!["OBS"]),
                (Category::Audio.id(), vec!["Audacity"]),
                (Category::Communication.id(), vec!["Discord"]),
                (Category::Other.id(), vec!["Custom Tool"]),
            ]
        );
    }

    #[test]
    fn wheel_magnitude_does_not_skip_layers() {
        assert_eq!(layer_after_wheel(2, 8, -10_000.0), 3);
        assert_eq!(layer_after_wheel(2, 8, 10_000.0), 1);
    }
}
