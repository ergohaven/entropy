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

pub(super) fn exchange(
    dev: &HostDataHid,
    owner: &Mutex<ApplicationLayoutOwnerState>,
    snapshot: &crate::application_layouts::ApplicationLayoutSnapshot,
    generation: u64,
    stop: &AtomicBool,
    started: Instant,
    request: &[u8; 32],
) -> anyhow::Result<Option<bool>> {
    let current =
        || !stop.load(Ordering::Acquire) && started.elapsed() < Duration::from_millis(2500);
    if !current() {
        return Ok(None);
    }
    let mut answer = None;
    if !write_application_layout_payload(owner, snapshot, generation, request, |p| {
        if current() {
            answer = match dev {
                HostDataHid::Shared(output) => output.poll_application_layout_event(p)?,
                HostDataHid::Dedicated(hid) => Some(hid.usb_send(p)?),
            };
        }
        Ok(())
    })? {
        return Ok(None);
    }
    // A late reply cannot confirm a revoked publication, stopped worker, expired
    // transfer or replaced serialized host-output lease.
    if !current()
        || !write_application_layout_payload(owner, snapshot, generation, &[], |_| Ok(()))?
        || matches!(dev, HostDataHid::Shared(output) if !output.host_session_is_current())
    {
        return Ok(None);
    }
    let Some(reply) = answer else {
        return Ok(None);
    }; // settings/unlock owns the lease
    let mut expected = *request;
    expected[2] |= 0x80;
    expected[3] = reply[3];
    anyhow::ensure!(
        reply == expected && reply[3] <= 1,
        "Unconfirmed resident cache reply"
    );
    Ok(Some(reply[3] == 1))
}
