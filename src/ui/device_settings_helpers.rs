use super::*;

impl EntropyApp {
    pub(super) fn is_encoder_layout_option(option: &LayoutOption) -> bool {
        if !option.choices.is_empty() {
            return false;
        }

        let label = option.label.trim().to_ascii_lowercase();
        label.starts_with("hide ")
            && label
                .split_whitespace()
                .any(|word| matches!(word, "encoder" | "encoders"))
    }

    pub(super) fn encoder_layout_option_indices(layout: &KeyboardLayout) -> Vec<usize> {
        layout
            .layout_options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| Self::is_encoder_layout_option(option).then_some(idx))
            .collect()
    }

    pub(super) fn firmware_managed_layout_option_indices(
        &self,
    ) -> std::collections::BTreeSet<usize> {
        self.module_settings
            .fields
            .iter()
            .filter_map(|field| field.layout_option)
            .collect()
    }

    pub(super) fn user_layout_option_indices(&self, layout: &KeyboardLayout) -> Vec<usize> {
        let firmware_managed = self.firmware_managed_layout_option_indices();
        layout
            .layout_options
            .iter()
            .enumerate()
            .filter_map(|(idx, option)| {
                (!Self::is_encoder_layout_option(option) && !firmware_managed.contains(&idx))
                    .then_some(idx)
            })
            .collect()
    }

    fn apply_firmware_managed_layout_options(
        options: &[LayoutOption],
        settings: &ModuleSettingsState,
        packed: u32,
    ) -> u32 {
        let mut values = Self::unpack_layout_option_values(options, packed);
        for field in &settings.fields {
            let Some(option_idx) = field.layout_option else {
                continue;
            };
            let Some(value) = values.get_mut(option_idx) else {
                continue;
            };
            let enabled = settings.value(field.qsid) & (1u16 << field.bit) != 0;
            *value = u32::from(enabled);
        }
        Self::pack_layout_option_values(options, &values)
    }

    pub(super) fn sync_firmware_managed_layout_options(&mut self) {
        let Some(options) = self
            .layout
            .as_ref()
            .map(|layout| layout.layout_options.clone())
        else {
            return;
        };
        if options.is_empty() {
            return;
        }

        let mut settings = self.module_settings.clone();
        for field in &self.module_settings.fields {
            if field.layout_option.is_none() {
                continue;
            }
            if let Some(pending) = self.pending_settings_write_value(field.qsid) {
                settings.set_value(field.qsid, pending);
            }
        }
        let packed = Self::apply_firmware_managed_layout_options(
            &options,
            &settings,
            self.layout_options_value.unwrap_or(0),
        );
        self.layout_options_value = Some(packed);
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

    fn module_group_encoder_field(group: &ModuleSettingsGroup) -> Option<&ModuleSettingField> {
        group
            .supports_module_kind(ModuleDeviceKind::Encoder)
            .then(|| group.module_selector_field())
            .flatten()
    }

    fn module_settings_encoder_visible_for_position(
        module_settings: &ModuleSettingsState,
        encoder_position: usize,
    ) -> bool {
        if !module_settings.supported {
            return true;
        }
        let module_groups = module_settings
            .groups
            .iter()
            .filter(|group| {
                matches!(
                    group.kind,
                    ModuleSettingsGroupKind::Left | ModuleSettingsGroupKind::Right
                ) && Self::module_group_encoder_field(group).is_some()
            })
            .collect::<Vec<_>>();
        let Some(group) = module_groups.get(encoder_position) else {
            return true;
        };
        let Some(field) = Self::module_group_encoder_field(group) else {
            return true;
        };
        match group.selected_module_kind(module_settings.value(field.qsid)) {
            Some(ModuleDeviceKind::Encoder) => true,
            Some(
                ModuleDeviceKind::None | ModuleDeviceKind::Trackball | ModuleDeviceKind::Touchpad,
            ) => false,
            Some(ModuleDeviceKind::Other) | None => true,
        }
    }

    pub(super) fn module_settings_encoder_visible(
        module_settings: &ModuleSettingsState,
        layout: &KeyboardLayout,
        encoder_idx: u8,
    ) -> bool {
        let encoder_indices = layout
            .encoders
            .iter()
            .map(|encoder| encoder.encoder_idx)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let Some(encoder_position) = encoder_indices
            .iter()
            .position(|candidate| *candidate == encoder_idx)
        else {
            return true;
        };
        Self::module_settings_encoder_visible_for_position(module_settings, encoder_position)
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

    fn settings_title_words(title: &str) -> Vec<String> {
        title
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    }

    fn is_generic_touchpad_settings_tab(tab: &serde_json::Value) -> bool {
        let Some(title) = tab.get("name").and_then(serde_json::Value::as_str) else {
            return false;
        };
        let words = Self::settings_title_words(title);
        let identifies_touchpad = words
            .iter()
            .any(|word| matches!(word.as_str(), "touchpad" | "trackpad"));
        let identifies_side = words
            .iter()
            .any(|word| matches!(word.as_str(), "left" | "right"));

        identifies_touchpad && !identifies_side
    }

    pub(super) fn touchpad_setting_field(
        json: &serde_json::Value,
        qsid: u16,
    ) -> Option<&serde_json::Value> {
        json.get("settings")
            .and_then(|value| value.as_array())?
            .iter()
            .filter(|tab| Self::is_generic_touchpad_settings_tab(tab))
            .filter_map(|tab| tab.get("fields").and_then(|value| value.as_array()))
            .flatten()
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
        [120u16, 121, 122, 123, 124]
            .iter()
            .all(|qsid| Self::touchpad_setting_exists(json, *qsid))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_behavior_settings(
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> BehaviorSettingsState {
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);

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
                Ok(value) => AutoShiftOptionsState::from_bits(value),
                Err(e) => {
                    log::warn!("get_qmk_setting_u8(auto_shift_flags): {e}");
                    AutoShiftOptionsState::default()
                }
            }
        } else {
            AutoShiftOptionsState::default()
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

        let mouse_keys = {
            let mut settings = MouseKeysSettingsState::default();
            match has_qmk_setting(9).then(|| dev_conn.get_qmk_setting_u8(9)) {
                Some(Ok(value)) => {
                    settings.delay = value as u16;
                    settings.supported = true;
                    let read = |qsid: u16| -> u16 {
                        if !has_qmk_setting(qsid) {
                            return 0;
                        }
                        match dev_conn.get_qmk_setting_u8(qsid) {
                            Ok(value) => value as u16,
                            Err(e) => {
                                log::warn!("get_qmk_setting_u8(mouse_keys qsid {qsid}): {e}");
                                0
                            }
                        }
                    };
                    settings.interval = read(10);
                    settings.move_delta = read(11);
                    settings.max_speed = read(12);
                    settings.time_to_max = read(13);
                    settings.wheel_delay = read(14);
                    settings.wheel_interval = read(15);
                    settings.wheel_max_speed = read(16);
                    settings.wheel_time_to_max = read(17);
                }
                Some(Err(e)) => {
                    log::warn!("get_qmk_setting_u8(mouse_keys delay): {e}");
                }
                None => {}
            }
            settings
        };

        let tap_hold = {
            let mut settings = TapHoldSettingsState::default();
            match has_qmk_setting(7).then(|| dev_conn.get_qmk_setting_u16(7)) {
                Some(Ok(value)) => {
                    settings.tapping_term = value;
                    settings.supported = true;
                    for qsid in [7u16, 18, 19, 20, 22, 23, 24, 25, 26, 27] {
                        if has_qmk_setting(qsid) {
                            settings.set_qsid_supported(qsid);
                        }
                    }
                    let read_bool = |qsid: u16| -> bool {
                        if !has_qmk_setting(qsid) {
                            return false;
                        }
                        match dev_conn.get_qmk_setting_u8(qsid) {
                            Ok(value) => value != 0,
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
                            Ok(value) => value,
                            Err(e) => {
                                log::warn!("get_qmk_setting_u16(tap_hold qsid {qsid}): {e}");
                                0
                            }
                        }
                    };
                    settings.permissive_hold = read_bool(22);
                    settings.hold_on_other_key_press = read_bool(23);
                    settings.retro_tapping = read_bool(24);
                    settings.quick_tap_term = read_u16(25);
                    settings.tap_code_delay = read_u16(18);
                    settings.tap_hold_caps_delay = read_u16(19);
                    settings.tapping_toggle = if has_qmk_setting(20) {
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
                    settings.chordal_hold = read_bool(26);
                    settings.flow_tap = read_u16(27);
                }
                Some(Err(e)) => {
                    log::warn!("get_qmk_setting_u16(tap_hold tapping_term): {e}");
                }
                None => {}
            }
            settings
        };

        let magic = match has_qmk_setting(21).then(|| dev_conn.get_qmk_setting_u16(21)) {
            Some(Ok(bits)) => MagicSettingsState {
                bits,
                supported: true,
            },
            Some(Err(e)) => {
                log::warn!("get_qmk_setting_u16(magic qsid 21): {e}");
                MagicSettingsState::default()
            }
            None => MagicSettingsState::default(),
        };

        let one_shot = {
            let mut settings = OneShotSettingsState::default();
            if has_qmk_setting(5) {
                match dev_conn.get_qmk_setting_u8(5) {
                    Ok(value) => {
                        settings.tap_toggle = value;
                        settings.set_qsid_supported(5);
                    }
                    Err(e) => {
                        log::warn!("get_qmk_setting_u8(one_shot tap toggle qsid 5): {e}");
                    }
                }
            }
            if has_qmk_setting(6) {
                match dev_conn.get_qmk_setting_u16(6) {
                    Ok(value) => {
                        settings.timeout = value;
                        settings.set_qsid_supported(6);
                    }
                    Err(e) => {
                        log::warn!("get_qmk_setting_u16(one_shot timeout qsid 6): {e}");
                    }
                }
            }
            settings.supported = settings.supported_qsids != 0;
            settings
        };

        let grave_escape = match has_qmk_setting(1).then(|| dev_conn.get_qmk_setting_u8(1)) {
            Some(Ok(bits)) => GraveEscapeSettingsState {
                bits,
                supported: true,
            },
            Some(Err(e)) => {
                log::warn!("get_qmk_setting_u8(grave_escape qsid 1): {e}");
                GraveEscapeSettingsState::default()
            }
            None => GraveEscapeSettingsState::default(),
        };

        BehaviorSettingsState {
            combo_term,
            auto_shift_options,
            auto_shift_timeout,
            mouse_keys,
            tap_hold,
            magic,
            one_shot,
            grave_escape,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_touchpad_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> TouchpadSettingsState {
        let mut settings = TouchpadSettingsState::default();
        if !Self::layout_json_has_touchpad_settings(json)
            || ![120u16, 121, 122, 123, 124]
                .iter()
                .all(|qsid| supported_qmk_settings.contains(qsid))
        {
            return settings;
        }

        settings.dpi_variants = Self::touchpad_setting_variants(json, 120);
        let dpi_read = if settings.dpi_variants.is_empty() {
            dev_conn.get_qmk_setting_u16(120)
        } else {
            dev_conn.get_qmk_setting_u8(120).map(|value| value as u16)
        };
        let Ok(dpi) = dpi_read else {
            log::warn!("get_qmk_setting(touchpad dpi) failed");
            return settings;
        };

        settings.dpi = dpi;
        settings.supported = true;
        settings.sniper_sens = dev_conn.get_qmk_setting_u8(121).unwrap_or_else(|error| {
            log::warn!("get_qmk_setting_u8(touchpad sniper sens): {error}");
            0
        });
        settings.scroll_sens = dev_conn.get_qmk_setting_u8(122).unwrap_or_else(|error| {
            log::warn!("get_qmk_setting_u8(touchpad scroll sens): {error}");
            0
        });
        settings.text_sens = dev_conn.get_qmk_setting_u8(123).unwrap_or_else(|error| {
            log::warn!("get_qmk_setting_u8(touchpad text sens): {error}");
            0
        });
        settings.bits = dev_conn.get_qmk_setting_u8(124).unwrap_or_else(|error| {
            log::warn!("get_qmk_setting_u8(touchpad bits): {error}");
            0
        });

        if supported_qmk_settings.contains(&142) && Self::touchpad_setting_exists(json, 142) {
            settings.auto_layer_enable_supported = true;
            settings.auto_layer_enable = dev_conn
                .get_qmk_setting_u8(142)
                .map(|value| value != 0)
                .unwrap_or_else(|error| {
                    log::warn!("get_qmk_setting_u8(touchpad auto layer enable): {error}");
                    false
                });
        }
        if supported_qmk_settings.contains(&143) && Self::touchpad_setting_exists(json, 143) {
            settings.auto_layer_variants = Self::touchpad_setting_variants(json, 143);
            settings.auto_layer = dev_conn.get_qmk_setting_u8(143).unwrap_or_else(|error| {
                log::warn!("get_qmk_setting_u8(touchpad auto layer): {error}");
                0
            });
        }

        settings
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
    fn read_bluetooth_boolean_setting(
        dev_conn: &crate::hid::HidDevice,
        field: &serde_json::Value,
        qsid: u16,
        label: &str,
    ) -> Option<BluetoothBooleanSetting> {
        if field.get("type").and_then(|value| value.as_str()) != Some("boolean") {
            return None;
        }
        let value = dev_conn.get_qmk_setting_u8(qsid).unwrap_or_else(|e| {
            log::warn!("get_qmk_setting({label} qsid {qsid}): {e}");
            0
        }) != 0;
        Some(BluetoothBooleanSetting { qsid, value })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_bluetooth_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> BluetoothSettingsState {
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);
        let mut sleep_timeout = None;
        let mut charge_indicator = None;
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
                } else if lower_title.contains("charge") && lower_title.contains("indicator") {
                    charge_indicator = Self::read_bluetooth_boolean_setting(
                        dev_conn,
                        field,
                        qsid,
                        "bluetooth charge indicator",
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
        let supported =
            sleep_timeout.is_some() || charge_indicator.is_some() || !profile_colors.is_empty();

        BluetoothSettingsState {
            sleep_timeout,
            charge_indicator,
            profile_colors,
            supported,
        }
    }

    pub(super) fn bluetooth_settings_supported(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> bool {
        json.get("settings")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|tab| {
                tab.get("name")
                    .and_then(|value| value.as_str())
                    .map(|name| name.to_ascii_lowercase().contains("bluetooth"))
                    .unwrap_or(false)
            })
            .filter_map(|tab| tab.get("fields").and_then(|value| value.as_array()))
            .flatten()
            .any(|field| {
                let Some(qsid) = field
                    .get("qsid")
                    .and_then(|value| value.as_u64())
                    .and_then(|value| u16::try_from(value).ok())
                else {
                    return false;
                };
                if !supported_qmk_settings.contains(&qsid) {
                    return false;
                }
                let title = field
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                (title.contains("sleep")
                    && title.contains("timeout")
                    && !Self::bluetooth_setting_variants(field).is_empty())
                    || (title.contains("charge")
                        && title.contains("indicator")
                        && field.get("type").and_then(|value| value.as_str()) == Some("boolean"))
                    || (title.contains("bt profile")
                        && title.contains("color")
                        && !Self::bluetooth_setting_variants(field).is_empty())
            })
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

    pub(super) fn layer_led_settings_supported(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> bool {
        if supported_qmk_settings.contains(&300) {
            return true;
        }
        json.get("settings")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|tab| {
                tab.get("name")
                    .and_then(|value| value.as_str())
                    .map(|name| name.to_ascii_lowercase().contains("led"))
                    .unwrap_or(false)
            })
            .filter_map(|tab| tab.get("fields").and_then(|value| value.as_array()))
            .flatten()
            .any(|field| {
                let Some(qsid) = field
                    .get("qsid")
                    .and_then(|value| value.as_u64())
                    .and_then(|value| u16::try_from(value).ok())
                else {
                    return false;
                };
                if !supported_qmk_settings.contains(&qsid) {
                    return false;
                }
                let title = field
                    .get("title")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                title.contains("led brightness")
                    || title.contains("led timeout")
                    || (title.contains("bt profile") && title.contains("color"))
                    || (title.contains("layer") && title.contains("color"))
            })
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
        let qsid = u16::try_from(field.get("qsid")?.as_u64()?).ok()?;
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
            layout_option: matches!(kind, ModuleSettingKind::Boolean)
                .then(|| {
                    field
                        .get("layoutOption")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok())
                        .filter(|value| *value < 32)
                })
                .flatten(),
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

    fn module_setting_widths(fields: &[ModuleSettingField]) -> std::collections::BTreeMap<u16, u8> {
        let mut widths = std::collections::BTreeMap::<u16, u8>::new();
        for field in fields {
            widths
                .entry(field.qsid)
                .and_modify(|width| *width = (*width).max(field.width))
                .or_insert(field.width);
        }
        widths
    }

    fn module_settings_kind_from_words(words: &[String]) -> Option<ModuleSettingsGroupKind> {
        match words.first().map(String::as_str) {
            Some("left") => Some(ModuleSettingsGroupKind::Left),
            Some("right") => Some(ModuleSettingsGroupKind::Right),
            _ if words
                .windows(2)
                .any(|pair| pair[0] == "auto" && pair[1] == "layer") =>
            {
                Some(ModuleSettingsGroupKind::AutoLayer)
            }
            _ => None,
        }
    }

    fn module_settings_metadata_kind(value: &serde_json::Value) -> Option<ModuleSettingsGroupKind> {
        ["side", "module_side", "moduleSide"]
            .iter()
            .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
            .find_map(|side| {
                Self::module_settings_kind_from_words(&Self::settings_title_words(side))
            })
    }

    fn is_module_settings_title(title: &str) -> bool {
        let words = Self::settings_title_words(title);
        let identifies_side = words
            .iter()
            .any(|word| matches!(word.as_str(), "left" | "right"));
        let identifies_controller = words.iter().any(|word| {
            matches!(
                word.as_str(),
                "controller" | "pointing" | "touchpad" | "trackpad"
            )
        });

        words
            .iter()
            .any(|word| matches!(word.as_str(), "module" | "modules" | "trackball"))
            || words
                .windows(2)
                .any(|pair| pair[0] == "auto" && pair[1] == "layer")
            || (identifies_side && identifies_controller)
    }

    fn is_module_setting_field_title(title: &str) -> bool {
        let words = Self::settings_title_words(title);
        if Self::module_settings_kind_from_words(&words).is_none() {
            return false;
        }

        words.iter().skip(1).any(|word| {
            matches!(
                word.as_str(),
                "module"
                    | "modules"
                    | "trackball"
                    | "ball"
                    | "touchpad"
                    | "scroll"
                    | "pointer"
                    | "pointing"
                    | "mode"
                    | "cpi"
                    | "dpi"
                    | "acceleration"
                    | "layer"
            )
        })
    }

    fn module_settings_group_title(
        tab_title: &str,
        tab_kind: Option<ModuleSettingsGroupKind>,
        group_kind: ModuleSettingsGroupKind,
    ) -> String {
        if tab_kind == Some(group_kind) || group_kind == ModuleSettingsGroupKind::Other {
            return tab_title.to_string();
        }

        let prefix = match group_kind {
            ModuleSettingsGroupKind::Left => "Left",
            ModuleSettingsGroupKind::Right => "Right",
            ModuleSettingsGroupKind::AutoLayer => "Auto Layer",
            ModuleSettingsGroupKind::Other => unreachable!(),
        };
        format!("{prefix} {tab_title}")
    }

    pub(super) fn module_settings_groups(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> Vec<ModuleSettingsGroup> {
        let Some(tabs) = json.get("settings").and_then(|value| value.as_array()) else {
            return Vec::new();
        };

        let mut groups = Vec::new();
        for tab in tabs {
            let Some(title) = tab
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
            else {
                continue;
            };
            let tab_metadata_kind = Self::module_settings_metadata_kind(tab);
            let tab_kind = tab_metadata_kind.or_else(|| {
                Self::module_settings_kind_from_words(&Self::settings_title_words(title))
            });
            let title_identifies_modules = Self::is_module_settings_title(title);
            let Some(raw_fields) = tab.get("fields").and_then(serde_json::Value::as_array) else {
                continue;
            };

            let parsed_fields = raw_fields
                .iter()
                .filter_map(|raw_field| {
                    let field =
                        Self::parse_module_setting_field(raw_field, supported_qmk_settings)?;
                    let metadata_kind = Self::module_settings_metadata_kind(raw_field);
                    let title_kind = Self::module_settings_kind_from_words(
                        &Self::settings_title_words(&field.title),
                    );
                    let identifies_modules = metadata_kind.is_some()
                        || Self::is_module_setting_field_title(&field.title);
                    Some((field, metadata_kind.or(title_kind), identifies_modules))
                })
                .collect::<Vec<_>>();
            if parsed_fields.is_empty()
                || (!title_identifies_modules
                    && tab_metadata_kind.is_none()
                    && !parsed_fields
                        .iter()
                        .any(|(_, _, identifies_modules)| *identifies_modules))
            {
                continue;
            }

            let mut partitioned = Vec::<(ModuleSettingsGroupKind, Vec<ModuleSettingField>)>::new();
            for (field, field_kind, _) in parsed_fields {
                let kind = field_kind
                    .or(tab_kind)
                    .unwrap_or(ModuleSettingsGroupKind::Other);
                if let Some((_, fields)) = partitioned
                    .iter_mut()
                    .find(|(existing_kind, _)| *existing_kind == kind)
                {
                    fields.push(field);
                } else {
                    partitioned.push((kind, vec![field]));
                }
            }

            groups.extend(
                partitioned
                    .into_iter()
                    .map(|(kind, fields)| ModuleSettingsGroup {
                        title: Self::module_settings_group_title(title, tab_kind, kind),
                        kind,
                        fields,
                    }),
            );
        }

        let mut coalesced_side_groups = Vec::<ModuleSettingsGroup>::new();
        for group in groups {
            if matches!(
                group.kind,
                ModuleSettingsGroupKind::Left
                    | ModuleSettingsGroupKind::Right
                    | ModuleSettingsGroupKind::AutoLayer
            ) {
                if let Some(existing) = coalesced_side_groups
                    .iter_mut()
                    .find(|existing| existing.kind == group.kind)
                {
                    existing.fields.extend(group.fields);
                    continue;
                }
            }
            coalesced_side_groups.push(group);
        }

        coalesced_side_groups.sort_by_key(|group| match group.kind {
            ModuleSettingsGroupKind::Left => 0,
            ModuleSettingsGroupKind::Right => 1,
            ModuleSettingsGroupKind::AutoLayer => 2,
            ModuleSettingsGroupKind::Other => 3,
        });
        coalesced_side_groups
    }

    pub(super) fn module_settings_from_definition(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
    ) -> ModuleSettingsState {
        let groups = Self::module_settings_groups(json, supported_qmk_settings);
        let fields = groups
            .iter()
            .flat_map(|group| group.fields.iter().cloned())
            .collect::<Vec<_>>();
        let supported = !fields.is_empty();
        ModuleSettingsState {
            fields,
            groups,
            selected_module_group: 0,
            values: std::collections::BTreeMap::new(),
            supported,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_initial_module_values(
        settings: &mut ModuleSettingsState,
        dev_conn: &crate::hid::HidDevice,
    ) {
        let selectors = settings
            .groups
            .iter()
            .flat_map(|group| group.fields.iter())
            .filter(|field| {
                field.layout_option.is_some()
                    || settings.groups.iter().any(|group| {
                        group
                            .module_selector_field()
                            .is_some_and(|selector| selector.qsid == field.qsid)
                    })
            })
            .map(|field| (field.qsid, field.width))
            .collect::<std::collections::BTreeMap<_, _>>();

        for (qsid, width) in selectors {
            let value = if width > 1 {
                dev_conn.get_qmk_setting_u16(qsid)
            } else {
                dev_conn.get_qmk_setting_u8(qsid).map(u16::from)
            };
            match value {
                Ok(value) => {
                    settings.values.insert(qsid, value);
                }
                Err(error) => {
                    log::warn!(
                        "get_qmk_setting(module selector qsid {qsid}) during staged load: {error}"
                    );
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn read_module_settings(
        json: &serde_json::Value,
        supported_qmk_settings: &[u16],
        dev_conn: &crate::hid::HidDevice,
    ) -> ModuleSettingsState {
        let mut settings = Self::module_settings_from_definition(json, supported_qmk_settings);
        if !settings.supported {
            return settings;
        }

        for (qsid, width) in Self::module_setting_widths(&settings.fields) {
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
    fn qmk_hid_host_metadata_mode(layout: &KeyboardLayout) -> crate::qmk_hid_host::HostDataMode {
        crate::qmk_hid_host::HostDataMode {
            time: layout.live_features.time,
            volume: layout.live_features.volume,
            layout: layout.live_features.layout
                || layout
                    .custom_keycodes
                    .iter()
                    .any(|keycode| keycode.name.eq_ignore_ascii_case("LG_SYNC")),
            media: layout.live_features.media,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn add_qmk_hid_host_display_preset(mode: &mut crate::qmk_hid_host::HostDataMode, preset: &str) {
        if !Self::display_preset_needs_entropy(preset) {
            return;
        }
        let preset = preset.to_ascii_lowercase();
        mode.time |= preset.contains("clock");
        mode.volume |= preset.contains("volume");
        mode.layout |= preset.contains("layout");
        mode.media |= preset.contains("media");
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn qmk_hid_host_supported_mode_for(
        layout: &KeyboardLayout,
    ) -> crate::qmk_hid_host::HostDataMode {
        let mut mode = Self::qmk_hid_host_metadata_mode(layout);
        for option in &layout.layout_options {
            if Self::is_encoder_layout_option(option) || option.choices.is_empty() {
                continue;
            }
            for choice in &option.choices {
                Self::add_qmk_hid_host_display_preset(&mut mode, choice);
            }
        }
        mode
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn qmk_hid_host_mode_for(
        layout: &KeyboardLayout,
        packed: Option<u32>,
    ) -> crate::qmk_hid_host::HostDataMode {
        let values = Self::unpack_layout_option_values(&layout.layout_options, packed.unwrap_or(0));
        let mut mode = Self::qmk_hid_host_metadata_mode(layout);
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
            Self::add_qmk_hid_host_display_preset(&mut mode, selected);
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

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn sync_qmk_hid_host_bridges(&mut self) {
        let selected_path = self
            .selected_device
            .and_then(|idx| self.device_manager.devices().get(idx))
            .map(|device| device.path.as_str());

        let mut desired = std::collections::HashMap::<
            String,
            (
                crate::device::Device,
                crate::qmk_hid_host::HostDataMode,
                Option<crate::hid::SharedHidOutput>,
            ),
        >::new();

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
                mode.time = true;
                mode.volume = true;
                mode.media = true;
            }
            if !self.app_settings.layout_sync_enabled {
                mode.layout = false;
            }

            if !mode.is_empty() {
                let shared_output = (Some(device.path.as_str()) == selected_path)
                    .then(|| self.shared_hid_output.clone())
                    .flatten();
                desired.insert(device.path.clone(), (device.clone(), mode, shared_output));
            }
        }

        self.qmk_hid_hosts.retain(|path, bridge| {
            desired.get(path).is_some_and(|(_, mode, shared_output)| {
                *mode == bridge.mode() && shared_output.is_some() == bridge.uses_shared_output()
            })
        });

        for (path, (device, mode, shared_output)) in desired {
            self.qmk_hid_hosts.entry(path).or_insert_with(|| {
                crate::qmk_hid_host::QmkHidHostBridge::start(device, mode, shared_output)
            });
        }
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

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

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

    #[test]
    fn module_settings_groups_include_trackball_settings_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Trackball",
                    "fields": [
                        {
                            "title": "Ball DPI",
                            "qsid": 120,
                            "type": "integer",
                            "width": 2,
                            "min": 100,
                            "max": 16000
                        }
                    ]
                }
            ]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Trackball");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Other);
        assert_eq!(groups[0].fields.len(), 1);
        assert_eq!(groups[0].fields[0].title, "Ball DPI");
        assert_eq!(groups[0].fields[0].qsid, 120);
        assert_eq!(groups[0].fields[0].kind, ModuleSettingKind::Integer);
    }

    #[test]
    fn generic_touchpad_settings_can_span_multiple_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Touchpad Pointer",
                    "fields": [
                        {
                            "title": "DPI",
                            "qsid": 120,
                            "type": "select",
                            "variants": ["400", "800"]
                        },
                        { "title": "Sniper sensitivity", "qsid": 121, "type": "integer" }
                    ]
                },
                {
                    "name": "Touchpad Behavior",
                    "fields": [
                        { "title": "Scroll sensitivity", "qsid": 122, "type": "integer" },
                        { "title": "Text sensitivity", "qsid": 123, "type": "integer" },
                        { "title": "Options", "qsid": 124, "type": "integer" }
                    ]
                }
            ]
        });

        assert!(EntropyApp::layout_json_has_touchpad_settings(&json));
        assert_eq!(
            EntropyApp::touchpad_setting_variants(&json, 120),
            vec!["400", "800"]
        );
        assert_eq!(
            EntropyApp::touchpad_setting_field(&json, 123)
                .and_then(|field| field.get("title"))
                .and_then(serde_json::Value::as_str),
            Some("Text sensitivity")
        );
        assert!(EntropyApp::module_settings_groups(&json, &[120, 121, 122, 123, 124]).is_empty());
    }

    #[test]
    fn generic_touchpad_settings_ignore_side_specific_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Left Touchpad",
                    "fields": [
                        { "title": "Ball DPI", "qsid": 120, "type": "select" },
                        { "title": "Touch DPI", "qsid": 122, "type": "select" },
                        { "title": "Scroll sensitivity", "qsid": 124, "type": "integer" }
                    ]
                },
                {
                    "name": "Right Touchpad",
                    "fields": [
                        { "title": "Ball DPI", "qsid": 121, "type": "select" },
                        { "title": "Touch DPI", "qsid": 123, "type": "select" }
                    ]
                }
            ]
        });

        assert!(!EntropyApp::layout_json_has_touchpad_settings(&json));
        assert!(EntropyApp::touchpad_setting_field(&json, 120).is_none());
    }

    #[test]
    fn module_settings_groups_include_split_touchpad_controller_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Left Touchpad",
                    "fields": [
                        {
                            "title": "Mode",
                            "qsid": 134,
                            "type": "select",
                            "variants": ["Normal", "Scroll"]
                        },
                        {
                            "title": "Scroll sensitivity",
                            "qsid": 125,
                            "type": "integer",
                            "min": 1,
                            "max": 255
                        }
                    ]
                },
                {
                    "name": "Right Controller",
                    "fields": [
                        {
                            "title": "Mode",
                            "qsid": 135,
                            "type": "select",
                            "variants": ["Normal", "Scroll"]
                        },
                        {
                            "title": "Scroll sensitivity",
                            "qsid": 128,
                            "type": "integer",
                            "min": 1,
                            "max": 255
                        }
                    ]
                }
            ]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[125, 128, 134, 135]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Left Touchpad");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Left);
        assert_eq!(
            groups[0]
                .fields
                .iter()
                .map(|field| field.qsid)
                .collect::<Vec<_>>(),
            vec![134, 125]
        );
        assert_eq!(groups[1].title, "Right Controller");
        assert_eq!(groups[1].kind, ModuleSettingsGroupKind::Right);
        assert_eq!(
            groups[1]
                .fields
                .iter()
                .map(|field| field.qsid)
                .collect::<Vec<_>>(),
            vec![135, 128]
        );
    }

    #[test]
    fn module_settings_groups_keep_known_split_controller_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Left Modules",
                    "fields": [module_select_field("Left Mode", 120)]
                },
                {
                    "name": "Right Modules",
                    "fields": [module_select_field("Right Mode", 121)]
                },
                {
                    "name": "Auto Layer",
                    "fields": [module_select_field("Timeout", 122)]
                }
            ]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120, 121, 122]);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].title, "Left Modules");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Left);
        assert_eq!(groups[0].fields[0].title, "Left Mode");
        assert_eq!(groups[1].title, "Right Modules");
        assert_eq!(groups[1].kind, ModuleSettingsGroupKind::Right);
        assert_eq!(groups[1].fields[0].title, "Right Mode");
        assert_eq!(groups[2].kind, ModuleSettingsGroupKind::AutoLayer);
    }

    #[test]
    fn module_settings_groups_coalesce_kinds_across_tabs() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Left Modules",
                    "fields": [module_select_field("Left Mode", 120)]
                },
                {
                    "name": "Right Modules",
                    "fields": [module_select_field("Right Mode", 121)]
                },
                {
                    "name": "Auto Layer",
                    "fields": [module_select_field("Timeout", 122)]
                },
                {
                    "name": "Controller Settings",
                    "fields": [
                        module_select_field("Left Scroll Sens", 123),
                        module_select_field("Right Scroll Sens", 124),
                        module_select_field("Auto Layer Timeout", 125)
                    ]
                }
            ]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120, 121, 122, 123, 124, 125]);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Left);
        assert_eq!(
            groups[0]
                .fields
                .iter()
                .map(|field| field.qsid)
                .collect::<Vec<_>>(),
            vec![120, 123]
        );
        assert_eq!(groups[1].kind, ModuleSettingsGroupKind::Right);
        assert_eq!(
            groups[1]
                .fields
                .iter()
                .map(|field| field.qsid)
                .collect::<Vec<_>>(),
            vec![121, 124]
        );
        assert_eq!(groups[2].kind, ModuleSettingsGroupKind::AutoLayer);
        assert_eq!(
            groups[2]
                .fields
                .iter()
                .map(|field| field.qsid)
                .collect::<Vec<_>>(),
            vec![122, 125]
        );
        let mut state = ModuleSettingsState {
            groups,
            selected_module_group: 0,
            ..Default::default()
        };
        assert_eq!(state.selected_module_group(), Some(0));
        state.set_selected_module_group(1);
        assert_eq!(state.selected_module_group(), Some(1));
    }

    #[test]
    fn module_settings_groups_split_mixed_controller_tabs() {
        let json = serde_json::json!({
            "settings": [{
                "name": "Modules",
                "fields": [
                    module_select_field("Left Mode", 120),
                    module_select_field("Right Mode", 121),
                    module_select_field("Shared Resolution", 122)
                ]
            }]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120, 121, 122]);

        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].title, "Left Modules");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Left);
        assert_eq!(groups[0].fields[0].title, "Left Mode");
        assert_eq!(groups[1].title, "Right Modules");
        assert_eq!(groups[1].kind, ModuleSettingsGroupKind::Right);
        assert_eq!(groups[1].fields[0].title, "Right Mode");
        assert_eq!(groups[2].title, "Modules");
        assert_eq!(groups[2].kind, ModuleSettingsGroupKind::Other);
        assert_eq!(groups[2].fields[0].title, "Shared Resolution");
    }

    #[test]
    fn module_settings_groups_prefer_explicit_side_metadata() {
        let json = serde_json::json!({
            "settings": [{
                "name": "Controller Settings",
                "fields": [
                    module_select_field("Mode", 120).with_value("side", "left"),
                    module_select_field("Mode", 121).with_value("module_side", "right")
                ]
            }]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120, 121]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Left Controller Settings");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Left);
        assert_eq!(groups[1].title, "Right Controller Settings");
        assert_eq!(groups[1].kind, ModuleSettingsGroupKind::Right);
    }

    #[test]
    fn module_settings_groups_do_not_match_side_name_substrings() {
        let json = serde_json::json!({
            "settings": [
                {
                    "name": "Brightness Modules",
                    "fields": [module_select_field("Brightness", 120)]
                },
                {
                    "name": "Modulator",
                    "fields": [module_select_field("Left Shift", 121)]
                },
                {
                    "name": "Left Keyboard",
                    "fields": [module_select_field("Layout", 122)]
                }
            ]
        });

        let groups = EntropyApp::module_settings_groups(&json, &[120, 121, 122]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Brightness Modules");
        assert_eq!(groups[0].kind, ModuleSettingsGroupKind::Other);
        assert_eq!(groups[0].fields[0].qsid, 120);
    }

    trait JsonValueExt {
        fn with_value(self, key: &str, value: &str) -> serde_json::Value;
    }

    impl JsonValueExt for serde_json::Value {
        fn with_value(mut self, key: &str, value: &str) -> serde_json::Value {
            self[key] = serde_json::Value::String(value.to_string());
            self
        }
    }

    fn module_select_field(title: &str, qsid: u16) -> serde_json::Value {
        serde_json::json!({
            "title": title,
            "qsid": qsid,
            "type": "select",
            "variants": ["Normal", "Scrolling"]
        })
    }

    fn test_module_field(title: &str, qsid: u16) -> ModuleSettingField {
        ModuleSettingField {
            title: title.to_string(),
            qsid,
            kind: ModuleSettingKind::Select,
            bit: 0,
            layout_option: None,
            width: 1,
            min: 0,
            max: 2,
            variants: vec![
                "Trackball".to_string(),
                "Touchpad".to_string(),
                "Encoder".to_string(),
            ],
        }
    }

    fn test_layout_with_encoders(indices: &[u8]) -> KeyboardLayout {
        KeyboardLayout {
            name: "Test".to_string(),
            rows: 1,
            cols: 1,
            keys: Vec::new(),
            encoders: indices
                .iter()
                .map(|encoder_idx| PhysicalEncoder {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    label: String::new(),
                    encoder_idx: *encoder_idx,
                    direction: 0,
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            layers: Vec::new(),
            encoder_layers: Vec::new(),
            layer_names: Vec::new(),
            custom_keycodes: Vec::new(),
            layout_options: Vec::new(),
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn module_settings_hide_encoder_when_side_module_is_not_encoder() {
        let layout = test_layout_with_encoders(&[0, 1]);
        let settings = ModuleSettingsState {
            groups: vec![
                ModuleSettingsGroup {
                    title: "Left Module".to_string(),
                    kind: ModuleSettingsGroupKind::Left,
                    fields: vec![test_module_field("Left Module", 300)],
                },
                ModuleSettingsGroup {
                    title: "Right Module".to_string(),
                    kind: ModuleSettingsGroupKind::Right,
                    fields: vec![test_module_field("Right Module", 301)],
                },
            ],
            values: std::collections::BTreeMap::from([(300, 0), (301, 2)]),
            supported: true,
            ..Default::default()
        };

        assert!(!EntropyApp::module_settings_encoder_visible(
            &settings, &layout, 0
        ));
        assert!(EntropyApp::module_settings_encoder_visible(
            &settings, &layout, 1
        ));
    }

    #[test]
    fn module_settings_do_not_filter_keyboards_without_encoder_module_choice() {
        let layout = test_layout_with_encoders(&[0]);
        let settings = ModuleSettingsState {
            groups: vec![ModuleSettingsGroup {
                title: "Left Module".to_string(),
                kind: ModuleSettingsGroupKind::Left,
                fields: vec![ModuleSettingField {
                    title: "Left Module".to_string(),
                    qsid: 300,
                    kind: ModuleSettingKind::Select,
                    bit: 0,
                    layout_option: None,
                    width: 1,
                    min: 0,
                    max: 1,
                    variants: vec!["Trackball".to_string(), "Touchpad".to_string()],
                }],
            }],
            values: std::collections::BTreeMap::from([(300, 0)]),
            supported: true,
            ..Default::default()
        };

        assert!(EntropyApp::module_settings_encoder_visible(
            &settings, &layout, 0
        ));
    }

    #[test]
    fn encoder_layout_options_include_left_and_right_module_labels() {
        let mut layout = test_layout_with_encoders(&[0, 1]);
        layout.layout_options = vec![
            LayoutOption {
                label: "Hide left encoder module".to_string(),
                choices: Vec::new(),
            },
            LayoutOption {
                label: "Hide right encoder module".to_string(),
                choices: Vec::new(),
            },
            LayoutOption {
                label: "OLED master".to_string(),
                choices: vec!["Disabled".to_string(), "Clock".to_string()],
            },
        ];

        assert_eq!(
            EntropyApp::encoder_layout_option_indices(&layout),
            vec![0, 1]
        );

        let packed = EntropyApp::pack_layout_option_values(&layout.layout_options, &[1, 0, 0]);
        let mut visibility = vec![true, true];
        EntropyApp::apply_encoder_layout_options_to_visibility(
            &layout,
            Some(packed),
            &mut visibility,
        );
        assert_eq!(visibility, vec![false, true]);
    }

    #[test]
    fn encoder_layout_option_requires_boolean_hide_label() {
        for label in [
            "Hide encoder",
            "Hide left encoder",
            "Hide right encoder",
            "Hide left encoder module",
            "Hide right encoder module",
        ] {
            assert!(EntropyApp::is_encoder_layout_option(&LayoutOption {
                label: label.to_string(),
                choices: Vec::new(),
            }));
        }

        assert!(!EntropyApp::is_encoder_layout_option(&LayoutOption {
            label: "Encoder display preset".to_string(),
            choices: Vec::new(),
        }));
        assert!(!EntropyApp::is_encoder_layout_option(&LayoutOption {
            label: "Hide encoder style".to_string(),
            choices: vec!["Compact".to_string(), "Full".to_string()],
        }));
    }

    #[test]
    fn qmk_live_features_remain_available_with_a_static_display_preset() {
        let mut layout = test_layout_with_encoders(&[]);
        layout.layout_options = vec![LayoutOption {
            label: "OLED Master".to_string(),
            choices: vec![
                "Status (classic)".to_string(),
                "Clock & Volume (qmk-hid-host)".to_string(),
                "Media (qmk-hid-host)".to_string(),
                "Disabled".to_string(),
            ],
        }];

        let active = EntropyApp::qmk_hid_host_mode_for(&layout, Some(0));
        let supported = EntropyApp::qmk_hid_host_supported_mode_for(&layout);

        assert!(active.is_empty());
        assert!(supported.time);
        assert!(supported.volume);
        assert!(supported.media);
        assert!(!supported.layout);
    }

    #[test]
    fn qmk_ruen_keycodes_advertise_layout_sync_without_live_feature_metadata() {
        let mut layout = test_layout_with_encoders(&[]);
        layout.custom_keycodes = vec![crate::keyboard::CustomKeycode {
            name: "LG_SYNC".to_string(),
            label: "RuEn\nSync".to_string(),
            title: "Sync language".to_string(),
        }];

        let active = EntropyApp::qmk_hid_host_mode_for(&layout, Some(0));
        let supported = EntropyApp::qmk_hid_host_supported_mode_for(&layout);

        assert!(active.layout);
        assert!(supported.layout);
        assert!(!active.time);
        assert!(!active.volume);
        assert!(!active.media);
    }

    #[test]
    fn selected_live_features_reuse_the_connected_hid_owner() {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        let mut app = EntropyApp::new(&creation_context);
        let device = crate::device::Device {
            name: "K:04".to_owned(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".to_owned(),
            serial_number: "test".to_owned(),
            bus_type: "Bluetooth".to_owned(),
            path: "test-shared-live-features".to_owned(),
            firmware: FirmwareProtocol::Vial,
        };
        let mut layout = test_layout_with_encoders(&[]);
        layout.custom_keycodes = vec![crate::keyboard::CustomKeycode {
            name: "LG_SYNC".to_owned(),
            label: "RuEn\nSync".to_owned(),
            title: "Sync language".to_owned(),
        }];
        let (hid, _) = crate::hid::HidDevice::test_device();

        app.device_manager.replace_devices(vec![device.clone()]);
        app.selected_device = Some(0);
        app.layout = Some(layout);
        app.app_settings.layout_sync_enabled = true;
        app.shared_hid_output = hid.shared_output();
        app.hid_device = Some(hid);

        app.sync_qmk_hid_host_bridges();

        let bridge = app.qmk_hid_hosts.get(&device.path).unwrap();
        assert!(bridge.uses_shared_output());
        app.qmk_hid_hosts.clear();
    }

    #[test]
    fn module_setting_parser_preserves_integer_width_and_bounds() {
        let field = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": " Ball DPI ",
                "qsid": 120,
                "type": "integer",
                "width": 2,
                "min": 100,
                "max": 16000
            }),
            &[120],
        )
        .expect("supported integer field should parse");

        assert_eq!(field.title, "Ball DPI");
        assert_eq!(field.qsid, 120);
        assert_eq!(field.kind, ModuleSettingKind::Integer);
        assert_eq!(field.width, 2);
        assert_eq!(field.min, 100);
        assert_eq!(field.max, 16000);
    }

    #[test]
    fn module_setting_parser_normalizes_select_metadata() {
        let field = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Mode",
                "qsid": 134,
                "type": "select",
                "width": 0,
                "bit": 99,
                "variants": ["Normal", " Scroll ", ""]
            }),
            &[134],
        )
        .expect("supported select field should parse");

        assert_eq!(field.kind, ModuleSettingKind::Select);
        assert_eq!(field.width, 1);
        assert_eq!(field.bit, 15);
        assert_eq!(field.variants, vec!["Normal", "Scroll"]);
        assert_eq!(field.min, 0);
        assert_eq!(field.max, 1);
    }

    #[test]
    fn boolean_module_setting_can_own_a_layout_option() {
        let field = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Trackball enabled",
                "qsid": 334,
                "type": "boolean",
                "bit": 0,
                "layoutOption": 0
            }),
            &[334],
        )
        .expect("supported boolean field should parse");

        assert_eq!(field.layout_option, Some(0));

        let select = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Mode",
                "qsid": 135,
                "type": "select",
                "layoutOption": 0,
                "variants": ["Normal", "Scroll"]
            }),
            &[135],
        )
        .expect("supported select field should parse");
        assert_eq!(select.layout_option, None);
    }

    #[test]
    fn firmware_managed_layout_option_tracks_boolean_setting_and_is_not_user_configurable() {
        let mut app = test_app();
        let mut layout = test_layout_with_encoders(&[]);
        layout.layout_options = vec![LayoutOption {
            label: "Right trackball instead of key".to_owned(),
            choices: Vec::new(),
        }];
        app.layout = Some(layout.clone());
        app.layout_options_value = Some(0);
        app.module_settings.fields = vec![ModuleSettingField {
            title: "Trackball enabled".to_owned(),
            qsid: 334,
            kind: ModuleSettingKind::Boolean,
            bit: 0,
            layout_option: Some(0),
            width: 1,
            min: 0,
            max: 1,
            variants: Vec::new(),
        }];
        app.module_settings.supported = true;

        app.module_settings.set_value(334, 1);
        app.sync_firmware_managed_layout_options();
        assert_eq!(app.layout_options_value, Some(1));
        assert!(app.user_layout_option_indices(&layout).is_empty());
        assert!(!EntropyApp::layout_condition_visible(
            &layout,
            Some(crate::keyboard::LayoutCondition {
                option_idx: 0,
                value: 0,
            }),
            app.layout_options_value,
        ));

        app.module_settings.set_value(334, 0);
        app.sync_firmware_managed_layout_options();
        assert_eq!(app.layout_options_value, Some(0));
        assert!(EntropyApp::layout_condition_visible(
            &layout,
            Some(crate::keyboard::LayoutCondition {
                option_idx: 0,
                value: 0,
            }),
            app.layout_options_value,
        ));
    }

    #[test]
    fn module_setting_parser_rejects_unsupported_or_invalid_fields() {
        let unsupported = serde_json::json!({
            "title": "Mode",
            "qsid": 134,
            "type": "select"
        });
        let overflowed_qsid = serde_json::json!({
            "title": "Mode",
            "qsid": 65536,
            "type": "select"
        });
        let blank_title = serde_json::json!({
            "title": "  ",
            "qsid": 134,
            "type": "select"
        });
        let unknown_type = serde_json::json!({
            "title": "Mode",
            "qsid": 134,
            "type": "slider"
        });

        assert!(EntropyApp::parse_module_setting_field(&unsupported, &[]).is_none());
        assert!(EntropyApp::parse_module_setting_field(&overflowed_qsid, &[0]).is_none());
        assert!(EntropyApp::parse_module_setting_field(&blank_title, &[134]).is_none());
        assert!(EntropyApp::parse_module_setting_field(&unknown_type, &[134]).is_none());
    }

    #[test]
    fn duplicate_module_qsid_uses_widest_read_width() {
        let narrow = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Mode",
                "qsid": 134,
                "type": "select",
                "width": 1
            }),
            &[134, 135],
        )
        .unwrap();
        let wide = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Resolution",
                "qsid": 134,
                "type": "integer",
                "width": 2
            }),
            &[134, 135],
        )
        .unwrap();
        let other = EntropyApp::parse_module_setting_field(
            &serde_json::json!({
                "title": "Invert",
                "qsid": 135,
                "type": "boolean"
            }),
            &[134, 135],
        )
        .unwrap();

        assert_eq!(
            EntropyApp::module_setting_widths(&[narrow, wide, other]),
            std::collections::BTreeMap::from([(134, 2), (135, 1)])
        );
    }
}
