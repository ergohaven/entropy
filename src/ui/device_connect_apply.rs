use super::*;

fn is_default_layer_name(index: usize, name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty()
        || trimmed == index.to_string()
        || (index == 0 && trimmed.eq_ignore_ascii_case("main"))
        || trimmed.eq_ignore_ascii_case(&format!("layer {index}"))
}

/// Merge firmware-read layer names with locally saved ones.
///
/// A name the firmware actually stored (`from_firmware[i]`) is authoritative,
/// even when it looks like a default such as "Main". A locally saved name only
/// fills a layer the firmware left as a generated descriptor placeholder, so
/// provenance — not the string — decides. One real firmware name therefore no
/// longer suppresses saved names for the other layers.
fn resolve_layer_names(
    firmware_names: &[String],
    from_firmware: &[bool],
    local_names: Option<&[String]>,
    layer_count: usize,
) -> Vec<String> {
    let mut names: Vec<String> = firmware_names.to_vec();
    if names.len() < layer_count {
        let start = names.len();
        names.extend((start..layer_count).map(|layer| layer.to_string()));
    }
    names.truncate(layer_count);
    if let Some(local) = local_names {
        for (idx, name) in local.iter().enumerate().take(layer_count) {
            let from_firmware = from_firmware.get(idx).copied().unwrap_or(false);
            if !name.trim().is_empty() && !from_firmware && is_default_layer_name(idx, &names[idx])
            {
                names[idx] = name.clone();
            }
        }
    }
    names
}

fn connect_apply_start_log(
    device_name: &str,
    layer_count: usize,
    firmware: FirmwareProtocol,
) -> String {
    format!("Applying keyboard layout for {device_name} ({layer_count} layers, {firmware:?})")
}

fn connect_apply_error_log(error: &str) -> String {
    format!("Connect failed before applying keyboard layout: {error}")
}

fn is_hid_open_failure(error: &str) -> bool {
    error.starts_with("Open failed:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_apply_start_log_includes_device_layer_count_and_firmware() {
        assert_eq!(
            connect_apply_start_log("HPD2", 4, FirmwareProtocol::Vial),
            "Applying keyboard layout for HPD2 (4 layers, Vial)"
        );
    }

    #[test]
    fn connect_apply_error_log_includes_context() {
        assert_eq!(
            connect_apply_error_log("Layout parse failed: missing matrix"),
            "Connect failed before applying keyboard layout: Layout parse failed: missing matrix"
        );
    }

    #[test]
    fn detects_hid_open_failure_status() {
        assert!(is_hid_open_failure(
            "Open failed: Failed to open HID device"
        ));
        assert!(!is_hid_open_failure("Layout parse failed: missing matrix"));
    }

    #[test]
    fn empty_connect_poll_is_throttled() {
        assert_eq!(CONNECT_POLL_INTERVAL, std::time::Duration::from_millis(250));
    }

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolve_prefers_firmware_names_even_when_default_looking() {
        // Firmware stored "Main" for layer 0; a stale local name must not win.
        let names = resolve_layer_names(
            &s(&["Main", "1"]),
            &[true, false],
            Some(&s(&["OLD", "LOWER"])),
            2,
        );
        assert_eq!(names, s(&["Main", "LOWER"]));
    }

    #[test]
    fn resolve_fills_placeholder_layers_from_local() {
        // Firmware provided nothing (all placeholders); local names fill them.
        let names = resolve_layer_names(
            &s(&["0", "1", "2"]),
            &[false, false, false],
            Some(&s(&["BASE", "LOWER", ""])),
            3,
        );
        // Empty local name leaves the placeholder untouched.
        assert_eq!(names, s(&["BASE", "LOWER", "2"]));
    }

    #[test]
    fn resolve_mixed_firmware_and_local() {
        // Layer 0 real firmware name kept; layers 1/2 placeholders filled locally.
        let names = resolve_layer_names(
            &s(&["BASE", "1", "2"]),
            &[true, false, false],
            Some(&s(&["X", "RAISE", "ADJUST"])),
            3,
        );
        assert_eq!(names, s(&["BASE", "RAISE", "ADJUST"]));
    }

    #[test]
    fn resolve_extends_and_truncates_to_layer_count() {
        let names = resolve_layer_names(&s(&["BASE"]), &[true], None, 3);
        assert_eq!(names, s(&["BASE", "1", "2"]));
        let names = resolve_layer_names(&s(&["A", "B", "C"]), &[true, true, true], None, 2);
        assert_eq!(names, s(&["A", "B"]));
    }
}

impl EntropyApp {
    /// Poll background thread for connect result.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn poll_connect(&mut self, ctx: &egui::Context) {
        const CONNECT_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
        const CONNECT_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(240);

