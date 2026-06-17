use super::hid_parse::{encode_macro_buffer, parse_macro_buffer};
use super::hid_protocol::*;
use super::HidDevice;
use anyhow::{Context, Result};

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    /// Returns a bitmask of pressed keys: bits per row/col.
    /// Response: [cmd, value_id, data...] where data is ceil(rows*cols/8) bytes.
    /// Get macro count from device.
    pub fn get_macro_count(&self) -> Result<u8> {
        let resp = self
            .usb_send(&[CMD_VIA_MACRO_GET_COUNT])
            .context("failed to read macro count")?;
        // Vial GUI trusts the firmware-provided one-byte macro count.
        // Some QMK/Vial keyboards legitimately expose more than 64 slots
        // (for example 109), and Entropy's macro keycodes cover 0x7700..0x77FF.
        Ok(resp[1])
    }

    /// Get macro buffer size.
    pub fn get_macro_buffer_size(&self) -> Result<u16> {
        let resp = self
            .usb_send(&[CMD_VIA_MACRO_GET_BUFFER_SIZE])
            .context("failed to read macro buffer size")?;
        let size = u16::from_be_bytes([resp[1], resp[2]]);
        if size > 8192 {
            anyhow::bail!("invalid macro buffer size: {size}");
        }
        Ok(size)
    }

    /// Read macro buffer (all macros as null-separated strings).
    pub fn get_macro_buffer(&self, size: u16, count: u8) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(size as usize);
        let mut offset: u16 = 0;
        while offset < size {
            let chunk = 28u16.min(size - offset) as u8; // vial-gui BUFFER_FETCH_CHUNK
            let cmd = [
                CMD_VIA_MACRO_GET_BUFFER,
                (offset >> 8) as u8,
                (offset & 0xFF) as u8,
                chunk,
            ];
            let resp = self
                .usb_send(&cmd)
                .with_context(|| format!("failed to read macro buffer at offset {offset}"))?;
            let n = chunk as usize;
            buf.extend_from_slice(&resp[4..4 + n]);
            if buf.iter().filter(|&&byte| byte == 0).count() > count as usize {
                break;
            }
            offset += chunk as u16;
        }
        Ok(buf)
    }

    /// Write macro buffer to device.
    pub fn set_macro_buffer(&self, data: &[u8]) -> Result<()> {
        let mut offset: u16 = 0;
        let total = data.len() as u16;
        while offset < total {
            let chunk = 28u16.min(total - offset);
            let mut cmd = [0u8; 32];
            cmd[0] = CMD_VIA_MACRO_SET_BUFFER;
            cmd[1] = (offset >> 8) as u8;
            cmd[2] = (offset & 0xFF) as u8;
            cmd[3] = chunk as u8;
            let start = offset as usize;
            cmd[4..4 + chunk as usize].copy_from_slice(&data[start..start + chunk as usize]);
            self.usb_send(&cmd)
                .with_context(|| format!("failed to write macro buffer at offset {offset}"))?;
            offset += chunk;
        }
        Ok(())
    }

    /// Parse macro buffer into individual macro bytecode entries.
    pub fn parse_macros(buf: &[u8], count: u8) -> Vec<Vec<u8>> {
        parse_macro_buffer(buf, count)
    }

    /// Encode macro bytecode entries into buffer (null-separated).
    pub fn encode_macros(macros: &[Vec<u8>], buf_size: u16) -> Vec<u8> {
        encode_macro_buffer(macros, buf_size)
    }
}
