use super::*;

fn vial_cache_dir() -> Option<std::path::PathBuf> {
    let dir = dirs::config_dir()?.join("entropy").join("vial_cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

const VIAL_DEFINITION_CACHE_VERSION: u8 = 4;
const QMK_SETTINGS_CACHE_VERSION: u8 = 2;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CachedVialDefinition {
    version: u8,
    runtime_firmware_version: String,
    json: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct QmkSettingsCacheContext {
    definition_size: u32,
    definition_fingerprint: u64,
    via_protocol: u16,
    vial_protocol: u32,
    runtime_firmware_version: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct CachedQmkSettings {
    version: u8,
    context: QmkSettingsCacheContext,
    settings: Vec<u16>,
}

impl CachedQmkSettings {
    fn new(context: &QmkSettingsCacheContext, settings: &[u16]) -> Self {
        Self {
            version: QMK_SETTINGS_CACHE_VERSION,
            context: context.clone(),
            settings: settings.to_vec(),
        }
    }

    fn matches(&self, context: &QmkSettingsCacheContext) -> bool {
        self.version == QMK_SETTINGS_CACHE_VERSION && &self.context == context
    }
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

fn device_cache_key_with_identity(
    device: &crate::device::Device,
    keyboard_id: u64,
    include_manufacturer: bool,
    include_serial: bool,
) -> String {
    // RMK boards can report the same Vial keyboard id across different layouts.
    // Keep the cache tied to the concrete HID identity so one board's layout
    // definition cannot be reused for another board. Bluetooth addresses are
    // deliberately excluded from the canonical key: BlueZ can expose a new
    // address after clearing a bond even though the keyboard schema is unchanged.
    let mut parts = vec![
        format!("{keyboard_id:016x}"),
        format!("{:04x}", device.vendor_id),
        format!("{:04x}", device.product_id),
        cache_component(&device.name),
    ];
    if include_manufacturer && !device.manufacturer.trim().is_empty() {
        parts.push(cache_component(&device.manufacturer));
    }
    if include_serial && !device.serial_number.trim().is_empty() {
        parts.push(cache_component(&device.serial_number));
    }
    parts.join("_")
}

fn device_cache_keys(device: &crate::device::Device, keyboard_id: u64) -> Vec<String> {
    if device.is_bluetooth_transport() {
        let mut keys = Vec::with_capacity(3);
        for (include_manufacturer, include_serial) in [(false, false), (true, false), (true, true)]
        {
            let key = device_cache_key_with_identity(
                device,
                keyboard_id,
                include_manufacturer,
                include_serial,
            );
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    } else {
        vec![device_cache_key_with_identity(
            device,
            keyboard_id,
            true,
            true,
        )]
    }
}

fn cached_vial_definition_file_name(cache_key: &str, definition_size: u32) -> String {
    format!("definition_v{VIAL_DEFINITION_CACHE_VERSION}_{cache_key}_{definition_size:08x}.json")
}

fn cached_vial_definition_path_in(
    cache_dir: &std::path::Path,
    cache_key: &str,
    definition_size: u32,
) -> std::path::PathBuf {
    cache_dir.join(cached_vial_definition_file_name(cache_key, definition_size))
}

fn cached_vial_definition_path(
    cache_key: &str,
    definition_size: u32,
) -> Option<std::path::PathBuf> {
    Some(cached_vial_definition_path_in(
        &vial_cache_dir()?,
        cache_key,
        definition_size,
    ))
}

fn cached_qmk_settings_path(cache_key: &str) -> Option<std::path::PathBuf> {
    Some(vial_cache_dir()?.join(format!("qmk_settings_{cache_key}.json")))
}

fn vial_definition_fingerprint(json: &serde_json::Value) -> Result<u64, serde_json::Error> {
    // FNV-1a is deterministic across app versions; cryptographic strength is unnecessary here.
    let mut hash = 0xcbf29ce484222325u64;
    for byte in serde_json::to_vec(json)? {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(hash)
}

fn qmk_settings_cache_context(
    definition_size: u32,
    json: &serde_json::Value,
    via_protocol: u16,
    vial_protocol: u32,
    runtime_firmware_version: &str,
) -> Result<QmkSettingsCacheContext, serde_json::Error> {
    Ok(QmkSettingsCacheContext {
        definition_size,
        definition_fingerprint: vial_definition_fingerprint(json)?,
        via_protocol,
        vial_protocol,
        runtime_firmware_version: runtime_firmware_version.trim().to_owned(),
    })
}

fn runtime_firmware_version_cache_token(version: Option<&str>) -> Option<&str> {
    version.map(str::trim).filter(|version| !version.is_empty())
}

fn parse_cached_vial_definition(
    text: &str,
    runtime_firmware_version: Option<&str>,
) -> Option<serde_json::Value> {
    let runtime_firmware_version = runtime_firmware_version_cache_token(runtime_firmware_version)?;
    let cached = serde_json::from_str::<CachedVialDefinition>(text).ok()?;
    (cached.version == VIAL_DEFINITION_CACHE_VERSION
        && cached.runtime_firmware_version == runtime_firmware_version)
        .then_some(cached.json)
}

fn load_cached_vial_definition_from_dir(
    cache_dir: &std::path::Path,
    cache_key: &str,
    definition_size: u32,
    runtime_firmware_version: Option<&str>,
) -> Option<serde_json::Value> {
    let path = cached_vial_definition_path_in(cache_dir, cache_key, definition_size);
    let text = std::fs::read_to_string(path).ok()?;
    parse_cached_vial_definition(&text, runtime_firmware_version)
}

fn load_cached_vial_definition(
    cache_keys: &[String],
    definition_size: u32,
    runtime_firmware_version: Option<&str>,
) -> Option<(serde_json::Value, String)> {
    let cache_dir = vial_cache_dir()?;
    cache_keys.iter().find_map(|cache_key| {
        load_cached_vial_definition_from_dir(
            &cache_dir,
            cache_key,
            definition_size,
            runtime_firmware_version,
        )
        .map(|json| (json, cache_key.clone()))
    })
}

fn save_cached_vial_definition(
    cache_key: &str,
    definition_size: u32,
    runtime_firmware_version: &str,
    json: &serde_json::Value,
) {
    let Some(path) = cached_vial_definition_path(cache_key, definition_size) else {
        return;
    };
    let cached = CachedVialDefinition {
        version: VIAL_DEFINITION_CACHE_VERSION,
        runtime_firmware_version: runtime_firmware_version.trim().to_owned(),
        json: json.clone(),
    };
    match serde_json::to_vec(&cached) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                log::warn!("failed to write Vial definition cache: {e}");
            }
        }
        Err(e) => log::warn!("failed to serialize Vial definition cache: {e}"),
    }
}

fn parse_cached_qmk_settings(text: &str, context: &QmkSettingsCacheContext) -> Option<Vec<u16>> {
    let cached = serde_json::from_str::<CachedQmkSettings>(text).ok()?;
    cached.matches(context).then_some(cached.settings)
}

fn load_cached_qmk_settings(
    cache_keys: &[String],
    context: &QmkSettingsCacheContext,
) -> Option<(Vec<u16>, String)> {
    cache_keys.iter().find_map(|cache_key| {
        let path = cached_qmk_settings_path(cache_key)?;
        let text = std::fs::read_to_string(path).ok()?;
        parse_cached_qmk_settings(&text, context).map(|settings| (settings, cache_key.clone()))
    })
}

fn save_cached_qmk_settings(cache_key: &str, context: &QmkSettingsCacheContext, settings: &[u16]) {
    let Some(path) = cached_qmk_settings_path(cache_key) else {
        return;
    };
    match serde_json::to_vec(&CachedQmkSettings::new(context, settings)) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(path, bytes) {
                log::warn!("failed to write QMK settings cache: {e}");
            }
        }
        Err(e) => log::warn!("failed to serialize QMK settings cache: {e}"),
    }
}

fn is_cached_vial_definition_for_device(file_name: &str, cache_key: &str) -> bool {
    let Some(stem) = file_name.strip_suffix(".json") else {
        return false;
    };
    let Some((device_prefix, definition_size)) = stem.rsplit_once('_') else {
        return false;
    };
    let expected_prefix = format!("definition_v{VIAL_DEFINITION_CACHE_VERSION}_{cache_key}");

    device_prefix == expected_prefix
        && definition_size.len() == 8
        && definition_size.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn clear_cached_device_data(cache_key: &str) -> Result<(), String> {
    let cache_dir =
        vial_cache_dir().ok_or_else(|| "device cache directory is unavailable".to_owned())?;
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&cache_dir)
        .map_err(|error| format!("failed to read {}: {error}", cache_dir.display()))?
    {
        let entry = entry.map_err(|error| {
            format!(
                "failed to inspect cached device data in {}: {error}",
                cache_dir.display()
            )
        })?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| is_cached_vial_definition_for_device(name, cache_key))
        {
            paths.push(entry.path());
        }
    }
    paths.push(cache_dir.join(format!("qmk_settings_{cache_key}.json")));

    for path in paths {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("failed to remove {}: {error}", path.display()));
            }
        }
    }
    Ok(())
}

