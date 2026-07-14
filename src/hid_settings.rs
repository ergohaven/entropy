use super::hid_parse::{
    parse_rmk_switch_matrix_payload, parse_switch_matrix_payload,
    parse_vialrgb_supported_effects_payload,
};
use super::hid_protocol::*;
use super::HidDevice;
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

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
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

        let safe_value = value.replace('%', "%%");
        let bytes = safe_value.as_bytes();
        let max_len = cmd.len().saturating_sub(4);
        let copy_len = bytes.len().min(max_len.saturating_sub(1));
        cmd[4..4 + copy_len].copy_from_slice(&bytes[..copy_len]);
        cmd[4 + copy_len] = 0;

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
    fn qmk_setting_writeback_accepts_matching_value() {
        assert!(verify_qmk_setting_writeback(7, 150, 150).is_ok());
    }

    #[test]
    fn qmk_setting_writeback_rejects_stale_value() {
        let error = verify_qmk_setting_writeback(7, 150, 250).unwrap_err();

        assert!(error.to_string().contains("qmk setting writeback mismatch"));
        assert!(error.to_string().contains("qsid 7"));
    }
}
