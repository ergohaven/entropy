//! SM62 USB-only compact FULL v1. Legacy packets (including semantic digest)
//! remain the authority. TEST23 additionally advertises three resident slots.
pub(super) const LIMIT: usize = 4560;
pub(super) fn query(nonce: u32) -> [u8; 32] {
    let mut p = [0; 32];
    p[..4].copy_from_slice(&[0xeb, 8, 0xa5, 1]);
    p[4..8].copy_from_slice(&nonce.to_le_bytes());
    p
}
pub(super) fn supported(request: &[u8; 32], reply: &[u8; 32]) -> bool {
    if request[..4] != [0xeb, 8, 0xa5, 1] || request[8..].iter().any(|b| *b != 0) {
        return false;
    }
    let mut expected = *request;
    expected[2] = 0x5a;
    expected[8] = 3;
    expected[9] = reply[9];
    expected[10..12].copy_from_slice(&(LIMIT as u16).to_le_bytes());
    matches!(reply[9], 0 | 3) && *reply == expected
}
/// A negative decision needs a byte-exact correlated legacy rejection. Anything
/// else (including a plausible reply with the wrong nonce) remains unknown.
pub(super) fn capability(request: &[u8; 32], reply: &[u8; 32]) -> Option<bool> {
    if request[..4] != [0xeb, 8, 0xa5, 1] || request[8..].iter().any(|b| *b != 0) {
        return None;
    }
    if supported(request, reply) {
        Some(true)
    } else {
        let mut rejected = *request;
        rejected[0] = 0xff; // VIA's explicit unhandled-command response.
        (*reply == *request || *reply == rejected).then_some(false)
    }
}
pub(super) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !crc
}
fn lz(input: &[u8]) -> Vec<u8> {
    // At most 64 candidates per position, 130 compared bytes per candidate.
    // Fixed input bound; worker-only CPU and allocations.
    let mut positions =
        std::collections::HashMap::<[u8; 3], std::collections::VecDeque<usize>>::new();
    let mut out = Vec::new();
    let mut literals = Vec::new();
    let mut i = 0;
    fn flush(out: &mut Vec<u8>, literals: &mut Vec<u8>) {
        if !literals.is_empty() {
            out.push((literals.len() - 1) as u8);
            out.append(literals);
        }
    }
    while i < input.len() {
        let mut best = 0;
        let mut distance = 0;
        if i + 3 <= input.len() {
            let key: [u8; 3] = input[i..i + 3].try_into().unwrap();
            if let Some(ps) = positions.get(&key) {
                for &p in ps.iter().rev() {
                    if i - p > 4096 {
                        break;
                    }
                    let mut n = 0;
                    while n < 130 && i + n < input.len() && input[p + n] == input[i + n] {
                        n += 1;
                    }
                    if n > best {
                        best = n;
                        distance = i - p;
                    }
                }
            }
        }
        let n = if best >= 4 { best } else { 1 };
        if n > 1 {
            flush(&mut out, &mut literals);
            out.push((128 + n - 3) as u8);
            out.extend_from_slice(&(distance as u16).to_le_bytes());
        } else {
            literals.push(input[i]);
            if literals.len() == 128 {
                flush(&mut out, &mut literals);
            }
        }
        for p in i..i + n {
            if p + 3 <= input.len() {
                let ps = positions
                    .entry(input[p..p + 3].try_into().unwrap())
                    .or_default();
                if ps.len() == 64 {
                    ps.pop_front();
                }
                ps.push_back(p);
            }
        }
        i += n;
    }
    flush(&mut out, &mut literals);
    out
}
pub(super) fn full(packets: &[[u8; 32]]) -> Option<Vec<u8>> {
    if packets.len() != 210
        || packets[0][..2] != [0xe1, 8]
        || packets[0][7..9] != [62, 16]
        || packets[209][..2] != [0xe3, 8]
    {
        return None;
    }
    let mut raw = Vec::with_capacity(LIMIT);
    raw.extend_from_slice(&packets[0]);
    raw.extend_from_slice(&packets[209]);
    for p in &packets[1..209] {
        match p[0] {
            0xe2 if p[4] <= 13 => raw.extend_from_slice(&p[5..5 + usize::from(p[4]) * 2]),
            0xe7 if p[4] <= 27 => raw.extend_from_slice(&p[5..5 + usize::from(p[4])]),
            0xe9 if p[5] <= 12 => raw.extend_from_slice(&p[4..10 + usize::from(p[5])]),
            0xe4 if p[3] <= 22 => raw.extend_from_slice(&p[3..4 + usize::from(p[3])]),
            _ => return None,
        }
    }
    if raw.len() > LIMIT {
        return None;
    }
    Some(raw)
}
pub(super) fn encode(packets: &[[u8;32]], transaction:u32)->Option<Vec<[u8;32]>> {
    let raw=full(packets)?;
    let compressed = lz(&raw);
    let codec = u8::from(compressed.len() < raw.len());
    let data = if codec == 1 { &compressed } else { &raw };
    let count = data.len().div_ceil(24);
    let mut begin = [0; 32];
    begin[..4].copy_from_slice(&[0xec, 8, 1, codec]);
    begin[4..8].copy_from_slice(&transaction.to_le_bytes());
    begin[8..10].copy_from_slice(&(data.len() as u16).to_le_bytes());
    begin[10..12].copy_from_slice(&(raw.len() as u16).to_le_bytes());
    begin[12..14].copy_from_slice(&(count as u16).to_le_bytes());
    begin[16..20].copy_from_slice(&crc32(&raw).to_le_bytes());
    begin[20..24].copy_from_slice(&crc32(data).to_le_bytes());
    let mut out = vec![begin];
    for (seq, part) in data.chunks(24).enumerate() {
        let mut p = [0; 32];
        p[..2].copy_from_slice(&[0xed, 8]);
        p[2..6].copy_from_slice(&transaction.to_le_bytes());
        p[6..8].copy_from_slice(&(seq as u16).to_le_bytes());
        p[8..8 + part.len()].copy_from_slice(part);
        out.push(p);
    }
    let mut end = [0; 32];
    end[..2].copy_from_slice(&[0xee, 8]);
    end[2..6].copy_from_slice(&transaction.to_le_bytes());
    end[6..8].copy_from_slice(&(count as u16).to_le_bytes());
    end[8..12].copy_from_slice(&crc32(data).to_le_bytes());
    out.push(end);
    (out.len() < packets.len()).then_some(out)
}
