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
pub(super) struct Background {
    epoch: Option<(u64, u64)>,
    library: Arc<Vec<crate::application_layouts::ApplicationLayoutSnapshot>>,
    enumerated: usize,
    enum_errors: u8,
    blocked: bool,
    inventory: Vec<Option<[u8; 24]>>,
    next: usize,
    job: Option<Job>,
    foreground: Option<crate::application_layouts::ApplicationLayoutSnapshot>,
    recover_next: usize,
    recovery: Option<Recovery>,
}
struct Job {
    snapshot: crate::application_layouts::ApplicationLayoutSnapshot,
    packets: Vec<[u8; 32]>,
    key: [u8; 24],
    token: u32,
    phase: usize,
    started: Instant,
}
impl Background {
    pub(super) fn pending(&self) -> bool {
        self.epoch.is_some()
            && !self.blocked
            && (self.enumerated < 100
                || self.recover_next < 100
                || self.recovery.is_some()
                || self.job.is_some()
                || self.next < self.library.len())
    }
    pub(super) fn tick(
        &mut self,
        dev: &HostDataHid,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        stop: &AtomicBool,
        transaction: &mut u32,
    ) {
        let (epoch, library) = {
            let s = owner.lock().unwrap_or_else(|e| e.into_inner());
            ((generation, s.library_generation), s.library.clone())
        };
        // The device is the source of truth on first connect. An empty host
        // library must still enumerate/read existing device records; with no
        // host jobs the scan ends without any STORE/SELECT/DELETE operations.
        if self.epoch != Some(epoch) {
            let owner_changed=self.epoch.is_none_or(|previous|previous.0!=epoch.0);
            self.epoch = Some(epoch);
            self.library = library;
            self.next = 0;
            self.job = None;
            self.blocked = false;
            self.enum_errors = 0;
            // Adopting each completed profile changes the host library, not the
            // physical read session. Keep its cursor and in-progress READ.
            if owner_changed {
                self.enumerated = 0;
                self.inventory = vec![None; 100];
                self.recover_next = 0;
                self.recovery = None;
            }
        }
        if self.blocked {
            return;
        }
        if self.foreground.as_ref() != Some(foreground) {
            if self.job.take().is_some() {
                self.next = self.next.saturating_sub(1);
            }
            self.foreground = Some(foreground.clone());
        }
        if stop.load(Ordering::Acquire) {
            return;
        }
        if self.enumerated < 100 && self.recovery.is_none() {
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
        if self.recover_next < 100 || self.recovery.is_some() {
            if self.recovery.is_none() {
                while self.recover_next < 100 {
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
        {
            let mut state=owner.lock().unwrap_or_else(|e|e.into_inner());
            if self.epoch == Some((state.generation, state.library_generation)) && state.generation==generation {
                state.resident_inventory_complete=self.enumerated==100 && self.recovery.is_none()
                    && self.job.is_none()
                    && self.inventory.iter().enumerate().all(|(slot, key)| key.is_none_or(|key|
                        state.resident_recoveries.get(&(slot as u8)).is_some_and(|r|
                            r.identity_crc == key && r.validation.is_none())))
                    && state.resident_recoveries.values().all(|r|r.validation.is_none());
            }
        }
        if self.job.is_none() {
            while self.next < self.library.len() {
                let snap = self.library[self.next].clone();
                self.next += 1;
                let Some(slot) = snap.resident_slot.filter(|s| *s < 100) else {
                    continue;
                };
                let id = snap.profile_id.clone().unwrap_or_default();
                *transaction = next_transaction();
                let legacy = ApplicationLayoutTransmission.packets(&snap);
                let Some(mut packets) = compact::encode(&legacy, *transaction) else {
                    self.status(
                        owner,
                        &id,
                        SyncStatus::Error("FULL cannot be encoded".into()),
                    );
                    continue;
                };
                packets[0][2] = 2;
                let mut key = [0; 24];
                key[..20].copy_from_slice(&legacy.last().unwrap()[11..31]);
                key[20..].copy_from_slice(&packets[0][16..20]);
                if self.inventory[usize::from(slot)].is_some_and(|old| old[..20] != key[..20]) {
                    self.status(
                        owner,
                        &id,
                        SyncStatus::Error(
                            "Identity conflict: existing device record preserved".into(),
                        ),
                    );
                    continue;
                }
                if !snap.active {
                    let Some(old) = self.inventory[usize::from(slot)] else {
                        self.status(owner, &id, SyncStatus::Synced);
                        continue;
                    };
                    key = old;
                }
                self.status(owner, &id, SyncStatus::Syncing);
                self.job = Some(Job {
                    snapshot: snap,
                    packets,
                    key,
                    token: *transaction,
                    phase: 0,
                    started: Instant::now(),
                });
                break;
            }
        }
        let Some(mut job) = self.job.take() else {
            return;
        };
        let id = job.snapshot.profile_id.clone().unwrap_or_default();
        let slot = job.snapshot.resident_slot.unwrap();
        if job.started.elapsed() >= Duration::from_millis(2400) {
            self.status(
                owner,
                &id,
                SyncStatus::Error("Staging timeout; republish to retry".into()),
            );
            return;
        }
        // One read/mutation or at most eight complete output reports per pass.
        let result: anyhow::Result<bool> = (|| {
            if !job.snapshot.active {
                let op = if job.phase == 0 { 4 } else { 3 };
                let p = if op == 4 {
                    request(4, slot, None, &job.key[..20], &job.key[20..])
                } else {
                    request(3, slot, None, &[0; 20], &[0; 4])
                };
                let mut reply = None;
                let result = self.send(dev, owner, foreground, generation, stop, &p, |r| {
                    reply = r;
                    Ok(())
                });
                if op == 4 {
                    // Any delivered/uncertain DELETE is reconciled by ENUM, never blindly replayed.
                    if matches!(result, Ok(true)) && reply.is_none() {
                        return Ok(false);
                    }
                    job.phase = 1;
                } else {
                    if !result? {
                        return Ok(false);
                    }
                    let Some(r) = reply else {
                        return Ok(false);
                    };
                    anyhow::ensure!(
                        r[..3] == [0xef, 9, 0x83]
                            && r[4..8] == p[4..8]
                            && r[3] == 1
                            && r[8..].iter().all(|b| *b == 0),
                        "DELETE not confirmed absent"
                    );
                    self.inventory[usize::from(slot)] = None;
                    // Retire stale READ data together with confirmed absence;
                    // otherwise the next UI tick can recreate the deleted row
                    // after its tombstone has legitimately been cleared.
                    {
                        let mut state = owner.lock().unwrap_or_else(|e| e.into_inner());
                        if self.epoch == Some((state.generation, state.library_generation)) {
                            state.resident_recoveries.remove(&slot);
                        }
                    }
                    self.status(owner, &id, SyncStatus::Synced);
                    job.phase = usize::MAX;
                }
            } else if job.phase == 0 {
                let p = request(0, slot, None, &job.key[..20], &job.key[20..]);
                let mut status = None;
                let sent = self.send(dev, owner, foreground, generation, stop, &p, |r| {
                    status = validate_reply(&p, r)?;
                    Ok(())
                })?;
                if !sent || status.is_none() {
                    return Ok(false);
                }
                match status.unwrap() {
                    0 => {
                        self.reconcile_query_hit(owner, slot, job.key);
                        self.status(owner, &id, SyncStatus::Synced);
                        job.phase = usize::MAX;
                    }
                    1 => job.phase = 1,
                    x => anyhow::bail!("QUERY status {x}"),
                };
            } else if job.phase <= job.packets.len() {
                let end = (job.phase + 8).min(job.packets.len() + 1);
                while job.phase < end {
                    let p = job.packets[job.phase - 1];
                    if !self.output(dev, owner, foreground, generation, stop, &p)? {
                        return Ok(false);
                    }
                    job.phase += 1;
                }
            } else {
                let n = job.phase - job.packets.len() - 1;
                let op = [5, 1, 0][n];
                let p = request(
                    op,
                    slot,
                    matches!(op, 1 | 5).then_some(job.token),
                    &job.key[..20],
                    &job.key[20..],
                );
                // A possibly delivered STORE makes old export bytes uncertain.
                // Only a subsequent validated FULL READ may restore that proof.
                if op == 1 {
                    let mut state = owner.lock().unwrap_or_else(|e| e.into_inner());
                    if self.epoch == Some((state.generation, state.library_generation)) {
                        state.resident_inventory_complete = false;
                        state.resident_recoveries.remove(&slot);
                    }
                }
                let mut status = None;
                let result = self.send(dev, owner, foreground, generation, stop, &p, |r| {
                    status = validate_reply(&p, r)?;
                    Ok(())
                });
                if op == 1 && (result.is_err() || status.is_some_and(|s| s != 0)) {
                    job.phase += 1;
                    return Ok(true);
                }
                let sent = result?;
                if !sent || status.is_none() {
                    return Ok(false);
                }
                anyhow::ensure!(status == Some(0), "resident op {op} status {status:?}");
                job.phase += 1;
                if op == 0 {
                    self.reconcile_query_hit(owner, slot, job.key);
                    self.status(owner, &id, SyncStatus::Synced);
                    job.phase = usize::MAX;
                }
            }
            Ok(true)
        })();
        match result {
            Err(e) => self.status(
                owner,
                &id,
                SyncStatus::Error(format!("{e:#}; reconcile before retry")),
            ),
            Ok(_) => {
                if job.phase != usize::MAX {
                    self.job = Some(job);
                }
            }
        }
    }
    fn reconcile_query_hit(
        &mut self,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        slot: u8,
        key: [u8; 24],
    ) {
        let mut state = owner.lock().unwrap_or_else(|e| e.into_inner());
        if self.epoch != Some((state.generation, state.library_generation)) {
            return;
        }
        self.inventory[usize::from(slot)] = Some(key);
        // A cancelled STORE may resume at the initial QUERY after ENUM/READ
        // cursors are exhausted. Neither QUERY path proves the FULL bytes.
        if !state
            .resident_recoveries
            .get(&slot)
            .is_some_and(|record| record.identity_crc == key && record.validation.is_none())
        {
            state.resident_inventory_complete = false;
            self.recovery = Some(Recovery {
                slot,
                key,
                raw: Vec::new(),
                confirm: false,
                errors: 0,
            });
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
    fn output(
        &self,
        dev: &HostDataHid,
        owner: &Mutex<ApplicationLayoutOwnerState>,
        foreground: &crate::application_layouts::ApplicationLayoutSnapshot,
        generation: u64,
        stop: &AtomicBool,
        p: &[u8; 32],
    ) -> anyhow::Result<bool> {
        if stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        let mut sent = false;
        let admitted = self.admit(owner, foreground, generation, p, |p| {
            sent = match dev {
                HostDataHid::Shared(o) => o.try_write_application_layout_stage(p)?,
                HostDataHid::Dedicated(_) => {
                    write_payload(dev, p)?;
                    true
                }
            };
            Ok(())
        })?;
        Ok(admitted && sent)
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
fn validate_reply(p: &[u8; 32], r: Option<[u8; 32]>) -> anyhow::Result<Option<u8>> {
    let Some(r) = r else {
        return Ok(None);
    };
    let mut expected = *p;
    expected[2] |= 128;
    expected[3] = r[3];
    anyhow::ensure!(r == expected && r[3] <= 8, "Uncorrelated EF09 response");
    Ok(Some(r[3]))
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

pub(super) fn capability_query(nonce: u32) -> [u8; 32] {
    let mut p = compact::query(nonce);
    p[3] = 2;
    p
}
pub(super) fn supported(request: &[u8; 32], reply: &[u8; 32]) -> bool {
    if request[..4] != [0xeb, 8, 0xa5, 2] || request[8..].iter().any(|b| *b != 0) {
        return false;
    }
    let mut expected = *request;
    expected[2] = 0x5a;
    expected[8..15].copy_from_slice(&[2, 100, 0xd0, 0x11, 16, 62, 22]);
    expected[16..20].copy_from_slice(&4194304u32.to_le_bytes());
    expected[20..24].copy_from_slice(&15u32.to_le_bytes());
    expected[24] = 2;
    *reply == expected
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
pub(super) fn exchange(
    dev: &HostDataHid,
    owner: &Mutex<ApplicationLayoutOwnerState>,
    snapshot: &crate::application_layouts::ApplicationLayoutSnapshot,
    generation: u64,
    stop: &AtomicBool,
    started: Instant,
    request: &[u8; 32],
) -> anyhow::Result<Option<u8>> {
    if stop.load(Ordering::Acquire) || started.elapsed() >= Duration::from_millis(2500) {
        return Ok(None);
    }
    let mut answer = None;
    if !write_application_layout_payload(owner, snapshot, generation, request, |p| {
        answer = match dev {
            HostDataHid::Shared(o) => o.poll_application_layout_event(p)?,
            HostDataHid::Dedicated(h) => Some(h.usb_send(p)?),
        };
        Ok(())
    })? {
        return Ok(None);
    }
    if stop.load(Ordering::Acquire)
        || started.elapsed() >= Duration::from_millis(2500)
        || !write_application_layout_payload(owner, snapshot, generation, &[], |_| Ok(()))?
        || matches!(dev,HostDataHid::Shared(o) if !o.host_session_is_current())
    {
        return Ok(None);
    }
    let Some(r) = answer else {
        return Ok(None);
    };
    let mut expected = *request;
    expected[2] |= 128;
    expected[3] = r[3];
    anyhow::ensure!(r == expected && r[3] <= 8, "uncorrelated EF09 response");
    Ok(Some(r[3]))
}
