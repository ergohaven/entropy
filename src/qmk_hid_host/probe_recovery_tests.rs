use super::*;

// Real bridge acquisition/discovery/reopen; only OS HID replies and desktop
// providers are injected. No pre-set application-layout support or owner grant.
#[test]
fn cold_sm62_probe_timeout_recovers_without_clock_media_or_manual_reconnect() {
    for serial in ["left-mcu", "right-mcu"] {
        let mut target = crate::device::test_usb_device("3-3", 5, 0x00b2);
        target.serial_number = serial.into();
        target.path = format!("{}/src/main.rs", env!("CARGO_MANIFEST_DIR"));
        let (first, first_packets) = crate::hid::HidDevice::test_device();
        let retire_first = first_packets.clone();
        first_packets.respond_by(move |request| {
            if request[..2] == [0xfe, 0] {
                let mut reply=[0;32]; reply[..4].copy_from_slice(&6u32.to_le_bytes());
                reply[4..12].copy_from_slice(&0x37434f4c32364d53u64.to_le_bytes());
                Some(Ok(reply))
            } else if request[..2] == [0xe6,8] {
                retire_first.retire_transport();
                Some(Err("HID request timed out; proxy retired".into()))
            } else { None }
        });
        let (fresh, fresh_packets) = crate::hid::HidDevice::test_device();
        fresh_packets.respond_by(|request| {
            if request[..2] == [0xfe,0] {
                let mut reply=[0;32];reply[..4].copy_from_slice(&6u32.to_le_bytes());
                reply[4..12].copy_from_slice(&0x37434f4c32364d53u64.to_le_bytes());Some(Ok(reply))
            } else if request[..2] == [0xe6,8] {
                let mut reply=[0;32];reply[..3].copy_from_slice(&[0xe6,8,0x5a]);Some(Ok(reply))
            } else { None }
        });
        let opened=Arc::new(std::sync::atomic::AtomicUsize::new(0));let opens=opened.clone();
        let mut handles=std::collections::VecDeque::from([first,fresh]);
        let mut bridge=QmkHidHostBridge::start_with_sources(target,HostDataMode::default(),None,
            HostProtocol::Discover,||None,move |_,_| {
                opens.fetch_add(1,Ordering::SeqCst);
                handles.pop_front().map(HostDataHid::Dedicated).ok_or_else(||anyhow::anyhow!("no hardware fallback"))
            });
        let deadline=Instant::now()+Duration::from_secs(4);
        while !bridge.supports_application_layouts() && Instant::now()<deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let recovered=bridge.supports_application_layouts();
        let identity=bridge.discovered_application_layout_identity();
        bridge.stop();if let Some(worker)=bridge.thread.take(){worker.join().unwrap();}
        assert!(recovered,"{serial}: transient E6 failure permanently disabled autolayer; opens={}",opened.load(Ordering::SeqCst));
        assert_eq!(opened.load(Ordering::SeqCst),2,"must acquire a new owner, never retry a retired stream");
        assert!(identity.is_some());
        assert_eq!(first_packets.requests().iter().filter(|r|r[0]==0xe6).count(),1);
        assert_eq!(fresh_packets.requests().iter().filter(|r|r[0]==0xe6).count(),1);
        assert!(first_packets.requests().iter().all(|r|r[..2]==[0xfe,0]||r[..2]==[0xe6,8]));
    }
}
