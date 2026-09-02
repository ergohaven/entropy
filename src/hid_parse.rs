use anyhow::{bail, Result};

pub(crate) fn parse_macro_buffer(buf: &[u8], count: u8) -> Vec<Vec<u8>> {
    let mut macros = Vec::new();
    let mut start = 0;
    for _ in 0..count {
        let end = buf[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| start + p)
            .unwrap_or(buf.len());
        macros.push(buf[start..end].to_vec());
        start = end + 1;
        if start >= buf.len() {
            break;
        }
    }
    while macros.len() < count as usize {
        macros.push(Vec::new());
    }
    macros
}

pub(crate) fn encode_macro_buffer(macros: &[Vec<u8>], buf_size: u16) -> Vec<u8> {
    let max_len = buf_size as usize;
    let mut buf = Vec::new();
    for m in macros {
        buf.extend_from_slice(m);
        buf.push(0);
    }
    if buf.is_empty() {
        buf.push(0);
    }
    if buf.len() > max_len {
        buf.truncate(max_len);
    }
    buf
}

pub(crate) fn parse_keymap_u16_be(keymap: &[u8]) -> Vec<u16> {
    keymap
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect()
}

pub(crate) fn parse_combo_response(resp: &[u8]) -> Result<([u16; 4], u16)> {
    if resp.first().copied().unwrap_or(1) != 0 {
        bail!("combo get error: {}", resp[0]);
    }
    let mut keys = [0u16; 4];
    for (i, key) in keys.iter_mut().enumerate() {
        let off = 1 + i * 2;
        *key = u16::from_le_bytes([resp[off], resp[off + 1]]);
    }
    let output = u16::from_le_bytes([resp[9], resp[10]]);
    Ok((keys, output))
}

pub(crate) fn parse_key_override_response(resp: &[u8]) -> Result<(u16, u16, u16, u8, u8, u8, u8)> {
    if resp.first().copied().unwrap_or(1) != 0 {
        bail!("key override get error: {}", resp[0]);
    }
    let trigger = u16::from_le_bytes([resp[1], resp[2]]);
    let replacement = u16::from_le_bytes([resp[3], resp[4]]);
    let layers = u16::from_le_bytes([resp[5], resp[6]]);
    Ok((
        trigger,
        replacement,
        layers,
        resp[7],
        resp[8],
        resp[9],
        resp[10],
    ))
}

pub(crate) fn parse_alt_repeat_response(resp: &[u8]) -> Result<(u16, u16, u8, u8)> {
    if resp.first().copied().unwrap_or(1) != 0 {
        bail!("alt repeat key get error: {}", resp[0]);
    }
    Ok((
        u16::from_le_bytes([resp[1], resp[2]]),
        u16::from_le_bytes([resp[3], resp[4]]),
        resp[5],
        resp[6],
    ))
}

pub(crate) fn parse_tap_dance_response(resp: &[u8]) -> Result<(u16, u16, u16, u16, u16)> {
    if resp.first().copied().unwrap_or(1) != 0 {
        bail!("tap dance get error: {}", resp[0]);
    }
    Ok((
        u16::from_le_bytes([resp[1], resp[2]]),
        u16::from_le_bytes([resp[3], resp[4]]),
        u16::from_le_bytes([resp[5], resp[6]]),
        u16::from_le_bytes([resp[7], resp[8]]),
        u16::from_le_bytes([resp[9], resp[10]]),
    ))
}

/// Decode the VIA `id_switch_matrix_state` payload into a row-major pressed map.
///
/// Each matrix row is packed into `ceil(cols / 8)` bytes with the most significant
/// byte first: the first byte of a row carries its highest columns and the last
/// byte carries columns 0-7. QMK (`quantum/via.c`), Vial GUI and RMK all emit
/// and expect this wire order, so it does not depend on the firmware family.
/// Within the chunk `k = col / 8`, bit `n` is column `8 * k + n`; the chunks
/// themselves are serialised in descending order.
pub(crate) fn parse_switch_matrix_payload(data: &[u8], rows: usize, cols: usize) -> Vec<bool> {
    let total = rows * cols;
    let bytes_per_row = cols.div_ceil(8);
    let mut pressed = vec![false; total];

    for row in 0..rows {
        for col in 0..cols {
            let row_byte = bytes_per_row - 1 - col / 8;
            let byte_idx = row * bytes_per_row + row_byte;
            let bit_idx = col % 8;
            if byte_idx < data.len() {
                pressed[row * cols + col] = ((data[byte_idx] >> bit_idx) & 1) != 0;
            }
        }
    }

    pressed
}

pub(crate) fn parse_vialrgb_supported_effects_payload(
    payload: &[u8],
    effects: &mut Vec<u16>,
    current_max: u16,
) -> u16 {
    let mut batch_max = current_max;
    for chunk in payload.chunks_exact(2) {
        let value = u16::from_le_bytes([chunk[0], chunk[1]]);
        if value != 0xFFFF && !effects.contains(&value) {
            effects.push(value);
        }
        batch_max = batch_max.max(value);
    }
    batch_max
}

