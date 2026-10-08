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
        if self.settings_tab == SettingsTab::Encoders
            && !self.show_separate_encoder_visibility_settings(layout)
        {
            self.settings_tab = SettingsTab::AppSettings;
        }

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

        #[cfg(not(target_arch = "wasm32"))]
        if self.draw_deferred_settings_gate(ui, content_rect) {
            self.draw_settings_navigation_hint(ui, stable_hint_center_x, stable_hint_bottom, false);
            return;
        }

        let combo_keycap_hovered = match self.settings_tab {
            SettingsTab::AppSettings => {
                self.draw_app_settings_page(ui, content_rect);
                false
            }
            SettingsTab::ApplicationLayouts => {
                self.draw_application_layouts_settings_page(ui, content_rect);
                false
            }
            SettingsTab::MatrixTester => {
                self.draw_matrix_tester_settings(ui, layout, content_rect, dark);
                false
            }
            SettingsTab::TextExpanderSetup => {
                self.draw_text_expander_setup_page(ui, content_rect);
                false
            }
            SettingsTab::TextExpander => {
                self.draw_text_expander_settings_page(ui, content_rect);
                false
            }
            SettingsTab::TypingTrainer => {
                self.draw_typing_trainer_page(ui, layout, ctx, content_rect);
                false
            }
            SettingsTab::Macros => {
                self.draw_macro_settings_page(ui, content_rect);
                false
            }
            SettingsTab::TapDance => {
                self.draw_tap_dance_settings_page(ui, content_rect);
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
            SettingsTab::Display => {
                self.draw_display_settings_page(ui, content_rect);
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

    pub(super) fn open_application_layouts_page(&mut self) {
        self.application_layout_editor_active = false;
        self.settings_tab = SettingsTab::ApplicationLayouts;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_text_expander_setup_page(&mut self) {
        self.settings_tab = SettingsTab::TextExpanderSetup;
        self.main_menu_tab = MainMenuTab::Advanced;
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

    pub(super) fn open_macro_settings_page(&mut self) {
        self.settings_tab = SettingsTab::Macros;
        self.main_menu_tab = MainMenuTab::Advanced;
        let selected = self.keycode_picker.macro_inline_selected.unwrap_or(0).min(
            self.keycode_picker
                .macro_count
                .saturating_sub(1)
                .min(u8::MAX as usize) as u8,
        );
        self.keycode_picker.macro_inline_selected = Some(selected);
        if self.is_vial_locked() {
            self.unlock_open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.keyboard_locked_edit_macros",
            )
            .into();
        }
    }

    pub(super) fn open_tap_dance_settings_page(&mut self) {
        self.settings_tab = SettingsTab::TapDance;
        self.main_menu_tab = MainMenuTab::Advanced;
        let selected = self.keycode_picker.tap_dance_editor_open.unwrap_or(0).min(
            self.keycode_picker
                .tap_dance_entries
                .len()
                .saturating_sub(1)
                .min(u8::MAX as usize) as u8,
        );
        self.keycode_picker.tap_dance_editor_open = Some(selected);
        if self.is_vial_locked() {
            self.unlock_open = true;
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "connection.keyboard_locked_edit_tap_dance",
            )
            .into();
        }
    }

    pub(super) fn open_auto_shift_settings_page(&mut self) {
        self.settings_tab = SettingsTab::AutoShift;
        self.main_menu_tab = MainMenuTab::Advanced;
        if self.is_vial_locked() {
            self.unlock_open = true;
            self.status_msg = format!(
                "{} — {}",
                crate::i18n::tr(self.app_settings.language, crate::i18n::Key::KeyboardLocked),
                crate::i18n::tr(
                    self.app_settings.language,
                    crate::i18n::Key::AutoShiftUnlockHint
                ),
            );
        }
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
            && self.application_layout_rename_target_id.is_none()
            && self.editing_layer.is_none()
            && !self.keycode_picker.has_open_modal()
            && !self.unlock_open
            && !self.vial_unlock_polling
            && !modal_or_popup_open_at_frame_start
            && !keyboard_input_wanted_at_frame_start
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx)
            && !self.top_dropdown_open(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vial_app(locked: bool) -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.firmware = FirmwareProtocol::Vial;
        app.layout = Some(
            KeyboardLayout::from_vial_json(&serde_json::json!({
                "name": "Test keyboard",
                "matrix": { "rows": 1, "cols": 1 },
                "layouts": { "keymap": [["0,0"]] }
            }))
            .unwrap(),
        );
        app.vial_unlocked = Some(!locked);
        app
    }

    #[test]
    fn locked_vial_feature_navigation_opens_unlock_preflight() {
        let mut app = vial_app(true);

        app.open_macro_settings_page();
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
        assert!(app.settings_tab == SettingsTab::Macros);
        assert!(app.unlock_open);
        assert!(!app.vial_unlock_session_started);
        app.dismiss_vial_unlock_preflight();
        assert!(!app.unlock_open);

        app.open_tap_dance_settings_page();
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
        assert!(app.settings_tab == SettingsTab::TapDance);
        assert!(app.unlock_open);
        assert!(!app.vial_unlock_session_started);
    }

    #[test]
    fn unlocked_vial_feature_navigation_does_not_prompt() {
        let mut app = vial_app(false);
        app.open_macro_settings_page();
        assert!(app.settings_tab == SettingsTab::Macros);
        assert!(!app.unlock_open);
        app.open_tap_dance_settings_page();
        assert!(app.settings_tab == SettingsTab::TapDance);
        assert!(!app.unlock_open);
        app.open_auto_shift_settings_page();
        assert!(app.settings_tab == SettingsTab::AutoShift);
        assert!(!app.unlock_open);
    }

    #[test]
    fn locked_vial_feature_page_stays_selected_during_layout_draw() {
        for (tab, menu) in [
            (SettingsTab::MatrixTester, MainMenuTab::Settings),
            (SettingsTab::Macros, MainMenuTab::Advanced),
            (SettingsTab::TapDance, MainMenuTab::Advanced),
            (SettingsTab::AutoShift, MainMenuTab::Advanced),
        ] {
            let ctx = egui::Context::default();
            let mut app = vial_app(false);
            app.settings_tab = tab;
            app.main_menu_tab = menu;
            app.vial_unlocked = Some(false);
            let layout = app.layout.as_ref().unwrap().clone();

            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                let ctx = ui.ctx().clone();
                app.draw_layout(ui, &layout, &ctx);
            });

            assert!(app.main_menu_tab == menu);
            assert!(app.settings_tab == tab);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn locked_auto_shift_navigation_prompts_again_only_on_click() {
        let ctx = egui::Context::default();
        let mut app = vial_app(true);
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.auto_shift_timeout = Some(175);

        app.open_auto_shift_settings_page();
        assert!(app.settings_tab == SettingsTab::AutoShift);
        assert!(app.main_menu_tab == MainMenuTab::Advanced);
        assert!(app.unlock_open);
        app.dismiss_vial_unlock_preflight();

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw_auto_shift_settings_page(
                ui,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0)),
                true,
            );
        });
        assert!(!app.unlock_open);
        assert!(recorder.requests().is_empty());

        app.open_auto_shift_settings_page();
        assert!(app.unlock_open);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn locked_vial_matrix_tester_prompts_without_polling() {
        let ctx = egui::Context::default();
        let mut app = vial_app(true);
        let (hid_device, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid_device);
        app.settings_tab = SettingsTab::MatrixTester;
        app.main_menu_tab = MainMenuTab::Settings;
        app.reset_matrix_tester_state();
        let layout = app.layout.as_ref().unwrap().clone();

        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            app.draw_layout(ui, &layout, &ctx);
        });

        assert!(app.main_menu_tab == MainMenuTab::Settings);
        assert!(app.unlock_open);
        assert!(!app.vial_unlock_session_started);
        assert!(recorder.requests().is_empty());
    }
}
