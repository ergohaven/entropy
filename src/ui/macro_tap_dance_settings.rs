use super::*;

const ADVANCED_EDITOR_MAX_WIDTH: f32 = 900.0;

impl EntropyApp {
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
                let selected = app.keycode_picker.show_macro_settings_editor(
                    ui,
                    selected,
                    "macro_grid_settings",
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
        let page_width = (ADVANCED_EDITOR_MAX_WIDTH * scale).min(content_rect.width());
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
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("advanced_action_editor", title))
                            .max_height(editor_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| draw_editor(self, ui));
                    },
                );
            });
        });
    }
}
