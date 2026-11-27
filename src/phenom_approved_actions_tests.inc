#[test]
fn phenom_timeout20_portable_all_contracts_roundtrip_and_rejection() {
    let values = [50,100,150,200,250,300,350,400,450,500,750,1000,1250,1500,1750,2000,2250,2500,2750,3000];
    let make = |count: usize, value| {
        let mut app = phenom_settings_app(true);
        let variants: Vec<String> = if count == 6 { vec![250,500,750,1000,1250,1500] } else if count == 12 { vec![250,500,750,1000,1250,1500,1750,2000,2250,2500,2750,3000] } else { values[..count].to_vec() }.into_iter().map(|v| format!("{v} ms")).collect();
        for f in &mut app.module_settings.fields { if f.qsid == 324 { f.variants = variants.clone(); f.max = count as u16 - 1; } }
        for g in &mut app.module_settings.groups { for f in &mut g.fields { if f.qsid == 324 { f.variants = variants.clone(); f.max = count as u16 - 1; } } }
        app.module_settings.values.insert(324, value);
        app
    };
    for before in [6,12,14,20] { for after in [6,12,14,20] { for index in 0..before {
        let source = make(before, index as u16);
        let mut bundle = roundtrip(&source);
        bundle.data.portable_settings.retain(|s| s.qsid == 324);
        let label = serde_json::to_value(&bundle).unwrap()["data"]["portable_settings"][0]["module"][0]["variants"][index].as_str().unwrap().to_owned();
        let mut target = make(after, 255);
        // Target must have a valid loaded snapshot to import.
        target.module_settings.values.insert(324, (after - 1) as u16);
        let target_values = serde_json::to_value(target.entlayout_portable_settings_snapshot().into_iter().find(|s| s.qsid == 324).unwrap()).unwrap();
        let expected = target_values["module"][0]["variants"].as_array().unwrap().iter().position(|v| v.as_str() == Some(label.as_str()));
        let (hid, recorder) = crate::hid::HidDevice::test_device(); target.hid_device = Some(hid.into());
        let (_,_,report) = target.apply_entlayout(&bundle).unwrap();
        if let Some(wire) = expected {
            assert_eq!(report.imported,1,"{before}->{after} index {index}");
            assert_eq!(target.module_settings.values[&324],wire as u16);
            let exported = roundtrip(&target); let field = exported.data.portable_settings.iter().find(|s| s.qsid == 324).unwrap();
            assert_eq!(serde_json::to_value(field).unwrap()["module"][0]["variants"][usize::from(field.value)],label);
            let sets: Vec<_> = recorder.requests().into_iter().filter(|r| r[..2] == [0xfe,0x0b]).collect();
            assert_eq!(sets.len(),usize::from(wire != after-1));
            if let Some(r) = sets.first() { assert_eq!(r[2..6],[68,1,wire as u8,0]); }
        } else { assert_eq!(report.incompatible,1); assert!(recorder.requests().is_empty()); }
    } } }
    let source = make(20,19); let mut bundle = roundtrip(&source); bundle.data.portable_settings.retain(|s| s.qsid == 324);
    let mut json = serde_json::to_value(&bundle).unwrap(); json["data"]["portable_settings"][0]["module"][0]["variants"].as_array_mut().unwrap().swap(0,1); bundle = serde_json::from_value(json).unwrap();
    let mut target = make(14,0); let (hid,recorder)=crate::hid::HidDevice::test_device(); target.hid_device=Some(hid.into());
    assert_eq!(target.apply_entlayout(&bundle).unwrap().2.incompatible,1); assert!(recorder.requests().is_empty());
}