        let result = match &mut self.connect_state {
            ConnectState::Loading {
                rx,
                started_at,
                last_progress_at,
            } => match rx.try_recv() {
                Ok(ConnectTaskMessage::Progress(message)) => {
                    self.status_msg = message;
                    *last_progress_at = std::time::Instant::now();
                    ctx.request_repaint();
                    return;
                }
                Ok(ConnectTaskMessage::Done(result)) => {
                    ctx.request_repaint();
                    *result
                }
                Err(mpsc::TryRecvError::Empty) => {
                    let idle_timeout = last_progress_at.elapsed() > CONNECT_IDLE_TIMEOUT;
                    let total_timeout = started_at.elapsed() > CONNECT_TOTAL_TIMEOUT;
                    if idle_timeout || total_timeout {
                        let stage = if self.status_msg.is_empty() {
                            "unknown stage".to_owned()
                        } else {
                            self.status_msg.clone()
                        };
                        self.status_msg = format!(
                            "Connect timeout — RMK/Vial device did not finish loading while: {stage}"
                        );
                        log::warn!("Connect timeout while waiting for stage: {stage}");
                        self.connect_state = ConnectState::Idle;
                        return;
                    }
                    #[cfg(not(target_os = "windows"))]
                    ctx.request_repaint_after(CONNECT_POLL_INTERVAL);
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status_msg = "Connect thread died".into();
                    log::error!("Connect thread died before returning a result");
                    self.connect_state = ConnectState::Idle;
                    return;
                }
            },
            ConnectState::Idle => return,
        };

        self.connect_state = ConnectState::Idle;

