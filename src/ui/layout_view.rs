use super::*;

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn draw_bluetooth_reconnect_status(&mut self, ctx: &egui::Context) {
        let Some(device_name) = self.bluetooth_reconnect_display_name().map(str::to_owned) else {
            return;
        };
        let lang = self.app_settings.language;
        let text = crate::i18n::tr_catalog_format(
            lang,
            "connection.reconnecting",
            &[("device", &device_name)],
        );

        egui::Area::new(egui::Id::new("bluetooth_reconnect_status"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -22.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                top_dropdown_frame(self.dark_mode).show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add(
                            egui::Spinner::new()
                                .size(16.0)
                                .color(app_muted_text(self.dark_mode)),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(text)
                                .size(13.0)
                                .color(app_muted_text(self.dark_mode)),
                        );
                        ui.add_space(8.0);
                        if crate::ui_style::modern_button(
                            ui,
                            crate::i18n::tr_catalog(lang, "connection.choose_another_device"),
                            egui::vec2(132.0, 30.0),
                            true,
                        )
                        .clicked()
                        {
                            self.cancel_bluetooth_reconnect_for_device_selection();
                            ctx.request_repaint();
                        }
                    });
                });
            });
    }

    pub(super) fn draw_layout(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
    ) {
        let viewport = ui.max_rect();
        let avail = viewport.size();
        let layout_top_reserved_h = LAYOUT_TOP_RESERVED_H
            + if self.main_menu_has_battery_status() {
                layout_layer_switcher::MAIN_MENU_BATTERY_RESERVED_H
            } else {
                0.0
            };
        let geometry = layout_geometry_with_reserved_and_filter(
            ui.ctx(),
            layout,
            viewport,
            clamp_ui_scale(self.app_settings.ui_scale),
            layout_top_reserved_h,
            LAYOUT_BOTTOM_RESERVED_H,
            LAYOUT_FIT_MARGIN,
            None,
            |_| true,
            |encoder| {
                Self::module_settings_encoder_visible(
                    &self.module_settings,
                    layout,
                    encoder.encoder_idx,
                )
            },
        );
        let offset_x = geometry.offset_x;
        let offset_y = geometry.offset_y;
        let unit = geometry.unit;
        let padding = geometry.padding;
        let layout_h = geometry.layout_h;
        let main_tabs_h = 32.0_f32;
        let layer_bar_h = 68.0_f32;
        let top_reserved_h = LAYOUT_TOP_RESERVED_H;
        let top_base_y = viewport.top() + 6.0;
        self.last_layout_geometry = Some((offset_x, offset_y, unit, padding));

        if self.draw_layout_chrome(
            ui,
            layout,
            ctx,
            top_base_y,
            main_tabs_h,
            layer_bar_h,
            top_reserved_h,
            viewport,
        ) {
            return;
        }

        self.draw_layout_keyboard_canvas(
            ui, layout, ctx, avail, offset_x, offset_y, unit, padding, layout_h,
        );
    }

    pub(super) fn draw_placeholder(&mut self, ui: &mut egui::Ui) {
        let key_w = 52.0_f32;
        let key_h = 52.0_f32;
        let gap = 6.0_f32;
        let total_w = 6.0 * (key_w + gap);
        let start_x = (ui.available_width() - total_w * 2.0 - 40.0) / 2.0;
        let start_y = ui.min_rect().top() + 40.0;

        let mut keys: Vec<(usize, egui::Rect, egui::Response)> = vec![];
        for half in 0..2_usize {
            let half_offset = if half == 0 { 0.0 } else { total_w + 40.0 };
            for row in 0..4_usize {
                for col in 0..6_usize {
                    let key_idx = half * 24 + row * 6 + col;
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(
                            start_x + half_offset + col as f32 * (key_w + gap),
                            start_y + row as f32 * (key_h + gap),
                        ),
                        Vec2::new(key_w, key_h),
                    );
                    let response = ui.allocate_rect(rect, Sense::click());
                    keys.push((key_idx, rect, response));
                }
            }
        }

        for (key_idx, _, response) in &keys {
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                self.selected_key = Some((self.selected_layer, *key_idx));
            }
        }

        let painter = ui.painter();
        for (key_idx, rect, _) in &keys {
            let is_selected = self.selected_key == Some((self.selected_layer, *key_idx));
            let bg = if is_selected {
                Color32::from_rgb(70, 110, 190)
            } else {
                Color32::from_gray(45)
            };
            painter.rect(
                *rect,
                6.0,
                bg,
                Stroke::new(1.0_f32, Color32::from_gray(80)),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("K{key_idx}"),
                FontId::proportional(11.0),
                Color32::WHITE,
            );
        }
    }
}