pub(crate) fn parse_unlock_status_response(resp: &[u8]) -> (bool, Vec<(u8, u8)>) {
    let unlocked = resp.first().copied() == Some(1);
    let mut keys = Vec::new();
    let mut i = 2;
    while i + 1 < resp.len() {
        let row = resp[i];
        let col = resp[i + 1];
        if row == 0xFF && col == 0xFF {
            break;
        }
        keys.push((row, col));
        i += 2;
    }
    (unlocked, keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_pads_macro_buffer() {
        let parsed = parse_macro_buffer(b"one\0two", 4);
        assert_eq!(
            parsed,
            vec![b"one".to_vec(), b"two".to_vec(), Vec::new(), Vec::new()]
        );
    }

    #[test]
    fn encodes_macro_buffer_with_null_separators() {
        let encoded = encode_macro_buffer(&[b"a".to_vec(), b"bc".to_vec()], 6);
        assert_eq!(encoded, b"a\0bc\0".to_vec());
    }

    #[test]
    fn encodes_empty_macro_buffer_as_terminator() {
        let encoded = encode_macro_buffer(&[], 6);
        assert_eq!(encoded, b"\0".to_vec());
    }

    #[test]
    fn truncates_macro_buffer_to_firmware_size() {
        let encoded = encode_macro_buffer(&[b"abcd".to_vec(), b"ef".to_vec()], 5);
        assert_eq!(encoded, b"abcd\0".to_vec());
    }

    #[test]
    fn parses_keymap_big_endian_words() {
        assert_eq!(
            parse_keymap_u16_be(&[0x00, 0x04, 0x7e, 0x01]),
            vec![0x0004, 0x7e01]
        );
    }

    #[test]
    fn parses_combo_response() {
        let mut resp = [0u8; 32];
        resp[1..3].copy_from_slice(&0x0004u16.to_le_bytes());
        resp[3..5].copy_from_slice(&0x0005u16.to_le_bytes());
        resp[5..7].copy_from_slice(&0x0006u16.to_le_bytes());
        resp[7..9].copy_from_slice(&0x0007u16.to_le_bytes());
        resp[9..11].copy_from_slice(&0x0028u16.to_le_bytes());
        assert_eq!(parse_combo_response(&resp).unwrap(), ([4, 5, 6, 7], 0x0028));
    }

    #[test]
    fn rejects_combo_status_error() {
        let mut resp = [0u8; 32];
        resp[0] = 3;
        assert!(parse_combo_response(&resp).is_err());
    }

    #[test]
    fn parses_dynamic_entry_responses() {
        let mut key_override = [0u8; 32];
        key_override[1..3].copy_from_slice(&0x1234u16.to_le_bytes());
        key_override[3..5].copy_from_slice(&0x5678u16.to_le_bytes());
        key_override[5..7].copy_from_slice(&0x00ffu16.to_le_bytes());
        key_override[7] = 1;
        key_override[8] = 2;
        key_override[9] = 3;
        key_override[10] = 4;
        assert_eq!(
            parse_key_override_response(&key_override).unwrap(),
            (0x1234, 0x5678, 0x00ff, 1, 2, 3, 4)
        );

        let mut alt_repeat = [0u8; 32];
        alt_repeat[1..3].copy_from_slice(&0x0004u16.to_le_bytes());
        alt_repeat[3..5].copy_from_slice(&0x0005u16.to_le_bytes());
        alt_repeat[5] = 0xaa;
        alt_repeat[6] = 0x55;
        assert_eq!(
            parse_alt_repeat_response(&alt_repeat).unwrap(),
            (4, 5, 0xaa, 0x55)
        );

        let mut tap_dance = [0u8; 32];
        for (i, value) in [1u16, 2, 3, 4, 200].into_iter().enumerate() {
            let off = 1 + i * 2;
            tap_dance[off..off + 2].copy_from_slice(&value.to_le_bytes());
        }
        assert_eq!(
            parse_tap_dance_response(&tap_dance).unwrap(),
            (1, 2, 3, 4, 200)
        );
    }

    #[test]
    fn parses_switch_matrix_by_row() {
        let pressed = parse_switch_matrix_payload(&[0b0000_0101, 0b0000_0010], 2, 4);
        assert_eq!(
            pressed,
            vec![true, false, true, false, false, true, false, false]
        );
    }

    #[test]
    fn parses_switch_matrix_with_msb_row_byte_first() {
        let pressed = parse_switch_matrix_payload(
            &[0b0000_0010, 0b0000_0100, 0b0000_0001, 0b0000_0000],
            2,
            12,
        );
        assert!(pressed[2]);
        assert!(pressed[9]);
        assert!(pressed[20]);
        assert_eq!(pressed.iter().filter(|&&pressed| pressed).count(), 3);
    }

    #[test]
    fn parses_thirteen_column_switch_matrix_without_column_shift() {
        // 13 columns use two bytes per row: [columns 8-12, columns 0-7].
        // Row 0: column 0 pressed. Row 1: column 8 pressed. Row 2: column 5 pressed.
        let pressed = parse_switch_matrix_payload(
            &[
                0b0000_0000,
                0b0000_0001,
                0b0000_0001,
                0b0000_0000,
                0b0000_0000,
                0b0010_0000,
            ],
            3,
            13,
        );
        let pressed_cells: Vec<usize> = pressed
            .iter()
            .enumerate()
            .filter_map(|(idx, &is_pressed)| is_pressed.then_some(idx))
            .collect();
        assert_eq!(pressed_cells, vec![0, 13 + 8, 26 + 5]);
    }

    #[test]
    fn parses_vialrgb_effect_batch_and_deduplicates() {
        let mut effects = vec![0u16, 2u16];
        let max =
            parse_vialrgb_supported_effects_payload(&[1, 0, 2, 0, 0xff, 0xff], &mut effects, 0);
        effects.sort_unstable();
        assert_eq!(effects, vec![0, 1, 2]);
        assert_eq!(max, 0xffff);
    }

    #[test]
    fn parses_unlock_status_until_sentinel() {
        let resp = [1, 0, 3, 4, 5, 6, 0xff, 0xff, 7, 8];
        assert_eq!(
            parse_unlock_status_response(&resp),
            (true, vec![(3, 4), (5, 6)])
        );
    }
}
