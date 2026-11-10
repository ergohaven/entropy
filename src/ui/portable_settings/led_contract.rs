use super::*;

impl EntPortableSetting {
    pub(in crate::app) fn is_layer_led_color(&self) -> bool {
        self.id
            .strip_prefix("layer_led_color:")
            .and_then(|layer| layer.parse::<u8>().ok())
            .is_some()
    }

    pub(in crate::app) fn is_indication_setting(&self) -> bool {
        matches!(
            self.id.as_str(),
            "led_brightness" | "led_timeout:minutes" | "led_timeout:seconds"
        ) || self
            .id
            .strip_prefix("bt_profile_color:")
            .and_then(|p| p.parse::<usize>().ok())
            .is_some()
    }
}

pub(super) fn indication_setting_valid(setting: &EntPortableSetting) -> bool {
    if setting.module.is_some() {
        return false;
    }
    if let Some(profile) = setting.id.strip_prefix("bt_profile_color:") {
        return profile.parse::<usize>().is_ok_and(|p| p <= 4)
            && setting.width == EntPortableSettingWidth::U8
            && usize::from(setting.value) < LAYER_LED_PALETTE.len()
            && setting
                .variants
                .iter()
                .map(String::as_str)
                .eq(LAYER_LED_PALETTE);
    }
    let Some(max) = setting
        .variants
        .first()
        .and_then(|s| s.strip_prefix("max:"))
        .and_then(|s| s.parse::<u16>().ok())
    else {
        return false;
    };
    let min_entry = setting.variants.get(1).filter(|s| s.starts_with("min:"));
    let min = match min_entry {
        Some(s) => match s[4..].parse::<u16>() {
            Ok(v) => v,
            Err(_) => return false,
        },
        None => 0,
    };
    let metadata = 1 + usize::from(min_entry.is_some());
    let limit = match setting.width {
        EntPortableSettingWidth::U8 => 255,
        EntPortableSettingWidth::U16 => u16::MAX,
    };
    max <= limit
        && min <= max
        && setting.value >= min
        && setting.value <= max
        && (setting.id != "led_brightness" || max <= 255)
        && (setting.variants.len() == metadata
            || usize::from(setting.value) < setting.variants.len() - metadata)
}

impl EntropyApp {
    pub(in crate::app) fn invalidate_indication_setting(&mut self, qsid: u16) {
        for v in [
            &mut self.layer_led_settings.brightness,
            &mut self.layer_led_settings.timeout,
        ]
        .into_iter()
        .flatten()
        {
            if v.qsid == qsid {
                v.loaded = false;
            }
        }
        for v in self
            .layer_led_settings
            .layer_colors
            .iter_mut()
            .chain(self.layer_led_settings.bt_profile_colors.iter_mut())
        {
            if v.all_qsids().any(|id| id == qsid) {
                v.loaded = false;
            }
        }
        for p in &mut self.bluetooth_settings.profile_colors {
            if p.setting.qsid == qsid {
                p.setting.loaded = false;
            }
        }
    }

    pub(in crate::app) fn reconcile_indication_setting(&mut self, qsid: u16, value: u16) {
        self.layer_led_settings.set_value(qsid, value);
        for p in &mut self.bluetooth_settings.profile_colors {
            if p.setting.qsid == qsid {
                p.setting.value = value;
                p.setting.loaded = true;
            }
        }
    }

    pub(in crate::app) fn entlayout_unconfirmed_indication_qsids(&self) -> Vec<u16> {
        let state = &self.layer_led_settings;
        [state.brightness.as_ref(), state.timeout.as_ref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.loaded)
            .map(|s| s.qsid)
            .chain(
                state
                    .bt_profile_colors
                    .iter()
                    .filter(|s| !s.loaded)
                    .map(|s| s.qsid),
            )
            .chain(
                self.bluetooth_settings
                    .profile_colors
                    .iter()
                    .filter(|p| !p.setting.loaded)
                    .map(|p| p.setting.qsid),
            )
            .chain(
                state
                    .layer_colors
                    .iter()
                    .filter(|s| !s.loaded)
                    .flat_map(|s| s.all_qsids()),
            )
            .collect()
    }

    pub(in crate::app) fn entlayout_indication_settings_snapshot(
        &self,
        existing: &[EntPortableSetting],
        include_unloaded: bool,
    ) -> Vec<EntPortableSetting> {
        let state = &self.layer_led_settings;
        let mut settings = Vec::new();
        for (id, value) in [
            ("led_brightness", state.brightness.as_ref()),
            (
                match state.timeout_unit {
                    LayerLedTimeoutUnit::Minutes => "led_timeout:minutes",
                    LayerLedTimeoutUnit::Seconds => "led_timeout:seconds",
                },
                state.timeout.as_ref(),
            ),
        ] {
            let Some(value) = value.filter(|v| include_unloaded || v.loaded) else {
                continue;
            };
            let width = match value.width {
                1 => EntPortableSettingWidth::U8,
                2 => EntPortableSettingWidth::U16,
                _ => continue,
            };
            let mut variants = vec![format!("max:{}", value.max)];
            // Older backups omit the minimum and imply zero. Keep that contract
            // for unchanged zero-based ranges, and distinguish nonzero minima.
            if value.min != 0 {
                variants.push(format!("min:{}", value.min));
            }
            variants.extend(value.variants.clone());
            settings.push(EntPortableSetting {
                id: id.into(),
                qsid: value.qsid,
                width,
                value: value.value,
                variants,
                module: None,
            });
        }
        for color in &state.bt_profile_colors {
            let Some(profile) = color
                .portable_profile
                .filter(|_| include_unloaded || color.loaded)
            else {
                continue;
            };
            settings.push(EntPortableSetting {
                id: format!("bt_profile_color:{profile}"),
                qsid: color.qsid,
                width: EntPortableSettingWidth::U8,
                value: u16::from(color.value),
                variants: LAYER_LED_PALETTE.iter().map(|s| (*s).into()).collect(),
                module: None,
            });
        }
        for profile in &self.bluetooth_settings.profile_colors {
            let color = &profile.setting;
            if !color.portable
                || (!include_unloaded && !color.loaded)
                || color.width != 1
                || profile.profile > 4
            {
                continue;
            }
            settings.push(EntPortableSetting {
                id: format!("bt_profile_color:{}", profile.profile),
                qsid: color.qsid,
                width: EntPortableSettingWidth::U8,
                value: color.value,
                variants: color.variants.clone(),
                module: None,
            });
        }
        settings
            .iter()
            .filter(|s| {
                let numeric_owners = [state.brightness.as_ref(), state.timeout.as_ref()]
                    .into_iter()
                    .flatten()
                    .filter(|value| value.qsid == s.qsid)
                    .count();
                let color_owners = state
                    .bt_profile_colors
                    .iter()
                    .chain(&state.layer_colors)
                    .filter(|color| color.all_qsids().any(|qsid| qsid == s.qsid))
                    .count();
                let bluetooth_owners = self
                    .bluetooth_settings
                    .profile_colors
                    .iter()
                    .filter(|p| p.setting.qsid == s.qsid)
                    .count();
                numeric_owners + color_owners + bluetooth_owners == 1
                    && self.supported_qmk_settings.contains(&s.qsid)
                    && !existing.iter().any(|other| other.qsid == s.qsid)
                    && settings.iter().filter(|other| other.qsid == s.qsid).count() == 1
                    && indication_setting_valid(s)
            })
            .cloned()
            .collect()
    }
}
