use super::hid_parse::{
    parse_rmk_switch_matrix_payload, parse_switch_matrix_payload,
    parse_vialrgb_supported_effects_payload,
};
use super::hid_protocol::*;
use super::HidDevice;

/// Layer names land in a 16-byte firmware buffer (15 data bytes + NUL). The
/// name is `%`-decoded by the firmware, so what must fit is the *decoded* length
/// — the on-wire escaped form can be longer.
const QMK_STRING_MAX_DECODED_BYTES: usize = 15;

/// Escape `%` as `%%` and pack `value` into the wire payload, stopping at a whole
/// character so it never splits a UTF-8 codepoint or a `%%` pair. Honours two
/// limits: `max_decoded` bytes after the firmware un-escapes it (so it isn't cut
/// mid-character inside firmware's own buffer) and `max_escaped` on-wire bytes.
fn truncate_qmk_string_payload(value: &str, max_decoded: usize, max_escaped: usize) -> Vec<u8> {
    let mut payload: Vec<u8> = Vec::with_capacity(max_escaped.min(value.len() + 1));
    let mut decoded_len = 0usize;
    for ch in value.chars() {
        let decoded = ch.len_utf8();
        let escaped = if ch == '%' { 2 } else { decoded };
        if decoded_len + decoded > max_decoded || payload.len() + escaped > max_escaped {
            break;
        }
        if ch == '%' {
            payload.push(b'%');
            payload.push(b'%');
        } else {
            let mut buf = [0u8; 4];
            payload.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
        decoded_len += decoded;
    }
    payload
}

use anyhow::Result;

fn verify_qmk_setting_writeback(qsid: u16, requested: u16, readback: u16) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    anyhow::bail!(
        "qmk setting writeback mismatch for qsid {}: wrote {}, read back {}",
        qsid,
        requested,
        readback
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BatteryHalves {
    pub(crate) left: Option<u8>,
    pub(crate) right: Option<u8>,
}

fn format_via_firmware_version(value: u32) -> Option<String> {
    if value == 0 {
        return None;
    }

    let major = value >> 16;
    let minor = (value >> 8) & 0xFF;
    let patch = value & 0xFF;
    Some(format!("{major}.{minor}.{patch}"))
}

fn battery_level_from_response(flags: u8, valid_bit: u8, value: u8) -> Option<u8> {
    if flags & valid_bit == 0 || value == 0xFF || value > 100 {
        None
    } else {
        Some(value)
    }
}

fn parse_battery_halves_response(resp: &[u8; MSG_LEN]) -> Result<Option<BatteryHalves>> {
    if resp[0] != CMD_VIA_CUSTOM_GET_VALUE
        || resp[1] != ERGOHAVEN_CUSTOM_NAMESPACE
        || resp[2] != ERGOHAVEN_CUSTOM_BATTERY_HALVES
    {
        anyhow::bail!("unexpected Ergohaven battery halves response");
    }

    if resp[3] != ERGOHAVEN_BATTERY_HALVES_VERSION {
        log::warn!(
            "unsupported Ergohaven battery halves response version: {}",
            resp[3]
        );
        return Ok(None);
    }

    let flags = resp[4];
    let halves = BatteryHalves {
        left: battery_level_from_response(flags, 0x01, resp[5]),
        right: battery_level_from_response(flags, 0x02, resp[6]),
    };
    Ok(Some(halves))
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    pub fn get_battery_halves(&self) -> Result<Option<BatteryHalves>> {
        let _ = self.get_battery_halves_once()?;
        std::thread::sleep(std::time::Duration::from_millis(400));
        self.get_battery_halves_once()
    }

    fn get_battery_halves_once(&self) -> Result<Option<BatteryHalves>> {
        let mut cmd = [0u8; MSG_LEN];
        cmd[0] = CMD_VIA_CUSTOM_GET_VALUE;
        cmd[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        cmd[2] = ERGOHAVEN_CUSTOM_BATTERY_HALVES;
        let resp = self.usb_send(&cmd)?;
        parse_battery_halves_response(&resp)
    }

    pub fn get_firmware_version(&self) -> Result<Option<String>> {
        let resp = self.usb_send(&[CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION])?;
        if resp[0] != CMD_VIA_GET_KEYBOARD_VALUE || resp[1] != VIA_FIRMWARE_VERSION {
            anyhow::bail!("unexpected firmware version response");
        }

        Ok(format_via_firmware_version(u32::from_be_bytes([
            resp[2], resp[3], resp[4], resp[5],
        ])))
    }

    pub fn get_layout_options(&self) -> Result<u32> {
        let resp = self.usb_send(&[CMD_VIA_GET_KEYBOARD_VALUE, VIA_LAYOUT_OPTIONS])?;
        if resp.len() < 6 {
            anyhow::bail!("layout options response too short");
        }
        Ok(u32::from_be_bytes([resp[2], resp[3], resp[4], resp[5]]))
    }

    pub fn set_layout_options(&self, options: u32) -> Result<()> {
        let bytes = options.to_be_bytes();
        let _ = self.usb_send(&[
            CMD_VIA_SET_KEYBOARD_VALUE,
            VIA_LAYOUT_OPTIONS,
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
        ])?;
        Ok(())
    }

    pub fn query_qmk_settings(&self) -> Result<Vec<u16>> {
        let mut supported = Vec::new();
        let mut cur = 0u16;

        let mut reached_end = false;
        for _ in 0..1024 {
            let mut cmd = [0u8; 32];
            cmd[0] = CMD_VIA_VIAL_PREFIX;
            cmd[1] = CMD_VIAL_QMK_SETTINGS_QUERY;
            cmd[2..4].copy_from_slice(&cur.to_le_bytes());
            let resp = self.usb_send(&cmd)?;

            let mut next = cur;
            for chunk in resp.chunks_exact(2) {
                let qsid = u16::from_le_bytes([chunk[0], chunk[1]]);
                next = next.max(qsid);
                if qsid != 0xFFFF {
                    supported.push(qsid);
                }
            }

            if next == 0xFFFF {
                reached_end = true;
                break;
            }
            if next == cur {
                anyhow::bail!("qmk settings query did not advance from qsid: {cur}");
            }
            cur = next;
        }
        if !reached_end {
            anyhow::bail!("qmk settings query did not reach terminator");
        }

        supported.sort_unstable();
        supported.dedup();
        Ok(supported)
    }

    pub fn get_qmk_setting_u8(&self, qsid: u16) -> Result<u8> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_GET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting get error or unsupported qsid: {qsid}");
        }
        Ok(resp[1])
    }

    pub fn set_qmk_setting_u8(&self, qsid: u16, value: u8) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_SET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());
        cmd[4] = value;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting set error or unsupported qsid: {qsid}");
        }
        Ok(())
    }

    pub fn set_qmk_setting_u8_verified(&self, qsid: u16, value: u8) -> Result<()> {
        self.set_qmk_setting_u8(qsid, value)?;
        let readback = self.get_qmk_setting_u8(qsid)?;
        verify_qmk_setting_writeback(qsid, value as u16, readback as u16)
    }

    pub fn get_qmk_setting_u16(&self, qsid: u16) -> Result<u16> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_GET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting get error or unsupported qsid: {qsid}");
        }
        Ok(u16::from_le_bytes([resp[1], resp[2]]))
    }

    pub fn set_qmk_setting_u16(&self, qsid: u16, value: u16) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_SET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());
        cmd[4..6].copy_from_slice(&value.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting set error or unsupported qsid: {qsid}");
        }
        Ok(())
    }

    pub fn set_qmk_setting_u16_verified(&self, qsid: u16, value: u16) -> Result<()> {
        self.set_qmk_setting_u16(qsid, value)?;
        let readback = self.get_qmk_setting_u16(qsid)?;
        verify_qmk_setting_writeback(qsid, value, readback)
    }

    pub fn get_qmk_setting_string(&self, qsid: u16) -> Result<String> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_GET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting get error or unsupported qsid: {qsid}");
        }
        let bytes = &resp[1..];
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).trim().to_string())
    }

    pub fn set_qmk_setting_string(&self, qsid: u16, value: &str) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_QMK_SETTINGS_SET;
        cmd[2..4].copy_from_slice(&qsid.to_le_bytes());

        // Escape '%' as '%%' and pack into the fixed payload, truncating only at
        // whole characters — never mid-codepoint and never splitting a '%%' pair,
        // either of which would leave a corrupted string in firmware. Bound by
        // both the firmware label buffer (decoded) and the wire packet (escaped).
        let max_len = cmd.len().saturating_sub(4);
        let wire_budget = max_len.saturating_sub(1); // reserve one byte for the null terminator
        let payload = truncate_qmk_string_payload(value, QMK_STRING_MAX_DECODED_BYTES, wire_budget);
        cmd[4..4 + payload.len()].copy_from_slice(&payload);
        cmd[4 + payload.len()] = 0;

        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("qmk setting set error or unsupported qsid: {qsid}");
        }
        Ok(())
    }

    pub fn get_qmk_rgblight_brightness(&self) -> Result<u8> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, QMK_RGBLIGHT_BRIGHTNESS])?;
        Ok(resp[2])
    }

    pub fn set_qmk_rgblight_brightness(&self, value: u8) -> Result<()> {
        self.usb_send(&[CMD_VIA_LIGHTING_SET_VALUE, QMK_RGBLIGHT_BRIGHTNESS, value])?;
        Ok(())
    }

    pub fn get_qmk_rgblight_effect(&self) -> Result<u8> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, QMK_RGBLIGHT_EFFECT])?;
        Ok(resp[2])
    }

    pub fn set_qmk_rgblight_effect(&self, value: u8) -> Result<()> {
        self.usb_send(&[CMD_VIA_LIGHTING_SET_VALUE, QMK_RGBLIGHT_EFFECT, value])?;
        Ok(())
    }

    pub fn get_qmk_rgblight_effect_speed(&self) -> Result<u8> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, QMK_RGBLIGHT_EFFECT_SPEED])?;
        Ok(resp[2])
    }

    pub fn set_qmk_rgblight_effect_speed(&self, value: u8) -> Result<()> {
        self.usb_send(&[CMD_VIA_LIGHTING_SET_VALUE, QMK_RGBLIGHT_EFFECT_SPEED, value])?;
        Ok(())
    }

    pub fn get_qmk_rgblight_color(&self) -> Result<(u8, u8)> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, QMK_RGBLIGHT_COLOR])?;
        Ok((resp[2], resp[3]))
    }

    pub fn set_qmk_rgblight_color(&self, hue: u8, saturation: u8) -> Result<()> {
        self.usb_send(&[
            CMD_VIA_LIGHTING_SET_VALUE,
            QMK_RGBLIGHT_COLOR,
            hue,
            saturation,
        ])?;
        Ok(())
    }

    pub fn save_rgb(&self) -> Result<()> {
        self.usb_send(&[CMD_VIA_LIGHTING_SAVE])?;
        Ok(())
    }

    pub fn get_vialrgb_info(&self) -> Result<(u16, u8)> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, VIALRGB_GET_INFO])?;
        let data = &resp[2..];
        Ok((u16::from_le_bytes([data[0], data[1]]), data[2]))
    }

    pub fn get_vialrgb_supported_effects(&self) -> Result<Vec<u16>> {
        let mut effects = vec![0u16];
        let mut max_effect = 0u16;
        while max_effect < 0xFFFF {
            let mut cmd = [0u8; MSG_LEN];
            cmd[0] = CMD_VIA_LIGHTING_GET_VALUE;
            cmd[1] = VIALRGB_GET_SUPPORTED;
            cmd[2..4].copy_from_slice(&max_effect.to_le_bytes());
            let resp = self.usb_send(&cmd)?;
            let batch_max =
                parse_vialrgb_supported_effects_payload(&resp[2..], &mut effects, max_effect);
            if batch_max == 0xFFFF || batch_max == max_effect {
                break;
            }
            max_effect = batch_max;
        }
        effects.sort_unstable();
        Ok(effects)
    }

    pub fn get_vialrgb_mode(&self) -> Result<(u16, u8, u8, u8, u8)> {
        let resp = self.usb_send(&[CMD_VIA_LIGHTING_GET_VALUE, VIALRGB_GET_MODE])?;
        let data = &resp[2..];
        Ok((
            u16::from_le_bytes([data[0], data[1]]),
            data[2],
            data[3],
            data[4],
            data[5],
        ))
    }

    pub fn set_vialrgb_mode(
        &self,
        mode: u16,
        speed: u8,
        hue: u8,
        saturation: u8,
        brightness: u8,
    ) -> Result<()> {
        let mut cmd = [0u8; MSG_LEN];
        cmd[0] = CMD_VIA_LIGHTING_SET_VALUE;
        cmd[1] = VIALRGB_SET_MODE;
        cmd[2..4].copy_from_slice(&mode.to_le_bytes());
        cmd[4] = speed;
        cmd[5] = hue;
        cmd[6] = saturation;
        cmd[7] = brightness;
        self.usb_send(&cmd)?;
        Ok(())
    }

    pub fn get_switch_matrix_with_rmk_byte_order(
        &self,
        rows: usize,
        cols: usize,
        rmk_byte_order: bool,
    ) -> Result<Vec<bool>> {
        let resp = self.usb_send(&[CMD_VIA_GET_KEYBOARD_VALUE, VIA_SWITCH_MATRIX_STATE])?;
        // Matrix data is packed row-by-row, with each row padded to whole bytes.
        // QMK sends row bytes low-to-high; RMK reverses byte order inside each row.
        Ok(if rmk_byte_order {
            parse_rmk_switch_matrix_payload(&resp[2..], rows, cols)
        } else {
            parse_switch_matrix_payload(&resp[2..], rows, cols)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_ascii_is_unchanged() {
        assert_eq!(truncate_qmk_string_payload("BASE", 15, 27), b"BASE");
    }

    #[test]
    fn truncate_percent_is_escaped_and_not_split() {
        assert_eq!(truncate_qmk_string_payload("a%b", 15, 27), b"a%%b");
        // Wire budget only fits `a` plus one byte: the `%%` pair must not split.
        assert_eq!(truncate_qmk_string_payload("a%", 15, 2), b"a");
    }

    #[test]
    fn truncate_multibyte_on_codepoint_boundary() {
        // "🙂" is 4 bytes; a 3-byte wire budget fits nothing, output stays valid.
        let out = truncate_qmk_string_payload("🙂", 15, 3);
        assert!(out.is_empty());
        assert!(std::str::from_utf8(&out).is_ok());

        // Two emoji (8 bytes) with a 5-byte wire budget keeps one whole emoji.
        let out = truncate_qmk_string_payload("🙂🙂", 15, 5);
        assert_eq!(out, "🙂".as_bytes());
        assert!(std::str::from_utf8(&out).is_ok());
    }

    #[test]
    fn truncate_cyrillic_on_codepoint_boundary() {
        // Each Cyrillic letter is 2 bytes; a 5-byte wire budget keeps two.
        let out = truncate_qmk_string_payload("СЛОЙ", 15, 5);
        assert_eq!(out, "СЛ".as_bytes());
        assert!(std::str::from_utf8(&out).is_ok());
    }

    #[test]
    fn truncate_respects_decoded_label_limit() {
        // The wire budget is generous, but the firmware label buffer (decoded)
        // binds first: four 4-byte emoji (16 decoded bytes) keep only three,
        // never splitting the fourth mid-codepoint.
        let out = truncate_qmk_string_payload("🙂🙂🙂🙂", 15, 27);
        assert_eq!(out, "🙂🙂🙂".as_bytes());
        assert_eq!(out.len(), 12);
        assert!(std::str::from_utf8(&out).is_ok());

        // Sixteen ASCII chars decode to 16 bytes; only fifteen may be stored.
        let out = truncate_qmk_string_payload("0123456789ABCDEF", 15, 27);
        assert_eq!(out, b"0123456789ABCDE");
    }

    #[test]
    fn truncate_production_set_string_bounds_layer_name() {
        // Exercise the exact limits set_qmk_setting_string uses (15 decoded / 27
        // wire) so those production values can't regress silently.
        let out = truncate_qmk_string_payload("МАКРОСЛОЙ", QMK_STRING_MAX_DECODED_BYTES, 27);
        assert!(out.len() <= QMK_STRING_MAX_DECODED_BYTES);
        assert!(std::str::from_utf8(&out).is_ok());
    }

    #[test]
    fn qmk_setting_writeback_accepts_matching_value() {
        assert!(verify_qmk_setting_writeback(7, 150, 150).is_ok());
    }

    #[test]
    fn qmk_setting_writeback_rejects_stale_value() {
        let error = verify_qmk_setting_writeback(7, 150, 250).unwrap_err();

        assert!(error.to_string().contains("qmk setting writeback mismatch"));
        assert!(error.to_string().contains("qsid 7"));
    }

    #[test]
    fn formats_via_firmware_version_as_semver() {
        assert_eq!(
            format_via_firmware_version(0x0004_0005).as_deref(),
            Some("4.0.5")
        );
    }

    #[test]
    fn treats_zero_firmware_version_as_not_reported() {
        assert_eq!(format_via_firmware_version(0), None);
    }

    #[test]
    fn parses_ergo_battery_halves_response() {
        let mut resp = [0u8; MSG_LEN];
        resp[0] = CMD_VIA_CUSTOM_GET_VALUE;
        resp[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        resp[2] = ERGOHAVEN_CUSTOM_BATTERY_HALVES;
        resp[3] = ERGOHAVEN_BATTERY_HALVES_VERSION;
        resp[4] = 0x03;
        resp[5] = 87;
        resp[6] = 64;

        assert_eq!(
            parse_battery_halves_response(&resp).unwrap(),
            Some(BatteryHalves {
                left: Some(87),
                right: Some(64),
            })
        );
    }

    #[test]
    fn keeps_ergo_battery_halves_with_unknown_levels() {
        let mut resp = [0u8; MSG_LEN];
        resp[0] = CMD_VIA_CUSTOM_GET_VALUE;
        resp[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        resp[2] = ERGOHAVEN_CUSTOM_BATTERY_HALVES;
        resp[3] = ERGOHAVEN_BATTERY_HALVES_VERSION;
        resp[4] = 0x02;
        resp[5] = 90;
        resp[6] = 0xFF;

        assert_eq!(
            parse_battery_halves_response(&resp).unwrap(),
            Some(BatteryHalves {
                left: None,
                right: None,
            })
        );
    }
}
