use super::*;

impl EntropyApp {
    pub(super) fn draw_settings_screen(
        &mut self,
        ui: &mut egui::Ui,
        layout: &KeyboardLayout,
        ctx: &egui::Context,
        content_top: f32,
        viewport: egui::Rect,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        if self.settings_tab == SettingsTab::MatrixTester {
            self.poll_matrix_tester(ctx, layout);
        }

        if self.settings_tab == SettingsTab::MatrixTester {
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }

        let dark = ui.visuals().dark_mode;
        let stable_rect = viewport;
        let stable_hint_center_x = stable_rect.center().x;
        let stable_hint_bottom = stable_rect.bottom();
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(stable_rect.left() + 20.0, content_top),
            egui::pos2(stable_rect.right() - 20.0, stable_hint_bottom - 76.0),
        );

        let combo_keycap_hovered = match self.settings_tab {
            SettingsTab::AppSettings => {
                self.draw_app_settings_page(ui, content_rect);
                false
            }
            SettingsTab::MatrixTester => {
                self.draw_matrix_tester_settings(ui, layout, content_rect, dark);
                false
            }
            SettingsTab::UniversalSymbolsSetup => {
                self.draw_universal_symbols_setup_page(ui, content_rect);
                false
            }
            SettingsTab::TextExpander => {
                self.draw_text_expander_settings_page(ui, content_rect);
                false
            }
            SettingsTab::TypingTrainer => {
                self.draw_typing_trainer_page(ui, ctx, content_rect);
                false
            }
            SettingsTab::AutoShift => {
                self.draw_auto_shift_settings_page(ui, content_rect, dark);
                false
            }
            SettingsTab::Rgb => {
                self.draw_rgb_settings_page(ui, content_rect, dark);
                false
            }
            SettingsTab::LayerLeds => {
                self.draw_layer_led_settings_page(ui, content_rect);
                false
            }
            SettingsTab::Encoders => {
                self.draw_encoder_visibility_settings_page(ui, content_rect, dark);
                false
            }
            SettingsTab::TapHold => {
                self.draw_tap_hold_settings_page(ui, content_rect);
                false
            }
            SettingsTab::Magic => {
                self.draw_magic_settings_page(ui, content_rect);
                false
            }
            SettingsTab::GraveEscape => {
                self.draw_grave_escape_settings_page(ui, content_rect);
                false
            }
            SettingsTab::LayoutOptions => {
                self.draw_layout_options_settings_page(ui, content_rect);
                false
            }
            SettingsTab::Modules => {
                self.draw_module_settings_page(ui, content_rect);
                false
            }
            SettingsTab::Touchpad => {
                self.draw_touchpad_settings_page(ui, content_rect);
                false
            }
            SettingsTab::Bluetooth => {
                self.draw_bluetooth_settings_page(ui, content_rect);
                false
            }
            SettingsTab::LiveFeatures => {
                self.draw_live_features_settings_page(ui, content_rect);
                false
            }
            SettingsTab::AboutDevice => {
                self.draw_about_device_page(ui, content_rect);
                false
            }
            SettingsTab::AboutEntropy => {
                self.draw_about_entropy_page(ui, content_rect);
                false
            }
            SettingsTab::Combo => self.draw_combo_settings_page(ui, ctx, content_rect),
            SettingsTab::KeyOverrides => {
                self.draw_key_override_settings_page(ui, content_rect);
                false
            }
            SettingsTab::AltRepeat => {
                self.draw_alt_repeat_settings_page(ui, content_rect);
                false
            }
            SettingsTab::MouseKeys => {
                self.draw_mouse_keys_settings_page(ui, content_rect);
                false
            }
            SettingsTab::LayoutImageExport => {
                self.draw_layout_image_export_page(ui, layout, content_rect);
                false
            }
        };

        if self.settings_tab != SettingsTab::MatrixTester
            && self.settings_tab != SettingsTab::TypingTrainer
        {
            self.draw_settings_navigation_hint(
                ui,
                stable_hint_center_x,
                stable_hint_bottom,
                combo_keycap_hovered,
            );
        }
    }

    pub(super) fn draw_settings_navigation_hint(
        &self,
        ui: &mut egui::Ui,
        center_x: f32,
        bottom: f32,
        combo_keycap_hovered: bool,
    ) {
        let hint_color = if ui.visuals().dark_mode {
            Color32::from_gray(100)
        } else {
            Color32::from_gray(160)
        };
        let hint_font = FontId::proportional(12.0);
        if combo_keycap_hovered {
            ui.painter().text(
                egui::pos2(center_x, bottom - 52.0),
                egui::Align2::CENTER_CENTER,
                crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "combo_editor.keycap_mouse_hint",
                ),
                hint_font.clone(),
                hint_color,
            );
        }
        ui.painter().text(
            egui::pos2(center_x, bottom - 36.0),
            egui::Align2::CENTER_CENTER,
            crate::i18n::tr_catalog(
                self.app_settings.language,
                "navigation.return_to_layout_hint",
            ),
            hint_font,
            hint_color,
        );
    }

    pub(super) fn open_mouse_keys_settings_page(&mut self) {
        self.settings_tab = SettingsTab::MouseKeys;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_app_settings_page(&mut self) {
        self.settings_tab = SettingsTab::AppSettings;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_universal_symbols_setup_page(&mut self) {
        self.settings_tab = SettingsTab::UniversalSymbolsSetup;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_about_device_page(&mut self) {
        self.settings_tab = SettingsTab::AboutDevice;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_about_entropy_page(&mut self) {
        self.settings_tab = SettingsTab::AboutEntropy;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_text_expander_settings_page(&mut self) {
        self.settings_tab = SettingsTab::TextExpander;
        self.main_menu_tab = MainMenuTab::Advanced;
    }

    pub(super) fn open_typing_trainer_page(&mut self) {
        self.settings_tab = SettingsTab::TypingTrainer;
        self.main_menu_tab = MainMenuTab::Advanced;
    }

    pub(super) fn open_layer_led_settings_page(&mut self) {
        self.settings_tab = SettingsTab::LayerLeds;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_tap_hold_settings_page(&mut self) {
        self.settings_tab = SettingsTab::TapHold;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_magic_settings_page(&mut self) {
        self.settings_tab = SettingsTab::Magic;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_grave_escape_settings_page(&mut self) {
        self.settings_tab = SettingsTab::GraveEscape;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn can_return_from_settings_page(
        &self,
        ctx: &egui::Context,
        modal_or_popup_open_at_frame_start: bool,
        keyboard_input_wanted_at_frame_start: bool,
    ) -> bool {
        matches!(
            self.main_menu_tab,
            MainMenuTab::Settings | MainMenuTab::Advanced
        ) && self.settings_tab != SettingsTab::MatrixTester
            && self.settings_tab != SettingsTab::TypingTrainer
            && !self.secondary_click_handled
            && !self.keycode_picker.open
            && !self.unlock_open
            && !self.vial_unlock_polling
            && !modal_or_popup_open_at_frame_start
            && !keyboard_input_wanted_at_frame_start
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx)
            && !self.top_dropdown_open(ctx)
    }
}
