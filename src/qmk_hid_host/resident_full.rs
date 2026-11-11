//! Validate recovered raw FULLs without inventing a host profile identity.
use super::*;
pub(super) fn validate_recovered_full(raw: &[u8], key: &[u8; 24]) -> anyhow::Result<()> {
    anyhow::ensure!((64..=compact::LIMIT).contains(&raw.len()), "FULL length");
    anyhow::ensure!(compact::crc32(raw).to_le_bytes() == key[20..], "FULL CRC32");
    let b = &raw[..32];
    let c = &raw[32..64];
    anyhow::ensure!(
        b[..3] == [0xe1, 8, 1]
            && b[7..9] == [62, 16]
            && b[9] <= 22
            && b[10 + usize::from(b[9])..].iter().all(|b| *b == 0),
        "FULL begin"
    );
    anyhow::ensure!(
        c[..3] == [0xe3, 8, 1]
            && c[3..7] == b[3..7]
            && c[9..11] == [0x57, 2]
            && c[11..31] == key[..20]
            && c[31] == 0,
        "FULL commit/identity"
    );
    fn take<'a>(raw: &'a [u8], pos: &mut usize, n: usize) -> anyhow::Result<&'a [u8]> {
        let out = raw.get(*pos..*pos + n).ok_or_else(|| anyhow::anyhow!("truncated FULL"))?;
        *pos += n;
        Ok(out)
    }
    fn feed(crc: &mut u16, bytes: &[u8]) {
        for &b in bytes {
            *crc ^= u16::from(b) << 8;
            for _ in 0..8 {
                *crc = (*crc << 1) ^ if *crc & 0x8000 != 0 { 0x1021 } else { 0 };
            }
        }
    }
    // Match app_layout_crc16 on the decoded C staging structure, including
    // metadata in slots outside stack_counts. Reconstructing an editor snapshot
    // loses these fields because its Vec holds only the active stack actions.
    let mut crc = 0xffff;
    let mut pos = 64;
    feed(&mut crc, take(raw, &mut pos, 16 * 124 + 16 * 62)?);
    for _ in 0..16 {
        let mut count = None;
        for _ in 0..4 {
            let h = take(raw, &mut pos, 6)?;
            let n = h[0];
            let len = usize::from(h[1]);
            anyhow::ensure!(
                matches!(n, 0 | 2..=4) && len <= 12 && count.is_none_or(|c| c == n),
                "FULL stack count/name"
            );
            if count.is_none() { feed(&mut crc, &[n]); }
            count = Some(n);
            let name = take(raw, &mut pos, len)?;
            // C copies all bytes, appends NUL, then CRCs strlen(name).
            let used = name.iter().position(|b| *b == 0).unwrap_or(len);
            feed(&mut crc, &h[2..6]);
            feed(&mut crc, &[used as u8]);
            feed(&mut crc, &name[..used]);
        }
    }
    feed(&mut crc, &b[9..10 + usize::from(b[9])]);
    for _ in 0..16 {
        let len = take(raw, &mut pos, 1)?[0];
        anyhow::ensure!(len <= 22, "FULL layer name");
        feed(&mut crc, &[len]);
        feed(&mut crc, take(raw, &mut pos, usize::from(len))?);
    }
    anyhow::ensure!(pos == raw.len(), "FULL trailing bytes");
    anyhow::ensure!(crc.to_le_bytes() == c[7..9], "FULL semantic CRC16");
    Ok(())
}
