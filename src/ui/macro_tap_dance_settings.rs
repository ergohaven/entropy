use super::*;

impl EntropyApp {
    /// `(used, capacity)` in bytes of the keyboard's macro memory, which all
    /// macros share; `None` when the board did not report its size. The editor
    /// counter and the write guard both read this, so they cannot disagree.
    pub(super) fn macro_memory_use(&self) -> Option<(usize, usize)> {
        let capacity = self.device_about_info.as_ref()?.macro_memory_bytes?;
        let macros = &self.keycode_picker.macro_texts;
        // A Rynk board's pool holds ops, not Vial's bytes.
        let used = if self.rynk_board_connected() {
            crate::rynk::macros::vial_macros_space_used(macros, self.keycode_picker.macro_count)
        } else {
            crate::hid::macro_buffer_len(macros)
        };
        Some((used, usize::from(capacity)))
    }

    pub(super) fn draw_macro_settings_page(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect) {
        let lang = self.app_settings.language;
        self.keycode_picker.language = lang;
        self.keycode_picker.key_legend_layout = self.app_settings.key_legend_layout;
        self.draw_advanced_action_editor_page(
            ui,
            content_rect,
            crate::i18n::tr_catalog(lang, "macro_editor.title"),
            crate::i18n::tr_catalog(lang, "macro_editor.description"),
            |app, ui| {
                let selected = app.keycode_picker.macro_inline_selected.unwrap_or(0);
                let memory_use = app.macro_memory_use();
                let selected = app.keycode_picker.show_macro_settings_editor(
                    ui,
                    selected,
                    "macro_grid_settings",
                    memory_use,
                );
                app.keycode_picker.macro_inline_selected = Some(selected);
            },
        );
    }

    pub(super) fn draw_tap_dance_settings_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        self.keycode_picker.language = lang;
        self.keycode_picker.key_legend_layout = self.app_settings.key_legend_layout;
        self.draw_advanced_action_editor_page(
            ui,
            content_rect,
            crate::i18n::tr_catalog(lang, "tap_dance_editor.title"),
            crate::i18n::tr_catalog(lang, "tap_dance_editor.description"),
            |app, ui| app.keycode_picker.show_tap_dance_settings_editor(ui),
        );
    }

    fn draw_advanced_action_editor_page(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        title: &str,
        description: &str,
        draw_editor: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        let dark = ui.visuals().dark_mode;
        let scale = responsive_settings_editor_scale(ui.ctx());
        let page_width = crate::ui_style::ResponsiveMetrics::from_ctx(ui.ctx())
            .settings_content_width()
            .min(content_rect.width());
        crate::ui_style::allocate_ui_at_rect(ui, content_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(18.0 * scale);
                ui.label(RichText::new(title).size(18.0 * scale).strong());
                ui.add_space(6.0 * scale);
                ui.label(
                    RichText::new(description)
                        .size(13.0 * scale)
                        .color(app_muted_text(dark)),
                );
                ui.add_space(18.0 * scale);
                let editor_height = (content_rect.height() - 92.0 * scale).max(260.0 * scale);
                ui.allocate_ui_with_layout(
                    egui::vec2(page_width, editor_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| draw_editor(self, ui),
                );
            });
        });
    }
}
