//! Opt-in observational telemetry. This entire module is reachable only in cfg(test).
//! Context is explicitly inherited by the existing bridge worker, never process-global.
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use serde_json::{json, Value};

pub(crate) const TWIN: &str = "app::application_layout_runtime::tests::remaining_sm62_rebinds_after_first_unplug_without_editor_selection";
pub(crate) const CAPABILITY: &str = "qmk_hid_host::capability_review_tests::transient_fault_recovers_automatically_on_same_owner";

pub(crate) struct Trace {
    epoch: Instant,
    case: &'static str,
    sequence: AtomicU64,
    file: Mutex<File>,
}
thread_local! {
    static CONTEXT: RefCell<Option<Arc<Trace>>> = const { RefCell::new(None) };
}
pub(crate) fn context() -> Option<Arc<Trace>> {
    CONTEXT.with(|slot| slot.borrow().clone())
}
pub(crate) fn enabled() -> bool {
    CONTEXT.with(|slot| slot.borrow().is_some())
}
pub(crate) struct Scope {
    previous: Option<Arc<Trace>>,
    test: bool,
}
pub(crate) fn enter(trace: Option<Arc<Trace>>) -> Scope {
    Scope { previous: CONTEXT.with(|slot| slot.replace(trace)), test: false }
}
pub(crate) fn begin(case: &'static str) -> Scope {
    let mut scope = enter(None);
    if !matches!(case, TWIN | CAPABILITY) { return scope; }
    let Some(directory) = std::env::var_os("ENTROPY_PROFILE_DIAGNOSTICS_DIR") else { return scope; };
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() { eprintln!("native diagnostic directory must be absolute"); return scope; }
    // Never truncate a previous process/run. A new process-isolated file is mandatory.
    let label = if case == TWIN { "twin" } else { "capability" };
    let path = directory.join(format!("entropy-profile-{}-{label}.jsonl", std::process::id()));
    let file = std::fs::create_dir_all(&directory).and_then(|_| OpenOptions::new().write(true).create_new(true).open(path));
    let file = match file {
        Ok(file) => file,
        Err(error) => { eprintln!("native diagnostic file unavailable: {}", error.kind()); return scope; }
    };
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    CONTEXT.with(|slot| slot.replace(Some(Arc::new(Trace {
        epoch: *EPOCH.get_or_init(Instant::now), case, sequence: AtomicU64::new(0), file: Mutex::new(file),
    }))));
    scope.test = true;
    event("test_begin", json!({"format": 1, "clock": "shared_context_Instant_elapsed_ns"}));
    scope
}
impl Drop for Scope {
    fn drop(&mut self) {
        if self.test { event("test_end", json!({"panicking": std::thread::panicking()})); }
        CONTEXT.with(|slot| slot.replace(self.previous.take()));
    }
}
pub(crate) fn event(kind: &'static str, data: Value) {
    let Some(trace) = context() else { return; };
    // Timestamp before serialization/locking: never hold this mutex over owning code.
    let ns = trace.epoch.elapsed().as_nanos();
    let sequence = trace.sequence.fetch_add(1, Ordering::Relaxed);
    let line = json!({"monotonic_ns": ns, "seq": sequence, "pid": std::process::id(),
        "thread": format!("{:?}", std::thread::current().id()),
        "case": trace.case, "event": kind, "data": data}).to_string();
    let mut file = trace.file.lock().unwrap_or_else(|e| e.into_inner());
    // I/O failure cannot change a test outcome; the receipt must detect missing test_end.
    if writeln!(file, "{line}").is_err() { eprintln!("native diagnostic write failed"); }
}
pub(crate) fn deadline(phase: &'static str, deadline: Instant) {
    let Some(trace) = context() else { return; };
    event("original_deadline", json!({"phase": phase, "deadline_monotonic_ns": deadline.saturating_duration_since(trace.epoch).as_nanos()}));
}
// Whitelist static validator messages; never emit an arbitrary error string.
pub(crate) fn validation_code(error: Option<&str>) -> Option<&'static str> {
    error.map(|error| match error {
        "FULL length" => "full_length",
        "FULL CRC32" => "full_crc32",
        "FULL begin" => "full_begin",
        "FULL commit/identity" => "full_commit_identity",
        "truncated FULL" => "full_truncated",
        "FULL stack count/name" => "full_stack_count_name",
        "FULL layer name" => "full_layer_name",
        "FULL trailing bytes" => "full_trailing_bytes",
        "FULL semantic CRC16" => "full_semantic_crc16",
        "READ/ENUM recovery unconfirmed after3 attempts; raw partial preserved" => "recovery_attempts_exhausted",
        _ => "other_validation_error",
    })
}
pub(crate) struct Phase {
    name: &'static str,
    started: Option<Instant>,
    budget: Option<Duration>,
}
pub(crate) fn phase(name: &'static str, budget: Option<Duration>) -> Phase {
    let started = enabled().then(Instant::now);
    if started.is_some() { event("phase_begin", json!({"phase": name, "budget_ns": budget.map(|d| d.as_nanos())})); }
    Phase { name, started, budget }
}
impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            event("phase_end", json!({"phase": self.name, "elapsed_ns": started.elapsed().as_nanos(),
                "budget_ns": self.budget.map(|d| d.as_nanos()), "panicking": std::thread::panicking()}));
        }
    }
}

