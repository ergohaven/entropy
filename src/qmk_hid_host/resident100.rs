//! Resident schema2 wire validation. Never infer success from a request echo.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    Pending,
    Syncing,
    Synced,
    Error(String),
    Overflow,
}
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct RecoveredRecord {
    pub(crate) slot: u8,
    pub(crate) identity_crc: [u8; 24],
    pub(crate) raw: Vec<u8>,
    pub(crate) validation: Option<String>,
}
struct Recovery {
    slot: u8,
    key: [u8; 24],
    raw: Vec<u8>,
    confirm: bool,
    errors: u8,
}

#[derive(Default)]
enum CatalogScan {
    #[default]
    Probe,
    Legacy,
    Summary(resident_catalog::Catalog),
    Confirmed,
}

#[derive(Default)]
pub(super) struct Background {
    epoch: Option<(u64, u64)>,
    library: Arc<Vec<crate::application_layouts::ApplicationLayoutSnapshot>>,
    enumerated: usize,
    enum_errors: u8,
    blocked: bool,
    inventory: Vec<Option<[u8; 24]>>,
    catalog: CatalogScan,
    #[cfg(test)]
    legacy_fixture: bool,
    confirm_next: usize,
    foreground: Option<crate::application_layouts::ApplicationLayoutSnapshot>,
    recover_next: usize,
    recovery: Option<Recovery>,
}
impl Background {
    pub(super) fn pending(&self) -> bool {
        self.epoch.is_some()
            && !self.blocked
            && (matches!(self.catalog, CatalogScan::Probe | CatalogScan::Summary(_))
                || self.enumerated < self.inventory.len()
                || self.recover_next < self.inventory.len()
                || self.recovery.is_some()
                || matches!(self.catalog, CatalogScan::Legacy))
    }
    #[cfg(test)]
    pub(super) fn tick(
        &mut self,
        dev: &HostDataHid,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        stop: &AtomicBool,
        transaction: &mut u32,
    ) {
        // These historical peers model schema2 before compact inventory existed.
        self.legacy_fixture = true;
        if matches!(self.catalog, CatalogScan::Probe) { self.catalog = CatalogScan::Legacy; }
        self.tick_with_capabilities(resident_catalog::fixture_capabilities(), dev, owner, foreground, generation, stop, transaction);
    }