        match result {
            Ok(r) => {
                self.pending_tap_hold_numeric_writes.clear();
                self.tap_hold_numeric_write_due = None;
                log::info!(
                    "{}",
                    connect_apply_start_log(&r.device_name, r.layer_count, r.layout.firmware)
                );
                self.layer_count = r.layer_count;
                self.firmware = r.layout.firmware;
                self.current_device_name = r.device_name.clone();
                self.current_keyboard_id = Some(r.keyboard_id);
                self.device_about_info = Some(r.about_info.clone());
                self.matrix_tester_rmk_byte_order = self.current_device_is_likely_rmk();
                self.current_encoder_visibility_id =
                    encoder_visibility_id(&r.device_name, r.keyboard_id);
                if let Some(dev) = self
                    .selected_device
                    .and_then(|idx| self.device_manager.devices().get(idx))
                {
                    self.device_display_names
                        .insert(dev.display_name_cache_key(), r.device_name.clone());
                }
                self.keycode_picker.tap_dance_entries = r.tap_dance_entries.clone();
                self.keycode_picker.tap_dance_synced_entries = r.tap_dance_entries.clone();
                self.combo_entries = r.combo_entries.clone();
                self.combo_synced_entries = r.combo_entries.clone();
                self.combo_dirty = false;
                self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
                self.combo_attempted_revision = None;
                self.key_override_entries = r.key_override_entries.clone();
                self.alt_repeat_entries = r.alt_repeat_entries.clone();
                self.alt_repeat_names = load_alt_repeat_names(&self.current_device_name);
                self.alt_repeat_names
                    .resize(self.alt_repeat_entries.len(), String::new());
                self.alt_repeat_undo_stack.clear();
                self.selected_alt_repeat = 0;
                self.alt_repeat_visible_count = if self.alt_repeat_entries.is_empty() {
                    1
                } else {
                    1.min(self.alt_repeat_entries.len())
                };
                self.key_override_names = load_key_override_names(&self.current_device_name);
                self.key_override_names
                    .resize(self.key_override_entries.len(), String::new());
                self.key_override_visible_count = 1;
                self.key_override_undo_stack.clear();
                self.selected_key_override = 0;
                self.combo_names = load_combo_names(&self.current_device_name);
                self.combo_names
                    .resize(self.combo_entries.len(), String::new());
                self.combo_colors = load_combo_colors(&self.current_device_name);
                migrate_legacy_combo_default_colors(&mut self.combo_colors);
                normalize_combo_colors(&mut self.combo_colors, self.combo_entries.len());
                self.combo_term = r.combo_term.or(Some(50));
                self.auto_shift_options = r.auto_shift_options;
                self.auto_shift_timeout = r.auto_shift_timeout;
                self.auto_shift_timeout_text = r
                    .auto_shift_timeout
                    .map(|timeout| timeout.to_string())
                    .unwrap_or_default();
                self.mouse_keys_settings = r.mouse_keys_settings;
                self.touchpad_settings = r.touchpad_settings;
                self.bluetooth_settings = r.bluetooth_settings;
                self.module_settings = r.module_settings;
                self.tap_hold_settings = r.tap_hold_settings;
                self.magic_settings = r.magic_settings;
                self.one_shot_settings = r.one_shot_settings;
                self.grave_escape_settings = r.grave_escape_settings;
                self.layer_led_settings = r.layer_led_settings;
                self.rgb_settings = r.rgb_settings;
                self.layout_options_value = r.layout_options_value;
                let highest_used_combo = self
                    .combo_entries
                    .iter()
                    .enumerate()
                    .filter(|(i, combo)| {
                        combo.output != 0
                            || combo.keys.iter().any(|&k| k != 0)
                            || self
                                .combo_names
                                .get(*i)
                                .map(|n| !n.trim().is_empty())
                                .unwrap_or(false)
                    })
                    .map(|(i, _)| i + 1)
                    .max()
                    .unwrap_or(1);
                self.combo_visible_count = highest_used_combo.min(self.combo_entries.len().max(1));
                self.selected_combo = self
                    .selected_combo
                    .min(self.combo_visible_count.saturating_sub(1));
                self.keycode_picker.macro_count = r.macro_texts.len();
                self.keycode_picker.macro_texts = r.macro_texts.clone();
                let mut macro_metadata = load_macro_metadata(&self.current_device_name);
                macro_metadata.resize(r.macro_texts.len());
                self.keycode_picker.macro_names = macro_metadata.names;
                self.keycode_picker.macro_descriptions = macro_metadata.descriptions;
                self.keycode_picker.macro_metadata_dirty = false;
                self.keycode_picker.supports_macro_ext_keycodes = r.supports_macro_ext_keycodes;
                self.keycode_picker.macro_ext_keycodes_disabled_reason =
                    r.macro_ext_keycodes_disabled_reason;
                // Parse macro texts into actions (Vial protocol v2+ bytecode).
                self.keycode_picker.macro_actions = r
                    .macro_texts
                    .iter()
                    .map(|bytes| crate::keycode_picker::decode_macro_actions(bytes))
                    .collect();

                self.status_msg = format!("Connected: {}", r.device_name);

                // Load per-device layer names.
                let device_name = r.device_name.clone();
                let local_layer_names = load_saved_layer_names(&device_name);
                self.layer_names = resolve_layer_names(
                    &r.layout.layer_names,
                    &r.layer_names_from_firmware,
                    local_layer_names.as_deref(),
                    r.layer_count,
                );

                let encoder_count = r.layout.encoder_count();
                self.encoder_visibility =
                    load_encoder_visibility(&self.current_encoder_visibility_id, encoder_count);
                Self::apply_encoder_layout_options_to_visibility(
                    &r.layout,
                    self.layout_options_value,
                    &mut self.encoder_visibility,
                );

                // Populate picker
                self.keycode_picker.supports_rgb =
                    r.layout.supports_rgb || self.rgb_settings.supported;
                self.keycode_picker.supports_macro = self.keycode_picker.macro_count > 0;
                self.keycode_picker.supports_tap_dance = !r.tap_dance_entries.is_empty();
                // Mouse keycodes are assignable through the keymap even when a
                // firmware does not expose Vial/QMK mouse-key settings.
                self.keycode_picker.supports_mouse_keys = true;
                self.keycode_picker.supports_combo = !self.combo_entries.is_empty();
                self.keycode_picker.supports_auto_shift = self.auto_shift_timeout.is_some();
                self.keycode_picker.supports_caps_word = r.vial_features.caps_word;
                self.keycode_picker.supports_repeat_key = r.vial_features.repeat_key;
                self.keycode_picker.supports_layer_lock = r.vial_features.layer_lock;
                self.keycode_picker.supports_persistent_default_layer =
                    r.vial_features.persistent_default_layer;
                self.keycode_picker.layer_count = r.layout.layers.len().max(1);
                self.keycode_picker.tap_dance_names = load_tap_dance_names(&device_name);
                // Vial GUI maps customKeycodes to USER00.. at QK_KB + index.
                // Protocol v6: QK_KB = 0x7E00. Do not use QK_USER (0x7E40):
                // assigning those values writes the wrong keycodes to firmware.
                const QK_KB: u16 = 0x7E00;
                self.keycode_picker.custom_keycodes = r
                    .layout
                    .custom_keycodes
                    .iter()
                    .enumerate()
                    .map(|(i, custom)| {
                        (
                            custom.name.clone(),
                            custom.label.clone(),
                            custom.title.clone(),
                            QK_KB + i as u16,
                        )
                    })
                    .collect();
                self.keycode_picker.layer_names = self.layer_names.clone();
                self.sticky_layout_prev_pressed.clear();
                self.sticky_layout_pressed_key_layers.clear();
                self.sticky_layout_toggled_layers = vec![false; r.layout.layers.len().max(1)];
                self.sticky_layout_base_layer = 0;

                self.layout = Some(r.layout);
                self.refresh_layer_picker_content_flags();

                // Keep the same HID owner that loaded the keyboard, matching vial-gui's
                // open-once/reload/use model. Avoid Entropy-only reopen churn when switching
                // between qmk-vial and RMK devices.
                self.hid_device = r.hid_device;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.restore_entropy_display_preset_after_connect();
                    self.sync_qmk_hid_host_bridges();
                }

                log::info!(
                    "Connected: {} ({} layers, {:?})",
                    r.device_name,
                    r.layer_count,
                    self.firmware
                );
            }
            Err(e) => {
                if is_hid_open_failure(&e) {
                    self.selected_device = None;
                    self.clear_connected_keyboard_state(e);
                    return;
                }

                self.status_msg = e;
                log::error!("{}", connect_apply_error_log(&self.status_msg));
            }
        }
    }
}
