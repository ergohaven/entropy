use super::*;

fn vial_cache_dir() -> Option<std::path::PathBuf> {
    let dir = dirs::config_dir()?.join("entropy").join("vial_cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn cache_component(value: &str) -> String {
    let mut component = String::with_capacity(value.len());
    let mut previous_was_sep = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            component.push(ch);
            previous_was_sep = false;
        } else if !previous_was_sep && !component.is_empty() {
            component.push('-');
            previous_was_sep = true;
        }
    }
    while component.ends_with('-') {
        component.pop();
    }
    if component.is_empty() {
        "unknown".to_owned()
    } else {
        component
    }
}

fn device_cache_key(device: &crate::device::Device, keyboard_id: u64) -> String {
    // RMK boards can report the same Vial keyboard id across different layouts.
    // Keep the cache tied to the concrete HID identity so one board's layout
    // definition cannot be reused for another board.
    let mut parts = vec![
        format!("{keyboard_id:016x}"),
        format!("{:04x}", device.vendor_id),
        format!("{:04x}", device.product_id),
        cache_component(&device.name),
    ];
    if !device.manufacturer.trim().is_empty() {
        parts.push(cache_component(&device.manufacturer));
    }
    if !device.serial_number.trim().is_empty() {
        parts.push(cache_component(&device.serial_number));
    }
    parts.join("_")
}

fn cached_vial_definition_path(cache_key: &str) -> Option<std::path::PathBuf> {
    Some(vial_cache_dir()?.join(format!("definition_{cache_key}.json")))
}

fn cached_qmk_settings_path(cache_key: &str) -> Option<std::path::PathBuf> {
    Some(vial_cache_dir()?.join(format!("qmk_settings_{cache_key}.json")))
}

fn load_cached_vial_definition(cache_key: &str) -> Option<serde_json::Value> {
    let path = cached_vial_definition_path(cache_key)?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_vial_definition(cache_key: &str, json: &serde_json::Value) {
    let Some(path) = cached_vial_definition_path(cache_key) else {
        return;
    };
    match serde_json::to_vec(json) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                log::warn!("failed to write Vial definition cache: {e}");
            }
        }
        Err(e) => log::warn!("failed to serialize Vial definition cache: {e}"),
    }
}

fn load_cached_qmk_settings(cache_key: &str) -> Option<Vec<u16>> {
    let path = cached_qmk_settings_path(cache_key)?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_qmk_settings(cache_key: &str, settings: &[u16]) {
    let Some(path) = cached_qmk_settings_path(cache_key) else {
        return;
    };
    match serde_json::to_vec(settings) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                log::warn!("failed to write QMK settings cache: {e}");
            }
        }
        Err(e) => log::warn!("failed to serialize QMK settings cache: {e}"),
    }
}

fn normalize_reported_layer_count(reported_layer_count: usize) -> usize {
    reported_layer_count.max(1)
}

fn json_string_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn json_path_string(json: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut value = json;
    for key in path {
        value = value.get(*key)?;
    }
    json_string_value(value)
}

fn firmware_version_from_vial_json(json: &serde_json::Value) -> Option<String> {
    [
        &["firmware", "version"][..],
        &["firmware", "firmware_version"][..],
        &["firmware", "firmwareVersion"][..],
        &["firmware_version"][..],
        &["firmwareVersion"][..],
        &["qmk_firmware_version"][..],
        &["qmkFirmwareVersion"][..],
    ]
    .into_iter()
    .find_map(|path| json_path_string(json, path))
}

fn json_u16_value(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::Number(number) => {
            number.as_u64().and_then(|value| u16::try_from(value).ok())
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            let hex = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"));
            if let Some(hex) = hex {
                u16::from_str_radix(hex, 16).ok()
            } else {
                trimmed.parse::<u16>().ok()
            }
        }
        _ => None,
    }
}

fn json_path_u16(json: &serde_json::Value, path: &[&str]) -> Option<u16> {
    let mut value = json;
    for key in path {
        value = value.get(*key)?;
    }
    json_u16_value(value)
}

fn json_path_contains_rmk(json: &serde_json::Value, path: &[&str]) -> bool {
    json_path_string(json, path)
        .map(|value| value.to_ascii_lowercase().contains("rmk"))
        .unwrap_or(false)
}

