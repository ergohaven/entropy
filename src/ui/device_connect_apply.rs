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

    use std::cell::RefCell;
    use std::collections::{BTreeSet, HashMap};

    /// In-memory [`LayerNameStore`] standing in for the HID device so the
    /// production sync path can be tested without hardware. Records every read
    /// and write and lets a test inject transport errors per qsid.
    struct MockStore {
        supported: BTreeSet<u16>,
        stored: RefCell<HashMap<u16, String>>,
        read_errors: BTreeSet<u16>,
        write_errors: BTreeSet<u16>,
        reads: RefCell<Vec<u16>>,
        writes: RefCell<Vec<(u16, String)>>,
    }

    impl MockStore {
        fn new(supported: &[u16]) -> Self {
            MockStore {
                supported: supported.iter().copied().collect(),
                stored: RefCell::new(HashMap::new()),
                read_errors: BTreeSet::new(),
                write_errors: BTreeSet::new(),
                reads: RefCell::new(Vec::new()),
                writes: RefCell::new(Vec::new()),
            }
        }

        fn with_stored(mut self, qsid: u16, value: &str) -> Self {
            self.stored.get_mut().insert(qsid, value.to_string());
            self
        }

        fn failing_reads(mut self, qsids: &[u16]) -> Self {
            self.read_errors = qsids.iter().copied().collect();
            self
        }

        fn failing_writes(mut self, qsids: &[u16]) -> Self {
            self.write_errors = qsids.iter().copied().collect();
            self
        }
    }

    impl LayerNameStore for MockStore {
        fn is_supported(&self, qsid: u16) -> bool {
            self.supported.contains(&qsid)
        }

        fn get_string(&self, qsid: u16) -> anyhow::Result<String> {
            self.reads.borrow_mut().push(qsid);
            if self.read_errors.contains(&qsid) {
                anyhow::bail!("read transport error on qsid {qsid}");
            }
            Ok(self.stored.borrow().get(&qsid).cloned().unwrap_or_default())
        }

        fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()> {
            self.writes.borrow_mut().push((qsid, value.to_string()));
            if self.write_errors.contains(&qsid) {
                anyhow::bail!("write transport error on qsid {qsid}");
            }
            self.stored.borrow_mut().insert(qsid, value.to_string());
            Ok(())
        }
    }

    #[test]
    fn sync_unsupported_storage_is_not_a_failure_and_never_touches_the_wire() {
        // Firmware advertises no layer-name settings at all.
        let store = MockStore::new(&[]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert!(failed.is_empty());
        assert!(store.reads.borrow().is_empty());
        assert!(store.writes.borrow().is_empty());
    }

    #[test]
    fn sync_transient_first_layer_read_failure_is_reported_not_swallowed() {
        // Regression: a read error on layer 0 used to be treated as "storage
        // unsupported", aborting the sync and reporting success. Now the layer
        // is reported failed and later layers still get written.
        let store = MockStore::new(&[200, 201]).failing_reads(&[200]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert_eq!(failed, vec![0]);
        // Layer 1 was still read and written back.
        assert_eq!(
            store.writes.borrow().as_slice(),
            &[(201, "LOWER".to_string())]
        );
        assert_eq!(
            store.stored.borrow().get(&201).map(String::as_str),
            Some("LOWER")
        );
    }

    #[test]
    fn sync_middle_layer_read_failure_only_fails_that_layer() {
        let store = MockStore::new(&[200, 201, 202]).failing_reads(&[201]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER", "RAISE"]), 3);
        assert_eq!(failed, vec![1]);
        assert_eq!(
            store.stored.borrow().get(&200).map(String::as_str),
            Some("BASE")
        );
        assert_eq!(
            store.stored.borrow().get(&202).map(String::as_str),
            Some("RAISE")
        );
    }

    #[test]
    fn sync_set_failure_reports_layer_but_continues() {
        let store = MockStore::new(&[200, 201]).failing_writes(&[200]);
        let failed = sync_layer_names_to_store(&store, &s(&["BASE", "LOWER"]), 2);
        assert_eq!(failed, vec![0]);
        // The later layer still persisted despite the earlier SET error.
        assert_eq!(
            store.stored.borrow().get(&201).map(String::as_str),
            Some("LOWER")
        );
    }

    #[test]
    fn sync_skips_matching_and_empty_names() {
        let store = MockStore::new(&[200, 201]).with_stored(201, "LOWER");
        // Layer 0 empty (skipped, no read); layer 1 already matches (no write).
        let failed = sync_layer_names_to_store(&store, &s(&["", "LOWER"]), 2);
        assert!(failed.is_empty());
        assert_eq!(store.reads.borrow().as_slice(), &[201]);
        assert!(store.writes.borrow().is_empty());
    }

    #[test]
    fn sync_persists_then_is_idempotent_on_reconnect() {
        let names = s(&["BASE", "LOWER"]);
        let store = MockStore::new(&[200, 201]);
        // First sync writes both names.
        let failed = sync_layer_names_to_store(&store, &names, 2);
        assert!(failed.is_empty());
        assert_eq!(store.writes.borrow().len(), 2);
        // A second sync (as after a reconnect) reads them back and writes nothing.
        store.writes.borrow_mut().clear();
        let failed = sync_layer_names_to_store(&store, &names, 2);
        assert!(failed.is_empty());
        assert!(store.writes.borrow().is_empty());
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
                // A new device is now active; invalidate any file dialog that was
                // opened against the previous connection.
                self.connection_generation = self.connection_generation.wrapping_add(1);
                self.layer_count = r.layer_count;
                self.firmware = r.layout.firmware;
                self.current_device_name = r.device_name.clone();
                self.current_keyboard_id = Some(r.keyboard_id);
                match &r.vial_unlock_status {
                    Some((unlocked, keys)) => {
                        self.vial_unlocked = Some(*unlocked);
                        self.vial_unlock_keys = keys.clone();
                    }
                    None => {
                        self.vial_unlocked = None;
                        self.vial_unlock_keys.clear();
                    }
                }
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

                let connected_display_name = self
                    .selected_device
                    .and_then(|idx| self.device_manager.devices().get(idx))
                    .map(|device| device.display_name_with_transport(&r.device_name))
                    .unwrap_or_else(|| r.device_name.clone());
                self.status_msg = format!("Connected: {connected_display_name}");

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
                let hide_modular_encoders_by_default =
                    self.module_settings_include_encoder_visibility(&r.layout);
                self.encoder_visibility = Self::resolve_initial_encoder_visibility(
                    &r.layout,
                    self.layout_options_value,
                    load_saved_encoder_visibility(
                        &self.current_encoder_visibility_id,
                        encoder_count,
                    ),
                    hide_modular_encoders_by_default,
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
                self.sticky_layout_active_combos = vec![false; r.combo_entries.len()];
                self.sticky_layout_tap_dance_states.clear();
                self.sticky_layout_base_layer = 0;

                self.layout = Some(r.layout);
                self.refresh_layer_picker_content_flags();

                // Keep the same HID owner that loaded the keyboard, matching vial-gui's
                // open-once/reload/use model. Avoid Entropy-only reopen churn when switching
                // between qmk-vial and RMK devices.
                self.hid_device = r.hid_device;
                self.supported_qmk_settings = r.supported_qmk_settings;

                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.restore_entropy_display_preset_after_connect();
                    self.sync_qmk_hid_host_bridges();
                }

                log::info!(
                    "Connected: {} ({} layers, {:?})",
                    connected_display_name,
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

    #[cfg(not(target_arch = "wasm32"))]
    fn sync_layer_names_to_firmware(&self) {
        let _ = self.write_layer_names_to_firmware(&self.layer_names);
    }

    /// Write `names` to firmware (qsid 200 + layer), one layer at a time so a
    /// single failing SET does not abort the rest. Skips names that already
    /// match. Returns the indices of layers whose write did not land, so callers
    /// like the .entlayout import can aggregate and report them. An empty result
    /// means every non-empty name was already correct or was written back —
    /// never a masked transport error.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn write_layer_names_to_firmware(&self, names: &[String]) -> Vec<usize> {
        if self.firmware != FirmwareProtocol::Vial {
            return Vec::new();
        }
        let Some(dev) = &self.hid_device else {
            return Vec::new();
        };
        let store = HidLayerNameStore {
            dev,
            supported: &self.supported_qmk_settings,
        };
        sync_layer_names_to_store(&store, names, self.layer_count)
    }
}

/// Storage seam for layer-name persistence, so the sync loop can be exercised
/// without a real HID device. `is_supported` reports whether the firmware
/// exposes storage for the setting id at all — used to keep genuine
/// "unsupported storage" apart from a transient read/write failure.
#[cfg(not(target_arch = "wasm32"))]
pub(super) trait LayerNameStore {
    fn is_supported(&self, qsid: u16) -> bool;
    fn get_string(&self, qsid: u16) -> anyhow::Result<String>;
    fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()>;
}

/// Production [`LayerNameStore`] backed by the open HID connection and the
/// setting ids the firmware advertised during connect.
#[cfg(not(target_arch = "wasm32"))]
struct HidLayerNameStore<'a> {
    dev: &'a crate::hid::HidDevice,
    supported: &'a [u16],
}

#[cfg(not(target_arch = "wasm32"))]
impl LayerNameStore for HidLayerNameStore<'_> {
    fn is_supported(&self, qsid: u16) -> bool {
        self.supported.contains(&qsid)
    }
    fn get_string(&self, qsid: u16) -> anyhow::Result<String> {
        self.dev.get_qmk_setting_string(qsid)
    }
    fn set_string(&self, qsid: u16, value: &str) -> anyhow::Result<()> {
        self.dev.set_qmk_setting_string(qsid, value)
    }
}

/// Persist `names` to `store`, one layer at a time. A layer is reported failed
/// only when its storage is supported yet a read or write actually errors — an
/// unsupported id is skipped silently (the name lives on in the local per-device
/// store), and a transport error never masquerades as "unsupported".
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn sync_layer_names_to_store<S: LayerNameStore>(
    store: &S,
    names: &[String],
    layer_count: usize,
) -> Vec<usize> {
    let mut failed = Vec::new();
    for (layer, name) in names.iter().enumerate().take(layer_count) {
        let qsid = 200 + layer as u16;
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if !store.is_supported(qsid) {
            // Firmware exposes no storage for this layer name; nothing to persist.
            continue;
        }
        match store.get_string(qsid) {
            Ok(current) if current == name => {}
            Ok(_) => {
                if let Err(e) = store.set_string(qsid, name) {
                    log::warn!(
                        "Vial set_qmk_setting_string failed while syncing layer {layer}: {e}"
                    );
                    failed.push(layer);
                }
            }
            Err(e) => {
                // The id is advertised as supported, so a read error is a real
                // transport failure, not missing storage — surface it.
                log::warn!("Vial get_qmk_setting_string failed while syncing layer {layer}: {e}");
                failed.push(layer);
            }
        }
    }
    failed
}
