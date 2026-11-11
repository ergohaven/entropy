//! Source-only checkpoint: production HidDevice/bridge, modeled OS HID only.

// Predicate-level state fixture, not a claim of cold inventory/OS completion.
// FULL bytes and their integrity validation use the real producer and validator.
#[test]
fn unattended_inventory_requires_lossless_profile_payload_not_only_raw_membership() {
    for case in ["canonical", "visual", "author-draft"] {
        let bridge = inert_bound();
        let mut settings = crate::application_layouts::DeviceApplicationLayouts::default();
        let mut source = settings.layouts["default"].clone();
        source.id = "unattended-profile".into();
        source.name = "Unattended profile".into();
        source.layers = vec![vec![4; 62]; 16];
        let mut snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&source);
        if case == "visual" { snapshot.visuals[15][61] = 7; }
        let raw = compact::full(&ApplicationLayoutTransmission.packets(&snapshot)).unwrap();
        let mut identity_crc = [0u8; 24];
        identity_crc[..20].copy_from_slice(&raw[43..63]);
        identity_crc[20..].copy_from_slice(&compact::crc32(&raw).to_le_bytes());
        validate_recovered_full(&raw, &identity_crc).unwrap();
        let record = resident100::RecoveredRecord { slot: 99, identity_crc, raw: raw.clone(), validation: None };
        let mut decoded = decode_device_profile(&record).unwrap();
        if case == "author-draft" { decoded.layers[15][61] = 5; }
        settings.resident_slots.insert(decoded.id.clone(), 99);
        settings.device_raw_profiles.insert(decoded.id.clone(), raw);
        settings.layouts.insert(decoded.id.clone(), decoded);
        {
            let mut state = bridge.application_layout_snapshot.lock().unwrap();
            state.resident_export_capability = Some(true);
            state.resident_inventory_complete = true;
            state.resident_recoveries.insert(99, record);
        }
        assert_eq!(bridge.resident_export_inventory_confirmed(&settings), case == "canonical", "{case}");
    }
}

use super::*;

#[test]
fn export_probe_schema2_support_is_read_only_and_skips_fallback() {
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| Some(Ok(super::resident100_tests::capability(p))));
    assert!(hid.probe_resident_export_capability().unwrap());
    let ps = rec.requests();
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0][..4], [0xeb, 8, 0xa5, 2]);
}

#[test]
fn export_probe_correlated_legacy_rejection_requires_validated_v1() {
    for reject in [false, true] {
        for capacity in [None, Some(0), Some(3)] {
            let (hid, rec) = crate::hid::HidDevice::test_device();
            rec.respond_by(move |p| {
                let mut r = *p;
                if p[3] == 1 {
                    if let Some(capacity) = capacity {
                        r = super::resident100_tests::capability(p);
                        r[9] = capacity;
                    }
                }
                if reject && r[2] == 0xa5 { r[0] = 0xff; }
                Some(Ok(r))
            });
            assert!(!hid.probe_resident_export_capability().unwrap());
            let ps = rec.requests();
            assert_eq!(ps.len(), 2);
            assert_eq!(ps[0][..4], [0xeb, 8, 0xa5, 2]);
            assert_eq!(ps[1][..4], [0xeb, 8, 0xa5, 1]);
            assert_ne!(ps[0][4..8], ps[1][4..8]);
        }
    }
}

#[test]
fn export_probe_zero_wrong_nonce_padding_and_transport_remain_unknown() {
    for fallback_fault in [false, true] {
        for fault in 0..4 {
            let (hid, rec) = crate::hid::HidDevice::test_device();
            rec.respond_by(move |p| {
                if fallback_fault && p[3] == 2 { return Some(Ok(*p)); }
                // A healthy v1 must never legitimize an invalid schema2 reply.
                if !fallback_fault && p[3] == 1 {
                    return Some(Ok(super::resident100_tests::capability(p)));
                }
                let mut r = super::resident100_tests::capability(p);
                match fault {
                    0 => r = [0; 32],
                    1 => r[4] ^= 1,
                    2 => r[31] = 1,
                    3 => return Some(Err("modeled capability transport error".into())),
                    _ => unreachable!(),
                }
                Some(Ok(r))
            });
            assert!(hid.probe_resident_export_capability().is_err(), "fallback={fallback_fault} fault={fault}");
            assert!(rec.requests().iter().all(|p| p[0] == 0xeb));
        }
    }
}

