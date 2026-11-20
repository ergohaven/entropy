use super::*;

struct DisplaySetting {
    spec: EntPortableSettingSpec,
    confirmed_value: fn(&DisplaySettingsState) -> Option<u8>,
}

// IDs are part of the .entlayout contract. QSIDs alone are firmware-local.
macro_rules! display_settings {
    (
        scalar { $( $id:literal, $qsid:ident, $max:literal, $supported:ident, $field:ident; )* }
        rgb { $( $color:literal, $rgb_qsids:ident, $rgb_supported:ident, $rgb_field:ident; )* }
        date { $( $index:literal, $date_id:literal, $date_max:literal; )* }
    ) => {
        &[
            $(display_settings!(@entry $id, $qsid, $max, $supported, $field),)*
            $(
                display_settings!(@entry concat!($color, "_red"), $rgb_qsids[0], 255, $rgb_supported, $rgb_field[0]),
                display_settings!(@entry concat!($color, "_green"), $rgb_qsids[1], 255, $rgb_supported, $rgb_field[1]),
                display_settings!(@entry concat!($color, "_blue"), $rgb_qsids[2], 255, $rgb_supported, $rgb_field[2]),
            )*
            $(display_settings!(@entry $date_id, DATE_QSIDS[$index], $date_max, date_supported, confirmed_date[$index]),)*
        ]
    };
    (@entry $id:expr, $qsid:expr, $max:literal, $supported:ident, $field:ident $([$idx:literal])?) => {
        DisplaySetting {
            spec: EntPortableSettingSpec::new(
                concat!("macropad_display_", $id), $qsid, EntPortableSettingWidth::U8, $max,
            ),
            confirmed_value: |state| state.$supported.then(|| u8::from(state.$field $([$idx])?)),
        }
    };
}

const MACROPAD_DISPLAY_SETTINGS: &[DisplaySetting] = display_settings! {
    scalar {
        "brightness", DISPLAY_BRIGHTNESS_QSID, 100, brightness_supported, confirmed_brightness;
        "timeout", DISPLAY_TIMEOUT_QSID, 7, display_timeout_supported, confirmed_display_timeout;
        "button_style", DISPLAY_BUTTON_STYLE_QSID, 32, button_style_supported, confirmed_button_style;
        "clock_style", CLOCK_STYLE_QSID, 9, clock_settings_supported, confirmed_clock_style;
        "clock_size", CLOCK_SIZE_QSID, 3, clock_settings_supported, confirmed_clock_size;
        "clock_alignment", CLOCK_ALIGNMENT_QSID, 2, clock_settings_supported, confirmed_clock_alignment;
        "clock_delay", CLOCK_DELAY_QSID, 7, clock_settings_supported, confirmed_clock_delay;
        "clock_colon_blink", CLOCK_COLON_BLINK_QSID, 1, clock_colon_blink_supported, confirmed_clock_colon_blink;
        "clock_background_dim", CLOCK_BACKGROUND_DIM_QSID, 100, clock_background_dim_supported, confirmed_clock_background_dim;
        "clock_visible", CLOCK_VISIBLE_QSID, 1, clock_overlay_controls_supported, confirmed_clock_visible;
        "clock_opacity", CLOCK_OPACITY_QSID, 100, clock_overlay_controls_supported, confirmed_clock_opacity;
        "clock_info_visible", CLOCK_INFO_VISIBLE_QSID, 1, clock_overlay_controls_supported, confirmed_clock_info_visible;
        "clock_info_opacity", CLOCK_INFO_OPACITY_QSID, 100, clock_overlay_controls_supported, confirmed_clock_info_opacity;
        "clock_modifiers_visible", CLOCK_MODIFIERS_VISIBLE_QSID, 1, clock_overlay_controls_supported, confirmed_clock_modifiers_visible;
        "clock_modifiers_opacity", CLOCK_MODIFIERS_OPACITY_QSID, 100, clock_overlay_controls_supported, confirmed_clock_modifiers_opacity;
    }
    rgb {
        "accent", DISPLAY_COLOR_QSIDS, supported, confirmed_color;
        "background", DISPLAY_BACKGROUND_COLOR_QSIDS, background_color_supported, confirmed_background_color;
        "clock_text", CLOCK_TEXT_COLOR_QSIDS, clock_settings_supported, confirmed_clock_text_color;
        "clock_background", CLOCK_BACKGROUND_COLOR_QSIDS, clock_settings_supported, confirmed_clock_background_color;
        "clock_info", CLOCK_INFO_COLOR_QSIDS, clock_info_color_supported, confirmed_clock_info_color;
        "clock_modifiers", CLOCK_MODIFIERS_COLOR_QSIDS, clock_overlay_controls_supported, confirmed_clock_modifiers_color;
    }
    date {
        0, "date_visible", 1;
        1, "date_opacity", 100;
        2, "date_red", 255;
        3, "date_green", 255;
        4, "date_blue", 255;
        5, "date_style", 9;
        6, "date_size", 3;
        7, "date_alignment", 2;
        8, "date_format", 2;
        9, "date_delay", 7;
        10, "standby_brightness", 100;
        11, "song_visible", 1;
        12, "song_red", 255;
        13, "song_green", 255;
        14, "song_blue", 255;
    }
};

