use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntRgbKind {
    QmkRgblight,
    VialRgb,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct EntLayoutRgb {
    kind: EntRgbKind,
    effect: u16,
    brightness: u8,
    speed: u8,
    hue: u8,
    saturation: u8,
}

impl EntropyApp {
    pub(super) fn entlayout_rgb_snapshot(&self) -> Option<EntLayoutRgb> {
        self.rgb_settings
            .loaded
            .then(|| self.entlayout_rgb_contract())
            .flatten()
    }

    fn entlayout_rgb_contract(&self) -> Option<EntLayoutRgb> {
        let state = &self.rgb_settings;
        if !state.supported {
            return None;
        }
        let kind = match state.kind {
            RgbSupportKind::QmkRgblight => EntRgbKind::QmkRgblight,
            RgbSupportKind::VialRgb => EntRgbKind::VialRgb,
            RgbSupportKind::None => return None,
        };
        Some(EntLayoutRgb {
            kind,
            effect: state.effect,
            brightness: state.brightness,
            speed: state.speed,
            hue: state.hue,
            saturation: state.saturation,
        })
    }

    pub(super) fn import_entlayout_rgb(
        &mut self,
        source: Option<&EntLayoutRgb>,
        failures: &mut EntFirmwareFailures,
        report: &mut EntPortableSettingsImportReport,
    ) {
        let Some(source) = source else {
            return;
        };
        let was_unconfirmed = !self.rgb_settings.loaded;
        if was_unconfirmed {
            self.reload_rgb_settings();
        }
        let Some(target) = self.entlayout_rgb_contract() else {
            report.record_unsupported();
            return;
        };
        let state = &self.rgb_settings;
        let effect_supported = match source.kind {
            EntRgbKind::QmkRgblight => QMK_RGBLIGHT_EFFECTS
                .iter()
                .any(|(id, _)| *id == source.effect),
            EntRgbKind::VialRgb => {
                source.effect == 0 || state.supported_effects.contains(&source.effect)
            }
        };
        if !state.loaded && (source.kind != target.kind || !effect_supported) {
            report.record_failed();
            failures.push(
                "RGB lighting (capabilities could not be read; reconnect the keyboard)".into(),
            );
            return;
        }
        if source.kind != target.kind
            || !effect_supported
            || source.brightness > state.max_brightness
        {
            report.record_incompatible();
            return;
        }
        if source == &target && state.loaded && !was_unconfirmed {
            report.record_imported();
            return;
        }
        let Some(hid) = self
            .hid_device
            .as_ref()
            .and_then(crate::device_session::DeviceSession::as_vial)
        else {
            report.record_failed();
            return;
        };
        let result = (|| -> Result<()> {
            match source.kind {
                EntRgbKind::QmkRgblight => {
                    // RGBLight ignores HSV while disabled; animated effects can mask channels.
                    // Restore color through static mode before selecting the saved effect.
                    hid.set_qmk_rgblight_effect(1)?;
                    // The first SET enables a disabled board with its retained mode.
                    // Select static again now that mode changes are admitted.
                    hid.set_qmk_rgblight_effect(1)?;
                    hid.set_qmk_rgblight_brightness(source.brightness)?;
                    hid.set_qmk_rgblight_color(source.hue, source.saturation)?;
                    hid.set_qmk_rgblight_effect_speed(source.speed)?;
                    hid.set_qmk_rgblight_effect(source.effect as u8)?;
                    if hid.get_qmk_rgblight_brightness()? != source.brightness
                        || hid.get_qmk_rgblight_effect()? != source.effect as u8
                        || hid.get_qmk_rgblight_effect_speed()? != source.speed
                        || hid.get_qmk_rgblight_color()? != (source.hue, source.saturation)
                    {
                        bail!("RGB readback mismatch");
                    }
                }
                EntRgbKind::VialRgb => {
                    hid.set_vialrgb_mode(
                        source.effect,
                        source.speed,
                        source.hue,
                        source.saturation,
                        source.brightness,
                    )?;
                    if hid.get_vialrgb_mode()?
                        != (
                            source.effect,
                            source.speed,
                            source.hue,
                            source.saturation,
                            source.brightness,
                        )
                    {
                        bail!("RGB readback mismatch");
                    }
                }
            }
            hid.save_rgb()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                let state = &mut self.rgb_settings;
                state.loaded = true;
                state.effect = source.effect;
                state.brightness = source.brightness;
                state.speed = source.speed;
                state.hue = source.hue;
                state.saturation = source.saturation;
                if source.effect != 0 {
                    state.last_enabled_effect = source.effect;
                }
                report.record_imported();
            }
            Err(error) => {
                // SET may have partially succeeded: never re-export the old cache.
                self.reload_rgb_settings();
                self.rgb_settings.loaded = false;
                report.record_failed();
                failures.push(format!("RGB lighting ({error})"));
            }
        }
    }
}
