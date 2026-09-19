use super::hid_parse::parse_unlock_status_response;
use super::hid_protocol::*;
use super::HidDevice;
use anyhow::{bail, Context, Result};

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    pub fn get_protocol_version(&self) -> Result<u16> {
        let resp = self
            .usb_send(&[CMD_VIA_GET_PROTOCOL_VERSION])
            .context("failed to read VIA protocol version")?;
        // resp[1..3] = big-endian u16
        Ok(u16::from_be_bytes([resp[1], resp[2]]))
    }

    /// Returns (vial_protocol: u32, keyboard_id: u64)
    pub fn get_keyboard_id(&self) -> Result<(u32, u64)> {
        let resp = self.usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_GET_KEYBOARD_ID])?;
        let vial_proto = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
        let kb_id = u64::from_le_bytes([
            resp[4], resp[5], resp[6], resp[7], resp[8], resp[9], resp[10], resp[11],
        ]);
        Ok((vial_proto, kb_id))
    }

    pub fn get_definition_size(&self) -> Result<u32> {
        let resp = self
            .usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_GET_SIZE])
            .context("failed to read Vial definition size")?;
        // response: size as little-endian u32 starting at byte 0
        Ok(u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]))
    }

    pub fn get_layout_json(&self) -> Result<serde_json::Value> {
        let sz = self.get_definition_size()?;
        self.get_layout_json_with_size(sz)
    }

    pub fn get_layout_json_with_size(&self, sz: u32) -> Result<serde_json::Value> {
        self.get_layout_json_with_size_and_progress(sz, |_, _| Ok(()))
    }

    pub fn get_layout_json_with_size_and_progress(
        &self,
        sz: u32,
        mut progress: impl FnMut(usize, usize) -> Result<()>,
    ) -> Result<serde_json::Value> {
        let sz = sz as usize;
        if sz == 0 || sz > 2_000_000 {
            bail!("Invalid definition size: {sz}");
        }
        log::info!("Vial definition compressed size: {sz} bytes");

        let mut payload = Vec::with_capacity(sz);
        let mut block: u32 = 0;
        let mut remaining = sz;
        let total_blocks = sz.div_ceil(MSG_LEN);
        progress(0, total_blocks)?;

        while remaining > 0 {
            let mut cmd = [0u8; MSG_LEN];
            cmd[0] = CMD_VIA_VIAL_PREFIX;
            cmd[1] = CMD_VIAL_GET_DEFINITION;
            cmd[2..6].copy_from_slice(&block.to_le_bytes());
            let resp = self
                .usb_send(&cmd)
                .with_context(|| format!("failed to read Vial definition block {block}"))?;

            let chunk = remaining.min(MSG_LEN);
            payload.extend_from_slice(&resp[..chunk]);
            remaining -= chunk;
            block += 1;
            let completed = block as usize;
            if completed == total_blocks || completed % 32 == 0 {
                progress(completed, total_blocks)?;
            }
        }

        // Decompress: vial uses Python lzma which defaults to XZ container format
        let mut decompressed = Vec::new();
        let xz_result = lzma_rs::xz_decompress(&mut &payload[..], &mut decompressed);
        if xz_result.is_err() {
            // fallback: try raw LZMA
            decompressed.clear();
            lzma_rs::lzma_decompress(&mut &payload[..], &mut decompressed)
                .context("Failed to decompress vial definition (tried xz and lzma)")?;
        }

        let json_str =
            std::str::from_utf8(&decompressed).context("Vial definition is not valid UTF-8")?;

        let value: serde_json::Value =
            serde_json::from_str(json_str).context("Failed to parse vial JSON")?;
        Ok(value)
    }

    /// Check whether the keyboard is ready for normal Vial commands.
    /// A firmware may retain the unlocked bit while a new physical unlock hold
    /// is pending; that state is not ready until the pending sequence finishes.
    /// Returns (ready, unlock_keys: Vec<(row,col)>).
    pub fn get_unlock_status(&self) -> Result<(bool, Vec<(u8, u8)>)> {
        let (unlocked, in_progress, keys) = self.get_unlock_status_with_progress()?;
        Ok((unlocked && !in_progress, keys))
    }

    pub(crate) fn get_unlock_status_with_progress(&self) -> Result<(bool, bool, Vec<(u8, u8)>)> {
        let resp = self
            .usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_GET_UNLOCK_STATUS])
            .context("failed to read Vial unlock status")?;
        // resp[0] = unlocked (1=yes), resp[1] = unlock_in_progress
        // resp[2..] = pairs of (row, col), rest filled with 0xFF
        let (unlocked, keys) = parse_unlock_status_response(&resp);
        Ok((unlocked, resp.get(1).copied() == Some(1), keys))
    }

    /// Start unlock sequence — returns keys to hold (row, col pairs)
    pub fn unlock_start(&self) -> Result<()> {
        self.usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_UNLOCK_START])
            .context("failed to start Vial unlock sequence")?;
        Ok(())
    }

    /// Poll unlock status.
    ///
    /// Returns `(unlocked, in_progress, counter, is_rmk)`. RMK uses the
    /// counter as the number of missing combo keys, while Vial/QMK starts at
    /// 50 and decrements it over time. During the application-side RMK hold
    /// period, every successful sample is immediately re-locked so a short
    /// tap cannot leave the keyboard unlocked.
    pub fn unlock_poll(
        &self,
        known_rmk: Option<bool>,
        unlock_key_count: u8,
        keep_rmk_locked: bool,
    ) -> Result<(bool, bool, u8, bool)> {
        let resp = self
            .usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_UNLOCK_POLL])
            .context("failed to poll Vial unlock status")?;
        // resp[0] = unlocked, resp[1] = in_progress, resp[2] = counter
        let mut unlocked = resp[0] == 1;
        let mut in_progress = resp[1] == 1;
        let counter = resp[2];

        // The first QMK poll is 49 or 50. RMK reports only how many of the
        // configured unlock keys are missing, so its value cannot exceed the
        // number of keys. Latch the result in the UI after this first sample.
        let is_rmk = known_rmk.unwrap_or(counter <= unlock_key_count.max(1));

        if is_rmk {
            if keep_rmk_locked || counter > 0 {
                // RMK unlocks as soon as a poll observes every combo key. Keep
                // the real firmware lock engaged until the UI has observed a
                // continuous three-second hold. A released key also restarts
                // the sequence from a known locked state.
                self.lock()
                    .context("failed to keep RMK locked during unlock hold")?;
                self.unlock_start()
                    .context("failed to restart RMK unlock hold")?;
                return Ok((false, true, counter, true));
            }

            // `check_unlock()` runs after RMK fills the response, so counter 0
            // is the authoritative successful sample even if the returned
            // unlocked bit still contains the previous value.
            return Ok((true, false, counter, true));
        }

        // Some Vial-compatible firmware samples `unlocked` and `in_progress`
        // before checking the physical keys. Confirm a zero-counter response
        // without issuing another UNLOCK_POLL, which could restart its state.
        if in_progress && counter == 0 {
            let (confirmed_unlocked, confirmed_in_progress, _) =
                self.get_unlock_status_with_progress()?;
            unlocked = confirmed_unlocked;
            in_progress = confirmed_in_progress;
        }

        Ok((unlocked, in_progress, counter, false))
    }

    /// Lock the keyboard
    pub fn lock(&self) -> Result<()> {
        self.usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_LOCK])
            .context("failed to lock Vial device")?;
        Ok(())
    }
}
