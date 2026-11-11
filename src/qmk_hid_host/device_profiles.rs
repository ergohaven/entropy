//! Decode validated device-owned FULL records without guessing a host profile ID.
use super::*;

/// Classify schema2 with the production validators. Adapt only the schema byte
/// for the shared exact legacy-rejection validator; malformed bytes stay unknown.
pub(super) fn resident_capability_reply(request: &[u8; 32], reply: &[u8; 32]) -> Option<bool> {
    if resident100::supported(request, reply) { return Some(true); }
    if request[3] != 2 || reply[3] != 2 { return None; }
    let mut q = *request;
    let mut r = *reply;
    q[3] = 1;
    r[3] = 1;
    compact::capability(&q, &r).filter(|supported| !supported)
}

impl crate::hid::HidDevice {
    /// Read-only capability proof, independent of the application protocol version.
    /// Unknown transport/correlation/format is an error, never an empty catalog.
    pub(crate) fn probe_resident_export_capability(&self) -> anyhow::Result<bool> {
        let q = resident100::capability_query(resident100::next_transaction());
        let r = self.usb_send(&q)?;
        match resident_capability_reply(&q, &r) {
            Some(true) => Ok(true),
            Some(false) => {
                let q = compact::query(resident100::next_transaction());
                let r = self.usb_send(&q)?;
                anyhow::ensure!(compact::capability(&q, &r).is_some(), "Resident capability unknown: invalid v1 fallback");
                Ok(false)
            }
            None => anyhow::bail!("Resident capability unknown: invalid schema2 response"),
        }
    }

    /// A bounded, read-only inventory for complete-device export. No bridge or
    /// runtime publisher is started, and no host cache can satisfy a READ.
    pub(crate) fn read_resident_export_catalog(&self) -> anyhow::Result<Vec<resident100::RecoveredRecord>> {
        self.read_resident_export_catalog_checked(&mut || Ok(()))
    }

    pub(crate) fn read_resident_export_catalog_checked(&self, check: &mut dyn FnMut() -> anyhow::Result<()>) -> anyhow::Result<Vec<resident100::RecoveredRecord>> {
        fn header(hid: &crate::hid::HidDevice, slot: u8, check: &mut dyn FnMut() -> anyhow::Result<()>) -> anyhow::Result<Option<[u8; 24]>> {
            let p = resident100::request(3, slot, None, &[0; 20], &[0; 4]);
            check()?;
            let response = hid.usb_send(&p);
            check()?;
            let r = response?;
            anyhow::ensure!(r[..3] == [0xef, 9, 0x83] && r[3] <= 1 && r[4..8] == p[4..8], "Resident ENUM response");
            if r[3] == 1 {
                anyhow::ensure!(r[8..].iter().all(|b| *b == 0), "Resident empty ENUM padding");
                Ok(None)
            } else { Ok(Some(r[8..32].try_into()?)) }
        }
        let mut inventory = Vec::with_capacity(100);
        let mut records = Vec::new();
        for slot in 0..100 {
            let key = header(self, slot, check)?;
            inventory.push(key);
            let Some(key) = key else { continue; };
            let mut raw = Vec::new();
            loop {
                let mut p = resident100::request(6, slot, None, &[0; 20], &[0; 4]);
                p[8..10].copy_from_slice(&(raw.len() as u16).to_le_bytes()); p[10] = 20;
                check()?;
                let response = self.usb_send(&p);
                check()?;
                let r = response?;
                let mut expected = p; expected[2] = 0x86;
                let n = usize::from(r[10]);
                anyhow::ensure!(r[..10] == expected[..10] && n <= 20 && r[11] == 0
                    && r[12+n..].iter().all(|b| *b == 0) && raw.len()+n <= compact::LIMIT, "Resident READ response");
                raw.extend_from_slice(&r[12..12+n]);
                if n < 20 || raw.len() == compact::LIMIT { break; }
            }
            anyhow::ensure!(header(self, slot, check)? == Some(key), "Resident record changed during READ");
            validate_recovered_full(&raw, &key)?;
            records.push(resident100::RecoveredRecord { slot, identity_crc: key, raw, validation: None });
        }
        // Also detect empty->occupied, deletion and replacement after an earlier
        // slot was read. Never certify a partial or mixed inventory.
        for slot in 0..100 {
            anyhow::ensure!(header(self, slot, check)? == inventory[usize::from(slot)], "Resident inventory changed during export");
        }
        check()?;
        Ok(records)
    }
}

impl QmkHidHostBridge {
    /// Unattended snapshots may use only the exact current, completed readback.
    pub(crate) fn resident_export_inventory_confirmed(&self, settings: &crate::application_layouts::DeviceApplicationLayouts) -> bool {
        let state = self.application_layout_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        if self.application_layout_binding_generation != Some(state.generation)
            || state.verified_key.is_none()
            || state.verified_key.as_ref() != self.application_layout_device_key.as_ref() { return false; }
        match state.resident_export_capability {
            Some(false) => return true, // Confirmed legacy: no schema2 inventory exists.
            Some(true) => {},
            None => return false,
        }
        state.resident_inventory_complete && state.resident_recoveries.values().all(|r|
            r.validation.is_none() && settings.device_raw_profiles.iter().any(|(id, raw)|
                raw == &r.raw && settings.resident_slots.get(id) == Some(&r.slot)
                    && settings.layouts.get(id).is_some_and(|profile|
                        validate_portable_resident_profile(r, profile).is_ok())))
            && settings.layouts.keys().filter(|id| id.as_str() != "default").all(|id|
                state.resident_recoveries.values().any(|r| settings.device_raw_profiles.get(id) == Some(&r.raw)
                    && settings.resident_slots.get(id) == Some(&r.slot)))
    }
}

