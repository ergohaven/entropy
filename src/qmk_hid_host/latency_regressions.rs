use super::*;

fn inert_sm62() -> QmkHidHostBridge {
    let mut target = crate::device::test_usb_device("autolayer-wake-no-hardware", 1, 1);
    target.vendor_id = 0xe126;
    target.product_id = 0x00b2;
    let mut bridge = QmkHidHostBridge::test_inert(
        target,
        HostDataMode::default(),
        None,
        HostProtocol::Selected(true),
    );
    bridge.bind_selected_application_layout_device("latency-model-library".into());
    bridge
}
fn profile(revision: u32) -> crate::application_layouts::ApplicationLayoutSnapshot {
    let mut layout =
        crate::application_layouts::DeviceApplicationLayouts::default().layouts["default"].clone();
    layout.id="latency-application".into();
    layout.layers = vec![vec![0x22; 62]; 16];
    layout.revision = revision;
    crate::application_layouts::ApplicationLayoutSnapshot::from_layout(&layout)
}

#[test]
fn changed_profile_publication_wakes_idle_bridge_without_waiting_for_tick() {
    let bridge = inert_sm62();
    let generation = bridge.application_layout_publication_generation().unwrap();
    // Consume any unrelated handoff notification before testing publication.
    bridge.control.wake.wait(Duration::ZERO);
    let snapshot = profile(42);
    assert!(bridge.publish_application_layout_snapshot_for_generation(snapshot.clone(), generation));
    assert!(
        bridge.control.wake.wait(Duration::ZERO),
        "changed profile remained queued until the periodic bridge tick"
    );
    assert!(bridge.publish_application_layout_snapshot_for_generation(snapshot, generation));
    assert!(
        !bridge.control.wake.wait(Duration::ZERO),
        "unchanged UI repaint woke idle bridge repeatedly"
    );
    assert!(!bridge.publish_application_layout_snapshot_for_generation(
        profile(43),
        generation.wrapping_add(1)
    ));
    assert!(
        !bridge.control.wake.wait(Duration::ZERO),
        "stale publication woke/replaced current owner"
    );
}

#[test]
fn sm62_wire_packet_inventory_includes_all_encoder_stack_frames() {
    let snapshot = profile(42);
    let packets = snapshot.packets();
    let counts: std::collections::BTreeMap<u8, usize> =
        packets.iter().fold(Default::default(), |mut counts, p| {
            *counts.entry(p[0]).or_default() += 1;
            counts
        });
    println!(
        "actual complete SM62 packet inventory: total={} opcodes={counts:?}",
        packets.len()
    );
    assert_eq!(packets.len(), 210);
    assert_eq!(packets.first().unwrap()[0], 0xe1);
    assert_eq!(packets.last().unwrap()[0], 0xe3);
    assert!(packets.iter().all(|p| p[1] == 8));
    assert_eq!(
        packets
            .iter()
            .filter(|p| p[0] == crate::application_layouts::HID_APPLICATION_LAYOUT_ENCODER_STACK)
            .count(),
        64
    );
}

#[test]
fn changed_profile_preempts_media_title_at_a_complete_packet_boundary_and_title_retries() {
    assert_profile_preempts_media_title(false);
    assert_profile_preempts_media_title(true);
}

fn assert_profile_preempts_media_title(compact_supported: bool) {
    let mut target = inert_sm62().device.clone();
    target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
    let desktop = HostDataService {
        owner: Arc::new(DesktopOwner {
            shared: Arc::new(DesktopShared::default()),
        }),
    };
    desktop.owner.shared.state.lock().unwrap().snapshot = DesktopSnapshot {
        media: Some(MediaInfo {
            title: "a".repeat(255),
            ..Default::default()
        }),
        media_sampled_at: Some(Instant::now()),
        ..Default::default()
    };
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    recorder.respond_by(move |p| {
        if p[0] == 0xeb && !compact_supported {
            // Explicit old-firmware rejection, not an uncorrelated zero reply.
            return Some(Ok(*p));
        }
        if compact_supported && p[0] == 0xeb {
            if p[3] == 2 { return Some(Ok(*p)); } // v1-only peer: correlated schema2 rejection
            let mut reply = *p;
            reply[2] = 0x5a;
            reply[8] = 3;
            reply[10..12].copy_from_slice(&4560u16.to_le_bytes());
            return Some(Ok(reply));
        }
        if p[0] == 0xe6 {
            let mut r = [0; 32];
            r[..3].copy_from_slice(&[0xe6, 8, 0x5a]);
            Some(Ok(r))
        } else {
            None
        }
    });
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut held = false;
    recorder.on_output(move |p| {
        if !held && p[0] == 0xbb && p[1] == 0 {
            held = true;
            let _ = entered_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(5));
        }
    });
    let mut bridge = QmkHidHostBridge::start_with_desktop_service(
        target,
        HostDataMode {
            media: true,
            ..Default::default()
        },
        hid.shared_output(),
        HostProtocol::Selected(true),
        desktop,
        |target, shared| {
            anyhow::ensure!(shared.is_some(), "no real HID fallback");
            open_host_data_hid(target, shared)
        },
    );
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    bridge.bind_selected_application_layout_device("latency-model-library".into());
    bridge.set_selected_application_layout_support(true);
    bridge.set_application_layout_snapshot(profile(42));
    let before = recorder.requests().len();
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let first_next = loop {
        let requests = recorder.requests();
        if requests.len() >= before + 2 {
            break requests[before + 1][0];
        }
        assert!(
            Instant::now() < deadline,
            "bridge did not resume after complete title packet"
        );
        thread::sleep(Duration::from_millis(1));
    };
    // Drain the real bridge through profile commit and resumed final title chunk.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let requests = recorder.requests();
        if requests[before..]
            .iter()
            .any(|p| p[0] == if compact_supported { 0xee } else { 0xe3 })
            && requests[before..]
                .iter()
                .any(|p| p[0] == 0xbb && p[1] == 232)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deferred title was not retried after application publication"
        );
        thread::sleep(Duration::from_millis(1));
    }
    bridge.suppress_shutdown();
    let worker = bridge.thread.take().unwrap();
    bridge.stop();
    worker.join().unwrap();
    assert_eq!(
        first_next, 0xeb,
        "changed profile capability query waited behind the remaining media title"
    );
    let requests = recorder.requests();
    let start = before + requests[before..].iter().position(|p|matches!(p[0],0xe1|0xec)).unwrap();
    assert_eq!(requests[before..start].iter().filter(|p|p[0]==0xeb).count(),2,"v2 probe and validated v1 fallback each run once");
    assert!(requests[before+1..start].iter().all(|p|p[0]==0xeb),"media resumed between capability negotiation and complete current profile");
    assert_eq!(
        requests[start][0],
        if compact_supported { 0xec } else { 0xe1 },
        "capability must immediately select the complete current profile, not resume media"
    );
    let legacy = ApplicationLayoutTransmission::default().packets(&profile(42));
    let expected = if compact_supported {
        let transaction = u32::from_le_bytes(requests[start][4..8].try_into().unwrap());
        compact::encode(&legacy, transaction).unwrap()
    } else {
        legacy
    };
    assert_eq!(&requests[start..start + expected.len()], expected.as_slice(),
        "media interleaved with, truncated or changed the complete identity-bearing profile transaction");
    assert!(
        requests[before..]
            .iter()
            .filter(|p| p[0] == 0xbb && p[1] == 0)
            .count()
            >= 2,
        "interrupted title must restart from its first chunk"
    );
}
