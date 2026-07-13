use super::*;

fn select_keyboard_title(lang: crate::i18n::Language, has_error: bool) -> &'static str {
    match (lang, has_error) {
        (crate::i18n::Language::Russian, true) => "Не удалось открыть устройство",
        (crate::i18n::Language::Russian, false) => "Выберите клавиатуру",
        (crate::i18n::Language::English, true) => "Could not open device",
        (crate::i18n::Language::English, false) => "Select keyboard",
    }
}

fn select_keyboard_hint(lang: crate::i18n::Language, has_error: bool) -> &'static str {
    match (lang, has_error) {
        (crate::i18n::Language::Russian, true) => {
            "Выберите другое найденное устройство или переподключите текущее"
        }
        (crate::i18n::Language::Russian, false) => "Выберите одно из найденных устройств",
        (crate::i18n::Language::English, true) => {
            "Choose another detected device or reconnect the current one"
        }
        (crate::i18n::Language::English, false) => "Choose one of the detected devices",
    }
}

impl EntropyApp {
    pub(super) fn clear_connected_keyboard_state(&mut self, status_msg: impl Into<String>) {
        self.layout = None;
        self.selected_key = None;
        self.selected_encoder = None;
        self.selected_layer = 0;
        self.layer_count = 0;
        self.qmk_hid_hosts.clear();
        self.layer_write_task = None;
        self.hid_device = None;
        self.undo_stack.clear();
        self.connect_state = ConnectState::Idle;
        self.unlock_open = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.pending_layout_indicator_open_after_unlock = false;
        self.keycode_picker.open = false;
        self.current_device_name.clear();
        self.current_keyboard_id = None;
        self.current_encoder_visibility_id.clear();
        self.device_about_info = None;
        self.mouse_keys_settings = MouseKeysSettingsState::default();
        self.touchpad_settings = TouchpadSettingsState::default();
        self.bluetooth_settings = BluetoothSettingsState::default();
        self.module_settings = ModuleSettingsState::default();
        self.tap_hold_settings = TapHoldSettingsState::default();
        self.pending_tap_hold_numeric_writes.clear();
        self.tap_hold_numeric_write_due = None;
        self.magic_settings = MagicSettingsState::default();
        self.one_shot_settings = OneShotSettingsState::default();
        self.grave_escape_settings = GraveEscapeSettingsState::default();
        self.layer_led_settings = LayerLedSettingsState::default();
        self.rgb_settings = RgbSettingsState::default();
        self.layout_options_value = None;
        self.matrix_tester_rmk_byte_order = false;
        self.sticky_layout_prev_pressed.clear();
        self.sticky_layout_pressed_key_layers.clear();
        self.sticky_layout_toggled_layers.clear();
        self.sticky_layout_base_layer = 0;
        self.status_msg = status_msg.into();
    }

    pub(super) fn draw_device_selection_empty_state(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
    ) {
        let lang = self.app_settings.language;
        let dark = ui.visuals().dark_mode;
        let devices: Vec<(usize, String)> = self
            .device_manager
            .devices()
            .iter()
            .enumerate()
            .map(|(idx, dev)| {
                let mut label = self
                    .device_display_names
                    .get(&dev.display_name_cache_key())
                    .cloned()
                    .unwrap_or_else(|| dev.name.clone());
                if dev.is_bluetooth_transport() {
                    label.push_str(" (Bluetooth)");
                }
                (idx, label)
            })
            .collect();
        let has_error = !self.status_msg.trim().is_empty();
        let list_rows = devices.len().min(6).max(1);
        let list_height = 12.0 + list_rows as f32 * 30.0;
        let status_height = if has_error { 38.0 } else { 0.0 };
        let panel_width = rect.width().min(520.0);
        let panel_height = 110.0 + status_height + list_height;
        let max_panel_height = (rect.height() - 32.0).max(120.0);
        let panel_rect = egui::Rect::from_center_size(
            rect.center(),
            egui::vec2(panel_width, panel_height.min(max_panel_height)),
        );

        crate::ui_style::allocate_ui_at_rect(ui, panel_rect, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("✦").size(28.0).color(app_accent()));
                ui.add_space(10.0);
                ui.label(
                    RichText::new(select_keyboard_title(lang, has_error))
                        .size(20.0)
                        .strong()
                        .color(if dark {
                            Color32::from_rgb(235, 235, 235)
                        } else {
                            Color32::from_rgb(42, 42, 44)
                        }),
                );
                ui.add_space(7.0);
                ui.label(
                    RichText::new(select_keyboard_hint(lang, has_error))
                        .size(13.0)
                        .color(app_muted_text(dark)),
                );

                if has_error {
                    ui.add_space(8.0);
                    ui.add_sized(
                        egui::vec2(panel_width.min(460.0), 30.0),
                        egui::Label::new(
                            RichText::new(crate::i18n::tr_text(lang, &self.status_msg))
                                .size(12.0)
                                .color(app_muted_text(dark)),
                        )
                        .wrap()
                        .halign(egui::Align::Center),
                    );
                }

                ui.add_space(14.0);
                top_dropdown_frame(dark).show(ui, |ui| {
                    let item_width = panel_width.min(360.0) - 16.0;
                    egui::ScrollArea::vertical()
                        .max_height(list_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(item_width);
                            for (idx, label) in devices {
                                if top_dropdown_item(ui, item_width, &label, true, false).clicked()
                                {
                                    self.selected_device = Some(idx);
                                    self.main_menu_tab = MainMenuTab::Keyboard;
                                    self.start_connect(idx);
                                }
                            }
                        });
                });
            });
        });
    }
}
