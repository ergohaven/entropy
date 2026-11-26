//! TEST23 resident cache, always transacted on the existing HID owner.
use super::*;

pub(super) fn request(command: u8, slot: u8, identity: &[u8], crc: &[u8]) -> [u8; 32] {
    // A bridge restart can reuse the same HID owner with the same identity/CRC.
    // Do not restart its correlation namespace with a worker-local counter.
    static NONCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let nonce = NONCE.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let mut p = [0; 32];
    p[..4].copy_from_slice(&[0xef, 8, command, slot]);
    p[4..8].copy_from_slice(&nonce.to_le_bytes());
    p[8..28].copy_from_slice(identity);
    p[28..32].copy_from_slice(crc);
    p
}
