use super::hid_parse::parse_keymap_u16_be;
use super::hid_protocol::*;
use super::HidDevice;
use anyhow::{bail, Context, Result};

#[derive(Debug)]
struct KeycodeWritebackMismatch {
    layer: u8,
    row: u8,
    col: u8,
    requested: u16,
    readback: u16,
}

impl std::fmt::Display for KeycodeWritebackMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "keycode writeback mismatch at layer {}, row {}, col {}: wrote {:#06X}, read back {:#06X}",
            self.layer, self.row, self.col, self.requested, self.readback
        )
    }
}

impl std::error::Error for KeycodeWritebackMismatch {}

pub(crate) fn keycode_writeback_readback(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<KeycodeWritebackMismatch>()
        .map(|mismatch| mismatch.readback)
}

fn verify_keycode_writeback(
    layer: u8,
    row: u8,
    col: u8,
    requested: u16,
    readback: u16,
) -> Result<()> {
    if readback == requested {
        return Ok(());
    }

    Err(KeycodeWritebackMismatch {
        layer,
        row,
        col,
        requested,
        readback,
    }
    .into())
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    pub fn get_layer_count(&self) -> Result<u8> {
        let resp = self.usb_send(&[CMD_VIA_GET_LAYER_COUNT])?;
        let count = resp[1];
        if count == 0 || count > 32 {
            bail!("invalid layer count reported by firmware: {count}");
        }
        Ok(count)
    }

    /// Read entire keymap buffer at once (faster than per-key requests).
    /// Returns Vec of keycodes indexed by [layer * rows * cols + row * cols + col].
    pub fn get_keymap_buffer(&self, layers: usize, rows: usize, cols: usize) -> Result<Vec<u16>> {
        if layers == 0 || layers > 32 || rows == 0 || rows > 32 || cols == 0 || cols > 32 {
            bail!("invalid keymap dimensions: layers={layers}, rows={rows}, cols={cols}");
        }
        let total_keys = layers
            .checked_mul(rows)
            .and_then(|v| v.checked_mul(cols))
            .context("keymap dimensions overflow")?;
        if total_keys > 4096 {
            bail!("keymap is too large: {total_keys} keys");
        }
        let total_bytes = total_keys.checked_mul(2).context("keymap size overflow")?;
        if total_bytes > u16::MAX as usize {
            bail!("keymap buffer is too large for VIA offset: {total_bytes} bytes");
        }
        let mut keymap = vec![0u8; total_bytes];

        let mut offset = 0usize;
        while offset < total_bytes {
            let sz = (total_bytes - offset).min(BUFFER_FETCH_CHUNK);
            // CMD_VIA_KEYMAP_GET_BUFFER, offset (big-endian u16), size (u8)
            let cmd = [
                CMD_VIA_KEYMAP_GET_BUFFER,
                ((offset >> 8) & 0xFF) as u8,
                (offset & 0xFF) as u8,
                sz as u8,
            ];
            let resp = self
                .usb_send(&cmd)
                .with_context(|| format!("failed to read keymap buffer at offset {offset}"))?;
            // response: [cmd, offset_hi, offset_lo, sz, data[0..sz]]
            keymap[offset..offset + sz].copy_from_slice(&resp[4..4 + sz]);
            offset += sz;
        }

        Ok(parse_keymap_u16_be(&keymap))
    }

    /// Read one layer from the VIA keymap buffer using absolute buffer offsets.
    /// This keeps staged Bluetooth loading proportional to one layer instead of
    /// re-reading every layer before the first usable screen can be shown.
    pub fn get_keymap_layer(
        &self,
        layer: usize,
        layers: usize,
        rows: usize,
        cols: usize,
    ) -> Result<Vec<u16>> {
        if layers == 0
            || layers > 32
            || layer >= layers
            || rows == 0
            || rows > 32
            || cols == 0
            || cols > 32
        {
            bail!(
                "invalid keymap layer request: layer={layer}, layers={layers}, rows={rows}, cols={cols}"
            );
        }
        let layer_keys = rows.checked_mul(cols).context("keymap layer overflow")?;
        let layer_bytes = layer_keys
            .checked_mul(2)
            .context("keymap layer size overflow")?;
        let offset = layer
            .checked_mul(layer_bytes)
            .context("keymap layer offset overflow")?;
        let total_bytes = layers
            .checked_mul(layer_bytes)
            .context("keymap size overflow")?;
        if total_bytes > u16::MAX as usize {
            bail!("keymap buffer is too large for VIA offset: {total_bytes} bytes");
        }

        let mut keymap = vec![0u8; layer_bytes];
        let mut local_offset = 0usize;
        while local_offset < layer_bytes {
            let absolute_offset = offset + local_offset;
            let sz = (layer_bytes - local_offset).min(BUFFER_FETCH_CHUNK);
            let cmd = [
                CMD_VIA_KEYMAP_GET_BUFFER,
                ((absolute_offset >> 8) & 0xFF) as u8,
                (absolute_offset & 0xFF) as u8,
                sz as u8,
            ];
            let resp = self.usb_send(&cmd).with_context(|| {
                format!("failed to read keymap layer {layer} at buffer offset {absolute_offset}")
            })?;
            keymap[local_offset..local_offset + sz].copy_from_slice(&resp[4..4 + sz]);
            local_offset += sz;
        }

        Ok(parse_keymap_u16_be(&keymap))
    }

    pub fn get_keycode(&self, layer: u8, row: u8, col: u8) -> Result<u16> {
        let resp = self
            .usb_send(&[CMD_VIA_GET_KEYCODE, layer, row, col])
            .with_context(|| {
                format!("failed to read keycode at layer {layer}, row {row}, col {col}")
            })?;
        Ok(u16::from_be_bytes([resp[4], resp[5]]))
    }

    pub fn set_keycode(&self, layer: u8, row: u8, col: u8, keycode: u16) -> Result<()> {
        let [hi, lo] = keycode.to_be_bytes();
        self.usb_send(&[CMD_VIA_SET_KEYCODE, layer, row, col, hi, lo])
            .with_context(|| {
                format!("failed to set keycode at layer {layer}, row {row}, col {col}")
            })?;
        let readback = self.get_keycode(layer, row, col)?;
        verify_keycode_writeback(layer, row, col, keycode, readback)?;
        Ok(())
    }

    pub fn get_encoder(&self, layer: u8, idx: u8) -> Result<(u16, u16)> {
        let resp = self
            .usb_send(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_GET_ENCODER, layer, idx])
            .with_context(|| format!("failed to read encoder {idx} on layer {layer}"))?;
        if resp.len() < 4 {
            anyhow::bail!("encoder get response too short for layer {layer}, idx {idx}");
        }
        Ok((
            u16::from_be_bytes([resp[0], resp[1]]),
            u16::from_be_bytes([resp[2], resp[3]]),
        ))
    }

    pub fn set_encoder(&self, layer: u8, idx: u8, direction: u8, keycode: u16) -> Result<()> {
        let bytes = keycode.to_be_bytes();
        let _ = self
            .usb_send(&[
                CMD_VIA_VIAL_PREFIX,
                CMD_VIAL_SET_ENCODER,
                layer,
                idx,
                direction,
                bytes[0],
                bytes[1],
            ])
            .with_context(|| {
                format!("failed to set encoder {idx} direction {direction} on layer {layer}")
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycode_writeback_accepts_matching_kc_no() {
        assert!(verify_keycode_writeback(5, 3, 7, 0x0000, 0x0000).is_ok());
    }

    #[test]
    fn keycode_writeback_rejects_stale_transparent_after_kc_no_write() {
        let err = verify_keycode_writeback(5, 3, 7, 0x0000, 0x0001).unwrap_err();

        assert!(
            err.to_string().contains("read back 0x0001"),
            "unexpected error: {err}"
        );
        assert_eq!(keycode_writeback_readback(&err), Some(0x0001));
    }

    #[test]
    fn staged_layer_read_uses_absolute_offset_and_only_one_layer() {
        let (hid, recorder) = HidDevice::test_device();

        let layer = hid.get_keymap_layer(3, 16, 10, 6).unwrap();

        assert_eq!(layer.len(), 60);
        let requests = recorder.requests();
        assert_eq!(requests.len(), 5);
        assert_eq!(
            &requests[0][..4],
            &[CMD_VIA_KEYMAP_GET_BUFFER, 0x01, 0x68, 28]
        );
        assert_eq!(
            &requests[4][..4],
            &[CMD_VIA_KEYMAP_GET_BUFFER, 0x01, 0xD8, 8]
        );
    }

    #[test]
    fn staged_layer_read_rejects_layer_outside_reported_count() {
        let (hid, _) = HidDevice::test_device();

        let error = hid.get_keymap_layer(16, 16, 10, 6).unwrap_err();

        assert!(error.to_string().contains("layer=16"));
    }
}
