//! Real bridge/owner with a modeled OS HID reply boundary, not firmware interop.
use super::*;

fn exercise_unknown_recovery(fault: u8, automatic: bool) {
    let (hid, rec) = crate::hid::HidDevice::test_device();
    let healthy = Arc::new(AtomicBool::new(false));
    let h = healthy.clone();
    rec.respond_by(move |p| {
        if p[0] == 0xeb {
            if h.load(Ordering::Acquire) {
                return Some(Ok(super::resident100_tests::capability(p)));
            }
            let mut r = super::resident100_tests::capability(p);
            match fault {
                0 => r = [0; 32],
                1 => r[4] ^= 1,
                2 => r[31] = 1,
                3 => return Some(Err("modeled transient capability read failure".into())),
                _ => unreachable!(),
            }
            return Some(Ok(r));
        }
        if p[..2] == [0xef, 9] {
            let mut r = *p;
            r[2] |= 128;
            r[3] = if p[2] == 3 { 1 } else { 0 };
            return Some(Ok(r));
        }
        None
    });
    let target = crate::device::Device {
        path: format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR")),
        ..crate::device::test_usb_device("capability-review", 1, 0x00b2)
    };
    let opens = Arc::new(AtomicU64::new(0));
    let o = opens.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut gate = Some(rx);
    let mut bridge = QmkHidHostBridge::start_with_sources(
        target,
        Default::default(),
        hid.shared_output(),
        HostProtocol::Selected(false),
        || None,
        move |t, s| {
            o.fetch_add(1, Ordering::SeqCst);
            if let Some(g) = gate.take() {
                g.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            anyhow::ensure!(s.is_some(), "synthetic shared owner only");
            open_host_data_hid(t, s)
        },
    );
    bridge.bind_selected_application_layout_device("capability-review".into());
    bridge.set_selected_application_layout_support(true);
    let generation = bridge.application_layout_publication_generation().unwrap();
    let mut layout =
        crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
    layout.id = "capability-profile".into();
    layout.layers = vec![vec![0x22; 62]; 16];
    let mut snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&layout);
    snapshot.resident_slot = Some(0);
    // Keep foreground native: recovery must not depend on changing its snapshot.
    assert!(bridge.publish_resident_library_for_generation(vec![snapshot], generation));
    tx.send(()).unwrap();
    super::resident_regressions::wait_for(&rec, |ps| {
        ps.iter().filter(|p| p[0] == 0xeb).count() >= if automatic { 2 } else { 1 }
    });
    let before;
    if automatic {
        before = rec.requests();
        healthy.store(true, Ordering::Release);
    } else {
        thread::sleep(Duration::from_millis(2600));
        before = rec.requests();
        let probes = before.iter().filter(|p| p[0] == 0xeb).count();
        let status = bridge.resident_sync_status("capability-profile");
        assert!(
            matches!(&status, Some(resident100::SyncStatus::Error(s)) if s.contains("unknown")),
            "uncorrelated capability must remain unknown, got {status:?}"
        );
        assert!(
            (1..=6).contains(&probes),
            "negotiation must be bounded: {probes}"
        );
        assert!(
            !before.iter().any(|p| matches!(p[0], 0xec..=0xef)),
            "unknown cannot authorize resident/compact traffic"
        );
        thread::sleep(Duration::from_millis(350));
        assert_eq!(
            rec.requests().iter().filter(|p| p[0] == 0xeb).count(),
            probes,
            "exhausted automatic retries must settle"
        );
        healthy.store(true, Ordering::Release);
        bridge.retry_resident_sync();
    }
    super::resident_regressions::wait_for(&rec, |ps| ps.iter().any(|p| p[..3] == [0xef, 9, 3]));
    assert_eq!(
        bridge.application_layout_publication_generation(),
        Some(generation)
    );
    assert_eq!(
        opens.load(Ordering::SeqCst),
        1,
        "recovery must retain connection"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while bridge.resident_sync_status("capability-profile") != Some(resident100::SyncStatus::Synced)
    {
        assert!(
            Instant::now() < deadline,
            "recovered negotiation must complete synchronization"
        );
        thread::sleep(Duration::from_millis(5));
    }
    bridge.stop();
    let after = rec.requests();
    assert!(
        after[before.len()..]
            .iter()
            .any(|p| p[..4] == [0xeb, 8, 0xa5, 2])
    );
}

#[test]
fn only_correlated_v1_support_or_legacy_rejection_is_a_capability_decision() {
    let q = compact::query(0x12345678);
    let supported = super::resident100_tests::capability(&q);
    assert_eq!(compact::capability(&q, &supported), Some(true));
    let mut rejected = q;
    rejected[0] = 0xff;
    assert_eq!(compact::capability(&q, &q), Some(false));
    assert_eq!(compact::capability(&q, &rejected), Some(false));
    for original in [supported, q, rejected] {
        for index in 0..32 {
            let mut malformed = original;
            malformed[index] ^= 0x40;
            assert_eq!(compact::capability(&q, &malformed), None, "index {index}");
        }
    }
    assert_eq!(compact::capability(&q, &[0; 32]), None);
}

#[test]
fn validators_require_exact_query_shape_before_accepting_replies() {
    for v2 in [false, true] {
        let query = if v2 {
            resident100::capability_query(71)
        } else {
            compact::query(71)
        };
        for index in [0, 1, 2, 3, 8, 31] {
            let mut malformed = query;
            malformed[index] ^= 0x40;
            // A reply copied from the malformed request must not legitimize it.
            let reply = super::resident100_tests::capability(&malformed);
            let accepted = if v2 {
                resident100::supported(&malformed, &reply)
            } else {
                compact::supported(&malformed, &reply)
            };
            assert!(!accepted, "malformed query index {index}, v2={v2}");
        }
    }
}

#[test]
fn transient_fault_recovers_automatically_on_same_owner() {
    exercise_unknown_recovery(1, true);
}

#[test]
fn zero_frames_remain_unknown_and_retry_same_owner() {
    exercise_unknown_recovery(0, false);
}
#[test]
fn wrong_nonce_remains_unknown_and_retry_same_owner() {
    exercise_unknown_recovery(1, false);
}
#[test]
fn malformed_padding_remains_unknown_and_retry_same_owner() {
    exercise_unknown_recovery(2, false);
}
#[test]
fn read_failure_remains_unknown_and_retry_same_owner() {
    exercise_unknown_recovery(3, false);
}