fn normalize_reported_layer_count(reported_layer_count: usize) -> usize {
    reported_layer_count.max(1)
}

fn is_default_layer_name(index: usize, name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty()
        || trimmed == index.to_string()
        || (index == 0 && trimmed.eq_ignore_ascii_case("main"))
        || trimmed.eq_ignore_ascii_case(&format!("layer {index}"))
}

fn has_firmware_layer_names(names: &[String]) -> bool {
    names
        .iter()
        .enumerate()
        .any(|(index, name)| !is_default_layer_name(index, name))
}

fn layer_name_sync_updates(
    names: &[String],
    current_names: &[Option<String>],
    supported_qmk_settings: &[u16],
) -> Vec<(u16, String)> {
    names
        .iter()
        .enumerate()
        .filter_map(|(layer, name)| {
            let qsid = u16::try_from(layer).ok()?.checked_add(200)?;
            if !supported_qmk_settings.contains(&qsid) {
                return None;
            }

            let current = current_names.get(layer)?.as_deref()?;
            let name = name.trim();
            if name.is_empty() || current == name {
                None
            } else {
                Some((qsid, name.to_owned()))
            }
        })
        .collect()
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

fn supports_battery_halves_from_vial_json(json: &serde_json::Value) -> bool {
    let candidates = [
        json.get("entropy").and_then(|v| v.get("batteryHalves")),
        json.get("entropy").and_then(|v| v.get("battery_halves")),
        json.get("features")
            .and_then(|v| v.get("entropyBatteryHalves")),
        json.get("features")
            .and_then(|v| v.get("entropy_battery_halves")),
    ];

    candidates.into_iter().flatten().any(|value| {
        value.as_bool().unwrap_or(false)
            || value
                .as_str()
                .map(|text| {
                    matches!(
                        text.trim().to_ascii_lowercase().as_str(),
                        "true" | "1" | "yes"
                    )
                })
                .unwrap_or(false)
    })
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
    pub(super) fn refresh_current_device_data(&mut self) {
        let lang = self.app_settings.language;
        if self.hid_write_lifecycle_busy() {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "status_messages.refresh_device_data_pending_write")
                    .to_owned();
            return;
        }
        let Some(device_idx) = self.selected_device else {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "status_messages.refresh_device_data_missing_device")
                    .to_owned();
            return;
        };
        let Some(device) = self.device_manager.devices().get(device_idx).cloned() else {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "status_messages.refresh_device_data_missing_device")
                    .to_owned();
            return;
        };
        let Some(info) = self.device_about_info.as_ref() else {
            self.status_msg =
                crate::i18n::tr_catalog(lang, "status_messages.refresh_device_data_missing_info")
                    .to_owned();
            return;
        };

        let cache_keys = device_cache_keys(&device, info.keyboard_id);
        for cache_key in &cache_keys {
            if let Err(error) = clear_cached_device_data(cache_key) {
                self.status_msg = crate::i18n::tr_catalog(
                    lang,
                    "status_messages.refresh_device_data_delete_failed",
                )
                .to_owned();
                log::warn!("device cache refresh failed for key {cache_key}: {error}");
                return;
            }
        }

        log::info!(
            "Cleared Vial definition and QMK settings cache for keys {}",
            cache_keys.join(", ")
        );
        self.start_connect(device_idx);
    }

    pub(super) fn start_connect(&mut self, device_idx: usize) {
        self.start_connect_with_reconnect(device_idx, None);
    }

    pub(super) fn start_reconnect_connect(
        &mut self,
        device_idx: usize,
        reconnect: BluetoothReconnectState,
    ) {
        self.start_connect_with_reconnect(device_idx, Some(reconnect));
    }

    fn start_connect_with_reconnect(
        &mut self,
        device_idx: usize,
        reconnect: Option<BluetoothReconnectState>,
    ) {
        if self.hid_write_task_active() {
            if let Some(reconnect) = reconnect {
                self.schedule_bluetooth_reconnect_retry(reconnect, "HID write still active");
            } else {
                self.pending_device_connect = Some(device_idx);
            }
            return;
        }
        if self.qmk_settings_write_pending() {
            if reconnect.is_none() {
                self.pending_device_connect = Some(device_idx);
            }
            self.flush_pending_qmk_setting_writes();
            if self.qmk_settings_write_busy() {
                if let Some(reconnect) = reconnect {
                    self.schedule_bluetooth_reconnect_retry(
                        reconnect,
                        "QMK settings write still active",
                    );
                }
                return;
            }
        }
        self.pending_device_connect = None;
        self.pending_layout_undo = false;
        let dev = match self.device_manager.devices().get(device_idx) {
            Some(d) => d.clone(),
            None => {
                if let Some(reconnect) = reconnect {
                    self.schedule_bluetooth_reconnect_retry(reconnect, "device not found");
                } else {
                    self.status_msg = "Device not found".into();
                }
                return;
            }
        };

        if let Some(reconnect) = &reconnect {
            self.status_msg = crate::i18n::tr_catalog_format(
                self.app_settings.language,
                "connection.reconnecting",
                &[("device", &reconnect.display_name)],
            );
            self.hid_device = None;
            self.selected_key = None;
            self.selected_encoder = None;
            self.keycode_picker.open = false;
            self.reset_matrix_tester_state();
        } else {
            self.status_msg = format!(
                "Connecting to {}…",
                dev.display_name_with_transport(&dev.name)
            );
            self.layout = None;
            self.selected_key = None;
            self.selected_encoder = None;
            self.selected_layer = 0;
            self.layer_write_task = None;
            self.combo_write_task = None;
            self.settings_write_task = None;
            self.vial_hid_task = None;
            self.deferred_device_load = DeferredDeviceLoadState::default();
            self.deferred_full_layout_action = None;
            self.reset_settings_write_context();
            self.qmk_settings_write_queue.clear();
            self.hid_device = None;
            self.undo_stack.clear();
            self.device_about_info = None;
            self.next_battery_refresh_at = None;
            self.qmk_hid_hosts.clear();
            self.combo_visible_count = 1;
            self.combo_undo_stack.clear();
            self.combo_pick_target = None;
            self.combo_dirty = false;
            self.combo_synced_entries.clear();
            self.combo_edit_revision = self.combo_edit_revision.wrapping_add(1);
            self.combo_attempted_revision = None;
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
            self.vial_unlocked = None;
            self.vial_unlock_keys.clear();
            self.reset_matrix_tester_state();
        }

        let (tx, rx) = mpsc::channel();
        let now = std::time::Instant::now();
        self.connect_state = ConnectState::Loading {
            rx,
            started_at: now,
            last_progress_at: now,
            reconnect,
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
                    HidDevice::open_fresh_for(&dev).map_err(|e| format!("Open failed: {e:#}"))?;
                let staged_bluetooth_load = dev_conn.is_bluetooth_transport();

                progress("Reading VIA protocol version…");
                log::info!("Getting protocol version…");
                let via_protocol = dev_conn
                    .get_protocol_version()
                    .map_err(|e| format!("VIA protocol read failed: {e:#}"))?;
                log::info!("VIA protocol version: {via_protocol}");

                progress("Reading Vial keyboard id…");
                let (vial_protocol, keyboard_id) = dev_conn
                    .get_keyboard_id()
                    .map_err(|e| format!("Vial keyboard id read failed: {e:#}"))?;
                log::info!("Vial protocol: {vial_protocol}, keyboard id: {keyboard_id:016X}");
                let cache_keys = device_cache_keys(&dev, keyboard_id);
                let cache_key = &cache_keys[0];
                if ![-1i32, 9].contains(&(via_protocol as i32)) {
                    return Err(format!("Unsupported VIA protocol version: {via_protocol}"));
                }
                if !matches!(vial_protocol, 0..=6) {
                    return Err(format!(
                        "Unsupported Vial protocol version: {vial_protocol}"
                    ));
                }

                let vial_unlock_status = match dev_conn.get_unlock_status() {
                    Ok(status) => Some(status),
                    Err(error) => {
                        log::warn!("Vial unlock status read failed during connect: {error:#}");
                        None
                    }
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

                progress("Reading Vial layout definition…");
                log::info!("Getting layout JSON…");
                let definition_size = dev_conn
                    .get_definition_size()
                    .map_err(|e| format!("Layout size read failed: {e:#}"))?;
                let runtime_firmware_cache_token =
                    runtime_firmware_version_cache_token(runtime_firmware_version.as_deref());
                let json = if let Some((cached, source_cache_key)) = load_cached_vial_definition(
                    &cache_keys,
                    definition_size,
                    runtime_firmware_cache_token,
                ) {
                    log::info!(
                        "Loaded Vial definition from cache for keyboard id {keyboard_id:016X}, key {source_cache_key}, size {definition_size}"
                    );
                    if source_cache_key != *cache_key {
                        if let Some(runtime_firmware_version) = runtime_firmware_cache_token {
                            save_cached_vial_definition(
                                cache_key,
                                definition_size,
                                runtime_firmware_version,
                                &cached,
                            );
                        }
                    }
                    cached
                } else {
                    let json = dev_conn
                        .get_layout_json_with_size(definition_size)
                        .map_err(|e| format!("Layout read failed: {e:#}"))?;
                    if let Some(runtime_firmware_version) = runtime_firmware_cache_token {
                        save_cached_vial_definition(
                            cache_key,
                            definition_size,
                            runtime_firmware_version,
                            &json,
                        );
                    } else {
                        log::info!(
                            "Runtime firmware version unavailable; Vial definition and QMK settings caches disabled"
                        );
                    }
                    json
                };
                let firmware_version = runtime_firmware_version
                    .clone()
                    .or_else(|| firmware_version_from_vial_json(&json));
                let supports_battery_halves = supports_battery_halves_from_vial_json(&json);
                let battery_halves = if supports_battery_halves && !staged_bluetooth_load {
                    progress("Reading split battery levels…");
                    match dev_conn.get_battery_halves() {
                        Ok(levels) => levels,
                        Err(e) => {
                            log::warn!("split battery levels read failed: {e}");
                            None
                        }
                    }
                } else {
                    None
                };

                let touchpad_settings_in_definition =
                    Self::layout_json_has_touchpad_settings(&json);
                let qmk_cache_context = runtime_firmware_cache_token.and_then(
                    |runtime_firmware_version| match qmk_settings_cache_context(
                        definition_size,
                        &json,
                        via_protocol,
                        vial_protocol,
                        runtime_firmware_version,
                    ) {
                        Ok(context) => Some(context),
                        Err(error) => {
                            log::warn!(
                                "failed to fingerprint Vial definition; QMK settings cache disabled: {error}"
                            );
                            None
                        }
                    },
                );
                let supported_qmk_settings = if vial_protocol >= 4 {
                    if let Some(cached) = qmk_cache_context
                        .as_ref()
                        .and_then(|context| load_cached_qmk_settings(&cache_keys, context))
                    {
                        let (cached, source_cache_key) = cached;
                        log::info!(
                            "Loaded {} QMK settings from definition-aware cache for keyboard id {keyboard_id:016X}, key {source_cache_key}",
                            cached.len(),
                        );
                        if source_cache_key != *cache_key {
                            if let Some(context) = qmk_cache_context.as_ref() {
                                save_cached_qmk_settings(cache_key, context, &cached);
                            }
                        }
                        cached
                    } else {
                        progress("Querying QMK settings…");
                        match dev_conn.query_qmk_settings() {
                            Ok(settings) => {
                                if let Some(context) = qmk_cache_context.as_ref() {
                                    save_cached_qmk_settings(cache_key, context, &settings);
                                }
                                settings
                            }
                            Err(error) => {
                                log::warn!("qmk settings query failed: {error}");
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
                    .map_err(|e| format!("Layer count read failed: {e:#}"))?;
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
                let initial_layer_count = if staged_bluetooth_load {
                    1
                } else {
                    layer_count
                };
                match dev_conn.get_keymap_buffer(initial_layer_count, layout.rows, layout.cols) {
                    Ok(buf) => {
                        for layer in 0..initial_layer_count {
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
                        if staged_bluetooth_load {
                            return Err(format!("Initial Bluetooth layer read failed: {e:#}"));
                        }
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
                let mut current_firmware_layer_names = vec![None; layer_count];
                let mut layer_names_from_firmware = vec![false; layer_count];
                if has_qmk_setting(200) {
                    for (layer, current_firmware_name) in current_firmware_layer_names
                        .iter_mut()
                        .enumerate()
                        .take(initial_layer_count)
                    {
                        let qsid = 200 + layer as u16;
                        if !has_qmk_setting(qsid) {
                            continue;
                        }
                        match dev_conn.get_qmk_setting_string(qsid) {
                            Ok(name) => {
                                *current_firmware_name = Some(name.clone());
                                if !name.is_empty() {
                                    layout.layer_names[layer] = name;
                                    layer_names_from_firmware[layer] = true;
                                }
                            }
                            Err(e) => {
                                log::warn!("get_qmk_setting_string(layer name qsid {qsid}): {e}")
                            }
                        }
                    }
                }

                if staged_bluetooth_load || !has_firmware_layer_names(&layout.layer_names) {
                    if let Some(local_layer_names) = load_saved_layer_names(&dev.name) {
                        for (layer, name) in
                            local_layer_names.into_iter().enumerate().take(layer_count)
                        {
                            if !layer_names_from_firmware[layer] && !name.trim().is_empty() {
                                layout.layer_names[layer] = name;
                            }
                        }
                    }
                }

                let layer_name_updates = (!staged_bluetooth_load)
                    .then(|| {
                        layer_name_sync_updates(
                            &layout.layer_names,
                            &current_firmware_layer_names,
                            &supported_qmk_settings,
                        )
                    })
                    .unwrap_or_default();
                if !layer_name_updates.is_empty() {
                    progress("Syncing layer names…");
                    for (qsid, name) in layer_name_updates {
                        if let Err(e) = dev_conn.set_qmk_setting_string(qsid, &name) {
                            log::warn!(
                                "Vial set_qmk_setting_string failed while syncing qsid {qsid}: {e}"
                            );
                        }
                    }
                }

                progress("Reading Vial-core extras…");
                if !layout.encoders.is_empty() {
                    layout.encoder_layers = vec![vec![0u16; layout.encoders.len()]; layer_count];
                    let encoder_count = layout.encoder_count();
                    for layer in 0..initial_layer_count {
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
                                let macro_texts = if staged_bluetooth_load {
                                    vec![Vec::new(); count as usize]
                                } else {
                                    match dev_conn.get_macro_buffer(size, count) {
                                        Ok(buf) => crate::hid::HidDevice::parse_macros(&buf, count),
                                        Err(e) => {
                                            log::warn!("get_macro_buffer: {e}");
                                            vec![Vec::new(); count as usize]
                                        }
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
                let combo_entries = if staged_bluetooth_load {
                    vec![ComboEntry::default(); combo_count as usize]
                } else {
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

                let behavior_settings = if staged_bluetooth_load {
                    BehaviorSettingsState::default()
                } else {
                    progress("Reading QMK settings values…");
                    Self::read_behavior_settings(&supported_qmk_settings, &dev_conn)
                };

                let touchpad_settings = if staged_bluetooth_load {
                    TouchpadSettingsState::default()
                } else {
                    Self::read_touchpad_settings(&json, &supported_qmk_settings, &dev_conn)
                };

                progress("Reading Bluetooth settings…");
                let bluetooth_settings = if staged_bluetooth_load {
                    BluetoothSettingsState::default()
                } else {
                    Self::read_bluetooth_settings(&json, &supported_qmk_settings, &dev_conn)
                };

                progress("Reading module settings…");
                let module_settings = if staged_bluetooth_load {
                    ModuleSettingsState::default()
                } else {
                    Self::read_module_settings(&json, &supported_qmk_settings, &dev_conn)
                };

                let layer_led_settings = if staged_bluetooth_load {
                    LayerLedSettingsState::default()
                } else {
                    Self::read_layer_led_settings(
                        &json,
                        &supported_qmk_settings,
                        layer_count,
                        &dev_conn,
                    )
                };

                let rgb_settings = if staged_bluetooth_load {
                    RgbSettingsState::default()
                } else if layer_led_settings.supported && layout.lighting_mode.is_none() {
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
                let tap_dance_entries = if staged_bluetooth_load {
                    vec![crate::keycode_picker::TapDanceEntry::default(); tap_dance_count as usize]
                } else {
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
                let key_override_entries = if staged_bluetooth_load {
                    vec![KeyOverrideEntry::default(); key_override_count as usize]
                } else {
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

                let alt_repeat_entries = if staged_bluetooth_load {
                    vec![AltRepeatKeyEntry::default(); reported_alt_repeat_count as usize]
                } else {
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
                let deferred_load = if staged_bluetooth_load {
                    let definition_fingerprint = vial_definition_fingerprint(&json)
                        .map_err(|error| format!("Layout fingerprint failed: {error}"))?;
                    let modules_supported =
                        !Self::module_settings_groups(&json, &supported_qmk_settings).is_empty();
                    let touchpad_supported = touchpad_settings_in_definition
                        && [120u16, 121, 122, 123, 124]
                            .iter()
                            .all(|qsid| supported_qmk_settings.contains(qsid));
                    let bluetooth_supported =
                        Self::bluetooth_settings_supported(&json, &supported_qmk_settings);
                    let layer_leds_supported =
                        Self::layer_led_settings_supported(&json, &supported_qmk_settings);
                    DeferredDeviceLoadState::staged(DeferredDeviceLoadContext {
                        json: std::sync::Arc::new(json.clone()),
                        supported_qmk_settings: std::sync::Arc::new(supported_qmk_settings.clone()),
                        definition_fingerprint,
                        layer_count,
                        rows: layout.rows,
                        cols: layout.cols,
                        encoder_count: layout.encoder_count(),
                        macro_count: macro_texts.len().min(u8::MAX as usize) as u8,
                        macro_memory_bytes,
                        tap_dance_count,
                        combo_count,
                        key_override_count,
                        alt_repeat_count: reported_alt_repeat_count,
                        modules_supported,
                        touchpad_supported,
                        bluetooth_supported,
                        layer_leds_supported,
                        lighting_mode: layout.lighting_mode.clone(),
                    })
                } else {
                    DeferredDeviceLoadState::complete(layer_count)
                };

                let about_info = DeviceAboutInfo {
                    manufacturer: dev.manufacturer.clone(),
                    product: dev.name.clone(),
                    vendor_id: dev.vendor_id,
                    product_id: dev.product_id,
                    path: dev.path.clone(),
                    firmware_version,
                    supports_battery_halves,
                    battery_halves,
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
                    vial_unlock_status,
                    hid_device: Some(dev_conn),
                    about_info,
                    layer_names_from_firmware,
                    macro_texts,
                    supports_macro_ext_keycodes,
                    macro_ext_keycodes_disabled_reason,
                    tap_dance_entries,
                    combo_entries,
                    combo_term: behavior_settings.combo_term,
                    auto_shift_options: behavior_settings.auto_shift_options,
                    auto_shift_timeout: behavior_settings.auto_shift_timeout,
                    mouse_keys_settings: behavior_settings.mouse_keys,
                    touchpad_settings,
                    bluetooth_settings,
                    module_settings,
                    tap_hold_settings: behavior_settings.tap_hold,
                    magic_settings: behavior_settings.magic,
                    one_shot_settings: behavior_settings.one_shot,
                    grave_escape_settings: behavior_settings.grave_escape,
                    layer_led_settings,
                    rgb_settings,
                    layout_options_value,
                    key_override_entries,
                    alt_repeat_entries,
                    vial_features,
                    layout,
                    layer_count,
                    supported_qmk_settings,
                    deferred_load,
                })
            })();

            let _ = tx.send(ConnectTaskMessage::Done(Box::new(result)));
        });
    }

    pub(super) fn resume_pending_device_connect(&mut self) {
        if self.hid_write_task_owner_active() || self.qmk_settings_write_busy() {
            return;
        }
        if let Some(device_idx) = self.pending_device_connect.take() {
            self.start_connect(device_idx);
        }
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
    fn unsupported_layer_name_qsids_do_not_schedule_hid_requests() {
        let names = vec!["Main".to_owned(), "Nav".to_owned()];
        let current = vec![None, None];

        assert!(layer_name_sync_updates(&names, &current, &[1, 2, 3]).is_empty());
    }

    #[test]
    fn layer_name_sync_only_writes_supported_changed_names() {
        let names = vec!["Main".to_owned(), "Nav".to_owned(), "Symbols".to_owned()];
        let current = vec![
            Some("Main".to_owned()),
            Some(String::new()),
            Some("Old symbols".to_owned()),
        ];

        assert_eq!(
            layer_name_sync_updates(&names, &current, &[200, 201]),
            vec![(201, "Nav".to_owned())]
        );
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

    #[test]
    fn reads_entropy_battery_halves_metadata() {
        let json = serde_json::json!({
            "entropy": {
                "batteryHalves": true
            }
        });

        assert!(supports_battery_halves_from_vial_json(&json));
    }

    fn cache_device(bus_type: &str, serial_number: &str) -> crate::device::Device {
        crate::device::Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: serial_number.to_owned(),
            bus_type: bus_type.to_owned(),
            path: if bus_type.eq_ignore_ascii_case("bluetooth") {
                format!("/org/bluez/hci0/dev_{}", serial_number.replace(':', "_"))
            } else {
                "/dev/hidraw4".to_owned()
            },
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn bluetooth_schema_cache_key_survives_peer_address_changes() {
        let first = device_cache_keys(&cache_device("Bluetooth", "AA:BB:CC:DD:EE:01"), 0x1234);
        let second = device_cache_keys(&cache_device("Bluetooth", "AA:BB:CC:DD:EE:02"), 0x1234);

        assert_eq!(first[0], second[0]);
        assert_ne!(first.last(), second.last());
    }

    #[test]
    fn bluetooth_schema_cache_key_survives_kernel_and_bluez_metadata() {
        let kernel = cache_device("Bluetooth", "AA:BB:CC:DD:EE:01");
        let mut bluez = kernel.clone();
        bluez.manufacturer.clear();

        assert_eq!(
            device_cache_keys(&kernel, 0x1234)[0],
            device_cache_keys(&bluez, 0x1234)[0]
        );
    }

    #[test]
    fn usb_schema_cache_stays_bound_to_the_device_serial() {
        let first = device_cache_keys(&cache_device("Usb", "keyboard-1"), 0x1234);
        let second = device_cache_keys(&cache_device("Usb", "keyboard-2"), 0x1234);

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0], second[0]);
    }

    #[test]
    fn vial_definition_cache_filename_includes_schema_version_and_size() {
        assert_eq!(
            cached_vial_definition_file_name("keyboard", 0x1234),
            "definition_v4_keyboard_00001234.json"
        );
        assert_ne!(
            cached_vial_definition_file_name("keyboard", 0x1234),
            cached_vial_definition_file_name("keyboard", 0x1235)
        );
    }

    #[test]
    fn device_cache_refresh_matches_all_definition_sizes_for_only_one_device() {
        assert!(is_cached_vial_definition_for_device(
            "definition_v4_keyboard_00001234.json",
            "keyboard"
        ));
        assert!(is_cached_vial_definition_for_device(
            "definition_v4_keyboard_00005678.json",
            "keyboard"
        ));
        assert!(!is_cached_vial_definition_for_device(
            "definition_v4_other-keyboard_00001234.json",
            "keyboard"
        ));
        assert!(!is_cached_vial_definition_for_device(
            "definition_v3_keyboard_00001234.json",
            "keyboard"
        ));
        assert!(!is_cached_vial_definition_for_device(
            "definition_v4_keyboard_pro_00001234.json",
            "keyboard"
        ));
        assert!(!is_cached_vial_definition_for_device(
            "definition_v4_keyboard_0000123.json",
            "keyboard"
        ));
        assert!(!is_cached_vial_definition_for_device(
            "definition_v4_keyboard_not-hex.json",
            "keyboard"
        ));
    }

    fn cache_context(json: &serde_json::Value) -> QmkSettingsCacheContext {
        qmk_settings_cache_context(0x1234, json, 9, 6, "1.2.3").unwrap()
    }

    fn test_cache_dir(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "entropy-device-connect-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn cached_vial_definition_same_size_requires_direct_runtime_firmware_version() {
        let cache_dir = test_cache_dir("same-size-definition");
        let cache_key = "keyboard";
        let definition_size = 0x1234;
        let cached_json = serde_json::json!({"settings": [120]});
        let cached = CachedVialDefinition {
            version: VIAL_DEFINITION_CACHE_VERSION,
            runtime_firmware_version: "1.2.3".to_owned(),
            json: cached_json.clone(),
        };
        let path = cached_vial_definition_path_in(&cache_dir, cache_key, definition_size);
        std::fs::write(path, serde_json::to_vec(&cached).unwrap()).unwrap();

        assert_eq!(
            load_cached_vial_definition_from_dir(
                &cache_dir,
                cache_key,
                definition_size,
                Some("1.2.3")
            ),
            Some(cached_json)
        );
        assert_eq!(
            load_cached_vial_definition_from_dir(
                &cache_dir,
                cache_key,
                definition_size,
                Some("1.2.4")
            ),
            None
        );
        assert_eq!(
            load_cached_vial_definition_from_dir(&cache_dir, cache_key, definition_size, None),
            None
        );

        std::fs::remove_dir_all(cache_dir).unwrap();
    }

    #[test]
    fn qmk_settings_cache_reuses_matching_definition_and_protocol_metadata() {
        let context = cache_context(&serde_json::json!({"settings": [120, 121]}));
        let cached = CachedQmkSettings::new(&context, &[120, 121]);
        let text = serde_json::to_string(&cached).unwrap();

        assert_eq!(
            parse_cached_qmk_settings(&text, &context),
            Some(vec![120, 121])
        );
    }

    #[test]
    fn qmk_settings_cache_rejects_same_size_definition_changes() {
        let first_json = serde_json::json!({"settings": [120]});
        let second_json = serde_json::json!({"settings": [121]});
        assert_eq!(
            serde_json::to_vec(&first_json).unwrap().len(),
            serde_json::to_vec(&second_json).unwrap().len()
        );

        let first_context = cache_context(&first_json);
        let second_context = cache_context(&second_json);
        let cached = CachedQmkSettings::new(&first_context, &[120]);
        let text = serde_json::to_string(&cached).unwrap();

        assert_eq!(parse_cached_qmk_settings(&text, &second_context), None);
    }

    #[test]
    fn qmk_settings_cache_rejects_protocol_and_firmware_changes() {
        let context = cache_context(&serde_json::json!({"settings": [120]}));
        let cached = CachedQmkSettings::new(&context, &[120]);
        let text = serde_json::to_string(&cached).unwrap();

        let mut changed_size = context.clone();
        changed_size.definition_size += 1;
        assert_eq!(parse_cached_qmk_settings(&text, &changed_size), None);

        let mut changed_via_protocol = context.clone();
        changed_via_protocol.via_protocol += 1;
        assert_eq!(
            parse_cached_qmk_settings(&text, &changed_via_protocol),
            None
        );

        let mut changed_protocol = context.clone();
        changed_protocol.vial_protocol += 1;
        assert_eq!(parse_cached_qmk_settings(&text, &changed_protocol), None);

        let mut changed_firmware = context.clone();
        changed_firmware.runtime_firmware_version = "1.2.4".to_owned();
        assert_eq!(parse_cached_qmk_settings(&text, &changed_firmware), None);
    }

    #[test]
    fn runtime_firmware_version_cache_token_requires_a_direct_value() {
        assert_eq!(runtime_firmware_version_cache_token(None), None);
        assert_eq!(runtime_firmware_version_cache_token(Some("   ")), None);
        assert_eq!(
            runtime_firmware_version_cache_token(Some(" 1.2.3 ")),
            Some("1.2.3")
        );
    }

    #[test]
    fn qmk_settings_cache_rejects_legacy_unversioned_entries() {
        let context = cache_context(&serde_json::json!({"settings": [120]}));

        assert_eq!(parse_cached_qmk_settings("[120,121]", &context), None);
    }
}