// Protocol envelope only: never serialize identity/CRC bytes or profile data.
pub(crate) struct Wire {
    id: Option<u64>,
    request: Option<[u8; 32]>,
    stage: &'static str,
    ended: bool,
}
impl Wire {
    pub(crate) fn new(packet: &[u8], phase: &'static str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = if enabled() && packet.len() == 32 && matches!(packet[0], 0xeb | 0xef) {
            Some(<[u8; 32]>::try_from(packet).unwrap())
        } else { None };
        let id = request.map(|_| NEXT.fetch_add(1, Ordering::Relaxed));
        if let (Some(id), Some(p)) = (id, request) {
            event("wire_request", json!({"wire": id, "phase": phase, "opcode": p[0], "family": p[1],
                "op_or_schema": if p[0] == 0xeb {p[3]} else {p[2]},
                "slot": if p[0] == 0xef {Some(p[3])} else {None},
                "nonce": u32::from_le_bytes(p[4..8].try_into().unwrap()),
                "offset": if p[..3] == [0xef,9,6] {Some(u16::from_le_bytes([p[8],p[9]]))} else {None}}));
        }
        Self { id, request, stage: "before_admission", ended: false }
    }
    pub(crate) fn stage(&mut self, stage: &'static str) { self.stage = stage; }
    pub(crate) fn reply(&mut self, reply: Option<[u8; 32]>) {
        self.stage = "reply_observed";
        if let (Some(id), Some(p)) = (self.id, self.request) {
            if let Some(r) = reply {
                event("wire_reply", json!({"wire": id, "opcode": r[0], "family": r[1],
                    "op_or_schema": if p[0] == 0xeb {r[3]} else {r[2]}, "status_or_slot": r[3],
                    "status": if p[0] == 0xef && matches!(p[2],3|7) {Some(r[3])} else {None},
                    "nonce": u32::from_le_bytes(r[4..8].try_into().unwrap()), "nonce_matches": r[4..8] == p[4..8],
                    "ack": r[2], "returned_len": if p[..3] == [0xef,9,6] {Some(r[10])} else {None},
                    "returned_offset": if p[..3] == [0xef,9,6] {Some(u16::from_le_bytes([r[8],r[9]]))} else {None},
                    "schema2_slots": if p[..4] == [0xeb,8,0xa5,2] {Some(r[9])} else {None},
                    "schema_valid": if p[0] == 0xeb {Some(super::resident_catalog::capabilities(&p,&r).is_some())} else {None},
                    "compact_valid": if p[0] == 0xeb && p[3] == 1 {super::compact::capability(&p,&r)} else {None}}));
            } else { event("wire_no_reply", json!({"wire": id})); }
        }
    }
    pub(crate) fn end(&mut self, outcome: &'static str) {
        self.ended = true;
        if let Some(id) = self.id { event("wire_end", json!({"wire": id, "outcome": outcome, "stage": self.stage})); }
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        if !self.ended {
            self.end(if std::thread::panicking() { "panic" } else { match self.stage {
                "transport_call" => "transport_error",
                "consume_call" => "consume_error",
                _ => "early_return_before_completion",
            }});
        }
    }
}