    pub(super) fn tick_with_capabilities(
        &mut self,
        capabilities: resident_catalog::Capabilities,
        dev: &HostDataHid,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        stop: &AtomicBool,
        _transaction: &mut u32,
    ) {
        let (epoch, library) = {
            let s = owner.lock().unwrap_or_else(|e| e.into_inner());
            ((generation, s.library_generation), s.library.clone())
        };
        // The device is the source of truth on first connect. An empty host
        // library must still enumerate/read existing device records; with no
        // host jobs the scan ends without any STORE/SELECT/DELETE operations.
        if self.epoch != Some(epoch) || self.inventory.len() != usize::from(capabilities.slots) {
            let owner_changed=self.epoch.is_none_or(|previous|previous.0!=epoch.0)
                || self.inventory.len() != usize::from(capabilities.slots);
            self.epoch = Some(epoch);
            self.library = library;
            self.blocked = false;
            self.enum_errors = 0;
            // Adopting each completed profile changes the host library, not the
            // physical read session. Keep its cursor and in-progress READ.
            if owner_changed {
                self.enumerated = 0;
                self.inventory = vec![None; usize::from(capabilities.slots)];
                self.catalog = CatalogScan::Probe;
                #[cfg(test)]
                if self.legacy_fixture { self.catalog = CatalogScan::Legacy; }
                self.confirm_next = 0;
                self.recover_next = 0;
                self.recovery = None;
            }
        }
        if self.blocked {
            return;
        }
        if self.foreground.as_ref() != Some(foreground) {
            self.foreground = Some(foreground.clone());
        }
        if stop.load(Ordering::Acquire) {
            return;
        }
        if matches!(self.catalog, CatalogScan::Probe) {
            let p = resident_catalog::query();
            let mut answer = None;
            let sent = self.send(dev, owner, foreground, generation, stop, &p, |r| { answer = r; Ok(()) });
            match (sent, answer) {
                (Ok(true), Some(r)) => match resident_catalog::decode(&p, &r, capabilities) {
                    Ok(Some(catalog)) => { self.catalog = CatalogScan::Summary(catalog); self.enum_errors = 0; },
                    Ok(None) => { self.catalog = CatalogScan::Legacy; self.enum_errors = 0; },
                    Err(_) => self.enum_failure(owner),
                },
                (Ok(false), _) => {},
                _ => self.enum_failure(owner),
            }
            return;
        }
        if self.recovery.is_none() {
            if let CatalogScan::Summary(catalog) = &self.catalog {
                while self.enumerated < self.inventory.len() && !catalog.occupied(self.enumerated) {
                    self.enumerated += 1;
                    self.recover_next = self.enumerated;
                }
            }
        }
        if self.enumerated < self.inventory.len() && self.recovery.is_none() {
            let p = request(3, self.enumerated as u8, None, &[0; 20], &[0; 4]);
            let mut answer = None;
            let sent = self.send(dev, owner, foreground, generation, stop, &p, |r| {
                answer = r;
                Ok(())
            });
            if sent.is_err() {
                self.enum_failure(owner);
                return;
            }
            if !matches!(sent, Ok(true)) {
                return;
            }
            let Some(r) = answer else {
                return;
            };
            if r[..3] != [0xef, 9, 0x83]
                || r[3] > 1
                || (r[3] == 1 && matches!(self.catalog, CatalogScan::Summary(_)))
                || r[4..8] != p[4..8]
                || (r[3] == 1 && r[8..].iter().any(|b| *b != 0))
            {
                self.enum_failure(owner);
                return;
            }
            if r[3] == 0 {
                let slot=self.enumerated as u8;
                let key:[u8;24]=r[8..32].try_into().unwrap();
                self.inventory[self.enumerated] = Some(key);
                // The occupied device slot can reject the stale host mapping
                // before any bulk transfer; keep the existing error channel.
                for profile in std::iter::once(foreground).chain(self.library.iter()) {
                    if profile.resident_slot==Some(slot) {
                        if let Some(id)=profile.profile_id.as_ref() {
                            let identity=profile.device_identity.unwrap_or_else(||wake_identity::digest(id));
                            if identity!=key[..20] {self.status(owner,id,SyncStatus::Error("Identity conflict: existing device record preserved".into()));}
                        }
                    }
                }
                let already_read=owner.lock().unwrap_or_else(|e|e.into_inner()).resident_recoveries.get(&slot)
                    .is_some_and(|record|record.identity_crc==key && record.validation.is_none());
                if !already_read {
                    self.recovery=Some(Recovery {slot,key,raw:Vec::new(),confirm:false,errors:0});
                }
            }
            self.enum_errors = 0;
            self.enumerated += 1;
            // Every occupied slot is read immediately, before the next ENUM.
            self.recover_next=self.enumerated;
            return;
        }
        if self.recover_next < self.inventory.len() || self.recovery.is_some() {
            if self.recovery.is_none() {
                while self.recover_next < self.inventory.len() {
                    let slot = self.recover_next;
                    self.recover_next += 1;
                    let Some(key) = self.inventory[slot] else {
                        continue;
                    };
                    let known = self.library.iter().any(|p| {
                        p.resident_slot == Some(slot as u8)
                            && p.profile_id
                                .as_ref()
                                .is_some_and(|id| wake_identity::digest(id) == key[..20])
                    });
                    if !known {
                        self.recovery = Some(Recovery {
                            slot: slot as u8,
                            key,
                            raw: Vec::new(),
                            confirm: false,
                            errors: 0,
                        });
                        break;
                    }
                }
            }
            if let Some(mut recovery) = self.recovery.take() {
                let mut p = request(
                    if recovery.confirm { 3 } else { 6 },
                    recovery.slot,
                    None,
                    &[0; 20],
                    &[0; 4],
                );
                if !recovery.confirm {
                    p[8..10].copy_from_slice(&(recovery.raw.len() as u16).to_le_bytes());
                    p[10] = 20;
                }
                let mut answer = None;
                let result = self.send(dev, owner, foreground, generation, stop, &p, |r| {
                    answer = r;
                    Ok(())
                });
                let mut finished = false;
                if result.is_err() {
                    recovery.errors += 1;
                } else if let Some(r) = answer {
                    if recovery.confirm {
                        if r[..4] == [0xef, 9, 0x83, 0]
                            && r[4..8] == p[4..8]
                            && r[8..32] == recovery.key
                        {
                            let validation = validate_recovered_full(&recovery.raw, &recovery.key)
                                .err()
                                .map(|e| format!("{e:#}"));
                            self.save_recovery(owner, &recovery, validation);
                            finished = true;
                        } else {
                            recovery.errors += 1;
                        }
                    } else if r[..10] == {
                        let mut h = p;
                        h[2] = 0x86;
                        h[..10].to_vec()
                    } && r[10] <= 20
                        && r[11] == 0
                        && r[12 + usize::from(r[10])..].iter().all(|b| *b == 0)
                        && recovery.raw.len() + usize::from(r[10]) <= compact::LIMIT
                    {
                        recovery
                            .raw
                            .extend_from_slice(&r[12..12 + usize::from(r[10])]);
                        if r[10] < 20 || recovery.raw.len() == compact::LIMIT {
                            recovery.confirm = true;
                        }
                    } else {
                        recovery.errors += 1;
                    }
                }
                if recovery.errors >= 3 {
                    self.save_recovery(
                        owner,
                        &recovery,
                        Some(
                            "READ/ENUM recovery unconfirmed after3 attempts; raw partial preserved"
                                .into(),
                        ),
                    );
                    finished = true;
                }
                if !finished {
                    self.recovery = Some(recovery);
                }
                return;
            }
        }
        // Pre-INVENTORY firmware still requires a second complete ENUM fence:
        // no partial/changed archive may be certified as migrated.
        if matches!(self.catalog, CatalogScan::Legacy) {
            if self.confirm_next < self.inventory.len() {
                let slot = self.confirm_next;
                let p = request(3, slot as u8, None, &[0; 20], &[0; 4]);
                let mut reply = None;
                let sent = self.send(dev, owner, foreground, generation, stop, &p, |r| { reply = r; Ok(()) });
                let valid = reply.is_some_and(|r| r[..3] == [0xef, 9, 0x83]
                    && r[4..8] == p[4..8]
                    && match self.inventory[slot] { Some(key) => r[3] == 0 && r[8..32] == key,
                        None => r[3] == 1 && r[8..].iter().all(|b| *b == 0) });
                if matches!(sent, Ok(true)) && valid { self.confirm_next += 1; self.enum_errors = 0; }
                else if !matches!(sent, Ok(false)) { self.enum_failure(owner); }
                return;
            }
            self.catalog = CatalogScan::Confirmed;
        }
        if let CatalogScan::Summary(expected) = &self.catalog {
            let expected = expected.clone();
            let p = resident_catalog::query();
            let mut answer = None;
            let sent = self.send(dev, owner, foreground, generation, stop, &p, |r| { answer = r; Ok(()) });
            match (sent, answer) {
                (Ok(true), Some(r)) if resident_catalog::decode(&p, &r, capabilities).is_ok_and(|c| c.as_ref() == Some(&expected)) => {
                    self.catalog = CatalogScan::Confirmed;
                    self.enum_errors = 0;
                },
                (Ok(false), _) => {},
                _ => self.enum_failure(owner),
            }
            return;
        }
        {
            let mut state=owner.lock().unwrap_or_else(|e|e.into_inner());
            if self.epoch == Some((state.generation, state.library_generation)) && state.generation==generation {
                state.resident_inventory_complete=self.enumerated==self.inventory.len() && self.recovery.is_none()
                    && self.inventory.iter().enumerate().all(|(slot, key)| key.is_none_or(|key|
                        state.resident_recoveries.get(&(slot as u8)).is_some_and(|r|
                            r.identity_crc == key && r.validation.is_none())))
                    && state.resident_recoveries.values().all(|r|r.validation.is_none());
            }
        }
    }
    fn enum_failure(&mut self, owner: &Mutex<ApplicationLayoutOwnerState>) {
        self.enum_errors += 1;
        if self.enum_errors >= 3 {
            self.blocked = true;
            for snap in self.library.iter() {
                if let Some(id) = snap.profile_id.as_deref() {
                    self.status(
                        owner,
                        id,
                        SyncStatus::Error("ENUM unconfirmed after 3 attempts; Retry sync".into()),
                    );
                }
            }
        }
    }
    fn save_recovery(
        &self,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        r: &Recovery,
        validation: Option<String>,
    ) {
        let mut s = owner.lock().unwrap_or_else(|e| e.into_inner());
        if self.epoch == Some((s.generation, s.library_generation)) {
            s.resident_recoveries.insert(
                r.slot,
                RecoveredRecord {
                    slot: r.slot,
                    identity_crc: r.key,
                    raw: r.raw.clone(),
                    validation,
                },
            );
        }
    }
    fn status(&self, owner: &Mutex<ApplicationLayoutOwnerState>, id: &str, status: SyncStatus) {
        let mut s = owner.lock().unwrap_or_else(|e| e.into_inner());
        if self.epoch == Some((s.generation, s.library_generation)) {
            s.resident_status.insert(id.into(), status);
        }
    }
    fn send(
        &self,
        dev: &HostDataHid,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        stop: &AtomicBool,
        p: &[u8; 32],
        consume: impl FnOnce(Option<[u8; 32]>) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        let mut r = None;
        if !self.admit(owner, foreground, generation, p, |p| {
            r = match dev {
                HostDataHid::Shared(o) => o.poll_application_layout_event(p)?,
                HostDataHid::Dedicated(h) => Some(h.usb_send(p)?),
            };
            Ok(())
        })? {
            return Ok(false);
        }
        if !self.admit(owner, foreground, generation, &[], |_| Ok(()))?
            || matches!(dev,HostDataHid::Shared(o) if !o.host_session_is_current())
        {
            return Ok(false);
        }
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        consume(r)?;
        Ok(true)
    }
    fn admit(
        &self,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        p: &[u8],
        send: impl FnOnce(&[u8]) -> anyhow::Result<()>,
    ) -> anyhow::Result<bool> {
        let order = owner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .output_order
            .clone();
        let _guard = order.lock().unwrap_or_else(|e| e.into_inner());
        let s = owner.lock().unwrap_or_else(|e| e.into_inner());
        if self.epoch != Some((s.generation, s.library_generation))
            || s.generation != generation
            || s.snapshot != *foreground
        {
            return Ok(false);
        }
        drop(s);
        send(p)?;
        Ok(true)
    }
}
pub(super) fn next_transaction() -> u32 {
    static TOKEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    loop {
        let token = TOKEN.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        if token != 0 {
            return token;
        }
    }
}