/// Prove that the portable profile can reconstruct every physical FULL frame.
/// Raw records are not serialized by the current envelope, so a lossy decoder
/// must not turn an intact device record into a falsely complete backup.
pub(crate) fn validate_portable_resident_profile(
    record: &resident100::RecoveredRecord,
    profile: &crate::application_layouts::ApplicationLayout,
) -> anyhow::Result<()> {
    validate_recovered_full(&record.raw, &record.identity_crc)?;
    let original = legacy_packets(&record.raw)?;
    let mut snapshot = crate::application_layouts::ApplicationLayoutSnapshot::from_layout(profile);
    snapshot.device_identity = Some(record.identity_crc[..20].try_into()?);
    let reconstructed = ApplicationLayoutTransmission.packets(&snapshot);
    anyhow::ensure!(original == reconstructed,
        "Resident FULL contains data not representable in the portable profile; complete backup refused");
    Ok(())
}

pub(crate) fn decoded_layout(record: &resident100::RecoveredRecord) -> anyhow::Result<crate::application_layouts::ApplicationLayout> {
    anyhow::ensure!(record.validation.is_none(), "device record was not validated");
    validate_recovered_full(&record.raw, &record.identity_crc)?;
    let raw=&record.raw;
    let id=format!("device-resident-{}-{}",record.slot,record.identity_crc[..20].iter().map(|b|format!("{b:02x}")).collect::<String>());
    let mut pos=64;
    let mut layers=vec![vec![0u16;62];16];
    for layer in &mut layers {for code in layer { *code=u16::from_le_bytes(raw[pos..pos+2].try_into()?);pos+=2; }}
    pos+=16*62; // visual IDs remain preserved in the original FULL.
    let mut stacks=Vec::new();
    for _ in 0..16 {
        let mut actions=Vec::new();
        for slot in 0..4 {
            let count=usize::from(raw[pos]);let len=usize::from(raw[pos+1]);
            let ccw=u16::from_le_bytes(raw[pos+2..pos+4].try_into()?);
            let cw=u16::from_le_bytes(raw[pos+4..pos+6].try_into()?);pos+=6;
            let name=String::from_utf8_lossy(&raw[pos..pos+len]).into_owned();pos+=len;
            if slot<count { actions.push(serde_json::json!({"name":name,"counter_clockwise":ccw,"clockwise":cw})); }
        }
        stacks.push(actions);
    }
    let mut names=Vec::new();
    for _ in 0..16 {let n=usize::from(raw[pos]);pos+=1;names.push(String::from_utf8_lossy(&raw[pos..pos+n]).into_owned());pos+=n;}
    anyhow::ensure!(pos==raw.len(),"trailing FULL bytes");
    Ok(serde_json::from_value(serde_json::json!({
        "id":id,"name":String::from_utf8_lossy(&raw[10..10+usize::from(raw[9])]),
        "automatic_switching":false,"layers":layers,"layer_names":names,
        "encoder_stacks":stacks,"revision":u32::from_le_bytes(raw[3..7].try_into()?)
    }))?)
}

/// Reconstruct the original protocol frames, including inactive stack metadata.
/// This allows a recovered profile to SELECT its exact existing durable record.
pub(super) fn legacy_packets(raw: &[u8]) -> anyhow::Result<Vec<[u8;32]>> {
    let mut key=[0;24];anyhow::ensure!(raw.len()>=64,"short FULL");
    key[..20].copy_from_slice(&raw[43..63]);key[20..].copy_from_slice(&compact::crc32(raw).to_le_bytes());
    validate_recovered_full(raw,&key)?;
    let mut out=vec![raw[..32].try_into()?];let mut pos=64;
    for layer in 0..16 {
        for start in (0..62).step_by(13) {
            let n=(62-start).min(13);let mut p=[0;32];p[..5].copy_from_slice(&[0xe2,8,layer,start as u8,n as u8]);
            p[5..5+n*2].copy_from_slice(&raw[pos..pos+n*2]);pos+=n*2;out.push(p);
        }
    }
    for layer in 0..16 {
        for start in (0..62).step_by(27) {
            let n=(62-start).min(27);let mut p=[0;32];p[..5].copy_from_slice(&[0xe7,8,layer,start as u8,n as u8]);
            p[5..5+n].copy_from_slice(&raw[pos..pos+n]);pos+=n;out.push(p);
        }
    }
    for layer in 0..16 { for slot in 0..4 {
        let n=usize::from(raw[pos+1]);let mut p=[0;32];p[..4].copy_from_slice(&[0xe9,8,layer,slot]);
        p[4..10+n].copy_from_slice(&raw[pos..pos+6+n]);pos+=6+n;out.push(p);
    }}
    for layer in 0..16 {let n=usize::from(raw[pos]);let mut p=[0;32];p[..3].copy_from_slice(&[0xe4,8,layer]);p[3..4+n].copy_from_slice(&raw[pos..pos+1+n]);pos+=1+n;out.push(p);}
    out.push(raw[32..64].try_into()?);anyhow::ensure!(pos==raw.len()&&out.len()==210,"FULL framing");Ok(out)
}
