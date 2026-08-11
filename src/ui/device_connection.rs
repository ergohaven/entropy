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

fn device_selection_list_size(
    panel_width: f32,
    adaptive_width: f32,
    device_count: usize,
) -> egui::Vec2 {
    let visible_rows = device_count.clamp(1, 6);
    egui::vec2(
        panel_width.min(adaptive_width),
        12.0 + visible_rows as f32 * 30.0,
    )
}

fn device_selection_needs_scroll(device_count: usize) -> bool {
    device_count > 6
}

impl EntropyApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn bluetooth_reconnect_active(&self) -> bool {
        matches!(
            self.connect_state,
            ConnectState::Reconnecting(_)
                | ConnectState::Loading {
                    reconnect: Some(_),
                    ..
                }
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn bluetooth_reconnect_display_name(&self) -> Option<&str> {
        match &self.connect_state {
            ConnectState::Reconnecting(state)
            | ConnectState::Loading {
                reconnect: Some(state),
                ..
            } => Some(&state.display_name),
            ConnectState::Idle
            | ConnectState::SelectingDevice
            | ConnectState::Loading {
                reconnect: None, ..
            } => None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn begin_bluetooth_reconnect(&mut self, transport_error: impl Into<String>) -> bool {
        if self.bluetooth_reconnect_active() {
            return false;
        }
        let Some(device) = self
            .selected_device
            .and_then(|index| self.device_manager.devices().get(index))
            .filter(|device| device.is_bluetooth_transport())
            .cloned()
        else {
            return false;
        };
        if self.hid_write_task_owner_active() {
            return false;
        }

        let transport_error = transport_error.into();
        log::warn!(
            "Bluetooth HID connection unavailable for {}: {}",
            device.name,
            transport_error
        );
        let base_name = if self.current_device_name.trim().is_empty() {
            device.name.as_str()
        } else {
            self.current_device_name.as_str()
        };
        let display_name = device.display_name_with_transport(base_name);

        self.connection_generation = self.connection_generation.wrapping_add(1);
        self.hid_device = None;
        self.shared_hid_output = None;
        self.qmk_hid_hosts.clear();
        self.pending_device_connect = None;
        self.pending_entlayout_import_path = None;
        self.pending_entsettings_import_path = None;
        self.import_progress_started_at = None;
        self.reset_settings_write_context();
        self.cancel_pending_qmk_setting_writes();
        self.qmk_settings_write_queue.clear();
        self.pending_tap_hold_numeric_writes.clear();
        self.tap_hold_numeric_write_due = None;
        self.combo_dirty = false;
        self.combo_attempted_revision = None;
        self.combo_names_dirty = false;
        self.combo_colors_dirty = false;
        self.combo_term_dirty = false;
        self.undo_stack.clear();
        self.pending_layout_undo = false;
        self.pending_layer_write = None;
        self.keycode_picker.open = false;
        self.selected_key = None;
        self.selected_encoder = None;
        self.unlock_open = false;
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.pending_layout_indicator_open_after_unlock = false;
        self.reset_matrix_tester_state();
        self.next_battery_refresh_at = None;

        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "connection.reconnecting",
            &[("device", &display_name)],
        );
        self.connect_state = ConnectState::Reconnecting(BluetoothReconnectState::new(
            device.stable_identity(),
            display_name,
        ));
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn maybe_begin_bluetooth_reconnect(&mut self) {
        if !matches!(self.connect_state, ConnectState::Idle)
            || self.layout.is_none()
            || self.hid_device.is_some()
            || self.hid_write_task_owner_active()
        {
            return;
        }

        let error = if self.status_msg.trim().is_empty() {
            "Bluetooth HID transport became unavailable".to_owned()
        } else {
            self.status_msg.clone()
        };
        self.begin_bluetooth_reconnect(error);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn schedule_bluetooth_reconnect_retry(
        &mut self,
        state: BluetoothReconnectState,
        error: &str,
    ) {
        log::debug!(
            "Bluetooth reconnect attempt failed for {}: {}",
            state.display_name,
            error
        );
        self.status_msg = crate::i18n::tr_catalog_format(
            self.app_settings.language,
            "connection.reconnecting",
            &[("device", &state.display_name)],
        );
        self.connect_state =
            ConnectState::Reconnecting(state.schedule_retry(std::time::Instant::now()));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn cancel_bluetooth_reconnect_for_device_selection(&mut self) {
        if !self.bluetooth_reconnect_active() {
            return;
        }
        self.selected_device = None;
        self.clear_connected_keyboard_state("");
        self.connect_state = ConnectState::SelectingDevice;
        self.start_device_scan();
    }

    pub(super) fn clear_connected_keyboard_state(&mut self, status_msg: impl Into<String>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.connection_generation = self.connection_generation.wrapping_add(1);
        }
        self.layout = None;
        self.selected_key = None;
        self.selected_encoder = None;
        self.selected_layer = 0;
        self.layer_count = 0;
        self.qmk_hid_hosts.clear();
        self.layer_write_task = None;
        self.pending_layer_write = None;
        self.combo_write_task = None;
        self.settings_write_task = None;
        self.vial_hid_task = None;
        self.pending_layout_undo = false;
        self.deferred_device_load = DeferredDeviceLoadState::default();
        self.deferred_full_layout_action = None;
        self.reset_settings_write_context();
        self.cancel_pending_qmk_setting_writes();
        self.qmk_settings_write_queue.clear();
        self.pending_device_connect = None;
        self.hid_device = None;
        self.shared_hid_output = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.next_battery_refresh_at = None;
        }
        self.supported_qmk_settings.clear();
        self.undo_stack.clear();
        self.connect_state = ConnectState::Idle;
        self.unlock_open = false;
        self.vial_unlocked = None;
        self.vial_unlock_keys.clear();
        self.vial_unlock_polling = false;
        self.vial_unlock_last_poll = None;
        self.pending_layout_indicator_open_after_unlock = false;
        self.keycode_picker.open = false;
        self.current_device_name.clear();
        self.current_keyboard_id = None;
        self.current_encoder_visibility_id.clear();
        self.device_about_info = None;
        self.combo_dirty = false;
        self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
        self.combo_attempted_revision = None;
        self.supports_rmk_combo_layers = false;
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
        self.sticky_layout_active_combos.clear();
        self.sticky_layout_tap_dance_states.clear();
        self.sticky_layout_base_layer = 0;
        self.sticky_layout_active_layer = 0;
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
                let display_name = self
                    .device_display_names
                    .get(&dev.display_name_cache_key())
                    .map(String::as_str)
                    .unwrap_or(dev.name.as_str());
                (idx, dev.display_name_with_transport(display_name))
            })
            .collect();
        let has_error = !self.status_msg.trim().is_empty();
        let status_height = if has_error { 38.0 } else { 0.0 };
        let panel_width = rect.width().min(520.0);
        let adaptive_list_width =
            adaptive_top_dropdown_width(ui, devices.iter().map(|(_, label)| label.as_str()), 220.0);
        let list_size = device_selection_list_size(panel_width, adaptive_list_width, devices.len());
        let panel_height = 110.0 + status_height + list_size.y;
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
                let (list_rect, _) = ui.allocate_exact_size(list_size, Sense::hover());
                crate::ui_style::allocate_ui_at_rect(ui, list_rect, |ui| {
                    let mut selected_device = None;
                    top_dropdown_frame(dark).show(ui, |ui| {
                        let item_width = list_size.x - 16.0;
                        let mut draw_device_items = |ui: &mut egui::Ui| {
                            ui.set_width(item_width);
                            for (idx, label) in &devices {
                                if top_dropdown_item(ui, item_width, label, true, false).clicked() {
                                    selected_device = Some(*idx);
                                }
                            }
                        };

                        if device_selection_needs_scroll(devices.len()) {
                            egui::ScrollArea::vertical()
                                .max_height(list_size.y - 12.0)
                                .auto_shrink([false, true])
                                .show(ui, &mut draw_device_items);
                        } else {
                            draw_device_items(ui);
                        }
                    });
                    if let Some(idx) = selected_device {
                        self.selected_device = Some(idx);
                        self.main_menu_tab = MainMenuTab::Keyboard;
                        self.start_connect(idx);
                    }
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_layout() -> KeyboardLayout {
        KeyboardLayout {
            name: "K:04".to_owned(),
            rows: 0,
            cols: 0,
            keys: Vec::new(),
            encoders: Vec::new(),
            layers: vec![Vec::new(), Vec::new(), Vec::new()],
            encoder_layers: vec![Vec::new(), Vec::new(), Vec::new()],
            layer_names: vec!["Base".to_owned(), "Lower".to_owned(), "Raise".to_owned()],
            custom_keycodes: Vec::new(),
            layout_options: Vec::new(),
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn bluetooth_device(path: &str) -> Device {
        Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "AA:BB:CC:DD:EE:FF".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: path.to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn device_selection_list_stays_compact_and_caps_visible_rows() {
        assert_eq!(
            device_selection_list_size(520.0, 220.0, 1),
            egui::vec2(220.0, 42.0)
        );
        assert_eq!(
            device_selection_list_size(520.0, 360.0, 12),
            egui::vec2(360.0, 192.0)
        );
        assert!(!device_selection_needs_scroll(1));
        assert!(!device_selection_needs_scroll(6));
        assert!(device_selection_needs_scroll(7));
    }

    #[test]
    fn bluetooth_disconnect_preserves_layout_layer_and_battery_while_reconnecting() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![bluetooth_device("/dev/hidraw4")]);
        app.selected_device = Some(0);
        app.layout = Some(test_layout());
        app.selected_layer = 2;
        app.current_device_name = "K:04".to_owned();
        app.device_about_info = Some(DeviceAboutInfo {
            supports_battery_halves: true,
            battery_halves: Some(crate::hid::BatteryHalves {
                left: Some(91),
                right: Some(87),
            }),
            ..Default::default()
        });

        assert!(app.begin_bluetooth_reconnect("HID device disconnected"));

        assert!(app.layout.is_some());
        assert_eq!(app.selected_layer, 2);
        assert_eq!(
            app.device_about_info
                .as_ref()
                .and_then(|info| info.battery_halves)
                .and_then(|battery| battery.left),
            Some(91)
        );
        assert!(matches!(app.connect_state, ConnectState::Reconnecting(_)));
    }

    #[test]
    fn initial_bluetooth_failure_enters_reconnect_before_layout_loads() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![bluetooth_device("/dev/hidraw4")]);
        app.selected_device = Some(0);

        assert!(app.layout.is_none());
        assert!(app.begin_bluetooth_reconnect(
            "VIA protocol read failed: HID timeout — device did not respond"
        ));
        assert!(app.layout.is_none());
        assert!(matches!(app.connect_state, ConnectState::Reconnecting(_)));
        assert!(!app.status_msg.contains("device did not respond"));
    }

    #[test]
    fn choosing_another_device_leaves_reconnect_in_manual_selection() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        app.device_manager
            .replace_devices(vec![bluetooth_device("/dev/hidraw4")]);
        app.selected_device = Some(0);

        assert!(app.begin_bluetooth_reconnect("HID device disconnected"));
        app.cancel_bluetooth_reconnect_for_device_selection();

        assert!(app.selected_device.is_none());
        assert!(matches!(app.connect_state, ConnectState::SelectingDevice));
    }

    #[test]
    fn reconnect_backoff_caps_at_two_seconds() {
        assert_eq!(
            bluetooth_reconnect_retry_delay(0),
            std::time::Duration::from_millis(500)
        );
        assert_eq!(
            bluetooth_reconnect_retry_delay(1),
            std::time::Duration::from_secs(1)
        );
        assert_eq!(
            bluetooth_reconnect_retry_delay(2),
            std::time::Duration::from_secs(2)
        );
        assert_eq!(
            bluetooth_reconnect_retry_delay(u8::MAX),
            std::time::Duration::from_secs(2)
        );
    }
}
