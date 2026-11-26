//! Correlated resident capabilities and the read-only compact catalog.
use super::resident100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Capabilities {
    pub(crate) slots: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Capability {
    Legacy,
    Resident(Capabilities),
}

impl Capability {
    pub(crate) fn resident(self) -> Option<Capabilities> {
        match self {
            Self::Resident(c) => Some(c),
            Self::Legacy => None,
        }
    }
}

/// The bitmap wire format fits at most 128 slots, independent of product capacity.
pub(super) fn capabilities(request: &[u8; 32], reply: &[u8; 32]) -> Option<Capabilities> {
    if request[..4] == [0xeb, 8, 0xa5, 3] && request[8..].iter().all(|b| *b == 0) {
        let mut expected = *request;
        expected[2] = 0x5a;
        expected[8..16].copy_from_slice(&[3, 0, 0xd0, 0x11, 16, 62, 22, 0]);
        expected[16..20].copy_from_slice(&4194304u32.to_le_bytes());
        expected[20..24].copy_from_slice(&1u32.to_le_bytes());
        expected[24..27].copy_from_slice(&[2, 100, 1]);
        return (*reply == expected).then_some(Capabilities { slots: 100 });
    }
    if request[..4] != [0xeb, 8, 0xa5, 2]
        || request[8..].iter().any(|b| *b != 0)
        || !(1..=crate::application_layouts::SM62_RESIDENT_SLOT_LIMIT).contains(&reply[9])
    {
        return None;
    }
    let mut expected = *request;
    expected[2] = 0x5a;
    expected[8..15].copy_from_slice(&[2, reply[9], 0xd0, 0x11, 16, 62, 22]);
    expected[16..20].copy_from_slice(&4194304u32.to_le_bytes());
    expected[20..24].copy_from_slice(&15u32.to_le_bytes());
    expected[24] = 2;
    (*reply == expected).then_some(Capabilities { slots: reply[9] })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Catalog {
    generation: u32,
    slots: u8,
    bitmap: [u8; 16],
}

impl Catalog {
    pub(super) fn occupied(&self, slot: usize) -> bool {
        slot < usize::from(self.slots) && self.bitmap[slot / 8] & (1 << (slot % 8)) != 0
    }
}

pub(super) fn query() -> [u8; 32] {
    resident100::request(7, 0, None, &[0; 20], &[0; 4])
}

/// Only an exact, correlated BAD_ARGUMENT with zero data selects old firmware.
/// Echoes, transport failures and integrity errors never become an empty catalog.
pub(super) fn decode(
    request: &[u8; 32],
    reply: &[u8; 32],
    caps: Capabilities,
) -> anyhow::Result<Option<Catalog>> {
    anyhow::ensure!(
        request[..4] == [0xef, 9, 7, 0] && request[8..].iter().all(|b| *b == 0),
        "Resident catalog request"
    );
    anyhow::ensure!(
        reply[..3] == [0xef, 9, 0x87] && reply[4..8] == request[4..8],
        "Resident catalog correlation"
    );
    if reply[3] == 2 && reply[8..].iter().all(|b| *b == 0) {
        return Ok(None);
    }
    anyhow::ensure!(reply[3] == 0, "Resident catalog status {}", reply[3]);
    let len = usize::from(caps.slots).div_ceil(8);
    anyhow::ensure!(
        (1..=crate::application_layouts::SM62_RESIDENT_SLOT_LIMIT).contains(&caps.slots)
            && reply[12] == caps.slots
            && usize::from(reply[13]) == len,
        "Resident catalog capacity"
    );
    anyhow::ensure!(
        reply[14 + len..].iter().all(|b| *b == 0),
        "Resident catalog padding"
    );
    if caps.slots % 8 != 0 {
        anyhow::ensure!(
            reply[13 + len] >> (caps.slots % 8) == 0,
            "Resident catalog excess bits"
        );
    }
    let mut bitmap = [0; 16];
    bitmap[..len].copy_from_slice(&reply[14..14 + len]);
    Ok(Some(Catalog {
        generation: u32::from_le_bytes(reply[8..12].try_into()?),
        slots: caps.slots,
        bitmap,
    }))
}

#[cfg(test)]
pub(super) fn fixture_capabilities() -> Capabilities {
    Capabilities { slots: 100 }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test]
    fn host_managed_eb3_readonly_archive_contract() {
        let mut q = [0u8; 32];
        q[..4].copy_from_slice(&[0xeb, 8, 0xa5, 3]);
        q[4..8].copy_from_slice(&123u32.to_le_bytes());
        let mut r = q;
        r[2] = 0x5a;
        r[8..16].copy_from_slice(&[3, 0, 0xd0, 0x11, 16, 62, 22, 0]);
        r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
        r[20..24].copy_from_slice(&1u32.to_le_bytes());
        r[24..27].copy_from_slice(&[2, 100, 1]);
        assert_eq!(capabilities(&q, &r).map(|c| c.slots), Some(100));
        for i in [4, 8, 9, 15, 20, 24, 25, 26, 27, 31] {
            let mut bad = r; bad[i] ^= 1;
            assert!(capabilities(&q, &bad).is_none(), "offset {i}");
        }
    }

    #[test]
    fn host_managed_archive_reader_never_uploads_published_library() {
        let (hid, rec) = crate::hid::HidDevice::test_device();
        rec.respond_by(|p| Some(Ok(peer(p, 7, &[], 9))));
        let dev = HostDataHid::Shared(hid.shared_output().unwrap());
        let owner = Mutex::new(ApplicationLayoutOwnerState::default());
        let mut profile = crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
        profile.id = "host-draft".into(); profile.layers = vec![vec![4; 62]; 16];
        let mut snap = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&profile);
        snap.resident_slot = Some(0);
        owner.lock().unwrap().library = Arc::new(vec![snap]);
        let foreground = owner.lock().unwrap().snapshot.clone();
        let mut bg = resident100::Background::default();
        for _ in 0..40 { bg.tick_with_capabilities(Capabilities { slots: 7 }, &dev, &owner, &foreground, 0, &AtomicBool::new(false), &mut 0); }
        assert!(owner.lock().unwrap().resident_inventory_complete);
        assert!(!rec.requests().iter().any(|p| matches!(p[0], 0xec | 0xed | 0xee)), "archive reader staged a host draft");
        assert!(rec.requests().iter().all(|p| p[0] == 0xef && matches!(p[2], 3 | 6 | 7)));
    }

    #[test]
    fn host_managed_worker_streams_mode1_on_old_and_new_firmware_without_resident_writes() {
        for eb3 in [false, true] {
            let raw = full("host-owned-runtime");
            let mut key = [0; 24]; key[..20].copy_from_slice(&raw[43..63]);
            key[20..].copy_from_slice(&compact::crc32(&raw).to_le_bytes());
            let record = resident100::RecoveredRecord { slot: 7, identity_crc: key, raw: raw.clone(), validation: None };
            let profile = decode_device_profile(&record).unwrap();
            let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
            settings.device_raw_profiles.insert(profile.id.clone(), raw.clone());
            settings.resident_slots.insert(profile.id.clone(), 7);
            let snapshot = settings.snapshot_for_layout(&profile);
            let records = vec![(7, raw.clone())];
            let (hid, rec) = crate::hid::HidDevice::test_device();
            rec.respond_by(move |p| {
                if p[..4] == [0xeb, 8, 0xa5, 3] {
                    if !eb3 { return Some(Ok(*p)); }
                    let mut r = *p; r[2] = 0x5a;
                    r[8..16].copy_from_slice(&[3,0,0xd0,0x11,16,62,22,0]);
                    r[16..20].copy_from_slice(&4194304u32.to_le_bytes());
                    r[20..24].copy_from_slice(&1u32.to_le_bytes()); r[24..27].copy_from_slice(&[2,100,1]);
                    return Some(Ok(r));
                }
                if p[0] == 0xeb || p[..2] == [0xef, 9] { return Some(Ok(peer(p, 100, &records, 3))); }
                if p[0] == 0xe6 { let mut r = [0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]); return Some(Ok(r)); }
                None
            });
            let mut target = crate::device::test_usb_device("host-managed-worker", 1, 0x00b2);
            target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
            let mut b = test_start_bridge(target, Default::default(), hid.shared_output(), None, HostProtocol::Selected(false), || None);
            b.bind_selected_application_layout_device("host-managed".into());
            b.set_selected_application_layout_support(true);
            let g = b.application_layout_publication_generation().unwrap();
            assert!(b.publish_application_layout_snapshot_for_generation(snapshot, g));
            let deadline = Instant::now() + Duration::from_secs(15);
            while b.completed_legacy_archive().is_none() || !rec.requests().iter().any(|p|p[0] == 0xee) {
                assert!(Instant::now() < deadline, "worker never completed archive and RAM transfer, eb3={eb3}");
                thread::sleep(Duration::from_millis(5));
            }
            b.stop(); if let Some(t) = b.thread.take() { t.join().unwrap(); }
            let packets = rec.requests();
            assert!(packets.iter().any(|p|p[..3] == [0xec,8,1] && p[16..20] == key[20..]));
            assert!(packets.iter().all(|p|p[0] != 0xec || p[2] == 1));
            assert!(packets.iter().all(|p|p[..2] != [0xef,9] || matches!(p[2],3|6|7)));
        }
    }

    fn summary(p: &[u8; 32], slots: u8, occupied: &[u8], generation: u32) -> [u8; 32] {
        let mut r = *p;
        r[2] = 0x87;
        r[8..].fill(0);
        r[8..12].copy_from_slice(&generation.to_le_bytes());
        r[12] = slots;
        r[13] = slots.div_ceil(8);
        for slot in occupied {
            r[14 + usize::from(*slot) / 8] |= 1 << (*slot % 8);
        }
        r
    }

    fn full(id: &str) -> Vec<u8> {
        let mut p = crate::application_layouts::DeviceApplicationLayouts::default().layouts
            ["default"]
            .clone();
        p.id = id.into();
        p.name = id.into();
        p.layers = vec![vec![4; 62]; 16];
        let snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&p);
        compact::full(&ApplicationLayoutTransmission.packets(&snapshot)).unwrap()
    }

    fn peer(p: &[u8; 32], slots: u8, records: &[(u8, Vec<u8>)], generation: u32) -> [u8; 32] {
        if p[0] == 0xeb {
            if p[3] == 3 { return *p; }
            let mut r = super::super::resident100_tests::capability(p);
            r[9] = slots;
            return r;
        }
        if p[2] == 7 {
            return summary(
                p,
                slots,
                &records.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
                generation,
            );
        }
        let mut r = *p;
        r[2] |= 0x80;
        let record = records.iter().find(|(slot, _)| *slot == p[3]);
        if p[2] == 3 {
            r[8..].fill(0);
            r[3] = if record.is_some() { 0 } else { 1 };
            if let Some((_, raw)) = record {
                r[8..28].copy_from_slice(&raw[43..63]);
                r[28..].copy_from_slice(&compact::crc32(raw).to_le_bytes());
            }
        } else {
            assert_eq!(p[2], 6, "export must never mutate profiles");
            let raw = &record.unwrap().1;
            let off = usize::from(u16::from_le_bytes(p[8..10].try_into().unwrap()));
            let n = raw.len().saturating_sub(off).min(20);
            r[10] = n as u8;
            r[11..].fill(0);
            r[12..12 + n].copy_from_slice(&raw[off..off + n]);
        }
        r
    }

    #[test]
    fn empty_catalog_uses_three_readonly_packets_and_negotiated_capacity() {
        for slots in [1, 7, 100] {
            let (hid, rec) = crate::hid::HidDevice::test_device();
            rec.respond_by(move |p| Some(Ok(peer(p, slots, &[], 0))));
            assert!(hid.read_resident_export_catalog().unwrap().is_empty());
            let ps = rec.requests();
            assert_eq!(ps.len(), 4, "EB3 rejection, EB2 capability plus two generation-fenced bitmaps");
            assert_eq!(ps.iter().filter(|p| p[..3] == [0xef, 9, 7]).count(), 2);
            assert!(ps.iter().all(|p| p[0] == 0xeb || p[..3] == [0xef, 9, 7]));
        }
    }

    #[test]
    fn sparse_catalog_preserves_first_last_complete_full_and_skips_empty_slots() {
        let records = vec![(0, full("first")), (99, full("last"))];
        let expected = records.clone();
        let (hid, rec) = crate::hid::HidDevice::test_device();
        rec.respond_by(move |p| Some(Ok(peer(p, 100, &records, 17))));
        let recovered = hid.read_resident_export_catalog().unwrap();
        assert_eq!(
            recovered
                .iter()
                .map(|r| (r.slot, r.raw.clone()))
                .collect::<Vec<_>>(),
            expected
        );
        let ps = rec.requests();
        assert_eq!(ps.iter().filter(|p| p[..3] == [0xef, 9, 7]).count(), 2);
        assert_eq!(ps.iter().filter(|p| p[..3] == [0xef, 9, 3]).count(), 4);
        assert!(ps
            .iter()
            .filter(|p| p[0] == 0xef && p[2] != 7)
            .all(|p| p[3] == 0 || p[3] == 99));
    }

    #[test]
    fn catalog_generation_change_rejects_same_occupied_bitmap() {
        let records = vec![(99, full("last"))];
        let (hid, rec) = crate::hid::HidDevice::test_device();
        let mut calls = 0;
        rec.respond_by(move |p| {
            if p[..3] == [0xef, 9, 7] {
                calls += 1;
            }
            Some(Ok(peer(p, 100, &records, calls)))
        });
        assert!(hid
            .read_resident_export_catalog()
            .unwrap_err()
            .to_string()
            .contains("changed during export"));
    }

    #[test]
    fn malformed_or_failed_catalog_never_falls_back_to_empty_or_legacy() {
        for fault in 0..8 {
            let (hid, rec) = crate::hid::HidDevice::test_device();
            rec.respond_by(move |p| {
                if p[0] == 0xeb {
                    return Some(Ok(peer(p, 100, &[], 0)));
                }
                assert_eq!(p[..3], [0xef, 9, 7]);
                let mut r = summary(p, 100, &[], 0);
                match fault {
                    0 => r[4] ^= 1,
                    1 => {
                        r[3] = 7;
                        r[8..].fill(0);
                    }
                    2 => r[31] = 1,
                    3 => r[12] = 99,
                    4 => r[13] = 12,
                    5 => r[26] = 0x80,
                    6 => {
                        r[3] = 2;
                        r[8..].fill(0);
                        r[8] = 1;
                    }
                    7 => r = *p,
                    _ => unreachable!(),
                }
                Some(Ok(r))
            });
            assert!(hid.read_resident_export_catalog().is_err(), "fault {fault}");
            assert_eq!(rec.requests().len(), 3);
        }
    }

    #[test]
    fn exact_old_firmware_rejection_keeps_full_legacy_scan() {
        let (hid, rec) = crate::hid::HidDevice::test_device();
        rec.respond_by(move |p| {
            if p[..3] == [0xef, 9, 7] {
                let mut r = *p;
                r[2] = 0x87;
                r[3] = 2;
                return Some(Ok(r));
            }
            Some(Ok(peer(p, 7, &[], 0)))
        });
        assert!(hid.read_resident_export_catalog().unwrap().is_empty());
        assert_eq!(
            rec.requests()
                .iter()
                .filter(|p| p[..3] == [0xef, 9, 3])
                .count(),
            14
        );
    }

    #[test]
    fn background_uses_same_sparse_catalog_and_confirms_generation_before_completion() {
        let (hid, rec) = crate::hid::HidDevice::test_device();
        let records = vec![(0, full("first")), (6, full("last"))];
        rec.respond_by(move |p| Some(Ok(peer(p, 7, &records, 9))));
        let dev = HostDataHid::Shared(hid.shared_output().unwrap());
        let owner = Mutex::new(ApplicationLayoutOwnerState::default());
        let foreground = owner.lock().unwrap().snapshot.clone();
        let mut bg = resident100::Background::default();
        let mut token = 0;
        for _ in 0..500 {
            bg.tick_with_capabilities(
                Capabilities { slots: 7 },
                &dev,
                &owner,
                &foreground,
                0,
                &AtomicBool::new(false),
                &mut token,
            );
            if owner.lock().unwrap().resident_inventory_complete {
                break;
            }
        }
        let state = owner.lock().unwrap();
        assert!(state.resident_inventory_complete);
        assert_eq!(
            state
                .resident_recoveries
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 6]
        );
        assert_eq!(
            rec.requests()
                .iter()
                .filter(|p| p[..3] == [0xef, 9, 7])
                .count(),
            2
        );
        assert!(rec
            .requests()
            .iter()
            .all(|p| p[0] == 0xef && matches!(p[2], 3 | 6 | 7)));
    }
}
