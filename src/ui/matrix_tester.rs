#[cfg(not(target_arch = "wasm32"))]
use super::vial_hid_task::VialHidTaskStart;
use super::*;

impl EntropyApp {
    fn matrix_tester_uses_bluetooth_transport(&self) -> bool {
        self.selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(|device| device.is_bluetooth_transport())
            .unwrap_or(false)
    }

    pub(super) fn matrix_tester_poll_interval(&self) -> std::time::Duration {
        matrix_tester_poll_interval_for_target(
            self.matrix_tester_uses_bluetooth_transport(),
            cfg!(target_os = "macos"),
        )
    }

    pub(super) fn reset_matrix_tester_state(&mut self) {
        self.matrix_tester_pressed.clear();
        self.matrix_tester_ever_pressed.clear();
        self.sticky_layout_prev_pressed.clear();
        self.sticky_layout_pressed_key_layers.clear();
        self.sticky_layout_toggled_layers.clear();
        self.sticky_layout_active_combos.clear();
        self.sticky_layout_tap_dance_states.clear();
        self.sticky_layout_base_layer = 0;
        self.sticky_layout_active_layer = 0;
        self.matrix_tester_last_poll = std::time::Instant::now() - MATRIX_TESTER_POLL_INTERVAL;
        self.matrix_tester_last_lock_check =
            std::time::Instant::now() - MATRIX_TESTER_LOCK_CHECK_INTERVAL;
        self.matrix_tester_unlock_prompted = false;
        self.matrix_tester_lock_checked = false;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn prompt_if_vial_locked_for_matrix_poll(&mut self) {
        if self.firmware != FirmwareProtocol::Vial
            || self.layout.is_none()
            || self.hid_device.is_none()
            || self.unlock_open
            || self.vial_unlock_polling
            || self.matrix_tester_unlock_prompted
        {
            return;
        }

        let now = std::time::Instant::now();
        if self.matrix_tester_lock_checked
            && now.duration_since(self.matrix_tester_last_lock_check)
                < MATRIX_TESTER_LOCK_CHECK_INTERVAL
        {
            return;
        }

        self.matrix_tester_lock_checked = true;
        self.matrix_tester_last_lock_check = now;
        if self.is_vial_locked() {
            self.unlock_open = true;
            self.matrix_tester_unlock_prompted = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "matrix_tester.keyboard_is_locked_unlock_it_to_use_matrix_tester",
            )
            .into();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_switch_matrix_state(
        &mut self,
        ctx: &egui::Context,
        rows: usize,
        cols: usize,
        remember_ever_pressed: bool,
    ) {
        if self.firmware != FirmwareProtocol::Vial {
            return;
        }
        if self.unlock_open || self.vial_unlock_polling {
            return;
        }
        if self.is_vial_locked() {
            return;
        }
        let now = std::time::Instant::now();
        let poll_interval = self.matrix_tester_poll_interval();

        if now.duration_since(self.matrix_tester_last_poll) >= poll_interval {
            match self.start_vial_matrix_poll(ctx, rows, cols, remember_ever_pressed) {
                VialHidTaskStart::Started => {
                    self.matrix_tester_last_poll = now;
                }
                VialHidTaskStart::Busy | VialHidTaskStart::NoDevice => {}
            }
        }
        ctx.request_repaint_after(poll_interval);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn finish_matrix_tester_poll(
        &mut self,
        pressed: Vec<bool>,
        remember_ever_pressed: bool,
    ) {
        if remember_ever_pressed {
            if self.matrix_tester_ever_pressed.len() != pressed.len() {
                self.matrix_tester_ever_pressed = vec![false; pressed.len()];
            }
            for (idx, &is_pressed) in pressed.iter().enumerate() {
                if is_pressed {
                    if let Some(seen) = self.matrix_tester_ever_pressed.get_mut(idx) {
                        *seen = true;
                    }
                }
            }
        }
        self.matrix_tester_pressed = pressed;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn fail_matrix_tester_poll(&mut self, error: String) {
        log::warn!("Matrix poll error: {error}");
        self.matrix_tester_lock_checked = false;
        self.matrix_tester_last_lock_check =
            std::time::Instant::now() - MATRIX_TESTER_LOCK_CHECK_INTERVAL;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_matrix_tester(&mut self, ctx: &egui::Context, layout: &KeyboardLayout) {
        if self.main_menu_tab != MainMenuTab::Settings || self.top_dropdown_open(ctx) {
            return;
        }
        self.poll_switch_matrix_state(ctx, layout.rows, layout.cols, true);
    }

    pub(super) fn draw_matrix_tester_settings(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        content_rect: egui::Rect,
        dark: bool,
    ) {
        let title_y = content_rect.top() + 30.0;
        let desc_y = title_y + 28.0;
        let status_y = desc_y + 30.0;
        let supported = self.firmware == FirmwareProtocol::Vial;
        let hid_ready = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.hid_device.is_some() || self.vial_hid_task_active()
            }
            #[cfg(target_arch = "wasm32")]
            {
                false
            }
        };

        if supported && hid_ready {
            #[cfg(not(target_arch = "wasm32"))]
            self.prompt_if_vial_locked_for_matrix_poll();
        }

        let total_keys = layout.keys.len();
        let tested_count = layout
            .keys
            .iter()
            .filter(|key| {
                let idx = key.row as usize * layout.cols + key.col as usize;
                self.matrix_tester_ever_pressed
                    .get(idx)
                    .copied()
                    .unwrap_or(false)
            })
            .count();

        crate::ui_style::allocate_ui_at_rect(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(content_rect.left(), content_rect.top()),
                egui::pos2(content_rect.right(), desc_y + 10.0),
            ),
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new(crate::i18n::tr(
                            self.app_settings.language,
                            crate::i18n::Key::MatrixTesterTitle,
                        ))
                        .size(18.0)
                        .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(crate::i18n::tr(
                            self.app_settings.language,
                            crate::i18n::Key::MatrixTesterDescription,
                        ))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                    );
                });
            },
        );

        let painter = ui.painter().clone();
        let complete = tested_count == total_keys && total_keys > 0;
        let status_prefix =
            crate::i18n::tr_catalog(self.app_settings.language, "matrix_tester.tested");
        let status_text = format!("{status_prefix}: {tested_count}/{total_keys}");
        let status_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, status_y),
            Vec2::new(132.0, 30.0),
        );
        let status_resp = ui.interact(
            status_rect,
            ui.id().with("matrix_tester_status_reset"),
            egui::Sense::click(),
        );
        if status_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if status_resp.clicked() {
            self.reset_matrix_tester_state();
        }
        let status_hovered = status_resp.hovered();
        status_resp.on_hover_text(crate::i18n::tr_catalog(
            self.app_settings.language,
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "matrix_tester.click_to_reset_matrix_tester",
            ),
        ));
        painter.rect(
            status_rect,
            9.0,
            if status_hovered {
                crate::ui_style::hover_fill(dark)
            } else {
                app_surface_fill(dark)
            },
            crate::ui_style::modal_outline_stroke(dark),
            egui::StrokeKind::Inside,
        );
        painter.text(
            status_rect.center(),
            egui::Align2::CENTER_CENTER,
            status_text,
            FontId::proportional(13.0),
            if complete {
                app_accent()
            } else {
                app_muted_text(dark)
            },
        );
        let idle_fill = if dark {
            Color32::from_rgb(34, 34, 38)
        } else {
            Color32::from_rgb(252, 252, 254)
        };
        let tested_fill = crate::ui_style::hover_fill(dark);

        let board_top = content_rect.top() + 104.0;
        let hint_y = ui.max_rect().bottom() - 36.0;
        let board_rect = egui::Rect::from_min_max(
            egui::pos2(content_rect.left(), board_top),
            egui::pos2(content_rect.right(), hint_y - 22.0),
        );

        if !supported {
            painter.text(
                board_rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "matrix_tester.matrix_tester_is_currently_available_only_for_vial_keyboards",
                ),
                FontId::proportional(15.0),
                app_muted_text(dark),
            );
            return;
        }

        if !hid_ready {
            painter.text(
                board_rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "matrix_tester.connect_a_vial_keyboard_to_start_live_switch_testing",
                ),
                FontId::proportional(15.0),
                app_muted_text(dark),
            );
            return;
        }

        // The header, the status button and the bottom hint are laid out from
        // `content_rect`, which already sits below the top chrome. Fit the board
        // into the rectangle left between them instead of the whole panel, so the
        // caps never cover the header or the "Tested" button.
        let geometry = layout_geometry_with_reserved(
            ui.ctx(),
            layout,
            board_rect,
            clamp_ui_scale(self.app_settings.ui_scale),
            0.0,
            0.0,
            LAYOUT_FIT_MARGIN,
            None,
        );

        let hint_color = if dark {
            Color32::from_gray(100)
        } else {
            Color32::from_gray(160)
        };
        painter.text(
            egui::pos2(content_rect.center().x, hint_y),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "matrix_tester.click_tested_to_reset_progress",
            ),
            FontId::proportional(11.0),
            hint_color,
        );

        for key in &layout.keys {
            let matrix_idx = key.row as usize * layout.cols + key.col as usize;
            let is_pressed = self
                .matrix_tester_pressed
                .get(matrix_idx)
                .copied()
                .unwrap_or(false);
            let was_pressed = self
                .matrix_tester_ever_pressed
                .get(matrix_idx)
                .copied()
                .unwrap_or(false);
            let rect = layout_physical_key_rect(key, geometry);

            let fill = if is_pressed {
                app_accent()
            } else if was_pressed {
                tested_fill
            } else {
                idle_fill
            };
            let stroke = if is_pressed || was_pressed {
                app_accent()
            } else {
                app_border_color(dark)
            };
            paint_layout_keycap(
                &painter,
                rect,
                key.rotation,
                fill,
                Stroke::new(1.0_f32, stroke),
            );
        }
    }
}

