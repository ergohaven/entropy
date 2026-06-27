use super::*;

impl EntropyApp {
    pub(super) fn draw_macro_settings_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        ui.allocate_ui_at_rect(content_rect, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.keycode_picker.show_macro_settings_page(ui);
                });
        });
    }
}
