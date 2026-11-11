use super::*;

#[test]
fn empty_host_library_reads_device_resident_catalog_without_mutating_it() {
    let raw=include_bytes!("resident_stack_max_full.bin").to_vec();
    let mut key=[0;24];key[..20].copy_from_slice(&raw[43..63]);key[20..].copy_from_slice(&compact::crc32(&raw).to_le_bytes());
    validate_recovered_full(&raw,&key).unwrap();
    let expected=raw.clone();let (hid,rec)=crate::hid::HidDevice::test_device();
    rec.respond_by(move |p| {
        if p[0]==0xeb {return Some(Ok(super::resident100_tests::capability(p)));}
        if p[..2]==[0xe6,8] {let mut r=[0;32];r[..3].copy_from_slice(&[0xe6,8,0x5a]);return Some(Ok(r));}
        if p[..2]==[0xef,9] {
            let mut r=*p;r[2]|=0x80;
            match p[2] {
                3=>{r[8..].fill(0);if p[3]==7 {r[3]=0;r[8..32].copy_from_slice(&key);}else{r[3]=1;}},
                6=>{let offset=u16::from_le_bytes([p[8],p[9]]) as usize;let n=raw.len().saturating_sub(offset).min(20);r[10]=n as u8;r[11]=0;r[12..].fill(0);r[12..12+n].copy_from_slice(&raw[offset..offset+n]);},
                _=>panic!("device readback must not STORE/SELECT/DELETE: {}",p[2]),
            }
            return Some(Ok(r));
        }
        None
    });
    let mut target=crate::device::test_usb_device("read-first",1,0x00b2);
    target.path=format!("{}/src/main.rs",env!("CARGO_MANIFEST_DIR"));
    let mut bridge=super::test_start_bridge(target,Default::default(),hid.shared_output(),None,HostProtocol::Selected(false),||None);
    bridge.bind_selected_application_layout_device("device-owned-catalog".into());
    bridge.set_selected_application_layout_support(true);
    assert!(bridge.resident_recoveries().is_empty());
    let deadline=Instant::now()+Duration::from_secs(12);let mut library_republished=false;
    while bridge.resident_recoveries().is_empty() && Instant::now()<deadline {
        if !library_republished && rec.requests().iter().any(|p|p[..3]==[0xef,9,6]) {
            bridge.retry_resident_sync();library_republished=true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(library_republished,"fixture never changed library generation during READ");
    let readback=bridge.resident_recoveries();bridge.stop();if let Some(worker)=bridge.thread.take(){worker.join().unwrap();}
    assert_eq!(readback.len(),1,"empty host library prevented read-only recovery of the device's own profile");
    assert_eq!(readback[0].slot,7);assert_eq!(readback[0].raw,expected);assert!(readback[0].validation.is_none());
    let layout=decode_device_profile(&readback[0]).unwrap();
    let mut settings=crate::application_layouts::DeviceApplicationLayouts::default();
    for layer in &mut settings.layouts.get_mut("default").unwrap().layers {layer.resize(62,0);}
    settings.resident_slots.insert(layout.id.clone(),7);
    settings.device_raw_profiles.insert(layout.id.clone(),expected.clone());settings.layouts.insert(layout.id.clone(),layout.clone());
    settings.normalize();let normalized=settings.layouts.get(&layout.id).unwrap();
    let snapshot=settings.snapshot_for_layout(normalized);
    assert_eq!(snapshot.device_raw.as_ref(),Some(&expected),"normalization lost the exact device record");
    let legacy=ApplicationLayoutTransmission::default().packets(&snapshot);
    let encoded=compact::encode(&legacy,91).unwrap();
    assert_eq!(&encoded[0][16..20],&compact::crc32(&expected).to_le_bytes(),"readback would upload a lossy editor reconstruction");
    assert_eq!(&legacy.last().unwrap()[11..31],&readback[0].identity_crc[..20]);
    let mut inactive=snapshot.clone();inactive.active=false;
    let stopped=ApplicationLayoutTransmission::default().packets(&inactive);
    assert_eq!(stopped[0][2],0,"device raw preservation reactivated a disabled profile");
    assert_eq!(&stopped.last().unwrap()[11..31],&readback[0].identity_crc[..20]);
    let persisted=serde_json::to_string(&settings).unwrap();
    let reloaded:crate::application_layouts::DeviceApplicationLayouts=serde_json::from_str(&persisted).unwrap();
    assert_eq!(reloaded.snapshot_for_layout(reloaded.layouts.get(&layout.id).unwrap()),snapshot);
    let requests=rec.requests();
    assert_eq!(requests.iter().filter(|p|p[..3]==[0xef,9,6]&&p[8..10]==[0,0]).count(),1,"adopting one profile restarted the in-flight READ");
    let first_read=requests.iter().position(|p|p[..3]==[0xef,9,6]).unwrap();
    let preceding_enum=requests[..first_read].iter().filter(|p|p[..3]==[0xef,9,3]).count();
    assert!(preceding_enum<100,"the first profile waited for the complete 100-slot catalog: {preceding_enum} ENUM requests");
    assert!(!requests.iter().any(|p|p[..2]==[0xef,9]&&matches!(p[2],1|2|4)),"readback mutated or selected device data");
    assert!(!requests.iter().any(|p|matches!(p[0],0xe1|0xe2|0xe3|0xec|0xed|0xee)),"readback uploaded a host profile");
}
