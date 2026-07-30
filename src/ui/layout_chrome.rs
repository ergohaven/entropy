use super::*;

impl EntropyApp {
    pub(super) fn draw_layout_chrome(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
        geometry: LayoutChromeGeometry,
    ) -> bool {
        let LayoutChromeGeometry {
            top_base_y,
            main_tabs_h,
            layer_bar_h,
            top_reserved_h,
            viewport,
        } = geometry;
        let typing_trainer_page = self.main_menu_tab == MainMenuTab::Advanced
            && self.settings_tab == SettingsTab::TypingTrainer;
        if typing_trainer_page {
            let now = std::time::Instant::now();
            self.typing_trainer.remaining_secs_at(now);
            if self.typing_trainer.is_finished() {
                self.typing_trainer.ui_hidden = false;
            }
            self.handle_typing_trainer_input(ctx);
            if self.typing_trainer.ui_hidden {
                self.close_top_dropdowns(ctx);
            }
        }
        let chrome_opacity = self.typing_trainer_chrome_opacity(ctx);
        if chrome_opacity > 0.0 && chrome_opacity < 1.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // ── Main menu tabs ────────────────────────────────────────────────
        {
            let chrome_rect = viewport;
            let top_tabs = crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                ui.set_opacity(chrome_opacity);
                if chrome_opacity <= 0.96 {
                    ui.disable();
                }
                self.draw_layout_top_tabs(
                    ui,
                    ctx,
                    chrome_rect,
                    top_base_y,
                    self.unlock_open || self.vial_unlock_polling || chrome_opacity < 0.96,
                )
            })
            .inner;

            if self.unlock_open || self.vial_unlock_polling {
                self.close_top_dropdowns(ctx);
            } else if chrome_opacity > 0.01 {
                crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                    ui.set_opacity(chrome_opacity);
                    if chrome_opacity <= 0.96 {
                        ui.disable();
                    }
                    self.draw_layout_top_dropdowns(
                        ui,
                        layout,
                        ctx,
                        top_tabs.lang,
                        top_tabs.device_tab_rect,
                        top_tabs.device_tab_hovered && chrome_opacity > 0.96,
                        top_tabs.advanced_tab_rect,
                        top_tabs.advanced_tab_hovered && chrome_opacity > 0.96,
                        top_tabs.settings_tab_rect,
                        top_tabs.settings_tab_hovered && chrome_opacity > 0.96,
                    );
                });
            }
            if matches!(
                self.main_menu_tab,
                MainMenuTab::Settings | MainMenuTab::Advanced
            ) {
                self.draw_settings_screen(
                    ui,
                    layout,
                    ctx,
                    chrome_rect.top() + top_reserved_h,
                    chrome_rect,
                );
                return true;
            }

            crate::ui_style::allocate_ui_at_rect(ui, chrome_rect, |ui| {
                ui.set_opacity(chrome_opacity);
                if chrome_opacity <= 0.96 {
                    ui.disable();
                }
                self.draw_layout_layer_switcher_and_hints(ui, top_base_y, main_tabs_h, layer_bar_h);
            });
        }
        false
    }
}
