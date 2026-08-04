use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntPortableSettingWidth {
    U8,
    U16,
}

impl EntPortableSettingWidth {
    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(self) -> u8 {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct EntPortableSetting {
    pub(super) id: String,
    pub(super) qsid: u16,
    pub(super) width: EntPortableSettingWidth,
    pub(super) value: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) variants: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EntPortableSettingSpec {
    pub(super) id: &'static str,
    pub(super) qsid: u16,
    pub(super) width: EntPortableSettingWidth,
    pub(super) max: u16,
}

impl EntPortableSettingSpec {
    const fn new(id: &'static str, qsid: u16, width: EntPortableSettingWidth, max: u16) -> Self {
        Self {
            id,
            qsid,
            width,
            max,
        }
    }

    fn matches(self, setting: &EntPortableSetting) -> bool {
        setting.id == self.id
            && setting.qsid == self.qsid
            && setting.width == self.width
            && setting.value <= self.max
            && setting.variants.is_empty()
    }
}

pub(super) const ENT_PORTABLE_QSIDS: &[u16] = &[
    1, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
    121, 122, 123, 124, 142,
];

pub(super) fn ent_portable_setting_spec(qsid: u16) -> Option<EntPortableSettingSpec> {
    use EntPortableSettingWidth::*;
    macro_rules! spec {
        ($id:literal, $width:ident, $max:expr) => {
            EntPortableSettingSpec::new($id, qsid, $width, $max)
        };
    }
    Some(match qsid {
        1 => spec!("grave_escape", U8, 0x0f),
        3 => spec!("auto_shift_flags", U8, 0x7f),
        4 => spec!("auto_shift_timeout", U16, u16::MAX),
        5 => spec!("one_shot_tap_toggle", U8, u8::MAX as u16),
        6 => spec!("one_shot_timeout", U16, u16::MAX),
        7 => spec!("tapping_term", U16, u16::MAX),
        9 => spec!("mouse_keys_delay", U8, u8::MAX as u16),
        10 => spec!("mouse_keys_interval", U8, u8::MAX as u16),
        11 => spec!("mouse_keys_move_delta", U8, u8::MAX as u16),
        12 => spec!("mouse_keys_max_speed", U8, u8::MAX as u16),
        13 => spec!("mouse_keys_time_to_max", U8, u8::MAX as u16),
        14 => spec!("mouse_keys_wheel_delay", U8, u8::MAX as u16),
        15 => spec!("mouse_keys_wheel_interval", U8, u8::MAX as u16),
        16 => spec!("mouse_keys_wheel_max_speed", U8, u8::MAX as u16),
        17 => spec!("mouse_keys_wheel_time_to_max", U8, u8::MAX as u16),
        18 => spec!("tap_code_delay", U16, u16::MAX),
        19 => spec!("tap_hold_caps_delay", U16, u16::MAX),
        20 => spec!("tapping_toggle", U8, u8::MAX as u16),
        21 => spec!("magic", U16, 0x03ff),
        22 => spec!("permissive_hold", U8, 1),
        23 => spec!("hold_on_other_key_press", U8, 1),
        24 => spec!("retro_tapping", U8, 1),
        25 => spec!("quick_tap_term", U16, 1000),
        26 => spec!("chordal_hold", U8, 1),
        27 => spec!("flow_tap", U16, u16::MAX),
        121 => spec!("touchpad_sniper_sensitivity", U8, u8::MAX as u16),
        122 => spec!("touchpad_scroll_sensitivity", U8, u8::MAX as u16),
        123 => spec!("touchpad_text_sensitivity", U8, u8::MAX as u16),
        124 => spec!("touchpad_flags", U8, 0x07),
        142 => spec!("touchpad_auto_layer_enable", U8, 1),
        _ => return None,
    })
}

pub(super) fn ent_portable_setting_is_known(qsid: u16) -> bool {
    ent_portable_setting_spec(qsid).is_some() || matches!(qsid, 120 | 143)
}

pub(super) fn ent_portable_setting_is_valid(setting: &EntPortableSetting) -> bool {
    if let Some(spec) = ent_portable_setting_spec(setting.qsid) {
        return spec.matches(setting);
    }

    match setting.qsid {
        120 => {
            setting.id == "touchpad_dpi"
                && match setting.width {
                    EntPortableSettingWidth::U8 => {
                        !setting.variants.is_empty()
                            && usize::from(setting.value) < setting.variants.len()
                    }
                    EntPortableSettingWidth::U16 => setting.variants.is_empty(),
                }
        }
        143 => {
            setting.id == "touchpad_auto_layer"
                && setting.width == EntPortableSettingWidth::U8
                && !setting.variants.is_empty()
                && usize::from(setting.value) < setting.variants.len()
        }
        _ => false,
    }
}

pub(super) fn ent_portable_setting_contracts_match(
    source: &EntPortableSetting,
    target: &EntPortableSetting,
) -> bool {
    source.id == target.id
        && source.qsid == target.qsid
        && source.width == target.width
        && source.variants == target.variants
        && ent_portable_setting_is_valid(source)
        && ent_portable_setting_is_valid(target)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn write_entlayout_portable_setting(
    hid: &crate::hid::HidDevice,
    setting: &EntPortableSetting,
) -> anyhow::Result<()> {
    let policy = settings_write_queue::qmk_setting_write_policy(setting.qsid);
    let (result, _) = settings_write_queue::write_qmk_setting_value(
        hid,
        setting.qsid,
        setting.width.bytes(),
        setting.value,
        policy,
    );
    result
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

impl EntropyApp {
    pub(super) fn entlayout_portable_settings_snapshot(&self) -> Vec<EntPortableSetting> {
        let mut settings = ENT_PORTABLE_QSIDS
            .iter()
            .filter(|qsid| self.supported_qmk_settings.contains(qsid))
            .filter_map(|qsid| {
                let spec = ent_portable_setting_spec(*qsid)?;
                self.entlayout_portable_setting_value(spec.qsid)
                    .filter(|value| *value <= spec.max)
                    .map(|value| EntPortableSetting {
                        id: spec.id.to_owned(),
                        qsid: spec.qsid,
                        width: spec.width,
                        value,
                        variants: Vec::new(),
                    })
            })
            .collect::<Vec<_>>();

        if self.touchpad_settings.loaded_qsids.contains(120)
            && self.supported_qmk_settings.contains(&120)
        {
            let variants = self.touchpad_settings.dpi_variants.clone();
            let width = if variants.is_empty() {
                EntPortableSettingWidth::U16
            } else {
                EntPortableSettingWidth::U8
            };
            let setting = EntPortableSetting {
                id: "touchpad_dpi".to_owned(),
                qsid: 120,
                width,
                value: self.touchpad_settings.dpi,
                variants,
            };
            if ent_portable_setting_is_valid(&setting) {
                settings.push(setting);
            }
        }

        if self.touchpad_settings.loaded_qsids.contains(143)
            && self.touchpad_settings.auto_layer_supported()
            && self.supported_qmk_settings.contains(&143)
        {
            let setting = EntPortableSetting {
                id: "touchpad_auto_layer".to_owned(),
                qsid: 143,
                width: EntPortableSettingWidth::U8,
                value: self.touchpad_settings.auto_layer as u16,
                variants: self.touchpad_settings.auto_layer_variants.clone(),
            };
            if ent_portable_setting_is_valid(&setting) {
                settings.push(setting);
            }
        }

        settings.sort_by_key(|setting| setting.qsid);
        settings
    }

    fn entlayout_portable_setting_value(&self, qsid: u16) -> Option<u16> {
        match qsid {
            1 if self.grave_escape_settings.supported => {
                Some(self.grave_escape_settings.bits as u16)
            }
            3 if self.auto_shift_options.loaded => Some(self.auto_shift_options.bits() as u16),
            4 => self.auto_shift_timeout,
            5 if self.one_shot_settings.supports_qsid(5) => {
                Some(self.one_shot_settings.tap_toggle as u16)
            }
            6 if self.one_shot_settings.supports_qsid(6) => Some(self.one_shot_settings.timeout),
            7 if self.tap_hold_settings.qsid_loaded(7) => Some(self.tap_hold_settings.tapping_term),
            9 if self.mouse_keys_settings.loaded_qsids.contains(9) => {
                Some(self.mouse_keys_settings.delay)
            }
            10 if self.mouse_keys_settings.loaded_qsids.contains(10) => {
                Some(self.mouse_keys_settings.interval)
            }
            11 if self.mouse_keys_settings.loaded_qsids.contains(11) => {
                Some(self.mouse_keys_settings.move_delta)
            }
            12 if self.mouse_keys_settings.loaded_qsids.contains(12) => {
                Some(self.mouse_keys_settings.max_speed)
            }
            13 if self.mouse_keys_settings.loaded_qsids.contains(13) => {
                Some(self.mouse_keys_settings.time_to_max)
            }
            14 if self.mouse_keys_settings.loaded_qsids.contains(14) => {
                Some(self.mouse_keys_settings.wheel_delay)
            }
            15 if self.mouse_keys_settings.loaded_qsids.contains(15) => {
                Some(self.mouse_keys_settings.wheel_interval)
            }
            16 if self.mouse_keys_settings.loaded_qsids.contains(16) => {
                Some(self.mouse_keys_settings.wheel_max_speed)
            }
            17 if self.mouse_keys_settings.loaded_qsids.contains(17) => {
                Some(self.mouse_keys_settings.wheel_time_to_max)
            }
            18 if self.tap_hold_settings.qsid_loaded(18) => {
                Some(self.tap_hold_settings.tap_code_delay)
            }
            19 if self.tap_hold_settings.qsid_loaded(19) => {
                Some(self.tap_hold_settings.tap_hold_caps_delay)
            }
            20 if self.tap_hold_settings.qsid_loaded(20) => {
                Some(self.tap_hold_settings.tapping_toggle)
            }
            21 if self.magic_settings.supported => Some(self.magic_settings.bits),
            22 if self.tap_hold_settings.qsid_loaded(22) => {
                Some(self.tap_hold_settings.permissive_hold as u16)
            }
            23 if self.tap_hold_settings.qsid_loaded(23) => {
                Some(self.tap_hold_settings.hold_on_other_key_press as u16)
            }
            24 if self.tap_hold_settings.qsid_loaded(24) => {
                Some(self.tap_hold_settings.retro_tapping as u16)
            }
            25 if self.tap_hold_settings.qsid_loaded(25) => {
                Some(self.tap_hold_settings.quick_tap_term)
            }
            26 if self.tap_hold_settings.qsid_loaded(26) => {
                Some(self.tap_hold_settings.chordal_hold as u16)
            }
            27 if self.tap_hold_settings.qsid_loaded(27) => Some(self.tap_hold_settings.flow_tap),
            121 if self.touchpad_settings.loaded_qsids.contains(121) => {
                Some(self.touchpad_settings.sniper_sens as u16)
            }
            122 if self.touchpad_settings.loaded_qsids.contains(122) => {
                Some(self.touchpad_settings.scroll_sens as u16)
            }
            123 if self.touchpad_settings.loaded_qsids.contains(123) => {
                Some(self.touchpad_settings.text_sens as u16)
            }
            124 if self.touchpad_settings.loaded_qsids.contains(124) => {
                Some(self.touchpad_settings.bits as u16)
            }
            142 if self.touchpad_settings.loaded_qsids.contains(142) => {
                Some(self.touchpad_settings.auto_layer_enable as u16)
            }
            _ => None,
        }
    }

    pub(super) fn apply_entlayout_portable_setting_local(&mut self, setting: &EntPortableSetting) {
        let value = setting.value;
        match setting.qsid {
            1 => {
                self.grave_escape_settings.bits = value as u8;
                self.grave_escape_settings.supported = true;
            }
            3 => self.auto_shift_options = AutoShiftOptionsState::from_bits(value as u8),
            4 => {
                self.auto_shift_timeout = Some(value);
                self.auto_shift_timeout_text = value.to_string();
            }
            5 => self.one_shot_settings.tap_toggle = value as u8,
            6 => self.one_shot_settings.timeout = value,
            7 => {
                self.tap_hold_settings.tapping_term = value;
                self.tap_hold_settings.supported = true;
            }
            9 => self.mouse_keys_settings.delay = value,
            10 => self.mouse_keys_settings.interval = value,
            11 => self.mouse_keys_settings.move_delta = value,
            12 => self.mouse_keys_settings.max_speed = value,
            13 => self.mouse_keys_settings.time_to_max = value,
            14 => self.mouse_keys_settings.wheel_delay = value,
            15 => self.mouse_keys_settings.wheel_interval = value,
            16 => self.mouse_keys_settings.wheel_max_speed = value,
            17 => self.mouse_keys_settings.wheel_time_to_max = value,
            18 => self.tap_hold_settings.tap_code_delay = value,
            19 => self.tap_hold_settings.tap_hold_caps_delay = value,
            20 => self.tap_hold_settings.tapping_toggle = value,
            21 => {
                self.magic_settings.bits = value;
                self.magic_settings.supported = true;
            }
            22 => self.tap_hold_settings.permissive_hold = value != 0,
            23 => self.tap_hold_settings.hold_on_other_key_press = value != 0,
            24 => self.tap_hold_settings.retro_tapping = value != 0,
            25 => self.tap_hold_settings.quick_tap_term = value,
            26 => self.tap_hold_settings.chordal_hold = value != 0,
            27 => self.tap_hold_settings.flow_tap = value,
            120 => {
                self.touchpad_settings.dpi = value;
                self.touchpad_settings.dpi_variants = setting.variants.clone();
            }
            121 => self.touchpad_settings.sniper_sens = value as u8,
            122 => self.touchpad_settings.scroll_sens = value as u8,
            123 => self.touchpad_settings.text_sens = value as u8,
            124 => self.touchpad_settings.bits = value as u8,
            142 => {
                self.touchpad_settings.auto_layer_enable = value != 0;
                self.touchpad_settings.auto_layer_enable_supported = true;
            }
            143 => {
                self.touchpad_settings.auto_layer = value as u8;
                self.touchpad_settings.auto_layer_variants = setting.variants.clone();
            }
            _ => return,
        }

        match setting.qsid {
            5 | 6 => {
                self.one_shot_settings.set_qsid_supported(setting.qsid);
                self.one_shot_settings.supported = true;
            }
            7 | 18..=20 | 22..=27 => {
                self.tap_hold_settings.set_qsid_supported(setting.qsid);
                self.tap_hold_settings.set_qsid_loaded(setting.qsid);
            }
            9..=17 => {
                self.mouse_keys_settings.supported = true;
                self.mouse_keys_settings.loaded_qsids.mark(setting.qsid);
            }
            120..=124 | 142 | 143 => {
                self.touchpad_settings.supported = true;
                self.touchpad_settings.loaded_qsids.mark(setting.qsid);
            }
            _ => {}
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn test_app() -> EntropyApp {
        let ctx = egui::Context::default();
        let creation_context = eframe::CreationContext::_new_kittest(ctx);
        EntropyApp::new(&creation_context)
    }

    fn setting(qsid: u16, width: EntPortableSettingWidth, value: u16) -> EntPortableSetting {
        EntPortableSetting {
            id: ent_portable_setting_spec(qsid).unwrap().id.to_owned(),
            qsid,
            width,
            value,
            variants: Vec::new(),
        }
    }

    #[test]
    fn standard_setting_write_uses_one_set_without_readback() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        write_entlayout_portable_setting(&hid, &setting(7, EntPortableSettingWidth::U16, 200))
            .unwrap();

        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][..4], &[0xfe, 0x0b, 7, 0]);
    }

    #[test]
    fn touchpad_setting_write_uses_delayed_verified_readback() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();

        write_entlayout_portable_setting(&hid, &setting(121, EntPortableSettingWidth::U8, 42))
            .unwrap();

        let requests = recorder.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(&requests[0][..4], &[0xfe, 0x0b, 121, 0]);
        assert_eq!(&requests[1][..4], &[0xfe, 0x0a, 121, 0]);
    }

    #[test]
    fn snapshot_exports_only_individually_loaded_values() {
        let mut app = test_app();
        app.supported_qmk_settings = vec![5, 6, 9, 10];
        app.mouse_keys_settings.supported = true;
        app.mouse_keys_settings.loaded_qsids.mark(9);
        app.one_shot_settings.supported = true;
        app.one_shot_settings.set_qsid_supported(6);

        let qsids = app
            .entlayout_portable_settings_snapshot()
            .into_iter()
            .map(|setting| setting.qsid)
            .collect::<Vec<_>>();

        assert_eq!(qsids, vec![6, 9]);
    }
}