pub(super) fn macropad_display_setting_valid(setting: &EntPortableSetting) -> bool {
    setting.module.is_none()
        && MACROPAD_DISPLAY_SETTINGS
            .iter()
            .any(|entry| entry.spec.matches(setting))
}

#[cfg(not(target_arch = "wasm32"))]
pub(in crate::app) fn macropad_clock_color_accent_qsid(qsid: u16) -> Option<u16> {
    CLOCK_COLORS_FOLLOWING_ACCENT
        .iter()
        .find_map(|qsids| qsids.iter().position(|&channel| channel == qsid))
        .map(|channel| DISPLAY_COLOR_QSIDS[channel])
}

impl EntropyApp {
    fn macropad_display_settings(&self) -> impl Iterator<Item = (&EntPortableSettingSpec, u16)> {
        let compatible = self.display_settings.supported
            && !self.display_settings.sm62_supported
            && self
                .selected_device
                .and_then(|i| self.device_manager.devices().get(i))
                .is_some_and(|device| {
                    device.supports_dedicated_display_settings()
                        && device.is_ergohaven_display_macropad()
                });
        MACROPAD_DISPLAY_SETTINGS.iter().filter_map(move |entry| {
            (compatible && self.supported_qmk_settings.contains(&entry.spec.qsid))
                .then(|| (entry.confirmed_value)(&self.display_settings))
                .flatten()
                .map(|value| (&entry.spec, u16::from(value)))
        })
    }

    pub(in crate::app) fn entlayout_macropad_display_snapshot(&self) -> Vec<EntPortableSetting> {
        self.macropad_display_settings()
            .filter(|(spec, value)| *value <= spec.max)
            .map(|(spec, value)| EntPortableSetting {
                id: spec.id.to_owned(),
                qsid: spec.qsid,
                width: spec.width,
                value,
                variants: Vec::new(),
                module: None,
            })
            .collect()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::app) fn entlayout_macropad_display_confirmed(&self) -> bool {
        let reconnecting = self.bluetooth_reconnect_active();
        self.macropad_display_settings().all(|(spec, _)| {
            !reconnecting
                && !self.display_settings.unconfirmed_qsids.contains(&spec.qsid)
                && self.pending_settings_write_value(spec.qsid).is_none()
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(in crate::app) fn confirm_entlayout_macropad_display(&mut self) -> anyhow::Result<()> {
        // Reconnect keeps the previous layout and display cache until the new read completes.
        anyhow::ensure!(
            !self.bluetooth_reconnect_active() || self.macropad_display_settings().next().is_none(),
            "Macropad display settings are unconfirmed while the keyboard is reconnecting"
        );
        let unconfirmed: Vec<_> = self
            .macropad_display_settings()
            .filter(|(spec, _)| self.display_settings.unconfirmed_qsids.contains(&spec.qsid))
            .map(|(spec, _)| *spec)
            .collect();
        for spec in unconfirmed {
            let hid = self.hid_device.as_ref()
                .ok_or_else(|| anyhow::anyhow!("Macropad display settings are unconfirmed and the keyboard is busy or disconnected"))?;
            let observed = u16::from(hid.get_qmk_setting_u8(spec.qsid)?);
            anyhow::ensure!(
                observed <= spec.max,
                "Invalid Macropad display setting {}",
                spec.qsid
            );
            self.display_settings
                .reconcile_display_readback(spec.qsid, observed);
        }
        Ok(())
    }
}
