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
    pub(super) fn preserve_deferred_hid_settings_for_disconnect(&mut self) {
        let Some(about) = self.device_about_info.as_ref() else {
            return;
        };
        let macros_dirty = self.keycode_picker.macros_dirty;
        let combo_term_dirty = self.combo_term_dirty;
        let combo_entries = self
            .combo_entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| self.combo_synced_entries.get(*index) != Some(*entry))
            .map(|(index, entry)| (index, entry.clone()))
            .collect::<Vec<_>>();
        let tap_dance_entries = self
            .keycode_picker
            .tap_dance_entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                self.keycode_picker.tap_dance_synced_entries.get(*index) != Some(*entry)
            })
            .map(|(index, entry)| (index, entry.clone()))
            .collect::<Vec<_>>();
        if !macros_dirty
            && combo_entries.is_empty()
            && !combo_term_dirty
            && tap_dance_entries.is_empty()
            && self.pending_tap_hold_numeric_writes.is_empty()
        {
            return;
        }

        self.deferred_hid_settings = Some(DeferredHidSettings {
            identity: ModuleSettingsDeviceIdentity {
                path: about.path.clone(),
                keyboard_id: about.keyboard_id,
            },
            macro_texts: self.keycode_picker.macro_texts.clone(),
            macros_dirty,
            combo_entries,
            combo_term: self.combo_term,
            combo_term_dirty,
            tap_dance_entries,
            pending_tap_hold_numeric_writes: self.pending_tap_hold_numeric_writes.clone(),
            tap_hold_numeric_write_due: self.tap_hold_numeric_write_due,
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restore_deferred_hid_settings_after_connect(&mut self, about: &DeviceAboutInfo) {
        let Some(deferred) = self.deferred_hid_settings.as_ref() else {
            return;
        };
        if deferred.identity.path != about.path
            || deferred.identity.keyboard_id != about.keyboard_id
        {
            return;
        }

        let deferred = self
            .deferred_hid_settings
            .take()
            .expect("deferred settings checked above");

        if deferred.macros_dirty {
            self.keycode_picker.macro_texts = deferred.macro_texts;
            self.keycode_picker.macro_actions = self
                .keycode_picker
                .macro_texts
                .iter()
                .map(|bytes| crate::keycode_picker::decode_macro_actions(bytes))
                .collect();
            self.keycode_picker.macros_dirty = true;
        }
        if !deferred.combo_entries.is_empty() {
            for (index, entry) in deferred.combo_entries {
                if self.combo_entries.len() <= index {
                    self.combo_entries.resize(index + 1, ComboEntry::default());
                }
                self.combo_entries[index] = entry;
            }
            self.combo_dirty = true;
        }
        if deferred.combo_term_dirty {
            self.combo_term = deferred.combo_term;
            self.combo_term_dirty = true;
        }
        if !deferred.tap_dance_entries.is_empty() {
            for (index, entry) in deferred.tap_dance_entries {
                if self.keycode_picker.tap_dance_entries.len() <= index {
                    self.keycode_picker
                        .tap_dance_entries
                        .resize(index + 1, Default::default());
                }
                self.keycode_picker.tap_dance_entries[index] = entry;
            }
            self.keycode_picker.tap_dance_dirty = true;
        }
        if !deferred.pending_tap_hold_numeric_writes.is_empty() {
            self.pending_tap_hold_numeric_writes = deferred.pending_tap_hold_numeric_writes;
            self.tap_hold_numeric_write_due = deferred
                .tap_hold_numeric_write_due
                .or_else(|| Some(std::time::Instant::now()));
        }
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
        self.combo_write_task = None;
        self.settings_write_task = None;
        self.reset_settings_write_context();
        self.cancel_pending_qmk_setting_writes();
        self.qmk_settings_write_queue.clear();
        self.pending_device_connect = None;
        self.settings_write_queue.clear();
        self.module_settings_refresh_task = None;
        self.hid_device = None;
        self.supported_qmk_settings.clear();
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
        self.combo_dirty = false;
        self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
        self.combo_attempted_revision = None;
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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn reconnect_restores_deferred_hid_settings_for_the_same_device() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let about = DeviceAboutInfo {
            path: "usb:phenom".to_owned(),
            keyboard_id: 42,
            ..Default::default()
        };
        app.device_about_info = Some(about.clone());
        app.keycode_picker.macro_texts = vec![vec![1, 2, 3]];
        app.keycode_picker.macros_dirty = true;
        let dirty_combo = ComboEntry {
            keys: [0x0004, 0x0005, 0, 0],
            output: 0x0006,
        };
        let clean_combo = ComboEntry {
            keys: [0x0007, 0x0008, 0, 0],
            output: 0x0009,
        };
        app.combo_entries = vec![dirty_combo.clone(), clean_combo.clone()];
        app.combo_synced_entries = vec![ComboEntry::default(), clean_combo];
        app.combo_dirty = true;
        app.combo_term = Some(175);
        app.combo_term_dirty = true;
        let dirty_tap_dance = crate::keycode_picker::TapDanceEntry {
            on_tap: 0x0004,
            ..Default::default()
        };
        let clean_tap_dance = crate::keycode_picker::TapDanceEntry {
            on_tap: 0x0007,
            ..Default::default()
        };
        app.keycode_picker.tap_dance_entries =
            vec![dirty_tap_dance.clone(), clean_tap_dance.clone()];
        app.keycode_picker.tap_dance_synced_entries = vec![Default::default(), clean_tap_dance];
        app.keycode_picker.tap_dance_dirty = true;
        app.pending_tap_hold_numeric_writes.insert(7, 175);

        app.preserve_deferred_hid_settings_for_disconnect();
        app.clear_connected_keyboard_state("disconnected");

        let other_device = DeviceAboutInfo {
            path: "bluetooth:phenom".to_owned(),
            keyboard_id: 42,
            ..Default::default()
        };
        app.restore_deferred_hid_settings_after_connect(&other_device);
        assert!(app.deferred_hid_settings.is_some());

        app.keycode_picker.macro_texts = vec![Vec::new()];
        app.keycode_picker.macros_dirty = false;
        app.combo_entries = vec![
            ComboEntry::default(),
            ComboEntry {
                keys: [0x000a, 0x000b, 0, 0],
                output: 0x000c,
            },
        ];
        app.combo_synced_entries = app.combo_entries.clone();
        app.combo_dirty = false;
        app.combo_term = Some(50);
        app.combo_term_dirty = false;
        app.keycode_picker.tap_dance_entries = vec![
            Default::default(),
            crate::keycode_picker::TapDanceEntry {
                on_tap: 0x000a,
                ..Default::default()
            },
        ];
        app.keycode_picker.tap_dance_synced_entries = app.keycode_picker.tap_dance_entries.clone();
        app.keycode_picker.tap_dance_dirty = false;

        app.restore_deferred_hid_settings_after_connect(&about);

        assert_eq!(app.keycode_picker.macro_texts, vec![vec![1, 2, 3]]);
        assert!(app.keycode_picker.macros_dirty);
        assert_eq!(app.combo_entries[0].output, 0x0006);
        assert_eq!(app.combo_entries[1].output, 0x000c);
        assert!(app.combo_dirty);
        assert_eq!(app.combo_term, Some(175));
        assert!(app.combo_term_dirty);
        assert_eq!(app.keycode_picker.tap_dance_entries[0].on_tap, 0x0004);
        assert_eq!(app.keycode_picker.tap_dance_entries[1].on_tap, 0x000a);
        assert!(app.keycode_picker.tap_dance_dirty);
        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&175));
    }
}
