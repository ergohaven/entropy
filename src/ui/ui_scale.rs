use super::*;

impl EntropyApp {
    pub(super) fn ui_scale_percent(&self) -> i32 {
        (clamp_ui_scale(self.app_settings.ui_scale) * 100.0).round() as i32
    }

    pub(super) fn apply_ui_scale(&mut self, ctx: &egui::Context) {
        let scale = clamp_ui_scale(self.app_settings.ui_scale);
        if (scale - self.app_settings.ui_scale).abs() > f32::EPSILON {
            self.app_settings.ui_scale = scale;
            save_app_settings(&self.app_settings);
        }
        if (ctx.zoom_factor() - scale).abs() > 0.001 {
            ctx.set_zoom_factor(scale);
        }
    }

    pub(super) fn set_ui_scale(&mut self, ctx: &egui::Context, scale: f32) {
        let scale = clamp_ui_scale(scale);
        if (scale - self.app_settings.ui_scale).abs() <= 0.001 {
            return;
        }
        self.app_settings.ui_scale = scale;
        save_app_settings(&self.app_settings);
        // Apply the actual zoom at the start of the next frame. Changing
        // egui's zoom factor while top-bar controls are being painted can move
        // their hitboxes under the active pointer and create visible oscillation.
        ctx.request_repaint();
    }

    pub(super) fn step_ui_scale(&mut self, ctx: &egui::Context, steps: i32) {
        self.set_ui_scale(
            ctx,
            self.app_settings.ui_scale + UI_SCALE_STEP * steps as f32,
        );
    }

    pub(super) fn handle_ui_scale_shortcuts(&mut self, ctx: &egui::Context) {
        let action = ctx.input_mut(|i| {
            if !i.modifiers.ctrl {
                return None;
            }
            let wheel_delta = i.smooth_scroll_delta.y;
            if wheel_delta.abs() > 0.0 {
                i.smooth_scroll_delta = Vec2::ZERO;
                return Some(if wheel_delta > 0.0 { 1 } else { -1 });
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                Some(1)
            } else if i.key_pressed(egui::Key::Minus) {
                Some(-1)
            } else if i.key_pressed(egui::Key::Num0) {
                Some(0)
            } else {
                None
            }
        });

        match action {
            Some(1) => self.step_ui_scale(ctx, 1),
            Some(-1) => self.step_ui_scale(ctx, -1),
            Some(0) => self.set_ui_scale(ctx, default_ui_scale()),
            _ => {}
        }
    }

    pub(super) fn draw_ui_scale_controls(
        &mut self,
        ui: &mut egui::Ui,
        left_top: egui::Pos2,
    ) -> f32 {
        let height = 28.0;
        let minus_w = 24.0;
        let label_w = 52.0;
        let plus_w = 24.0;
        let gap = 4.0;
        let total_w = minus_w + label_w + plus_w + gap * 2.0;
        let text_color = ui.visuals().widgets.inactive.fg_stroke.color;
        let muted = app_muted_text(ui.visuals().dark_mode);
        let hover_fill = app_hover_fill(ui.visuals().dark_mode);
        let font = FontId::proportional(14.0);

        let draw_control =
            |ui: &mut egui::Ui, rect: egui::Rect, label: &str, enabled: bool| -> egui::Response {
                let response = ui.allocate_rect(rect, Sense::CLICK);
                if response.hovered() && enabled {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    ui.painter().rect_filled(rect, 7.0, hover_fill);
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    font.clone(),
                    if enabled { text_color } else { muted },
                );
                response
            };

        let minus_rect = egui::Rect::from_min_size(left_top, Vec2::new(minus_w, height));
        let label_rect = egui::Rect::from_min_size(
            egui::pos2(minus_rect.right() + gap, left_top.y),
            Vec2::new(label_w, height),
        );
        let plus_rect = egui::Rect::from_min_size(
            egui::pos2(label_rect.right() + gap, left_top.y),
            Vec2::new(plus_w, height),
        );

        let can_decrease = self.app_settings.ui_scale > UI_SCALE_MIN + 0.001;
        let can_increase = self.app_settings.ui_scale < UI_SCALE_MAX - 0.001;
        if draw_control(ui, minus_rect, "−", can_decrease).clicked() && can_decrease {
            self.step_ui_scale(ui.ctx(), -1);
        }

        let label_response = ui.allocate_rect(label_rect, Sense::CLICK);
        if label_response.hovered() && self.ui_scale_percent() != 100 {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            ui.painter().rect_filled(label_rect, 7.0, hover_fill);
        }
        if label_response.clicked() {
            self.set_ui_scale(ui.ctx(), default_ui_scale());
        }
        ui.painter().text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}%", self.ui_scale_percent()),
            FontId::proportional(13.0),
            text_color,
        );

        if draw_control(ui, plus_rect, "+", can_increase).clicked() && can_increase {
            self.step_ui_scale(ui.ctx(), 1);
        }

        total_w
    }
}