fn is_rmk_vial_definition(json: &serde_json::Value) -> bool {
    let text_marker = [
        &["name"][..],
        &["manufacturer"][..],
        &["firmware", "name"][..],
        &["firmware", "manufacturer"][..],
    ]
    .into_iter()
    .any(|path| json_path_contains_rmk(json, path));
    if text_marker {
        return true;
    }

    json_path_u16(json, &["vendorId"]) == Some(0x4C4B)
        && json_path_u16(json, &["productId"]) == Some(0x4643)
}

fn macro_ext_keycodes_disabled_reason(
    json: &serde_json::Value,
) -> Option<MacroExtKeycodesDisabledReason> {
    is_rmk_vial_definition(json)
        .then_some(MacroExtKeycodesDisabledReason::RmkVialMacroExtUnsupported)
}

fn supports_vial_macro_ext_keycodes(vial_protocol: u32, json: &serde_json::Value) -> bool {
    vial_protocol >= 5 && macro_ext_keycodes_disabled_reason(json).is_none()
}

impl EntropyApp {
    pub(super) fn start_connect(&mut self, device_idx: usize) {
        if self.layer_write_task.is_some() {
            return;
        }
        let dev = match self.device_manager.devices().get(device_idx) {
            Some(d) => d.clone(),
            None => {
                self.status_msg = "Device not found".into();
                return;
            }
        };

        self.status_msg = format!("Connecting to {}…", dev.name);
        self.layout = None;
        self.selected_key = None;
        self.selected_encoder = None;
        self.selected_layer = 0;
        self.layer_write_task = None;
        self.hid_device = None;
        self.undo_stack.clear();
        self.device_about_info = None;
        self.qmk_hid_hosts.clear();
        self.combo_visible_count = 1;
        self.combo_undo_stack.clear();
        self.combo_pick_target = None;
        self.combo_dirty = false;
        self.combo_names_dirty = false;
        self.combo_colors_dirty = false;
        self.combo_term_dirty = false;
        self.auto_shift_options = AutoShiftOptionsState::default();
        self.auto_shift_timeout = None;
        self.auto_shift_timeout_text.clear();
        self.mouse_keys_settings = MouseKeysSettingsState::default();
        self.touchpad_settings = TouchpadSettingsState::default();
        self.bluetooth_settings = BluetoothSettingsState::default();
        self.tap_hold_settings = TapHoldSettingsState::default();
        self.magic_settings = MagicSettingsState::default();
        self.one_shot_settings = OneShotSettingsState::default();
        self.layer_led_settings = LayerLedSettingsState::default();
        self.alt_repeat_entries.clear();
        self.alt_repeat_names.clear();
        self.alt_repeat_undo_stack.clear();
        self.selected_alt_repeat = 0;
        self.alt_repeat_visible_count = 1;
        self.alt_repeat_pick_target = None;
        self.rgb_settings = RgbSettingsState::default();
        self.layout_options_value = None;
        self.encoder_visibility.clear();
        self.keycode_picker.macro_count = 0;
        self.keycode_picker.macro_texts.clear();
        self.keycode_picker.macro_names.clear();
        self.keycode_picker.macro_descriptions.clear();
        self.keycode_picker.macro_actions.clear();
        self.keycode_picker.macros_dirty = false;
        self.key_override_entries.clear();
        self.key_override_names.clear();
        self.key_override_visible_count = 1;
        self.key_override_undo_stack.clear();
        self.selected_key_override = 0;
        self.key_override_pick_target = None;
        self.reset_matrix_tester_state();

        let (tx, rx) = mpsc::channel();
        let now = std::time::Instant::now();
        self.connect_state = ConnectState::Loading {
            rx,
            started_at: now,
            last_progress_at: now,
        };

        std::thread::spawn(move || {
            let progress = |message: &str| {
                let _ = tx.send(ConnectTaskMessage::Progress(message.to_owned()));
            };
            let result = (|| -> Result<ConnectResult, String> {
                use crate::hid::HidDevice;

                progress("Opening HID device…");
                log::info!(
                    "Opening HID device: {} {:04X}:{:04X}",
                    dev.name,
                    dev.vendor_id,
                    dev.product_id
                );
                let dev_conn =
                    HidDevice::open_fresh_for(&dev).map_err(|e| format!("Open failed: {e}"))?;

                progress("Reading VIA protocol version…");
                log::info!("Getting protocol version…");
                let via_protocol = dev_conn
                    .get_protocol_version()
                    .map_err(|e| format!("VIA protocol read failed: {e}"))?;
                log::info!("VIA protocol version: {via_protocol}");

                progress("Reading Vial keyboard id…");
                let (vial_protocol, keyboard_id) = dev_conn
                    .get_keyboard_id()
                    .map_err(|e| format!("Vial keyboard id read failed: {e}"))?;
                log::info!("Vial protocol: {vial_protocol}, keyboard id: {keyboard_id:016X}");
                let cache_key = device_cache_key(&dev, keyboard_id);
                if ![-1i32, 9].contains(&(via_protocol as i32)) {
                    return Err(format!("Unsupported VIA protocol version: {via_protocol}"));
                }
                if !matches!(vial_protocol, 0..=6) {
                    return Err(format!(
                        "Unsupported Vial protocol version: {vial_protocol}"
                    ));
                }

                progress("Reading Vial layout definition…");
                log::info!("Getting layout JSON…");
                let json = if let Some(cached) = load_cached_vial_definition(&cache_key) {
                    log::info!(
                        "Loaded Vial definition from cache for keyboard id {keyboard_id:016X}, key {cache_key}"
                    );
                    cached
                } else {
                    let json = dev_conn
                        .get_layout_json()
                        .map_err(|e| format!("Layout read failed: {e}"))?;
                    save_cached_vial_definition(&cache_key, &json);
                    json
                };

                progress("Reading firmware version…");
                let runtime_firmware_version = match dev_conn.get_firmware_version() {
                    Ok(Some(version)) => Some(version),
                    Ok(None) => {
                        log::info!("Runtime firmware version is not reported");
                        None
                    }
                    Err(e) => {
                        log::warn!(
                            "Runtime firmware version read failed, falling back to Vial JSON metadata: {e}"
                        );
                        None
                    }
                };
                let firmware_version =
                    runtime_firmware_version.or_else(|| firmware_version_from_vial_json(&json));

                let touchpad_settings_in_definition =
                    Self::layout_json_has_touchpad_settings(&json);
                let supported_qmk_settings = if vial_protocol >= 4 {
                    if let Some(cached) = load_cached_qmk_settings(&cache_key) {
                        log::info!(
                            "Loaded {} QMK settings from cache for keyboard id {keyboard_id:016X}, key {cache_key}",
                            cached.len(),
                        );
                        cached
                    } else {
                        progress("Querying QMK settings…");
                        match dev_conn.query_qmk_settings() {
                            Ok(settings) => {
                                save_cached_qmk_settings(&cache_key, &settings);
                                settings
                            }
                            Err(e) => {
                                log::warn!("qmk settings query failed: {e}");
                                Vec::new()
                            }
                        }
                    }
                } else {
                    Vec::new()
                };
                let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);

                progress("Parsing keyboard layout…");
                let mut layout = KeyboardLayout::from_vial_json(&json)
                    .map_err(|e| format!("Layout parse failed: {e}"))?;

                progress("Reading layer count…");
                log::info!("Getting layer count…");
                let reported_layer_count = dev_conn
                    .get_layer_count()
                    .map(|c| c as usize)
                    .map_err(|e| format!("Layer count read failed: {e}"))?;
                let layer_count = normalize_reported_layer_count(reported_layer_count);
                if layer_count != reported_layer_count {
                    log::warn!(
                        "Device reported invalid layer count {reported_layer_count}; using {layer_count}"
                    );
                }
                log::info!("Layer count: {layer_count}");

                let num_keys = layout.keys.len();
                layout.layers = vec![vec![0u16; num_keys]; layer_count];

                progress("Reading keymap…");
                match dev_conn.get_keymap_buffer(layer_count, layout.rows, layout.cols) {
                    Ok(buf) => {
                        for layer in 0..layer_count {
                            for (ki, key) in layout.keys.iter().enumerate() {
                                let idx = layer * layout.rows * layout.cols
                                    + key.row as usize * layout.cols
                                    + key.col as usize;
                                if let Some(&kc) = buf.get(idx) {
                                    layout.layers[layer][ki] = kc;
                                }
                            }
                        }
                        log::info!("Keymap loaded from buffer");
                    }
                    Err(e) => {
                        log::warn!("get_keymap_buffer failed: {e}");
                    }
                }

                if layout.layer_names.len() < layer_count {
                    let start = layout.layer_names.len();
                    layout
                        .layer_names
                        .extend((start..layer_count).map(|layer| layer.to_string()));
                }
                layout.layer_names.truncate(layer_count);
                if has_qmk_setting(200) {
                    for layer in 0..layer_count {
                        let qsid = 200 + layer as u16;
                        if !has_qmk_setting(qsid) {
                            continue;
                        }
                        match dev_conn.get_qmk_setting_string(qsid) {
                            Ok(name) if !name.is_empty() => layout.layer_names[layer] = name,
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!("get_qmk_setting_string(layer name qsid {qsid}): {e}")
                            }
                        }
                    }
                }

                progress("Reading Vial-core extras…");
                if !layout.encoders.is_empty() {
                    layout.encoder_layers = vec![vec![0u16; layout.encoders.len()]; layer_count];
                    let encoder_count = layout.encoder_count();
                    for layer in 0..layer_count {
                        let mut per_encoder = vec![(0u16, 0u16); encoder_count];
                        for (encoder_idx, encoder_values) in
                            per_encoder.iter_mut().enumerate().take(encoder_count)
                        {
                            match dev_conn.get_encoder(layer as u8, encoder_idx as u8) {
                                Ok((ccw, cw)) => *encoder_values = (ccw, cw),
                                Err(e) => log::warn!(
                                    "get_encoder(layer={}, idx={}): {}",
                                    layer,
                                    encoder_idx,
                                    e
                                ),
                            }
                        }
                        for (visual_idx, encoder) in layout.encoders.iter().enumerate() {
                            if let Some((ccw, cw)) = per_encoder.get(encoder.encoder_idx as usize) {
                                layout.encoder_layers[layer][visual_idx] =
                                    if encoder.direction == 0 { *ccw } else { *cw };
                            }
                        }
                    }
                }

                let layout_options_value = if layout.layout_options.is_empty() {
                    None
                } else {
                    match dev_conn.get_layout_options() {
                        Ok(value) => Some(value),
                        Err(e) => {
                            log::warn!("get_layout_options: {e}");
                            None
                        }
                    }
                };

                progress("Reading macros…");
                let (macro_texts, macro_memory_bytes) = match dev_conn.get_macro_count() {
                    Ok(count) => {
                        log::info!("Macro count: {count}");
                        match dev_conn.get_macro_buffer_size() {
                            Ok(size) => {
                                log::info!("Macro buffer size: {size}");
                                let macro_texts = match dev_conn.get_macro_buffer(size, count) {
                                    Ok(buf) => crate::hid::HidDevice::parse_macros(&buf, count),
                                    Err(e) => {
                                        log::warn!("get_macro_buffer: {e}");
                                        vec![Vec::new(); count as usize]
                                    }
                                };
                                (macro_texts, Some(size))
                            }
                            Err(e) => {
                                log::warn!("get_macro_buffer_size: {e}");
                                (vec![Vec::new(); count as usize], None)
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("get_macro_count: {e}");
                        (Vec::new(), None)
                    }
                };

                let (
                    tap_dance_count,
                    combo_count,
                    key_override_count,
                    reported_alt_repeat_count,
                    dynamic_feature_bits,
                ) = if vial_protocol >= 4 {
                    progress("Reading dynamic feature counts…");
                    match dev_conn.get_dynamic_entry_counts() {
                        Ok(counts) => counts,
                        Err(e) => {
                            log::warn!("get_dynamic_entry_counts: {e}");
                            (0, 0, 0, 0, 0)
                        }
                    }
                } else {
                    (0, 0, 0, 0, 0)
                };
                let vial_features = VialFeatureSupport {
                    caps_word: dynamic_feature_bits & (1 << 0) != 0,
                    layer_lock: dynamic_feature_bits & (1 << 1) != 0,
                    persistent_default_layer: vial_protocol >= 5,
                    repeat_key: reported_alt_repeat_count > 0,
                };

                progress("Reading combos…");
                let combo_entries = {
                    let count = combo_count;
                    log::info!("Combo count: {count}");
                    let mut entries = Vec::new();
                    for i in 0..count {
                        match dev_conn.get_combo(i) {
                            Ok((keys, output)) => entries.push(ComboEntry { keys, output }),
                            Err(e) => {
                                log::warn!("get_combo({i}): {e}");
                                entries.push(Default::default());
                            }
                        }
                    }
                    entries
                };

                progress("Reading QMK settings values…");
                let combo_term = if has_qmk_setting(2) {
                    match dev_conn.get_qmk_setting_u16(2) {
                        Ok(value) => Some(value),
                        Err(e) => {
                            log::warn!("get_qmk_setting_u16(combo_term): {e}");
                            None
                        }
                    }
                } else {
                    None
                };
                let auto_shift_options = if has_qmk_setting(3) {
                    match dev_conn.get_qmk_setting_u8(3) {
                        Ok(value) => Some(AutoShiftOptionsState::from_bits(value)),
                        Err(e) => {
                            log::warn!("get_qmk_setting_u8(auto_shift_flags): {e}");
                            None
                        }
                    }
                } else {
                    None
                };
                let auto_shift_timeout = if has_qmk_setting(4) {
                    match dev_conn.get_qmk_setting_u16(4) {
                        Ok(value) => Some(value),
                        Err(e) => {
                            log::warn!("get_qmk_setting_u16(auto_shift_timeout): {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                let mouse_keys_settings = {
                    let mut mk = MouseKeysSettingsState::default();
                    match has_qmk_setting(9).then(|| dev_conn.get_qmk_setting_u8(9)) {
                        Some(Ok(v)) => {
                            mk.delay = v as u16;
                            mk.supported = true;
                            let read = |qsid: u16| -> u16 {
                                if !has_qmk_setting(qsid) {
                                    return 0;
                                }
                                match dev_conn.get_qmk_setting_u8(qsid) {
                                    Ok(val) => val as u16,
                                    Err(e) => {
                                        log::warn!(
                                            "get_qmk_setting_u8(mouse_keys qsid {qsid}): {e}"
                                        );
                                        0
                                    }
                                }
                            };
                            mk.interval = read(10);
                            mk.move_delta = read(11);
                            mk.max_speed = read(12);
                            mk.time_to_max = read(13);
                            mk.wheel_delay = read(14);
                            mk.wheel_interval = read(15);
                            mk.wheel_max_speed = read(16);
                            mk.wheel_time_to_max = read(17);
                        }
                        Some(Err(e)) => {
                            log::warn!("get_qmk_setting_u8(mouse_keys delay): {e}");
                        }
                        None => {}
                    }
                    mk
                };

                let touchpad_settings = {
                    let mut tp = TouchpadSettingsState::default();
                    if touchpad_settings_in_definition
                        && [120u16, 121, 122, 123, 124]
                            .iter()
                            .all(|qsid| supported_qmk_settings.contains(qsid))
                    {
                        tp.dpi_variants = Self::touchpad_setting_variants(&json, 120);
                        let dpi_read = if tp.dpi_variants.is_empty() {
                            dev_conn.get_qmk_setting_u16(120)
                        } else {
                            dev_conn.get_qmk_setting_u8(120).map(|value| value as u16)
                        };
                        match dpi_read {
                            Ok(v) => {
                                tp.dpi = v;
                                tp.supported = true;
                                tp.sniper_sens =
                                    dev_conn.get_qmk_setting_u8(121).unwrap_or_else(|e| {
                                        log::warn!("get_qmk_setting_u8(touchpad sniper sens): {e}");
                                        0
                                    });
                                tp.scroll_sens =
                                    dev_conn.get_qmk_setting_u8(122).unwrap_or_else(|e| {
                                        log::warn!("get_qmk_setting_u8(touchpad scroll sens): {e}");
                                        0
                                    });
                                tp.text_sens =
                                    dev_conn.get_qmk_setting_u8(123).unwrap_or_else(|e| {
                                        log::warn!("get_qmk_setting_u8(touchpad text sens): {e}");
                                        0
                                    });
                                tp.bits = dev_conn.get_qmk_setting_u8(124).unwrap_or_else(|e| {
                                    log::warn!("get_qmk_setting_u8(touchpad bits): {e}");
                                    0
                                });
                                if supported_qmk_settings.contains(&142)
                                    && Self::touchpad_setting_exists(&json, 142)
                                {
                                    tp.auto_layer_enable_supported = true;
                                    tp.auto_layer_enable = dev_conn
                                        .get_qmk_setting_u8(142)
                                        .map(|value| value != 0)
                                        .unwrap_or_else(|e| {
                                            log::warn!(
                                                "get_qmk_setting_u8(touchpad auto layer enable): {e}"
                                            );
                                            false
                                        });
                                }
                                if supported_qmk_settings.contains(&143)
                                    && Self::touchpad_setting_exists(&json, 143)
                                {
                                    tp.auto_layer_variants =
                                        Self::touchpad_setting_variants(&json, 143);
                                    tp.auto_layer =
                                        dev_conn.get_qmk_setting_u8(143).unwrap_or_else(|e| {
                                            log::warn!(
                                                "get_qmk_setting_u8(touchpad auto layer): {e}"
                                            );
                                            0
                                        });
                                }
                            }
                            Err(e) => {
                                log::warn!("get_qmk_setting(touchpad dpi): {e}");
                            }
                        }
                    }
                    tp
                };

                progress("Reading Bluetooth settings…");
                let bluetooth_settings =
                    Self::read_bluetooth_settings(&json, &supported_qmk_settings, &dev_conn);

                progress("Reading module settings…");
                let module_settings =
                    Self::read_module_settings(&json, &supported_qmk_settings, &dev_conn);

                let tap_hold_settings = {
                    let mut th = TapHoldSettingsState::default();
                    match has_qmk_setting(7).then(|| dev_conn.get_qmk_setting_u16(7)) {
                        Some(Ok(v)) => {
                            th.tapping_term = v;
                            th.supported = true;
                            for qsid in [7u16, 18, 19, 20, 22, 23, 24, 25, 26, 27] {
                                if has_qmk_setting(qsid) {
                                    th.set_qsid_supported(qsid);
                                }
                            }
                            let read_bool = |qsid: u16| -> bool {
                                if !has_qmk_setting(qsid) {
                                    return false;
                                }
                                match dev_conn.get_qmk_setting_u8(qsid) {
                                    Ok(val) => val != 0,
                                    Err(e) => {
                                        log::warn!("get_qmk_setting_u8(tap_hold qsid {qsid}): {e}");
                                        false
                                    }
                                }
                            };
                            let read_u16 = |qsid: u16| -> u16 {
                                if !has_qmk_setting(qsid) {
                                    return 0;
                                }
                                match dev_conn.get_qmk_setting_u16(qsid) {
                                    Ok(val) => val,
                                    Err(e) => {
                                        log::warn!(
                                            "get_qmk_setting_u16(tap_hold qsid {qsid}): {e}"
                                        );
                                        0
                                    }
                                }
                            };
                            th.permissive_hold = read_bool(22);
                            th.hold_on_other_key_press = read_bool(23);
                            th.retro_tapping = read_bool(24);
                            th.quick_tap_term = read_u16(25);
                            th.tap_code_delay = read_u16(18);
                            th.tap_hold_caps_delay = read_u16(19);
                            th.tapping_toggle = if has_qmk_setting(20) {
                                dev_conn
                                    .get_qmk_setting_u8(20)
                                    .map(|value| value as u16)
                                    .unwrap_or_else(|e| {
                                        log::warn!("get_qmk_setting_u8(tap_hold qsid 20): {e}");
                                        0
                                    })
                            } else {
                                0
                            };
                            th.chordal_hold = read_bool(26);
                            th.flow_tap = read_u16(27);
                        }
                        Some(Err(e)) => {
                            log::warn!("get_qmk_setting_u16(tap_hold tapping_term): {e}");
                        }
                        None => {}
                    }
                    th
                };

                let magic_settings = {
                    match has_qmk_setting(21).then(|| dev_conn.get_qmk_setting_u16(21)) {
                        Some(Ok(bits)) => MagicSettingsState {
                            bits,
                            supported: true,
                        },
                        Some(Err(e)) => {
                            log::warn!("get_qmk_setting_u16(magic qsid 21): {e}");
                            MagicSettingsState::default()
                        }
                        None => MagicSettingsState::default(),
                    }
                };

                let one_shot_settings = {
                    let mut os = OneShotSettingsState::default();
                    match has_qmk_setting(5).then(|| dev_conn.get_qmk_setting_u8(5)) {
                        Some(Ok(v)) => {
                            os.tap_toggle = v;
                            os.supported = true;
                            os.timeout = if has_qmk_setting(6) {
                                dev_conn.get_qmk_setting_u16(6).unwrap_or_else(|e| {
                                    log::warn!("get_qmk_setting_u16(one_shot timeout qsid 6): {e}");
                                    0
                                })
                            } else {
                                0
                            };
                        }
                        Some(Err(e)) => {
                            log::warn!("get_qmk_setting_u8(one_shot tap toggle qsid 5): {e}");
                        }
                        None => {}
                    }
                    os
                };

                let grave_escape_settings = {
                    match has_qmk_setting(1).then(|| dev_conn.get_qmk_setting_u8(1)) {
                        Some(Ok(bits)) => GraveEscapeSettingsState {
                            bits,
                            supported: true,
                        },
                        Some(Err(e)) => {
                            log::warn!("get_qmk_setting_u8(grave_escape qsid 1): {e}");
                            GraveEscapeSettingsState::default()
                        }
                        None => GraveEscapeSettingsState::default(),
                    }
                };

                let layer_led_settings = Self::read_layer_led_settings(
                    &json,
                    &supported_qmk_settings,
                    layer_count,
                    &dev_conn,
                );

                let rgb_settings = if layer_led_settings.supported && layout.lighting_mode.is_none()
                {
                    // hpd3-style Ergohaven boards use QMK RGBLight internally only as a
                    // transport for per-layer LEDs. If the Vial definition does not
                    // explicitly advertise a standard lighting backend, expose Layer LEDs
                    // instead of the generic RGB page.
                    RgbSettingsState::default()
                } else {
                    progress("Reading RGB settings…");
                    load_rgb_settings(&dev_conn, &layout)
                };

                progress("Reading tap dance entries…");
                let tap_dance_entries = {
                    let count = tap_dance_count;
                    log::info!("Tap dance count: {count}");
                    let mut entries = Vec::new();
                    for i in 0..count {
                        match dev_conn.get_tap_dance(i) {
                            Ok((tap, hold, dtap, taphold, term)) => {
                                entries.push(crate::keycode_picker::TapDanceEntry {
                                    on_tap: tap,
                                    on_hold: hold,
                                    on_double_tap: dtap,
                                    on_tap_hold: taphold,
                                    tapping_term: term,
                                });
                            }
                            Err(e) => {
                                log::warn!("get_tap_dance({i}): {e}");
                                entries.push(Default::default());
                            }
                        }
                    }
                    entries
                };

                progress("Reading key overrides…");
                let key_override_entries = {
                    let count = key_override_count;
                    log::info!("Key Override count: {count}");
                    let mut entries = Vec::new();
                    for i in 0..count {
                        match dev_conn.get_key_override(i) {
                            Ok((
                                trigger,
                                replacement,
                                layers,
                                trigger_mods,
                                negative_mod_mask,
                                suppressed_mods,
                                options,
                            )) => {
                                entries.push(KeyOverrideEntry {
                                    trigger,
                                    replacement,
                                    layers,
                                    trigger_mods,
                                    negative_mod_mask,
                                    suppressed_mods,
                                    options: KeyOverrideOptionsState::from_bits(options),
                                });
                            }
                            Err(e) => {
                                log::warn!("get_key_override({i}): {e}");
                                entries.push(Default::default());
                            }
                        }
                    }
                    entries
                };

                let alt_repeat_entries = {
                    let count = reported_alt_repeat_count;
                    log::info!("Alt Repeat count: {count}");
                    let mut entries = Vec::new();
                    for i in 0..count {
                        match dev_conn.get_alt_repeat_key(i) {
                            Ok((keycode, alt_keycode, allowed_mods, options)) => {
                                entries.push(AltRepeatKeyEntry {
                                    keycode,
                                    alt_keycode,
                                    allowed_mods,
                                    options: AltRepeatKeyOptionsState::from_bits(options),
                                });
                            }
                            Err(e) => {
                                log::warn!("get_alt_repeat_key({i}): {e}");
                                entries.push(Default::default());
                            }
                        }
                    }
                    entries
                };

                let macro_ext_keycodes_disabled_reason = macro_ext_keycodes_disabled_reason(&json);
                let supports_macro_ext_keycodes =
                    supports_vial_macro_ext_keycodes(vial_protocol, &json);

                let about_info = DeviceAboutInfo {
                    manufacturer: dev.manufacturer.clone(),
                    product: dev.name.clone(),
                    vendor_id: dev.vendor_id,
                    product_id: dev.product_id,
                    path: dev.path.clone(),
                    firmware_version,
                    via_protocol,
                    vial_protocol,
                    keyboard_id,
                    macro_entries: macro_texts.len(),
                    macro_memory_bytes,
                    supports_macro_delays: !macro_texts.is_empty(),
                    supports_macro_ext_keycodes,
                    macro_ext_keycodes_disabled_reason,
                    tap_dance_entries: tap_dance_entries.len(),
                    combo_entries: combo_entries.len(),
                    key_override_entries: key_override_entries.len(),
                    alt_repeat_entries: alt_repeat_entries.len(),
                    caps_word: vial_features.caps_word,
                    layer_lock: vial_features.layer_lock,
                    qmk_settings: !supported_qmk_settings.is_empty(),
                };

                progress("Applying keyboard layout…");
                Ok(ConnectResult {
                    device_name: dev.name.clone(),
                    keyboard_id,
                    hid_device: Some(dev_conn),
                    about_info,
                    macro_texts,
                    supports_macro_ext_keycodes,
                    macro_ext_keycodes_disabled_reason,
                    tap_dance_entries,
                    combo_entries,
                    combo_term,
                    auto_shift_options: auto_shift_options.unwrap_or_default(),
                    auto_shift_timeout,
                    mouse_keys_settings,
                    touchpad_settings,
                    bluetooth_settings,
                    module_settings,
                    tap_hold_settings,
                    magic_settings,
                    one_shot_settings,
                    grave_escape_settings,
                    layer_led_settings,
                    rgb_settings,
                    layout_options_value,
                    key_override_entries,
                    alt_repeat_entries,
                    vial_features,
                    layout,
                    layer_count,
                })
            })();

            let _ = tx.send(ConnectTaskMessage::Done(Box::new(result)));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_layer_count_is_never_zero() {
        assert_eq!(normalize_reported_layer_count(0), 1);
        assert_eq!(normalize_reported_layer_count(4), 4);
    }

    #[test]
    fn rmk_default_vial_definition_disables_macro_ext_keycodes() {
        let json = serde_json::json!({
            "name": "RMK Keyboard",
            "vendorId": "0x4C4B",
            "productId": "0x4643"
        });

        assert!(!supports_vial_macro_ext_keycodes(6, &json));
    }

    #[test]
    fn non_rmk_vial_protocol_five_keeps_macro_ext_keycodes_enabled() {
        let json = serde_json::json!({
            "name": "Entropy Keyboard",
            "vendorId": "0xFEED",
            "productId": "0x0001"
        });

        assert!(supports_vial_macro_ext_keycodes(5, &json));
    }

    #[test]
    fn reads_firmware_version_from_embedded_firmware_metadata() {
        let json = serde_json::json!({
            "firmware": {
                "version": "4.0.5"
            },
            "firmwareVersion": "3.9.9"
        });

        assert_eq!(
            firmware_version_from_vial_json(&json).as_deref(),
            Some("4.0.5")
        );
    }

    #[test]
    fn reads_legacy_firmware_version_metadata() {
        let json = serde_json::json!({
            "firmwareVersion": "4.0.5"
        });

        assert_eq!(
            firmware_version_from_vial_json(&json).as_deref(),
            Some("4.0.5")
        );
    }
}
