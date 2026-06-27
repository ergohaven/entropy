use super::*;

impl EntropyApp {
    pub(super) fn is_encoder_layout_option(option: &LayoutOption) -> bool {
        option
            .label
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("hide encoder")
    }

    pub(super) fn encoder_layout_option_indices(layout: &KeyboardLayout) -> Vec<usize> {
        layout
            .layout_options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| Self::is_encoder_layout_option(option).then_some(idx))
            .collect()
    }

    pub(super) fn layout_condition_visible(
        layout: &KeyboardLayout,
        condition: Option<crate::keyboard::LayoutCondition>,
        packed: Option<u32>,
    ) -> bool {
        let Some(condition) = condition else {
            return true;
        };
        let values = Self::unpack_layout_option_values(&layout.layout_options, packed.unwrap_or(0));
        values
            .get(condition.option_idx)
            .copied()
            .map(|value| value == condition.value)
            .unwrap_or(true)
    }

    pub(super) fn apply_encoder_layout_options_to_visibility(
        layout: &KeyboardLayout,
        packed: Option<u32>,
        visibility: &mut Vec<bool>,
    ) {
        let Some(packed) = packed else {
            return;
        };
        let option_indices = Self::encoder_layout_option_indices(layout);
        if option_indices.is_empty() {
            return;
        }

        let values = Self::unpack_layout_option_values(&layout.layout_options, packed);
        let encoder_indices = layout
            .encoders
            .iter()
            .map(|encoder| encoder.encoder_idx as usize)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        for (encoder_idx, option_idx) in encoder_indices.into_iter().zip(option_indices) {
            if visibility.len() <= encoder_idx {
                visibility.resize(encoder_idx + 1, true);
            }
            let hide_encoder = values.get(option_idx).copied().unwrap_or(0) != 0;
            visibility[encoder_idx] = !hide_encoder;
        }
    }

    pub(super) fn display_preset_choice_label(
        language: crate::i18n::Language,
        label: &str,
    ) -> String {
        let label = label
            .replace(" (qmk-hid-host)", " (Entropy)")
            .replace("qmk-hid-host", "Entropy");
        if matches!(language, crate::i18n::Language::Russian) {
            let lower = label.to_ascii_lowercase();
            for (prefix, translated) in [
                ("oled master", "OLED мастер"),
                ("oled slave", "OLED ведомый"),
            ] {
                if lower == prefix || lower.starts_with(&format!("{} ", prefix)) {
                    return format!("{}{}", translated, &label[prefix.len()..]);
                }
            }
        }
        crate::i18n::tr_text(language, &label)
    }

    pub(super) fn display_preset_needs_entropy(label: &str) -> bool {
        let lower = label.to_ascii_lowercase();
        lower.contains("qmk-hid-host")
            || lower.contains("clock")
            || lower.contains("volume")
            || lower.contains("media")
    }

    pub(super) fn static_display_preset_fallback_idx(option: &LayoutOption) -> Option<usize> {
        option
            .choices
            .iter()
            .position(|choice| choice.eq_ignore_ascii_case("disabled"))
    }

    pub(super) fn is_display_preset_layout_option(option: &LayoutOption) -> bool {
        !Self::is_encoder_layout_option(option)
            && option
                .choices
                .iter()
                .any(|choice| Self::display_preset_needs_entropy(choice))
            && Self::static_display_preset_fallback_idx(option).is_some()
    }

    pub(super) fn selected_layout_option_idx(
        option: &LayoutOption,
        values: &[u32],
        idx: usize,
    ) -> usize {
        values
            .get(idx)
            .copied()
            .unwrap_or(0)
            .min(option.choices.len().saturating_sub(1) as u32) as usize
    }

    pub(super) fn restore_display_preset_packed(
        layout: &KeyboardLayout,
        current_packed: u32,
        restore_packed: u32,
    ) -> Option<u32> {
        let mut current_values =
            Self::unpack_layout_option_values(&layout.layout_options, current_packed);
        let restore_values =
            Self::unpack_layout_option_values(&layout.layout_options, restore_packed);
        let mut changed = false;

        for (idx, option) in layout.layout_options.iter().enumerate() {
            if !Self::is_display_preset_layout_option(option) {
                continue;
            }
            let Some(disabled_idx) = Self::static_display_preset_fallback_idx(option) else {
                continue;
            };
            let current_idx = Self::selected_layout_option_idx(option, &current_values, idx);
            if current_idx != disabled_idx {
                continue;
            }
            let restore_idx = Self::selected_layout_option_idx(option, &restore_values, idx);
            let restore_needs_entropy = option
                .choices
                .get(restore_idx)
                .map(|choice| Self::display_preset_needs_entropy(choice))
                .unwrap_or(false);
            if !restore_needs_entropy {
                continue;
            }
            current_values[idx] = restore_idx as u32;
            changed = true;
        }

        changed.then(|| Self::pack_layout_option_values(&layout.layout_options, &current_values))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn save_display_preset_restore(&self, packed: u32) {
        if self.current_device_name.is_empty() {
            return;
        }
        if let Err(e) = std::fs::write(
            display_preset_restore_path(&self.current_device_name),
            packed.to_string(),
        ) {
            log::warn!("save display preset restore failed: {e}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn load_display_preset_restore(&self) -> Option<u32> {
        if self.current_device_name.is_empty() {
            return None;
        }
        std::fs::read_to_string(display_preset_restore_path(&self.current_device_name))
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn clear_display_preset_restore(&self) {
        if self.current_device_name.is_empty() {
            return;
        }
        let path = display_preset_restore_path(&self.current_device_name);
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("clear display preset restore failed: {e}");
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn restore_entropy_display_preset_after_connect(&mut self) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let Some(current_packed) = self.layout_options_value else {
            return;
        };
        let Some(restore_packed) = self.load_display_preset_restore() else {
            return;
        };
        let Some(packed) =
            Self::restore_display_preset_packed(layout, current_packed, restore_packed)
        else {
            return;
        };

        self.layout_options_value = Some(packed);
        if let Some(hid) = &self.hid_device {
            if let Err(e) = hid.set_layout_options(packed) {
                log::warn!("restore display preset after connect failed: {e}");
                self.layout_options_value = Some(current_packed);
                return;
            }
        }
        self.sync_qmk_hid_host_bridges();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn device_uses_automatic_display_host_data(device: &crate::device::Device) -> bool {
        if device.firmware != FirmwareProtocol::Vial {
            return false;
        }

        let name = device.name.to_ascii_lowercase();
        let ergohaven_macropad_display =
            device.vendor_id == 0xE126 && matches!(device.product_id, 0x0041 | 0x0042);

        ergohaven_macropad_display || name.contains("m4cr0pad v2") || name.contains("m4cr0pad v3")
    }

    pub(super) fn touchpad_setting_field(
        json: &serde_json::Value,
        qsid: u16,
    ) -> Option<&serde_json::Value> {
        json.get("settings")
            .and_then(|value| value.as_array())?
            .iter()
            .find(|tab| {
                tab.get("name")
                    .and_then(|value| value.as_str())
                    .map(|name| name.to_ascii_lowercase().contains("touchpad"))
                    .unwrap_or(false)
            })?
            .get("fields")
            .and_then(|value| value.as_array())?
            .iter()
            .find(|field| field.get("qsid").and_then(|value| value.as_u64()) == Some(qsid as u64))
    }

    pub(super) fn touchpad_setting_exists(json: &serde_json::Value, qsid: u16) -> bool {
        Self::touchpad_setting_field(json, qsid).is_some()
    }

    pub(super) fn touchpad_setting_variants(json: &serde_json::Value, qsid: u16) -> Vec<String> {
        Self::touchpad_setting_field(json, qsid)
            .and_then(|field| field.get("variants"))
            .and_then(|value| value.as_array())
            .map(|variants| {
                variants
                    .iter()
                    .filter_map(|value| value.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn layout_json_has_touchpad_settings(json: &serde_json::Value) -> bool {
        let Some(tabs) = json.get("settings").and_then(|value| value.as_array()) else {
            return false;
        };

        tabs.iter().any(|tab| {
            let tab_name = tab
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let has_touchpad_name = tab_name.contains("touchpad");
            let has_touchpad_qsids = tab
                .get("fields")
                .and_then(|value| value.as_array())
                .map(|fields| {
                    [120u64, 121, 122, 123, 124].iter().all(|qsid| {
                        fields.iter().any(|field| {
                            field.get("qsid").and_then(|value| value.as_u64()) == Some(*qsid)
                        })
                    })
                })
                .unwrap_or(false);

            has_touchpad_name && has_touchpad_qsids
        })
    }

    fn bluetooth_setting_variants(field: &serde_json::Value) -> Vec<String> {
        field
            .get("variants")
            .and_then(|value| value.as_array())
            .map(|variants| {
                variants
                    .iter()
                    .filter_map(|value| value.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_bluetooth_select_setting(
        dev_conn: &crate::hid::HidDevice,
        field: &serde_json::Value,
        qsid: u16,
        label: &str,
    ) -> Option<BluetoothSelectSetting> {
        let variants = Self::bluetooth_setting_variants(field);
        if variants.is_empty() {
            return None;
        }
        let width = Self::layer_led_field_width(field);
        let value = if width > 1 {
            dev_conn.get_qmk_setting_u16(qsid)
        } else {
            dev_conn.get_qmk_setting_u8(qsid).map(|value| value as u16)
        }
        .unwrap_or_else(|e| {
            log::warn!("get_qmk_setting({label} qsid {qsid}): {e}");
            0
        })
        .min(variants.len().saturating_sub(1) as u16);

        Some(BluetoothSelectSetting {
            qsid,
            width,
            value,
            variants,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_bluetooth_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> BluetoothSettingsState {
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);
        let mut sleep_timeout = None;
        let mut profile_colors = Vec::<(usize, BluetoothSelectSetting)>::new();

        if let Some(tabs) = json.get("settings").and_then(|value| value.as_array()) {
            for field in tabs
                .iter()
                .filter(|tab| {
                    tab.get("name")
                        .and_then(|value| value.as_str())
                        .map(|name| name.to_ascii_lowercase().contains("bluetooth"))
                        .unwrap_or(false)
                })
                .filter_map(|tab| tab.get("fields").and_then(|value| value.as_array()))
                .flatten()
            {
                let Some(qsid) = field
                    .get("qsid")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u16)
                else {
                    continue;
                };
                if !has_qmk_setting(qsid) {
                    continue;
                }
                let title = field
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let lower_title = title.to_ascii_lowercase();

                if lower_title.contains("sleep") && lower_title.contains("timeout") {
                    sleep_timeout = Self::read_bluetooth_select_setting(
                        dev_conn,
                        field,
                        qsid,
                        "bluetooth sleep timeout",
                    );
                } else if lower_title.contains("bt profile") && lower_title.contains("color") {
                    if let Some(profile) =
                        Self::parse_trailing_setting_index(&lower_title, "bt profile", "color")
                    {
                        if let Some(setting) = Self::read_bluetooth_select_setting(
                            dev_conn,
                            field,
                            qsid,
                            "bluetooth profile color",
                        ) {
                            profile_colors.push((profile, setting));
                        }
                    }
                }
            }
        }

        profile_colors.sort_by_key(|(profile, _)| *profile);
        let profile_colors = profile_colors
            .into_iter()
            .filter(|(profile, _)| *profile <= 4)
            .map(|(profile, setting)| BluetoothProfileColorSetting { profile, setting })
            .collect::<Vec<_>>();
        let supported = sleep_timeout.is_some() || !profile_colors.is_empty();

        BluetoothSettingsState {
            sleep_timeout,
            profile_colors,
            supported,
        }
    }

    fn parse_trailing_setting_index(
        lower_title: &str,
        prefix: &str,
        suffix: &str,
    ) -> Option<usize> {
        let rest = lower_title.strip_prefix(prefix)?.trim_start();
        let leading_digits = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if !leading_digits.is_empty() {
            let after_digits = rest[leading_digits.len()..].trim_start();
            if after_digits.starts_with(suffix) {
                return leading_digits.parse().ok();
            }
        }
        let rest = rest.strip_prefix(suffix)?.trim_start();
        let trailing_digits = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if trailing_digits.is_empty() {
            None
        } else {
            trailing_digits.parse().ok()
        }
    }

    fn layer_led_color_qsid_groups(
        mut entries: Vec<(usize, u16)>,
        layer_count: usize,
    ) -> Vec<(usize, Vec<u16>)> {
        entries.sort_by_key(|(layer, qsid)| (*layer, *qsid));
        let mut groups = Vec::<(usize, Vec<u16>)>::new();
        for (layer, qsid) in entries {
            if layer >= layer_count {
                continue;
            }
            if let Some((_, qsids)) = groups.iter_mut().find(|(existing, _)| *existing == layer) {
                if !qsids.contains(&qsid) {
                    qsids.push(qsid);
                }
            } else {
                groups.push((layer, vec![qsid]));
            }
        }
        groups
    }

    fn layer_led_field_width(field: &serde_json::Value) -> u8 {
        field
            .get("width")
            .and_then(|value| value.as_u64())
            .unwrap_or(1)
            .clamp(1, 2) as u8
    }

    fn layer_led_field_max(field: &serde_json::Value, width: u8) -> u16 {
        field
            .get("max")
            .and_then(|value| value.as_u64())
            .unwrap_or(if width > 1 {
                u16::MAX as u64
            } else {
                u8::MAX as u64
            })
            .min(u16::MAX as u64) as u16
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_layer_led_numeric_setting(
        dev_conn: &crate::hid::HidDevice,
        qsid: u16,
        width: u8,
        max: u16,
        label: &str,
    ) -> LayerLedNumericSetting {
        let value = if width > 1 {
            dev_conn.get_qmk_setting_u16(qsid)
        } else {
            dev_conn.get_qmk_setting_u8(qsid).map(|value| value as u16)
        }
        .unwrap_or_else(|e| {
            log::warn!("get_qmk_setting({label} qsid {qsid}): {e}");
            0
        })
        .min(max);

        LayerLedNumericSetting {
            qsid,
            width,
            value,
            max,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_layer_led_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        layer_count: usize,
        dev_conn: &crate::hid::HidDevice,
    ) -> LayerLedSettingsState {
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);
        let mut brightness = None;
        let mut timeout = None;
        let mut timeout_unit = LayerLedTimeoutUnit::Minutes;
        let mut bt_profiles = Vec::<(usize, u16)>::new();
        let mut layers = Vec::<(usize, u16)>::new();

        if let Some(tabs) = json.get("settings").and_then(|value| value.as_array()) {
            for field in tabs
                .iter()
                .filter(|tab| {
                    tab.get("name")
                        .and_then(|value| value.as_str())
                        .map(|name| name.to_ascii_lowercase().contains("led"))
                        .unwrap_or(false)
                })
                .filter_map(|tab| tab.get("fields").and_then(|value| value.as_array()))
                .flatten()
            {
                let Some(qsid) = field
                    .get("qsid")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u16)
                else {
                    continue;
                };
                if !has_qmk_setting(qsid) {
                    continue;
                }
                let title = field
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let lower_title = title.to_ascii_lowercase();
                let width = Self::layer_led_field_width(field);
                let max = Self::layer_led_field_max(field, width);

                if lower_title.contains("led brightness") {
                    brightness = Some(Self::read_layer_led_numeric_setting(
                        dev_conn,
                        qsid,
                        width,
                        max.min(255),
                        "layer_led brightness",
                    ));
                } else if lower_title.contains("led timeout") {
                    let lower_unit = field
                        .get("unit")
                        .or_else(|| field.get("units"))
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    timeout_unit = if lower_title.contains("sec")
                        || lower_title.contains("second")
                        || lower_unit.contains("sec")
                        || lower_unit.contains("second")
                        || (width > 1 && max > u8::MAX as u16)
                    {
                        LayerLedTimeoutUnit::Seconds
                    } else {
                        LayerLedTimeoutUnit::Minutes
                    };
                    timeout = Some(Self::read_layer_led_numeric_setting(
                        dev_conn,
                        qsid,
                        width,
                        max,
                        "layer_led timeout",
                    ));
                } else if lower_title.contains("bt profile") && lower_title.contains("color") {
                    if let Some(profile) =
                        Self::parse_trailing_setting_index(&lower_title, "bt profile", "color")
                    {
                        bt_profiles.push((profile, qsid));
                    }
                } else if lower_title.contains("layer") && lower_title.contains("color") {
                    if let Some(layer) =
                        Self::parse_trailing_setting_index(&lower_title, "layer", "color")
                    {
                        layers.push((layer, qsid));
                    }
                }
            }
        }

        if brightness.is_none() && timeout.is_none() && bt_profiles.is_empty() && layers.is_empty()
        {
            return Self::read_legacy_layer_led_settings(
                supported_qmk_settings,
                layer_count,
                dev_conn,
            );
        }

        bt_profiles.sort_by_key(|(profile, _)| *profile);
        layers.sort_by_key(|(layer, _)| *layer);

        let bt_profile_colors = bt_profiles
            .into_iter()
            .filter(|(profile, _)| *profile <= 4)
            .map(|(_, qsid)| {
                let value = dev_conn.get_qmk_setting_u8(qsid).unwrap_or_else(|e| {
                    log::warn!("get_qmk_setting_u8(layer_led bt profile qsid {qsid}): {e}");
                    0
                });
                LayerLedColorSetting::new(qsid, value)
            })
            .collect::<Vec<_>>();

        let layer_colors = Self::layer_led_color_qsid_groups(layers, layer_count)
            .into_iter()
            .filter_map(|(_, qsids)| {
                let (&qsid, linked_qsids) = qsids.split_first()?;
                let value = dev_conn.get_qmk_setting_u8(qsid).unwrap_or_else(|e| {
                    log::warn!("get_qmk_setting_u8(layer_led layer qsid {qsid}): {e}");
                    0
                });
                Some(LayerLedColorSetting::with_linked_qsids(
                    qsid,
                    linked_qsids.to_vec(),
                    value,
                ))
            })
            .collect::<Vec<_>>();

        let supported = brightness.is_some()
            || timeout.is_some()
            || !bt_profile_colors.is_empty()
            || !layer_colors.is_empty();
        LayerLedSettingsState {
            bt_profile_colors,
            layer_colors,
            brightness,
            timeout,
            timeout_unit,
            supported,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_legacy_layer_led_settings(
        supported_qmk_settings: &[u16],
        layer_count: usize,
        dev_conn: &crate::hid::HidDevice,
    ) -> LayerLedSettingsState {
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);
        let mut leds = LayerLedSettingsState::default();
        if !has_qmk_setting(300) {
            return leds;
        }

        const LAYER_LED_COLOR_QSID_BASE: u16 = 300;
        const LAYER_LED_BRIGHTNESS_QSID: u16 = 316;
        let max_color_layers =
            layer_count.min((LAYER_LED_BRIGHTNESS_QSID - LAYER_LED_COLOR_QSID_BASE) as usize);
        for layer in 0..max_color_layers {
            let qsid = LAYER_LED_COLOR_QSID_BASE + layer as u16;
            if !has_qmk_setting(qsid) {
                break;
            }
            let value = dev_conn.get_qmk_setting_u8(qsid).unwrap_or_else(|e| {
                log::warn!("get_qmk_setting_u8(layer_led qsid {qsid}): {e}");
                0
            });
            leds.layer_colors
                .push(LayerLedColorSetting::new(qsid, value));
        }
        leds.supported = !leds.layer_colors.is_empty();
        if leds.supported {
            if has_qmk_setting(316) {
                leds.brightness = Some(Self::read_layer_led_numeric_setting(
                    dev_conn,
                    316,
                    2,
                    255,
                    "layer_led brightness",
                ));
            }
            if has_qmk_setting(317) {
                leds.timeout = Some(Self::read_layer_led_numeric_setting(
                    dev_conn,
                    317,
                    1,
                    255,
                    "layer_led timeout",
                ));
            }
        }
        leds
    }

    fn parse_module_setting_field(
        field: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> Option<ModuleSettingField> {
        let qsid = field.get("qsid")?.as_u64()? as u16;
        if !supported_qmk_settings.contains(&qsid) {
            return None;
        }
        let title = field.get("title")?.as_str()?.trim().to_string();
        if title.is_empty() {
            return None;
        }
        let kind = match field.get("type").and_then(|value| value.as_str())? {
            "boolean" => ModuleSettingKind::Boolean,
            "integer" => ModuleSettingKind::Integer,
            "select" => ModuleSettingKind::Select,
            _ => return None,
        };
        let width = field
            .get("width")
            .and_then(|value| value.as_u64())
            .unwrap_or(1)
            .clamp(1, 2) as u8;
        let variants = field
            .get("variants")
            .and_then(|value| value.as_array())
            .map(|variants| {
                variants
                    .iter()
                    .filter_map(|value| value.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(ModuleSettingField {
            title,
            qsid,
            kind,
            bit: field
                .get("bit")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
                .min(15) as u8,
            width,
            min: field
                .get("min")
                .and_then(|value| value.as_u64())
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16,
            max: field
                .get("max")
                .and_then(|value| value.as_u64())
                .unwrap_or(if matches!(kind, ModuleSettingKind::Select) {
                    variants.len().saturating_sub(1) as u64
                } else if width > 1 {
                    u16::MAX as u64
                } else {
                    u8::MAX as u64
                })
                .min(u16::MAX as u64) as u16,
            variants,
        })
    }

    fn module_settings_group_kind(tab_name: &str) -> ModuleSettingsGroupKind {
        let name = tab_name.to_ascii_lowercase();
        if name.contains("left") {
            ModuleSettingsGroupKind::Left
        } else if name.contains("right") {
            ModuleSettingsGroupKind::Right
        } else if name.contains("auto") && name.contains("layer") {
            ModuleSettingsGroupKind::AutoLayer
        } else {
            ModuleSettingsGroupKind::Other
        }
    }

    pub(super) fn module_settings_groups(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> Vec<ModuleSettingsGroup> {
        let Some(tabs) = json.get("settings").and_then(|value| value.as_array()) else {
            return Vec::new();
        };

        let mut groups = tabs
            .iter()
            .filter_map(|tab| {
                let title = tab.get("name")?.as_str()?.trim().to_string();
                let normalized_title = title.to_ascii_lowercase();
                let is_modules_tab = normalized_title.contains("module");
                let is_auto_layer_tab =
                    normalized_title.contains("auto") && normalized_title.contains("layer");
                if !is_modules_tab && !is_auto_layer_tab {
                    return None;
                }
                let fields = tab
                    .get("fields")
                    .and_then(|value| value.as_array())?
                    .iter()
                    .filter_map(|field| {
                        Self::parse_module_setting_field(field, supported_qmk_settings)
                    })
                    .collect::<Vec<_>>();
                if fields.is_empty() {
                    return None;
                }
                Some(ModuleSettingsGroup {
                    kind: Self::module_settings_group_kind(&title),
                    title,
                    fields,
                })
            })
            .collect::<Vec<_>>();

        groups.sort_by_key(|group| match group.kind {
            ModuleSettingsGroupKind::Left => 0,
            ModuleSettingsGroupKind::Right => 1,
            ModuleSettingsGroupKind::AutoLayer => 2,
            ModuleSettingsGroupKind::Other => 3,
        });
        groups
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_module_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> ModuleSettingsState {
        let groups = Self::module_settings_groups(json, supported_qmk_settings);
        let fields = groups
            .iter()
            .flat_map(|group| group.fields.iter().cloned())
            .collect::<Vec<_>>();
        if fields.is_empty() {
            return ModuleSettingsState::default();
        }

        let mut settings = ModuleSettingsState {
            fields,
            groups,
            selected_module_group: 0,
            values: std::collections::BTreeMap::new(),
            supported: true,
        };
        let mut widths = std::collections::BTreeMap::<u16, u8>::new();
        for field in &settings.fields {
            widths
                .entry(field.qsid)
                .and_modify(|width| *width = (*width).max(field.width))
                .or_insert(field.width);
        }
        for (qsid, width) in widths {
            let value = if width > 1 {
                dev_conn.get_qmk_setting_u16(qsid)
            } else {
                dev_conn.get_qmk_setting_u8(qsid).map(|value| value as u16)
            }
            .unwrap_or_else(|e| {
                log::warn!("get_qmk_setting(module qsid {qsid}): {e}");
                0
            });
            settings.values.insert(qsid, value);
        }
        settings
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn qmk_hid_host_mode_for(
        layout: &KeyboardLayout,
        packed: Option<u32>,
    ) -> crate::qmk_hid_host::HostDataMode {
        let values = Self::unpack_layout_option_values(&layout.layout_options, packed.unwrap_or(0));
        let mut mode = crate::qmk_hid_host::HostDataMode::default();
        for (idx, option) in layout.layout_options.iter().enumerate() {
            if Self::is_encoder_layout_option(option) || option.choices.is_empty() {
                continue;
            }
            let selected_idx = values
                .get(idx)
                .copied()
                .unwrap_or(0)
                .min(option.choices.len().saturating_sub(1) as u32)
                as usize;
            let selected = option
                .choices
                .get(selected_idx)
                .map(|s| s.as_str())
                .unwrap_or("");
            let selected_lower = selected.to_ascii_lowercase();
            if Self::display_preset_needs_entropy(selected)
                && (selected_lower.contains("clock") || selected_lower.contains("volume"))
            {
                mode.clock_volume = true;
            }
            if Self::display_preset_needs_entropy(selected) && selected_lower.contains("media") {
                mode.media = true;
            }
        }
        mode
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn fallback_entropy_display_presets_before_exit(&mut self) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        if layout.layout_options.is_empty() {
            return;
        }

        let mut values = Self::unpack_layout_option_values(
            &layout.layout_options,
            self.layout_options_value.unwrap_or(0),
        );
        let mut changed = false;

        for (idx, option) in layout.layout_options.iter().enumerate() {
            if Self::is_encoder_layout_option(option) || option.choices.is_empty() {
                continue;
            }
            let selected_idx = values
                .get(idx)
                .copied()
                .unwrap_or(0)
                .min(option.choices.len().saturating_sub(1) as u32)
                as usize;
            let selected = option
                .choices
                .get(selected_idx)
                .map(|s| s.as_str())
                .unwrap_or("");
            if !Self::display_preset_needs_entropy(selected) {
                continue;
            }
            if let Some(fallback_idx) = Self::static_display_preset_fallback_idx(option) {
                if fallback_idx != selected_idx {
                    values[idx] = fallback_idx as u32;
                    changed = true;
                }
            }
        }

        if !changed {
            return;
        }

        let original_packed = self.layout_options_value.unwrap_or(0);
        let packed = Self::pack_layout_option_values(&layout.layout_options, &values);
        self.save_display_preset_restore(original_packed);
        self.layout_options_value = Some(packed);
        self.qmk_hid_hosts.clear();
        if let Some(hid) = &self.hid_device {
            if let Err(e) = hid.set_layout_options(packed) {
                log::warn!("fallback display preset before exit failed: {e}");
            }
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    pub(super) fn sync_qmk_hid_host_bridges(&mut self) {
        let selected_path = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(|device| device.path.as_str());

        let mut desired =
            std::collections::HashMap::<String, crate::qmk_hid_host::HostDataMode>::new();

        for device in self.device_manager.devices() {
            if device.firmware != FirmwareProtocol::Vial {
                continue;
            }

            let mut mode = crate::qmk_hid_host::HostDataMode::default();
            if Some(device.path.as_str()) == selected_path {
                if let Some(layout) = self.layout.as_ref() {
                    mode = Self::qmk_hid_host_mode_for(layout, self.layout_options_value);
                }
            }

            if Self::device_uses_automatic_display_host_data(device) {
                mode.clock_volume = true;
                mode.media = true;
            }

            if !mode.is_empty() {
                desired.insert(device.path.clone(), mode);
            }
        }

        self.qmk_hid_hosts
            .retain(|path, bridge| desired.get(path).copied() == Some(bridge.mode()));

        for (path, mode) in desired {
            self.qmk_hid_hosts
                .entry(path.clone())
                .or_insert_with(|| crate::qmk_hid_host::QmkHidHostBridge::start(path, mode));
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))]
    pub(super) fn sync_qmk_hid_host_bridges(&mut self) {
        // Keep the single-owner HID path on platforms where a second Raw HID open
        // can collide with the active Vial connection.
        self.qmk_hid_hosts.clear();
    }

    pub(super) fn open_layout_options_settings_page(&mut self) {
        self.settings_tab = SettingsTab::LayoutOptions;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_modules_settings_page(&mut self) {
        self.settings_tab = SettingsTab::Modules;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_touchpad_settings_page(&mut self) {
        self.settings_tab = SettingsTab::Touchpad;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_bluetooth_settings_page(&mut self) {
        self.settings_tab = SettingsTab::Bluetooth;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn open_live_features_settings_page(&mut self) {
        self.settings_tab = SettingsTab::LiveFeatures;
        self.main_menu_tab = MainMenuTab::Settings;
    }

    pub(super) fn layout_option_width(option: &LayoutOption) -> usize {
        if option.choices.is_empty() {
            1
        } else {
            let max_value = option.choices.len().saturating_sub(1).max(1);
            (usize::BITS - max_value.leading_zeros()) as usize
        }
    }

    pub(super) fn unpack_layout_option_values(options: &[LayoutOption], packed: u32) -> Vec<u32> {
        let mut values = vec![0; options.len()];
        let mut remaining = packed;
        for (idx, option) in options.iter().enumerate().rev() {
            let width = Self::layout_option_width(option);
            let mask = if width >= 32 {
                u32::MAX
            } else {
                (1u32 << width) - 1
            };
            values[idx] = remaining & mask;
            remaining >>= width.min(31);
        }
        values
    }

    pub(super) fn pack_layout_option_values(options: &[LayoutOption], values: &[u32]) -> u32 {
        let mut packed = 0u32;
        for (idx, option) in options.iter().enumerate() {
            let width = Self::layout_option_width(option);
            let mask = if width >= 32 {
                u32::MAX
            } else {
                (1u32 << width) - 1
            };
            packed = (packed << width.min(31)) | (values.get(idx).copied().unwrap_or(0) & mask);
        }
        packed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_led_color_qsid_groups_collapse_duplicate_layer_entries() {
        let groups = EntropyApp::layer_led_color_qsid_groups(
            vec![(0, 300), (1, 301), (0, 400), (1, 401)],
            4,
        );

        assert_eq!(groups, vec![(0, vec![300, 400]), (1, vec![301, 401])]);
    }

    #[test]
    fn layer_led_color_qsid_groups_ignore_out_of_range_layers_and_duplicate_qsids() {
        let groups = EntropyApp::layer_led_color_qsid_groups(
            vec![(0, 300), (2, 302), (1, 301), (1, 301)],
            2,
        );

        assert_eq!(groups, vec![(0, vec![300]), (1, vec![301])]);
    }
}
