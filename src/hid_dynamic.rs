use super::hid_parse::{
    parse_alt_repeat_response, parse_combo_response, parse_key_override_response,
    parse_tap_dance_response,
};
use super::hid_protocol::*;
use super::HidDevice;
use anyhow::Result;

type TapDanceData = (u16, u16, u16, u16, u16);
type ComboData = ([u16; 4], u16);

fn verify_combo_writeback(index: u8, requested: ComboData, readback: ComboData) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    anyhow::bail!(
        "combo writeback mismatch at index {}: wrote {:?}, read back {:?}",
        index,
        requested,
        readback
    )
}

fn verify_tap_dance_writeback(
    index: u8,
    requested: TapDanceData,
    readback: TapDanceData,
) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    anyhow::bail!(
        "tap dance writeback mismatch at index {}: wrote {:?}, read back {:?}",
        index,
        requested,
        readback
    )
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    /// Get Vial dynamic entry counts and optional feature bits.
    /// Returns (tap_dance, combo, key_override, alt_repeat, feature_bits).
    pub fn get_dynamic_entry_counts(&self) -> Result<(u8, u8, u8, u8, u8)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_GET_NUM_ENTRIES,
        ])?;
        // Vial GUI trusts the firmware-provided one-byte dynamic entry counts.
        // Do not impose Entropy-only caps here: the firmware/Vial protocol is authoritative.
        Ok((resp[0], resp[1], resp[2], resp[3], resp[31]))
    }

    /// Get number of combo entries available
    pub fn get_combo_count(&self) -> Result<u8> {
        let (_, combo, _, _, _) = self.get_dynamic_entry_counts()?;
        Ok(combo)
    }

    /// Get combo entry: ([trigger_keys; 4], output_keycode)
    pub fn get_combo(&self, idx: u8) -> Result<([u16; 4], u16)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_COMBO_GET,
            idx,
        ])?;
        parse_combo_response(&resp)
    }

    /// Set combo entry
    pub fn set_combo(&self, idx: u8, keys: [u16; 4], output: u16) -> Result<()> {
        let requested = (keys, output);
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_COMBO_SET;
        cmd[3] = idx;
        for (i, key) in keys.iter().enumerate() {
            let [lo, hi] = key.to_le_bytes();
            let off = 4 + i * 2;
            cmd[off] = lo;
            cmd[off + 1] = hi;
        }
        let [out_lo, out_hi] = output.to_le_bytes();
        cmd[12] = out_lo;
        cmd[13] = out_hi;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("combo set error: {}", resp[0]);
        }
        let readback = self.get_combo(idx)?;
        verify_combo_writeback(idx, requested, readback)
    }

    pub fn set_combo_binding(
        &self,
        idx: u8,
        keys: [u16; 4],
        output: crate::keyboard::KeyBinding,
    ) -> Result<()> {
        self.set_combo(idx, keys, output.vial_keycode())?;
        if let Some(action) = output.rmk_action() {
            self.set_rmk_combo_output(idx, action)?;
        }
        Ok(())
    }

    /// Get number of key override entries available
    pub fn get_key_override_count(&self) -> Result<u8> {
        let (_, _, key_override, _, _) = self.get_dynamic_entry_counts()?;
        Ok(key_override)
    }

    /// Get number of alt repeat key entries available
    pub fn get_alt_repeat_key_count(&self) -> Result<u8> {
        let (_, _, _, alt_repeat, _) = self.get_dynamic_entry_counts()?;
        Ok(alt_repeat)
    }

    /// Get key override entry:
    /// (trigger, replacement, layers, trigger_mods, negative_mod_mask, suppressed_mods, options)
    pub fn get_key_override(&self, idx: u8) -> Result<(u16, u16, u16, u8, u8, u8, u8)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_KEY_OVERRIDE_GET,
            idx,
        ])?;
        parse_key_override_response(&resp)
    }

    /// Set key override entry
    pub fn set_key_override(
        &self,
        idx: u8,
        trigger: u16,
        replacement: u16,
        layers: u16,
        trigger_mods: u8,
        negative_mod_mask: u8,
        suppressed_mods: u8,
        options: u8,
    ) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_KEY_OVERRIDE_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&trigger.to_le_bytes());
        cmd[6..8].copy_from_slice(&replacement.to_le_bytes());
        cmd[8..10].copy_from_slice(&layers.to_le_bytes());
        cmd[10] = trigger_mods;
        cmd[11] = negative_mod_mask;
        cmd[12] = suppressed_mods;
        cmd[13] = options;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("key override set error: {}", resp[0]);
        }
        Ok(())
    }

    /// Get alt repeat key entry: (last_key, alt_key, allowed_mods, options)
    pub fn get_alt_repeat_key(&self, idx: u8) -> Result<(u16, u16, u8, u8)> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_ALT_REPEAT_KEY_GET,
            idx,
        ])?;
        parse_alt_repeat_response(&resp)
    }

    /// Set alt repeat key entry
    pub fn set_alt_repeat_key(
        &self,
        idx: u8,
        keycode: u16,
        alt_keycode: u16,
        allowed_mods: u8,
        options: u8,
    ) -> Result<()> {
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_ALT_REPEAT_KEY_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&keycode.to_le_bytes());
        cmd[6..8].copy_from_slice(&alt_keycode.to_le_bytes());
        cmd[8] = allowed_mods;
        cmd[9] = options;
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("alt repeat key set error: {}", resp[0]);
        }
        Ok(())
    }

    /// Get number of tap dance entries available
    pub fn get_tap_dance_count(&self) -> Result<u8> {
        let (tap_dance, _, _, _, _) = self.get_dynamic_entry_counts()?;
        Ok(tap_dance)
    }

    /// Get a tap dance entry: (on_tap, on_hold, on_double_tap, on_tap_hold, tapping_term)
    pub fn get_tap_dance(&self, idx: u8) -> Result<TapDanceData> {
        let resp = self.usb_send(&[
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_TAP_DANCE_GET,
            idx,
        ])?;
        // resp[0] = status (0=ok), resp[1..] = entry data
        parse_tap_dance_response(&resp)
    }

    /// Set a tap dance entry
    pub fn set_tap_dance(
        &self,
        idx: u8,
        on_tap: u16,
        on_hold: u16,
        on_double_tap: u16,
        on_tap_hold: u16,
        tapping_term: u16,
    ) -> Result<()> {
        let requested = (on_tap, on_hold, on_double_tap, on_tap_hold, tapping_term);
        let mut cmd = [0u8; 32];
        cmd[0] = CMD_VIA_VIAL_PREFIX;
        cmd[1] = CMD_VIAL_DYNAMIC_ENTRY_OP;
        cmd[2] = DYNAMIC_VIAL_TAP_DANCE_SET;
        cmd[3] = idx;
        cmd[4..6].copy_from_slice(&on_tap.to_le_bytes());
        cmd[6..8].copy_from_slice(&on_hold.to_le_bytes());
        cmd[8..10].copy_from_slice(&on_double_tap.to_le_bytes());
        cmd[10..12].copy_from_slice(&on_tap_hold.to_le_bytes());
        cmd[12..14].copy_from_slice(&tapping_term.to_le_bytes());
        let resp = self.usb_send(&cmd)?;
        if resp[0] != 0 {
            anyhow::bail!("tap dance set error: {}", resp[0]);
        }
        let readback = self.get_tap_dance(idx)?;
        verify_tap_dance_writeback(idx, requested, readback)
    }

    pub fn set_tap_dance_bindings(
        &self,
        idx: u8,
        actions: [crate::keyboard::KeyBinding; 4],
        tapping_term: u16,
    ) -> Result<()> {
        self.set_tap_dance(
            idx,
            actions[0].vial_keycode(),
            actions[1].vial_keycode(),
            actions[2].vial_keycode(),
            actions[3].vial_keycode(),
            tapping_term,
        )?;
        for (field, binding) in actions.into_iter().enumerate() {
            if let Some(action) = binding.rmk_action() {
                self.set_rmk_tap_dance_action(idx, field as u8, action)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_dance_writeback_accepts_matching_entry() {
        let entry = (0x002C, 0x0202, 0, 0, 150);

        assert!(verify_tap_dance_writeback(0, entry, entry).is_ok());
    }

    #[test]
    fn tap_dance_writeback_rejects_stale_entry() {
        let requested = (0x002C, 0x0202, 0, 0, 150);
        let stale = (0, 0, 0, 0, 200);

        let error = verify_tap_dance_writeback(0, requested, stale).unwrap_err();

        assert!(error.to_string().contains("tap dance writeback mismatch"));
        assert!(error.to_string().contains("index 0"));
    }

    #[test]
    fn combo_writeback_accepts_matching_entry() {
        let entry = ([0x002C, 0x002D, 0, 0], 0x002E);

        assert!(verify_combo_writeback(3, entry, entry).is_ok());
    }

    #[test]
    fn combo_writeback_rejects_stale_entry() {
        let requested = ([0x002C, 0x002D, 0, 0], 0x002E);
        let stale = ([0x002C, 0x002D, 0, 0], 0);

        let error = verify_combo_writeback(3, requested, stale).unwrap_err();

        assert!(error.to_string().contains("combo writeback mismatch"));
        assert!(error.to_string().contains("index 3"));
    }
}