#[test]
fn export_catalog_checked_cancels_before_and_after_enum_exchange() {
    for cancel_at in [1, 2, 201, 202, 400, 401] {
        let (hid, rec) = crate::hid::HidDevice::test_device();
        rec.respond_by(|p| {
            assert_eq!(p[..3], [0xef, 9, 3]);
            let mut r = *p;
            r[2] = 0x83;
            r[3] = 1;
            Some(Ok(r))
        });
        let mut checks = 0;
        let result = hid.read_resident_export_catalog_checked(&mut || {
            checks += 1;
            anyhow::ensure!(checks != cancel_at, "modeled cancellation");
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("modeled cancellation"));
        assert_eq!(checks, cancel_at);
        assert_eq!(rec.requests().len(), cancel_at / 2);
    }
}

#[test]
fn export_catalog_checked_cancels_before_and_after_read_exchange() {
    for cancel_at in [3, 4] {
        let (hid, rec) = crate::hid::HidDevice::test_device();
        rec.respond_by(|p| {
            let mut r = *p;
            r[2] |= 0x80;
            if p[2] == 3 { r[8..].fill(7); }
            else { r[10] = 0; }
            Some(Ok(r))
        });
        let mut checks = 0;
        let result = hid.read_resident_export_catalog_checked(&mut || {
            checks += 1;
            anyhow::ensure!(checks != cancel_at, "modeled READ cancellation");
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("modeled READ cancellation"));
        let ps = rec.requests();
        assert_eq!(ps.len(), cancel_at / 2);
        if cancel_at == 4 { assert_eq!(ps[1][..3], [0xef, 9, 6]); }
    }
}

fn inert_bound() -> QmkHidHostBridge {
    let mut bridge = QmkHidHostBridge::test_inert(
        crate::device::test_usb_device("export-capability", 1, 0x00b2),
        Default::default(), None, HostProtocol::Selected(false));
    bridge.bind_selected_application_layout_device("export-capability".into());
    bridge
}

#[test]
fn export_inventory_unknown_false_true_and_revoke_are_distinct() {
    let mut bridge = inert_bound();
    let settings = crate::application_layouts::DeviceApplicationLayouts::default();
    assert!(!bridge.resident_export_inventory_confirmed(&settings));
    {
        let mut s = bridge.application_layout_snapshot.lock().unwrap();
        s.resident_inventory_complete = true;
        assert_eq!(s.resident_export_capability, None);
    }
    assert!(!bridge.resident_export_inventory_confirmed(&settings));
    {
        let mut s = bridge.application_layout_snapshot.lock().unwrap();
        s.resident_inventory_complete = false;
        s.resident_export_capability = Some(false);
    }
    assert!(bridge.resident_export_inventory_confirmed(&settings));
    bridge.application_layout_snapshot.lock().unwrap().resident_export_capability = Some(true);
    assert!(!bridge.resident_export_inventory_confirmed(&settings));
    bridge.application_layout_snapshot.lock().unwrap().resident_inventory_complete = true;
    assert!(bridge.resident_export_inventory_confirmed(&settings));
    bridge.test_set_valid_device_inventory();
    assert!(!bridge.resident_export_inventory_confirmed(&settings), "missing physical payload must refuse");
    bridge.clear_application_layout_binding();
    assert_eq!(bridge.application_layout_snapshot.lock().unwrap().resident_export_capability, None);
    bridge.bind_selected_application_layout_device("replacement".into());
    assert!(!bridge.resident_export_inventory_confirmed(&settings));
    let mut s = bridge.application_layout_snapshot.lock().unwrap();
    s.resident_export_capability = Some(false);
    s.revoke();
    assert_eq!(s.resident_export_capability, None);
}

#[test]
fn bridge_real_query_records_confirmed_unsupported_without_inventory() {
    let (hid, rec) = crate::hid::HidDevice::test_device();
    rec.respond_by(|p| (p[0] == 0xeb).then_some(Ok(*p)));
    let target = crate::device::Device {
        path: format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")),
        ..crate::device::test_usb_device("export-capability-worker", 1, 0x00b2)
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(target, Default::default(),
        hid.shared_output(), HostProtocol::Selected(false), || None, move |t, s| {
            if let Some(g) = gate.take() { g.recv_timeout(Duration::from_secs(5)).unwrap(); }
            anyhow::ensure!(s.is_some(), "modeled shared owner only");
            open_host_data_hid(t, s)
        });
    bridge.bind_selected_application_layout_device("export-capability-worker".into());
    bridge.set_selected_application_layout_support(true);
    tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while bridge.application_layout_snapshot.lock().unwrap().resident_export_capability.is_none() {
        assert!(Instant::now() < deadline, "real worker must publish capability proof");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(bridge.resident_export_inventory_confirmed(&Default::default()));
    assert!(!bridge.application_layout_snapshot.lock().unwrap().resident_inventory_complete);
    bridge.stop();
    let queries: Vec<_> = rec.requests().into_iter().filter(|p| p[0] == 0xeb).collect();
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0][3], 2);
    assert_eq!(queries[1][3], 1);
    assert!(!rec.requests().iter().any(|p| matches!(p[0], 0xec..=0xef)));
}
