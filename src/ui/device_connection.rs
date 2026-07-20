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
        let macro_entries = self
            .keycode_picker
            .macro_texts
            .iter()
            .enumerate()
            .filter(|(index, text)| {
                self.keycode_picker.macro_synced_texts.get(*index) != Some(*text)
            })
            .map(|(index, text)| (index, text.clone()))
            .collect::<Vec<_>>();
        let combo_term_dirty = self.combo_term_dirty;
        let combo_entries = self
            .combo_entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| self.combo_synced_entries.get(*index) != Some(*entry))
            .map(|(index, entry)| (index, entry.clone()))
            .collect::<Vec<_>>();
        let key_override_entries = self
            .key_override_dirty
            .then(|| {
                self.key_override_entries
                    .iter()
                    .cloned()
                    .enumerate()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
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
        let picker_mutation = self.deferred_picker_mutation();
        let layer_write = self.pending_layer_write.clone();
        if macro_entries.is_empty()
            && combo_entries.is_empty()
            && !combo_term_dirty
            && key_override_entries.is_empty()
            && tap_dance_entries.is_empty()
            && self.pending_tap_hold_numeric_writes.is_empty()
            && picker_mutation.is_none()
            && layer_write.is_none()
        {
            return;
        }

        let identity = ModuleSettingsDeviceIdentity::from_about(about);
        let deferred = DeferredHidSettings {
            identity: identity.clone(),
            macro_entries,
            combo_entries,
            combo_term: self.combo_term,
            combo_term_dirty,
            key_override_entries,
            tap_dance_entries,
            pending_tap_hold_numeric_writes: self.pending_tap_hold_numeric_writes.clone(),
            tap_hold_numeric_write_due: self.tap_hold_numeric_write_due,
            picker_mutation,
            layer_write,
        };
        if let Some(existing) = self
            .deferred_hid_settings
            .iter_mut()
            .find(|existing| existing.identity.matches(&identity))
        {
            *existing = deferred;
        } else {
            self.deferred_hid_settings.push(deferred);
        }
        // Deferred record now owns this retry. Do not leave a transient copy
        // that could be associated with a later, unrelated disconnect.
        self.pending_layer_write = None;
        self.keycode_picker.macros_dirty = false;
        self.combo_dirty = false;
        self.combo_term_dirty = false;
        self.key_override_dirty = false;
        self.keycode_picker.tap_dance_dirty = false;
        self.pending_tap_hold_numeric_writes.clear();
        self.tap_hold_numeric_write_due = None;
        self.keycode_picker.result = None;
        self.combo_pick_target = None;
        self.key_override_pick_target = None;
        self.alt_repeat_pick_target = None;
        self.picker_retry_due = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restore_deferred_hid_settings_after_connect(&mut self, about: &DeviceAboutInfo) {
        let identity = ModuleSettingsDeviceIdentity::from_about(about);
        let Some(index) = self
            .deferred_hid_settings
            .iter()
            .position(|deferred| deferred.identity.matches(&identity))
        else {
            if let Some(deferred) = self.deferred_hid_settings.iter().find(|deferred| {
                deferred.identity.vendor_id == identity.vendor_id
                    && deferred.identity.product_id == identity.product_id
                    && deferred.identity.keyboard_id == identity.keyboard_id
                    && deferred.identity.serial_number == identity.serial_number
            }) {
                self.deferred_hid_settings_prompt = Some(DeferredHidSettingsPrompt::Reattach {
                    deferred_identity: deferred.identity.clone(),
                    connected_identity: identity,
                });
                self.status_msg = crate::i18n::tr_catalog(
                    self.app_settings.language,
                    "modules_settings.deferred_changes_confirmation_needed",
                )
                .to_owned();
            }
            return;
        };

        self.restore_deferred_hid_settings_at(index);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn restore_deferred_hid_settings_at(&mut self, index: usize) {
        let deferred = self.deferred_hid_settings.remove(index);

        if !deferred.macro_entries.is_empty() {
            for (index, text) in deferred.macro_entries {
                if self.keycode_picker.macro_texts.len() <= index {
                    self.keycode_picker
                        .macro_texts
                        .resize(index + 1, Vec::new());
                }
                self.keycode_picker.macro_texts[index] = text;
            }
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
        if !deferred.key_override_entries.is_empty() {
            for (index, entry) in deferred.key_override_entries {
                if self.key_override_entries.len() <= index {
                    self.key_override_entries
                        .resize(index + 1, KeyOverrideEntry::default());
                }
                self.key_override_entries[index] = entry;
            }
            self.key_override_dirty = true;
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
        if let Some(mutation) = deferred.picker_mutation {
            self.restore_deferred_picker_mutation(mutation);
        }
        if let Some(layer_write) = deferred.layer_write {
            self.apply_layer_snapshot_with_behavior(
                layer_write.layer,
                LayerSnapshot {
                    keycodes: layer_write.keycodes,
                    encoder_keycodes: layer_write.encoder_keycodes,
                },
                "layer_actions.paste",
                crate::app::layer_operations::LayerUndoBehavior::RecordOld,
            );
        }
        self.pending_layer_write = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn confirm_deferred_hid_settings_reattach(&mut self) {
        let Some(DeferredHidSettingsPrompt::Reattach {
            deferred_identity,
            connected_identity,
        }) = self.deferred_hid_settings_prompt.take()
        else {
            return;
        };
        let current = self
            .device_about_info
            .as_ref()
            .map(ModuleSettingsDeviceIdentity::from_about);
        if current.as_ref() != Some(&connected_identity) {
            self.status_msg = crate::i18n::tr_catalog(
                self.app_settings.language,
                "modules_settings.deferred_changes_target_changed",
            )
            .to_owned();
            return;
        }
        if let Some(index) = self
            .deferred_hid_settings
            .iter()
            .position(|deferred| deferred.identity == deferred_identity)
        {
            self.restore_deferred_hid_settings_at(index);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn discard_deferred_hid_settings_for_identity(
        &mut self,
        identity: &ModuleSettingsDeviceIdentity,
    ) {
        self.deferred_hid_settings
            .retain(|deferred| deferred.identity != *identity);
        self.deferred_hid_settings_prompt = None;
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn deferred_picker_mutation(&self) -> Option<DeferredPickerMutation> {
        let keycode = self.keycode_picker.result?;
        if let Some((index, field)) = self.combo_pick_target {
            return Some(DeferredPickerMutation::Combo {
                index,
                field,
                keycode,
            });
        }
        if let Some(field) = self.key_override_pick_target {
            return Some(DeferredPickerMutation::KeyOverride {
                index: self.selected_key_override,
                field,
                keycode,
            });
        }
        if let Some(field) = self.alt_repeat_pick_target {
            return Some(DeferredPickerMutation::AltRepeat {
                index: self.selected_alt_repeat,
                field,
                keycode,
            });
        }
        if let Some((layer, index)) = self.selected_key {
            return Some(DeferredPickerMutation::Key {
                layer,
                index,
                keycode,
            });
        }
        self.selected_encoder
            .map(|(layer, index)| DeferredPickerMutation::Encoder {
                layer,
                index,
                keycode,
            })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn restore_deferred_picker_mutation(&mut self, mutation: DeferredPickerMutation) {
        match mutation {
            DeferredPickerMutation::Combo {
                index,
                field,
                keycode,
            } => {
                self.combo_pick_target = Some((index, field));
                self.keycode_picker.result = Some(keycode);
            }
            DeferredPickerMutation::KeyOverride {
                index,
                field,
                keycode,
            } => {
                self.selected_key_override = index;
                self.key_override_pick_target = Some(field);
                self.keycode_picker.result = Some(keycode);
            }
            DeferredPickerMutation::AltRepeat {
                index,
                field,
                keycode,
            } => {
                self.selected_alt_repeat = index;
                self.alt_repeat_pick_target = Some(field);
                self.keycode_picker.result = Some(keycode);
            }
            DeferredPickerMutation::Key {
                layer,
                index,
                keycode,
            } => {
                self.selected_key = Some((layer, index));
                self.keycode_picker.result = Some(keycode);
            }
            DeferredPickerMutation::Encoder {
                layer,
                index,
                keycode,
            } => {
                self.selected_encoder = Some((layer, index));
                self.keycode_picker.result = Some(keycode);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn handoff_hid_worker_disconnect(&mut self, status_msg: impl Into<String>) {
        self.preserve_deferred_hid_settings_for_disconnect();
        self.clear_connected_keyboard_state(status_msg);
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
        self.pending_device_connect = None;
        self.settings_write_queue.clear();
        self.module_settings_refresh_task = None;
        self.deferred_hid_settings_prompt = None;
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
        // Cancellation restores the last confirmed QMK values into this fresh
        // device-scoped state. Do it after defaults, not before them.
        self.cancel_pending_qmk_setting_writes();
        self.qmk_settings_write_queue.clear();
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
            path: "/dev/hidraw0".to_owned(),
            serial_number: "same-device".to_owned(),
            vendor_id: 0xe126,
            product_id: 0x0042,
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
        let dirty_key_override = KeyOverrideEntry {
            trigger: 0x0004,
            replacement: 0x0005,
            ..Default::default()
        };
        app.key_override_entries = vec![dirty_key_override.clone()];
        app.key_override_dirty = true;
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
        assert!(!app.key_override_dirty);
        app.clear_connected_keyboard_state("disconnected");

        let other_device = DeviceAboutInfo {
            path: "/dev/hidraw1".to_owned(),
            serial_number: "other-identical-device".to_owned(),
            vendor_id: about.vendor_id,
            product_id: about.product_id,
            keyboard_id: 42,
            ..Default::default()
        };
        app.restore_deferred_hid_settings_after_connect(&other_device);
        assert!(!app.deferred_hid_settings.is_empty());

        // Fresh readback has a newer clean macro in slot 1. Reconnect must
        // overlay only our dirty slot, not replace this whole buffer.
        app.keycode_picker.macro_texts = vec![Vec::new(), vec![8, 9]];
        app.keycode_picker.macro_synced_texts = app.keycode_picker.macro_texts.clone();
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
        app.key_override_entries = vec![KeyOverrideEntry::default()];
        app.key_override_dirty = false;
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

        assert_eq!(app.keycode_picker.macro_texts[0], vec![1, 2, 3]);
        assert_eq!(app.keycode_picker.macro_texts[1], vec![8, 9]);
        assert!(app.keycode_picker.macros_dirty);
        assert_eq!(app.combo_entries[0].output, 0x0006);
        assert_eq!(app.combo_entries[1].output, 0x000c);
        assert!(app.combo_dirty);
        assert_eq!(app.combo_term, Some(175));
        assert!(app.combo_term_dirty);
        assert_eq!(app.key_override_entries[0], dirty_key_override);
        assert!(app.key_override_dirty);
        assert_eq!(app.keycode_picker.tap_dance_entries[0].on_tap, 0x0004);
        assert_eq!(app.keycode_picker.tap_dance_entries[1].on_tap, 0x000a);
        assert!(app.keycode_picker.tap_dance_dirty);
        assert_eq!(app.pending_tap_hold_numeric_writes.get(&7), Some(&175));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn no_serial_path_change_requires_confirmation_before_restoring_deferred_settings() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let original = DeviceAboutInfo {
            path: "/dev/hidraw0".to_owned(),
            vendor_id: 0xe126,
            product_id: 0x0042,
            keyboard_id: 42,
            ..Default::default()
        };
        app.device_about_info = Some(original.clone());
        app.keycode_picker.macro_texts = vec![vec![1, 2, 3]];
        app.keycode_picker.macros_dirty = true;
        app.preserve_deferred_hid_settings_for_disconnect();
        app.clear_connected_keyboard_state("disconnected");
        app.keycode_picker.macro_texts.clear();

        let reconnected = DeviceAboutInfo {
            path: "/dev/hidraw7".to_owned(),
            ..original
        };
        app.device_about_info = Some(reconnected.clone());
        app.restore_deferred_hid_settings_after_connect(&reconnected);

        assert_eq!(app.deferred_hid_settings.len(), 1);
        assert!(app.keycode_picker.macro_texts.is_empty());
        assert_eq!(
            app.status_msg,
            crate::i18n::tr_catalog(
                app.app_settings.language,
                "modules_settings.deferred_changes_confirmation_needed"
            )
        );
        assert!(matches!(
            app.deferred_hid_settings_prompt,
            Some(DeferredHidSettingsPrompt::Reattach { .. })
        ));

        app.confirm_deferred_hid_settings_reattach();
        assert!(app.deferred_hid_settings.is_empty());
        assert_eq!(app.keycode_picker.macro_texts, vec![vec![1, 2, 3]]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn duplicate_serial_on_a_changed_path_requires_confirmation_before_reattach() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let original = DeviceAboutInfo {
            path: "/dev/hidraw0".to_owned(),
            serial_number: "shared-serial".to_owned(),
            vendor_id: 0xe126,
            product_id: 0x0042,
            keyboard_id: 42,
            ..Default::default()
        };
        app.device_about_info = Some(original.clone());
        app.keycode_picker.macro_texts = vec![vec![1, 2, 3]];
        app.keycode_picker.macros_dirty = true;
        app.preserve_deferred_hid_settings_for_disconnect();
        app.clear_connected_keyboard_state("disconnected");

        let reconnected = DeviceAboutInfo {
            path: "/dev/hidraw7".to_owned(),
            ..original
        };
        app.device_about_info = Some(reconnected.clone());
        app.restore_deferred_hid_settings_after_connect(&reconnected);

        assert_eq!(app.deferred_hid_settings.len(), 1);
        assert!(matches!(
            app.deferred_hid_settings_prompt,
            Some(DeferredHidSettingsPrompt::Reattach { .. })
        ));
    }
}