/// Connection-local optional negotiation: at most three read-only rounds, with
/// cooldowns. A library retry/publication starts a fresh bounded budget.
#[derive(Default)]
pub(super) struct CapabilityProbe {
    epoch: Option<u64>,
    attempts: u8,
    last: Option<Instant>,
}
impl CapabilityProbe {
    pub(super) fn admit(&mut self, library_generation: u64) -> bool {
        if self.epoch != Some(library_generation) {
            *self = Self { epoch: Some(library_generation), ..Self::default() };
        }
        if self.attempts >= 3 || self.last.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
            return false;
        }
        self.attempts += 1;
        self.last = Some(Instant::now());
        true
    }
}

#[cfg(test)]
pub(super) fn capability_query(nonce: u32) -> [u8; 32] {
    let mut p = compact::query(nonce);
    p[3] = 2;
    p
}
#[cfg(test)]
pub(super) fn supported(request: &[u8; 32], reply: &[u8; 32]) -> bool {
    resident_catalog::capabilities(request, reply).is_some()
}
pub(super) fn request(
    op: u8,
    slot: u8,
    token: Option<u32>,
    identity: &[u8],
    crc: &[u8],
) -> [u8; 32] {
    let mut p = resident::request(op, slot, identity, crc);
    p[1] = 9;
    if let Some(t) = token {
        p[4..8].copy_from_slice(&t.to_le_bytes());
    }
    p
}