fn matrix_tester_poll_interval_for_target(
    bluetooth: bool,
    target_is_macos: bool,
) -> std::time::Duration {
    if bluetooth && !target_is_macos {
        MATRIX_TESTER_BLUETOOTH_POLL_INTERVAL
    } else {
        // macOS already keeps a 16 ms visible Bluetooth repaint cadence and
        // serializes Vial HID operations. Avoid holding fast BLE matrix
        // round-trips to an 80 ms request cadence.
        MATRIX_TESTER_POLL_INTERVAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_bluetooth_matrix_polling_keeps_realtime_cadence() {
        assert_eq!(
            matrix_tester_poll_interval_for_target(true, true),
            std::time::Duration::from_millis(16)
        );
    }

    #[test]
    fn other_bluetooth_matrix_polling_keeps_paced_cadence() {
        assert_eq!(
            matrix_tester_poll_interval_for_target(true, false),
            std::time::Duration::from_millis(80)
        );
    }

    #[test]
    fn usb_matrix_polling_keeps_realtime_cadence() {
        assert_eq!(
            matrix_tester_poll_interval_for_target(false, false),
            std::time::Duration::from_millis(16)
        );
        assert_eq!(
            matrix_tester_poll_interval_for_target(false, true),
            std::time::Duration::from_millis(16)
        );
    }
}
