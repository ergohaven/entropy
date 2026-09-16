//! The native HID helper's sole IPC owner. UI-facing operations never touch
//! child handles, pipe IO, or blocking mutexes. Healthy established endpoints
//! are not a product-wide device quota. One fixed reaper supervises their owners;
//! new admission is bounded by opening/retirement debt, with exclusive physical
//! reservations retained until child reaping AND IPC-thread termination.
use super::{device_transport, ensure_output_report_len, HidDevice, HidTransport, MSG_LEN};
use crate::device::Device;
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);
// Must exceed the longest helper-side USB command. Pictogram SLOT_COMMIT may
// synchronously program flash for up to 2.5 s; queued host-data writes share the
// same ordered owner and must not retire it while that commit is still active.
const USB_COMMAND_TIMEOUT: Duration = Duration::from_secs(4);
const BLE_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const REAPER_INTERVAL: Duration = Duration::from_millis(10);
const MAX_FRAME: usize = 4_096;
// Limit unproven/incomplete work, NOT the number of usable attached devices.
// Healthy established owners do not consume this budget. Existing owners can
// retire in a burst above this threshold; admission then stays closed until
// debt falls below it. Repeated failures cannot grow helpers/IPC workers without
// bound, and no new helper can bypass an unreclaimed physical reservation.
const MAX_UNSETTLED_HELPERS: usize = 2;
const OUTPUT_PREFIX: &str = "output:";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyResponse {
    ok: bool,
    data: Option<String>,
    error: Option<String>,
}

struct Slot {
    #[cfg(test)]
    submitted: Mutex<Vec<String>>,
    #[cfg(test)]
    completed_output: Mutex<Vec<Vec<u8>>>,
    device: Device,
    established: AtomicBool,
    retired: AtomicBool,
    child: Mutex<Option<Child>>,
    reaped: AtomicBool,
    worker_joined: AtomicBool,
}

struct Entry {
    slot: Arc<Slot>,
    worker: Option<JoinHandle<()>>,
    targets: Vec<Device>,
}

struct Registry {
    reaper_started: bool,
    entries: Vec<Entry>,
}

impl Registry {
    fn unsettled_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| {
                entry.slot.retired.load(Ordering::Acquire)
                    || !entry.slot.established.load(Ordering::Acquire)
            })
            .count()
    }
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    reaper_started: false,
    entries: Vec::new(),
});

struct Job {
    request: String,
    deadline: Instant,
    reply: mpsc::SyncSender<std::result::Result<String, String>>,
}

pub(super) struct HidProxy {
    pub(super) host_output: Arc<crate::qmk_hid_host::HostOutputOwner>,
    slot: Arc<Slot>,
    requests: mpsc::SyncSender<Job>,
    transport: HidTransport,
    #[cfg(test)]
    timeout: Option<Duration>,
}

/// Non-owning revocation capability. A background desktop query may hold its
/// HidDevice indefinitely, but cannot keep the physical reservation alive after
/// the bridge is stopped. Reaping remains exclusively owned by the registry.
#[derive(Clone)]
pub(crate) struct HidRetirement(std::sync::Weak<Slot>);

impl HidRetirement {
    pub(crate) fn retire(&self) {
        if let Some(slot) = self.0.upgrade() {
            slot.retired.store(true, Ordering::Release);
        }
    }
}

impl Drop for HidProxy {
    fn drop(&mut self) {
        // Do not kill/wait/join or acquire ANY mutex in Drop. The reaper owns
        // cleanup, and retains the physical reservation throughout retirement.
        self.slot.retired.store(true, Ordering::Release);
    }
}

struct HelperCommand {
    command: Command,
    #[cfg(test)]
    fake: bool,
    #[cfg(test)]
    exit_gate: Option<Arc<AtomicBool>>,
}

impl HidProxy {
    pub(super) fn open(device: &Device) -> Result<Self> {
        let exe = std::env::current_exe().context("Failed to find Entropy executable")?;
        let mut command = Command::new(exe);
        command
            .arg("--entropy-hid-proxy")
            .arg(serde_json::to_string(device)?);
        Self::start(
            device,
            HelperCommand {
                command,
                #[cfg(test)]
                fake: false,
                #[cfg(test)]
                exit_gate: None,
            },
            STARTUP_TIMEOUT,
        )
    }

    fn start(device: &Device, spec: HelperCommand, timeout: Duration) -> Result<Self> {
        let deadline = Instant::now() + timeout;
        // A stopped automatic bridge has revoked its transport, but its child
        // may not be reaped until the next supervisor tick. Await only retiring
        // reservations/opening capacity within this SAME startup deadline. A
        // successful startup releases admission credit without dropping its
        // exclusive endpoint reservation; healthy third/fourth devices may open.
        let mut registry = loop {
            if Instant::now() >= deadline {
                bail!("HID helper timed out waiting for opening/retiring transport capacity");
            }
            if let Ok(registry) = REGISTRY.try_lock() {
                let overlaps = |entry: &Entry| {
                    entry
                        .targets
                        .iter()
                        .any(|target| target.may_share_physical_device(device))
                };
                if registry
                    .entries
                    .iter()
                    .any(|entry| overlaps(entry) && !entry.slot.retired.load(Ordering::Acquire))
                {
                    bail!("HID device already reserved by a live transport");
                }
                let full = registry.unsettled_count() >= MAX_UNSETTLED_HELPERS;
                let overlap = registry.entries.iter().any(overlaps);
                if !full && !overlap {
                    break registry;
                }
                // Never hold the registry mutex during a wait, OS call or I/O.
                drop(registry);
            }
            std::thread::sleep(
                REAPER_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        };
        if !registry.reaper_started {
            std::thread::Builder::new()
                .name("entropy-hid-reaper".into())
                .spawn(reaper)
                .context("Failed to start HID helper reaper")?;
            registry.reaper_started = true;
        }
        let slot = Arc::new(Slot {
            #[cfg(test)]
            submitted: Default::default(),
            #[cfg(test)]
            completed_output: Default::default(),
            device: device.clone(),
            established: AtomicBool::new(false),
            retired: AtomicBool::new(false),
            child: Mutex::new(None),
            reaped: AtomicBool::new(false),
            worker_joined: AtomicBool::new(false),
        });
        let (requests, rx) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker_slot = slot.clone();
        let worker = std::thread::Builder::new()
            .name("entropy-hid-ipc".into())
            .spawn(move || {
                #[cfg(test)]
                let exit_gate = spec.exit_gate.clone();
                let result = ipc_owner(&worker_slot, spec, rx, &ready_tx);
                worker_slot.retired.store(true, Ordering::Release);
                if let Err(error) = result {
                    let _ = ready_tx.try_send(Err(format!("{error:#}")));
                }
                // Test-only scheduling control verifies that reaping a child
                // alone must NOT release a slot while its IO worker is alive.
                #[cfg(test)]
                if let Some(gate) = exit_gate {
                    while !gate.load(Ordering::Acquire) {
                        std::thread::sleep(REAPER_INTERVAL);
                    }
                }
            })
            .context("Failed to start HID IPC owner")?;
        registry.entries.push(Entry {
            slot: slot.clone(),
            worker: Some(worker),
            targets: vec![device.clone()],
        });
        drop(registry);
        let proxy = Self {
            host_output: Default::default(),
            slot,
            requests,
            transport: device_transport(device),
            #[cfg(test)]
            timeout: None,
        };
        match ready_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(())) if proxy.is_available() && Instant::now() < deadline => {
                // Publish only after validated startup was accepted by its owner.
                // A concurrent retirement still counts as debt, even if this
                // store races it; retirement is monotonic and takes precedence.
                proxy.slot.established.store(true, Ordering::Release);
                Ok(proxy)
            }
            Ok(Err(error)) => bail!(error),
            Ok(Ok(())) => bail!("HID helper disconnected during startup"),
            Err(_) => bail!("HID helper timed out or disconnected while opening device"),
        }
        // Every failure drops proxy, atomically retiring even malformed/EOF or
        // delayed spawn paths. No early return can orphan an unreserved child.
    }

    pub(super) fn retirement_handle(&self) -> HidRetirement {
        HidRetirement(Arc::downgrade(&self.slot))
    }

    pub(super) fn is_available(&self) -> bool {
        !self.slot.retired.load(Ordering::Acquire)
    }

    pub(super) fn is_bluetooth_transport(&self) -> bool {
        self.transport.is_bluetooth()
    }

    fn command_timeout(&self) -> Duration {
        #[cfg(test)]
        if let Some(timeout) = self.timeout {
            return timeout;
        }
        if self.transport.is_bluetooth() {
            BLE_COMMAND_TIMEOUT
        } else {
            USB_COMMAND_TIMEOUT
        }
    }

    fn request(&self, request: String) -> Result<String> {
        if !self.is_available() {
            bail!("HID helper disconnected (transport retired)");
        }
        let deadline = Instant::now() + self.command_timeout();
        let (reply, rx) = mpsc::sync_channel(1);
        // Bounded queue, no waiting for the IPC writer or another caller. Both
        // shared output and queries traverse this SAME owner and response stream.
        self.requests
            .try_send(Job {
                request,
                deadline,
                reply,
            })
            .map_err(|_| anyhow::anyhow!("HID helper disconnected or request queue busy"))?;
        let result = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
        match result {
            Ok(Ok(line)) if self.is_available() && Instant::now() < deadline => Ok(line),
            Ok(Err(error)) => {
                self.slot.retired.store(true, Ordering::Release);
                bail!(error)
            }
            _ => {
                self.slot.retired.store(true, Ordering::Release);
                bail!("HID helper timed out or disconnected during command")
            }
        }
    }

    pub(super) fn usb_send(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
        ensure_output_report_len(data)?;
        let line = self.request(bytes_to_hex(data))?;
        let response: ProxyResponse = serde_json::from_str(&line)?;
        if !response.ok {
            bail!(response
                .error
                .unwrap_or_else(|| "HID helper command failed".into()));
        }
        // ipc_owner already validated the full frame before delivering it.
        let bytes = hex_to_bytes(response.data.as_deref().context("Missing HID response")?)?;
        let mut out = [0; MSG_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    pub(super) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        ensure_output_report_len(data)?;
        let line = self.request(format!("{OUTPUT_PREFIX}{}", bytes_to_hex(data)))?;
        let response: ProxyResponse = serde_json::from_str(&line)?;
        if !response.ok {
            bail!(response
                .error
                .unwrap_or_else(|| "HID helper output failed".into()));
        }
        #[cfg(test)]
        self.slot
            .completed_output
            .lock()
            .unwrap()
            .push(data.to_vec());
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyClaim {
    claim: Device,
}

/// Extend this helper's reservation BEFORE opening any direct/fallback target.
/// This handshake travels over the existing startup IPC, not another transport.
pub(crate) fn claim_open_target(target: &Device) -> Result<()> {
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string(&ProxyClaim {
            claim: target.clone()
        })?
    )?;
    std::io::stdout().flush()?;
    let line = read_frame(&mut BufReader::new(std::io::stdin()))?;
    validate_response(&line, false, true)?;
    let response: ProxyResponse = serde_json::from_str(&line)?;
    if !response.ok {
        bail!(response
            .error
            .unwrap_or_else(|| "HID target reservation rejected".into()));
    }
    Ok(())
}

fn reserve_target(slot: &Arc<Slot>, target: Device) -> Result<()> {
    if slot.retired.load(Ordering::Acquire) || !slot.device.permits_hid_target(&target) {
        bail!("HID target does not match the reserved physical device");
    }
    let mut registry = REGISTRY
        .try_lock()
        .map_err(|_| anyhow::anyhow!("HID target registry busy"))?;
    for entry in &registry.entries {
        if !Arc::ptr_eq(&entry.slot, slot)
            && entry
                .targets
                .iter()
                .any(|reserved| reserved.may_share_physical_device(&target))
        {
            bail!("HID open target overlaps another live or retiring reservation");
        }
    }
    let entry = registry
        .entries
        .iter_mut()
        .find(|entry| Arc::ptr_eq(&entry.slot, slot))
        .context("HID reservation disappeared")?;
    if entry.targets.iter().any(|existing| {
        existing.path == target.path && existing.instance_token == target.instance_token
    }) {
        return Ok(());
    }
    if entry.targets.len() >= 16 {
        bail!("HID target alias budget exhausted");
    }
    entry.targets.push(target);
    Ok(())
}

fn reaper() {
    loop {
        let (slots, finished) = if let Ok(mut registry) = REGISTRY.try_lock() {
            let mut finished = Vec::new();
            let slots = registry
                .entries
                .iter_mut()
                .map(|entry| {
                    if entry.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                        entry.slot.retired.store(true, Ordering::Release);
                        finished.push((
                            entry.slot.clone(),
                            entry.worker.take().expect("finished worker"),
                        ));
                    }
                    entry.slot.clone()
                })
                .collect::<Vec<_>>();
            (slots, finished)
        } else {
            (Vec::new(), Vec::new())
        };
        for slot in slots {
            // Child syscalls never execute under the registry lock or on UI /
            // Drop / request threads. try_wait is the ONLY child reap primitive.
            if let Ok(mut guard) = slot.child.try_lock() {
                if let Some(child) = guard.as_mut() {
                    if slot.retired.load(Ordering::Acquire) {
                        let _ = child.kill();
                    }
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        slot.retired.store(true, Ordering::Release);
                        slot.reaped.store(true, Ordering::Release);
                        *guard = None;
                    }
                } else if slot.worker_joined.load(Ordering::Acquire) {
                    // Spawn failure: no child was created. A still-running
                    // spawn cannot take this branch, so its budget is retained.
                    slot.reaped.store(true, Ordering::Release);
                }
            }
        }
        for (slot, worker) in finished {
            // is_finished can become true just before thread-local destructors
            // finish. Join ONLY here, off-caller and outside every mutex; retain
            // the slot until actual thread termination, not a completion flag.
            let _ = worker.join();
            slot.worker_joined.store(true, Ordering::Release);
        }
        if let Ok(mut registry) = REGISTRY.try_lock() {
            registry.entries.retain(|entry| {
                !(entry.slot.reaped.load(Ordering::Acquire)
                    && entry.slot.worker_joined.load(Ordering::Acquire))
            });
        }
        std::thread::sleep(REAPER_INTERVAL);
    }
}

fn read_frame(reader: &mut impl BufRead) -> Result<String> {
    let mut bytes = Vec::new();
    // Bound allocation as well as time (time is enforced by retirement/kill).
    reader
        .take((MAX_FRAME + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > MAX_FRAME || !bytes.ends_with(b"\n") {
        bail!("HID helper disconnected or returned oversized/unterminated frame");
    }
    Ok(String::from_utf8(bytes).context("HID helper returned non-UTF8 frame")?)
}

fn validate_response(line: &str, output: bool, startup: bool) -> Result<()> {
    let response: ProxyResponse =
        serde_json::from_str(line).context("HID helper returned malformed response")?;
    if !response.ok {
        if response.error.is_none() || response.data.is_some() {
            bail!("HID helper returned malformed error response");
        }
    } else if response.error.is_some() {
        bail!("HID helper returned contradictory response");
    } else if output || startup {
        if response.data.is_some() {
            bail!("HID helper returned unexpected data");
        }
    } else if hex_to_bytes(
        response
            .data
            .as_deref()
            .context("HID helper response missing data")?,
    )?
    .len()
        != MSG_LEN
    {
        bail!("HID helper invalid response length");
    }
    Ok(())
}

fn ipc_owner(
    slot: &Arc<Slot>,
    mut spec: HelperCommand,
    rx: mpsc::Receiver<Job>,
    ready: &mpsc::SyncSender<std::result::Result<(), String>>,
) -> Result<()> {
    if slot.retired.load(Ordering::Acquire) {
        bail!("HID helper startup cancelled");
    }
    spec.command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(test)]
    if spec.fake {
        spec.command.stderr(Stdio::piped()).stdout(Stdio::null());
    }
    let child = spec.command.spawn().context("Failed to start HID helper")?;
    // Publish ownership immediately, before any fallible pipe extraction. The
    // reaper can now cancel even a worker blocked in write_all/read/flush.
    let (mut stdin, stdout): (_, Box<dyn Read + Send>) = {
        let mut guard = slot.child.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(child);
        let child = guard.as_mut().expect("just published child");
        let stdin = child.stdin.take().context("HID helper stdin unavailable")?;
        #[cfg(test)]
        let stdout: Box<dyn Read + Send> = if spec.fake {
            Box::new(
                child
                    .stderr
                    .take()
                    .context("Fake helper stderr unavailable")?,
            )
        } else {
            Box::new(
                child
                    .stdout
                    .take()
                    .context("HID helper stdout unavailable")?,
            )
        };
        #[cfg(not(test))]
        let stdout: Box<dyn Read + Send> = Box::new(
            child
                .stdout
                .take()
                .context("HID helper stdout unavailable")?,
        );
        (stdin, stdout)
    };
    let mut reader = BufReader::new(stdout);
    let mut claims = 0;
    let line = loop {
        let line = read_frame(&mut reader)?;
        if let Ok(claim) = serde_json::from_str::<ProxyClaim>(&line) {
            claims += 1;
            if claims > 32 {
                bail!("HID helper sent too many startup claims");
            }
            let result = reserve_target(slot, claim.claim);
            let response = ProxyResponse {
                ok: result.is_ok(),
                data: None,
                error: result.as_ref().err().map(|error| format!("{error:#}")),
            };
            writeln!(stdin, "{}", serde_json::to_string(&response)?)?;
            stdin.flush()?;
            result?;
        } else {
            break line;
        }
    };
    validate_response(&line, false, true)?;
    let response: ProxyResponse = serde_json::from_str(&line)?;
    if !response.ok {
        bail!(response
            .error
            .unwrap_or_else(|| "HID helper failed to open device".into()));
    }
    if slot.retired.load(Ordering::Acquire) {
        bail!("HID helper startup cancelled");
    }
    ready
        .try_send(Ok(()))
        .map_err(|_| anyhow::anyhow!("HID helper startup receiver disappeared"))?;
    while !slot.retired.load(Ordering::Acquire) {
        let job = match rx.recv_timeout(REAPER_INTERVAL) {
            Ok(job) => job,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        if slot.retired.load(Ordering::Acquire) || Instant::now() >= job.deadline {
            break;
        }
        let result = (|| -> Result<String> {
            // BOTH the blocking pipe write and read are off-caller. The caller's
            // single absolute deadline covers queueing, writing, and reading.
            writeln!(stdin, "{}", job.request).context("Failed to write HID helper request")?;
            stdin
                .flush()
                .context("Failed to flush HID helper request")?;
            #[cfg(test)]
            slot.submitted.lock().unwrap().push(job.request.clone());
            let line = read_frame(&mut reader)?;
            validate_response(&line, job.request.starts_with(OUTPUT_PREFIX), false)?;
            let response: ProxyResponse = serde_json::from_str(&line)?;
            // These are raw transport operations: genuine firmware rejection
            // is carried in the successful data bytes, NOT as an IPC error.
            // Any backend error (including timeout before our own deadline)
            // invalidates the stream, otherwise a late uncorrelated reply can
            // be consumed by the next request.
            if !response.ok {
                bail!(response
                    .error
                    .unwrap_or_else(|| "HID helper transport failed".into()));
            }
            Ok(line)
        })();
        if slot.retired.load(Ordering::Acquire) || Instant::now() >= job.deadline {
            break;
        }
        match result {
            Ok(line) => {
                let _ = job.reply.try_send(Ok(line));
            }
            Err(error) => {
                slot.retired.store(true, Ordering::Release);
                let _ = job.reply.try_send(Err(format!("{error:#}")));
                break;
            }
        }
    }
    Ok(())
}

pub fn run_hid_proxy_if_requested() -> bool {
    let mut args = std::env::args();
    let _ = args.next();
    if args.next().as_deref() != Some("--entropy-hid-proxy") {
        return false;
    }
    let result = (|| -> Result<()> {
        let json = args.next().context("Missing HID helper device argument")?;
        let device: Device = serde_json::from_str(&json)?;
        // The helper dispatch precedes normal startup. On macOS its OWN main
        // thread must initialize the process-global IOHIDManager/run loop.
        #[cfg(target_os = "macos")]
        super::initialize_macos_hid_on_main_thread();
        run_hid_proxy(device)
    })();
    if let Err(error) = result {
        let _ = emit_response(&ProxyResponse {
            ok: false,
            data: None,
            error: Some(format!("{error:#}")),
        });
    }
    true
}

fn emit_response(response: &ProxyResponse) -> Result<()> {
    writeln!(std::io::stdout(), "{}", serde_json::to_string(response)?)?;
    std::io::stdout().flush()?;
    Ok(())
}

fn run_hid_proxy(device: Device) -> Result<()> {
    let hid = HidDevice::open_in_helper(&device)?;
    emit_response(&ProxyResponse {
        ok: true,
        data: None,
        error: None,
    })?;
    let mut reader = BufReader::new(std::io::stdin());
    loop {
        let line = read_frame(&mut reader)?;
        let line = line.trim();
        // Keep the IOHIDManager guard INSIDE this helper for the entire local
        // operation. Parent Proxy calls explicitly do not acquire that lock.
        #[cfg(target_os = "macos")]
        let _hid_lock = hid.macos_hid_operation_lock();
        let result = if let Some(encoded) = line.strip_prefix(OUTPUT_PREFIX) {
            hex_to_bytes(encoded)
                .and_then(|data| hid.write_output_report(&data))
                .map(|()| None)
        } else {
            hex_to_bytes(line)
                .and_then(|data| hid.usb_send(&data))
                .map(|data| Some(bytes_to_hex(&data)))
        };
        let response = match result {
            Ok(data) => ProxyResponse {
                ok: true,
                data,
                error: None,
            },
            Err(error) => ProxyResponse {
                ok: false,
                data: None,
                error: Some(format!("{error:#}")),
            },
        };
        emit_response(&response)?;
        if !response.ok {
            return Ok(());
        }
    }
}

fn bytes_to_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for &byte in data {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 || hex.len() > MSG_LEN * 2 {
        bail!("Invalid HID hex length");
    }
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("Invalid HID hex digit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    static TEST_SERIAL: Mutex<()> = Mutex::new(());
    const TEST_TIMEOUT: Duration = Duration::from_millis(200);

    fn device(serial: &str) -> Device {
        Device {
            name: "Fake transport (never opened)".into(),
            vendor_id: 0x1209,
            product_id: 0x1234,
            manufacturer: "Entropy tests".into(),
            serial_number: serial.into(),
            bus_type: "Usb".into(),
            path: format!("fake-endpoint-{serial}"),
            instance_token: format!("fake-instance-{serial}"),
            firmware: crate::firmware::FirmwareProtocol::Vial,
        }
    }

    fn await_condition(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "bounded lifecycle assertion timed out"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn empty_registry() {
        await_condition(|| REGISTRY.try_lock().is_ok_and(|r| r.entries.is_empty()));
    }

    fn serial_test() -> MutexGuard<'static, ()> {
        let guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        empty_registry();
        guard
    }

    fn spec(mode: &str, gate: Option<Arc<AtomicBool>>, claim: Option<&Device>) -> HelperCommand {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "hid::hid_proxy::tests::helper_process",
                "--ignored",
                "--nocapture",
            ])
            .env("ENTROPY_HID_TEST_CASE", mode);
        if let Some(claim) = claim {
            command.env(
                "ENTROPY_HID_TEST_CLAIM",
                serde_json::to_string(claim).unwrap(),
            );
        }
        HelperCommand {
            command,
            fake: true,
            exit_gate: gate,
        }
    }

    fn start(mode: &str, serial: &str) -> HidProxy {
        let mut proxy = HidProxy::start(
            &device(serial),
            spec(mode, None, None),
            Duration::from_secs(3),
        )
        .unwrap();
        proxy.timeout = Some(TEST_TIMEOUT);
        proxy
    }

    // Compiled exclusively into libtest. Production builds have no fake
    // transport, environment bypass, or alternate helper executable selector.
    // Use stderr for IPC: libtest's own stdout framing is not the HID protocol.
    #[test]
    #[ignore = "subprocess harness; invoked only by real helper lifecycle tests"]
    fn helper_process() {
        let Ok(mode) = std::env::var("ENTROPY_HID_TEST_CASE") else {
            return;
        };
        let mut output = std::io::stderr().lock();
        let mut input = BufReader::new(std::io::stdin());
        let ready = r#"{"ok":true,"data":null,"error":null}"#;
        if mode == "startup-silence" {
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode == "startup-eof" {
            std::process::exit(0);
        }
        if mode == "startup-malformed" {
            writeln!(output, "not json").unwrap();
            output.flush().unwrap();
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode == "startup-oversized" {
            write!(output, "{}", "x".repeat(MAX_FRAME + 1)).unwrap();
            output.flush().unwrap();
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        if let Ok(claim) = std::env::var("ENTROPY_HID_TEST_CLAIM") {
            writeln!(output, "{{\"claim\":{claim}}}").unwrap();
            output.flush().unwrap();
            let response = read_frame(&mut input).unwrap();
            if !serde_json::from_str::<ProxyResponse>(&response).unwrap().ok {
                std::process::exit(0);
            }
        }
        writeln!(output, "{ready}").unwrap();
        output.flush().unwrap();
        if mode == "write-block" {
            std::thread::sleep(Duration::from_secs(60));
            return;
        }
        loop {
            let Ok(line) = read_frame(&mut input) else {
                std::process::exit(0);
            };
            if mode == "backend-timeout" {
                writeln!(
                    output,
                    "{}",
                    serde_json::to_string(&ProxyResponse {
                        ok: false,
                        data: None,
                        error: Some("Bluetooth Vial timeout — keyboard did not respond".into()),
                    })
                    .unwrap()
                )
                .unwrap();
                output.flush().unwrap();
                std::thread::sleep(Duration::from_millis(500));
                let _ = writeln!(
                    output,
                    "{}",
                    serde_json::to_string(&ProxyResponse {
                        ok: true,
                        data: Some(bytes_to_hex(&[99; MSG_LEN])),
                        error: None,
                    })
                    .unwrap()
                );
                let _ = output.flush();
                std::thread::sleep(Duration::from_secs(60));
                return;
            }
            if mode == "read-block" {
                std::thread::sleep(Duration::from_secs(60));
                return;
            }
            if mode == "request-eof" {
                std::process::exit(0);
            }
            if mode == "request-malformed" {
                writeln!(output, "{{}}").unwrap();
                output.flush().unwrap();
                std::thread::sleep(Duration::from_secs(60));
                return;
            }
            if mode == "late" {
                std::thread::sleep(Duration::from_millis(600));
            }
            let response = if line.trim().starts_with(OUTPUT_PREFIX) {
                ready.to_owned()
            } else {
                let mut data = hex_to_bytes(line.trim()).unwrap();
                if data.starts_with(&[0xFE, 0x09]) && mode == "discovery-block" {
                    std::thread::sleep(Duration::from_secs(60));
                    return;
                }
                if data.starts_with(&[0xFE, 0x09]) && mode == "host-extended" {
                    data = vec![0xFF; MSG_LEN];
                    for (index, qsid) in (357u16..=366).enumerate() {
                        data[index * 2..index * 2 + 2].copy_from_slice(&qsid.to_le_bytes());
                    }
                }
                data.resize(MSG_LEN, 0);
                serde_json::to_string(&ProxyResponse {
                    ok: true,
                    data: Some(bytes_to_hex(&data)),
                    error: None,
                })
                .unwrap()
            };
            if writeln!(output, "{response}")
                .and_then(|()| output.flush())
                .is_err()
            {
                return;
            }
        }
    }

    #[test]
    fn real_helper_startup_silence_is_bounded_and_reaped() {
        let _guard = serial_test();
        let start = Instant::now();
        assert!(HidProxy::start(
            &device("A"),
            spec("startup-silence", None, None),
            TEST_TIMEOUT
        )
        .is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        empty_registry();
    }

    #[test]
    fn real_helper_malformed_startup_is_bounded_and_reaped() {
        let _guard = serial_test();
        let start = Instant::now();
        assert!(HidProxy::start(
            &device("A"),
            spec("startup-malformed", None, None),
            Duration::from_secs(3)
        )
        .is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        empty_registry();
    }

    #[test]
    fn real_helper_eof_startup_is_bounded_and_reaped() {
        let _guard = serial_test();
        assert!(HidProxy::start(
            &device("A"),
            spec("startup-eof", None, None),
            Duration::from_secs(3)
        )
        .is_err());
        empty_registry();
    }

    #[test]
    fn real_helper_oversized_unterminated_startup_is_bounded() {
        let _guard = serial_test();
        assert!(HidProxy::start(
            &device("A"),
            spec("startup-oversized", None, None),
            Duration::from_secs(3)
        )
        .is_err());
        empty_registry();
    }

    #[test]
    fn real_helper_blocked_read_and_drop_are_bounded() {
        let _guard = serial_test();
        let proxy = start("read-block", "A");
        let slot = proxy.slot.clone();
        let start = Instant::now();
        assert!(proxy.usb_send(&[1]).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(!proxy.is_available());
        let start = Instant::now();
        drop(proxy);
        assert!(start.elapsed() < Duration::from_millis(100));
        empty_registry();
        assert!(slot.reaped.load(Ordering::Acquire));
    }

    #[test]
    fn real_helper_blocked_pipe_write_has_the_same_deadline() {
        let _guard = serial_test();
        let proxy = start("write-block", "A");
        let start = Instant::now();
        // Larger than any OS anonymous-pipe capacity. The real child deliberately
        // never reads stdin, so the sole owner blocks inside write_all.
        assert!(proxy.request("x".repeat(8 * 1024 * 1024)).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(proxy);
        empty_registry();
    }

    #[test]
    fn real_helper_drop_never_waits_for_child_lock() {
        let _guard = serial_test();
        let proxy = start("echo", "A");
        let slot = proxy.slot.clone();
        let held = slot.child.lock().unwrap();
        let start = Instant::now();
        drop(proxy);
        assert!(start.elapsed() < Duration::from_millis(100));
        drop(held);
        empty_registry();
    }

    #[test]
    fn real_helper_malformed_command_and_disconnect_retire_stream() {
        let _guard = serial_test();
        for mode in ["request-malformed", "request-eof"] {
            let proxy = start(mode, "A");
            assert!(proxy.usb_send(&[1]).is_err());
            assert!(!proxy.is_available());
            drop(proxy);
            empty_registry();
        }
    }

    #[test]
    fn real_helper_four_distinct_live_devices_remain_usable() {
        let _guard = serial_test();
        let helpers: Vec<_> = ["A", "B", "C", "D"]
            .into_iter()
            .map(|name| start("echo", name))
            .collect();
        assert_eq!(REGISTRY.lock().unwrap().entries.len(), 4);
        for (index, helper) in helpers.iter().enumerate() {
            let value = index as u8 + 10;
            assert_eq!(helper.usb_send(&[value]).unwrap()[0], value);
        }
        drop(helpers);
        empty_registry();
    }

    #[test]
    fn real_helper_allows_display_macropad_with_other_ergohaven_bluetooth_product() {
        let _guard = serial_test();
        let mut macropad = device("macropad");
        macropad.vendor_id = 0xE126;
        macropad.product_id = 0x0042;
        macropad.serial_number = "vial:f64c2b3c".into();
        let mut bluetooth = device("bluetooth");
        bluetooth.vendor_id = 0xE126;
        bluetooth.product_id = 0x00A1;
        bluetooth.serial_number = "AA:BB:CC:DD:EE:FF".into();
        bluetooth.bus_type = "Bluetooth".into();

        let macropad =
            HidProxy::start(&macropad, spec("echo", None, None), Duration::from_secs(3)).unwrap();
        let bluetooth =
            HidProxy::start(&bluetooth, spec("echo", None, None), Duration::from_secs(3)).unwrap();

        assert_eq!(macropad.usb_send(&[11]).unwrap()[0], 11);
        assert_eq!(bluetooth.usb_send(&[22]).unwrap()[0], 22);
        drop((macropad, bluetooth));
        empty_registry();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn real_helper_generic_usb_serial_separates_parents_but_keeps_retiring_alias_reserved() {
        let _guard = serial_test();
        let a_device = crate::device::test_usb_device("3-3", 5, 0x42);
        let b_device = crate::device::test_usb_device("3-2", 9, 0x42);
        let a = HidProxy::start(
            &a_device,
            spec("echo", None, Some(&a_device)),
            Duration::from_secs(3),
        )
        .unwrap();
        let b = HidProxy::start(
            &b_device,
            spec("echo", None, Some(&b_device)),
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(a.usb_send(&[11]).unwrap()[0], 11);
        assert_eq!(b.usb_send(&[22]).unwrap()[0], 22);
        assert!(reserve_target(&a.slot, b_device).is_err());

        let mut alias = crate::device::test_usb_device("3-3", 6, 0x42);
        alias.instance_token = alias.instance_token.replace(":1.1/", ":1.2/");
        reserve_target(&a.slot, alias.clone()).unwrap();
        alias.serial_number = "different-interface-hint".into();
        assert!(HidProxy::start(&alias, spec("echo", None, None), TEST_TIMEOUT).is_err());
        let slot = a.slot.clone();
        let held = slot.child.lock().unwrap();
        drop(a);
        assert!(HidProxy::start(&alias, spec("echo", None, None), TEST_TIMEOUT).is_err());
        let c_device = crate::device::test_usb_device("3-4", 10, 0x42);
        let c = HidProxy::start(
            &c_device,
            spec("echo", None, Some(&c_device)),
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(c.usb_send(&[33]).unwrap()[0], 33);
        assert_eq!(b.usb_send(&[44]).unwrap()[0], 44);
        drop(held);
        drop(b);
        drop(c);
        empty_registry();
    }

    #[test]
    fn real_helper_four_concurrent_startups_wait_for_credit_not_a_device_quota() {
        let _guard = serial_test();
        let workers: Vec<_> = ["A", "B", "C", "D"]
            .into_iter()
            .map(|name| std::thread::spawn(move || start("echo", name)))
            .collect();
        let helpers: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        {
            let registry = REGISTRY.lock().unwrap();
            assert_eq!(registry.entries.len(), 4);
            assert_eq!(registry.unsettled_count(), 0);
        }
        for helper in &helpers {
            assert_eq!(helper.usb_send(&[31]).unwrap()[0], 31);
        }
        drop(helpers);
        empty_registry();
    }

    #[test]
    fn real_helper_three_automatic_bridges_leave_room_for_a_selected_keyboard() {
        let _guard = serial_test();
        let mut bridges = Vec::new();
        for name in ["A", "B", "C"] {
            let hid = HidDevice {
                backend: super::super::HidBackend::Proxy(Arc::new(start("echo", name))),
            };
            let output = hid.shared_output().unwrap();
            let (bridge, release, finished) =
                crate::qmk_hid_host::test_bridge_holding_transport(hid);
            bridges.push((bridge, release, finished, output));
        }
        let selected = start("echo", "D");
        assert_eq!(REGISTRY.lock().unwrap().entries.len(), 4);
        for (_, _, _, output) in &bridges {
            output.write_output_report(&[0xac, 1]).unwrap();
        }
        assert_eq!(selected.usb_send(&[32]).unwrap()[0], 32);
        for (bridge, _, _, _) in &mut bridges {
            bridge.stop();
        }
        // Bulk retirement of existing healthy owners is legal. It does not pin
        // their physical reservations on unrelated blocked desktop queries.
        await_condition(|| REGISTRY.lock().unwrap().entries.len() == 1);
        assert_eq!(selected.usb_send(&[33]).unwrap()[0], 33);
        for (_, release, finished, output) in bridges {
            assert!(!finished.load(Ordering::Acquire));
            assert!(!output.is_available());
            release.send(()).unwrap();
            await_condition(|| finished.load(Ordering::Acquire));
        }
        drop(selected);
        empty_registry();
    }

    #[test]
    fn real_helper_failed_startups_cannot_accumulate_unreclaimed_workers() {
        let _guard = serial_test();
        let gate = Arc::new(AtomicBool::new(false));
        for name in ["failed-A", "failed-B"] {
            assert!(HidProxy::start(
                &device(name),
                spec("startup-silence", Some(gate.clone()), None),
                Duration::from_millis(80),
            )
            .is_err());
        }
        await_condition(|| {
            let registry = REGISTRY.lock().unwrap();
            registry
                .entries
                .iter()
                .all(|entry| entry.slot.reaped.load(Ordering::Acquire))
        });
        for index in 0..12 {
            let fresh_target = device(&format!("failed-retry-{index}"));
            assert!(HidProxy::start(
                &fresh_target,
                spec("echo", None, None),
                Duration::from_millis(30),
            )
            .is_err());
            let registry = REGISTRY.lock().unwrap();
            assert_eq!(registry.entries.len(), MAX_UNSETTLED_HELPERS);
            assert_eq!(registry.unsettled_count(), MAX_UNSETTLED_HELPERS);
            assert!(registry.entries.iter().all(|entry| {
                !entry.slot.established.load(Ordering::Acquire)
                    && !entry.slot.worker_joined.load(Ordering::Acquire)
            }));
        }
        gate.store(true, Ordering::Release);
        empty_registry();
        let recovered = start("echo", "recovered");
        assert_eq!(recovered.usb_send(&[34]).unwrap()[0], 34);
        drop(recovered);
        empty_registry();
    }

    #[test]
    fn real_helper_distinct_b_connects_while_a_is_blocked() {
        let _guard = serial_test();
        let mut a = start("read-block", "A");
        a.timeout = Some(Duration::from_secs(2));
        let slot = a.slot.clone();
        let worker = std::thread::spawn(move || {
            assert!(a.usb_send(&[1]).is_err());
        });
        std::thread::sleep(Duration::from_millis(50));
        let b = start("echo", "B");
        assert!(
            !slot.retired.load(Ordering::Acquire),
            "B must open before A's deadline"
        );
        assert_eq!(b.usb_send(&[42]).unwrap()[0], 42);
        let c = start("echo", "C");
        assert_eq!(c.usb_send(&[43]).unwrap()[0], 43);
        drop(c);
        drop(b);
        worker.join().unwrap();
        empty_registry();
    }

    #[test]
    fn real_helper_budget_retains_reaped_child_until_io_worker_exits() {
        let _guard = serial_test();
        let gate = Arc::new(AtomicBool::new(false));
        let a = HidProxy::start(
            &device("A"),
            spec("echo", Some(gate.clone()), None),
            Duration::from_secs(3),
        )
        .unwrap();
        let slot_a = a.slot.clone();
        drop(a);
        await_condition(|| slot_a.reaped.load(Ordering::Acquire));
        assert!(HidProxy::start(&device("A"), spec("echo", None, None), TEST_TIMEOUT).is_err());
        let b = start("echo", "B");
        let c = HidProxy::start(
            &device("C"),
            spec("echo", Some(gate.clone()), None),
            Duration::from_secs(3),
        )
        .unwrap();
        let slot_c = c.slot.clone();
        drop(c);
        await_condition(|| slot_c.reaped.load(Ordering::Acquire));
        assert!(!slot_a.worker_joined.load(Ordering::Acquire));
        assert!(!slot_c.worker_joined.load(Ordering::Acquire));
        for _ in 0..8 {
            assert!(HidProxy::start(
                &device("D"),
                spec("echo", None, None),
                Duration::from_millis(40)
            )
            .is_err());
            let registry = REGISTRY.lock().unwrap();
            assert_eq!(registry.entries.len(), 3); // live B + two retirement debts
            assert_eq!(registry.unsettled_count(), MAX_UNSETTLED_HELPERS);
        }
        assert_eq!(b.usb_send(&[19]).unwrap()[0], 19);
        gate.store(true, Ordering::Release);
        drop(b);
        empty_registry();
        drop(start("echo", "A"));
        empty_registry();
    }

    #[test]
    fn real_helper_same_and_ambiguous_devices_are_excluded() {
        let _guard = serial_test();
        let a = start("echo", "A");
        let mut alias = device("A");
        alias.path = "other-path".into();
        alias.instance_token = "other-instance".into();
        assert!(HidProxy::start(&alias, spec("echo", None, None), TEST_TIMEOUT).is_err());
        let mut ambiguous = device("B");
        ambiguous.serial_number.clear();
        assert!(HidProxy::start(&ambiguous, spec("echo", None, None), TEST_TIMEOUT).is_err());
        let mut endpoint = device("B");
        endpoint.path = device("A").path;
        assert!(HidProxy::start(&endpoint, spec("echo", None, None), TEST_TIMEOUT).is_err());
        drop(a);
        empty_registry();
    }

    #[test]
    fn real_helper_actual_fallback_target_is_reserved_before_open() {
        let _guard = serial_test();
        let a = start("echo", "A");
        // B's serial is distinct, but fallback would reuse A's endpoint. The
        // actual-target claim must be rejected before the fake transport opens.
        let mut target = device("B");
        target.path = device("A").path;
        assert!(HidProxy::start(
            &device("B"),
            spec("echo", None, Some(&target)),
            Duration::from_secs(3)
        )
        .is_err());
        assert_eq!(a.usb_send(&[10]).unwrap()[0], 10);
        drop(a);
        empty_registry();
    }

    #[test]
    fn real_helper_claim_rejects_serial_substitution_and_tracks_aliases() {
        let _guard = serial_test();
        let mut impostor = device("B");
        impostor.path = device("A").path;
        assert!(HidProxy::start(
            &device("A"),
            spec("echo", None, Some(&impostor)),
            Duration::from_secs(3)
        )
        .is_err());
        empty_registry();
        let mut target = device("A");
        target.path = "fallback-alias".into();
        target.instance_token = "fallback-instance".into();
        let a = HidProxy::start(
            &device("A"),
            spec("echo", None, Some(&target)),
            Duration::from_secs(3),
        )
        .unwrap();
        let mut b = device("B");
        b.path = target.path;
        assert!(HidProxy::start(&b, spec("echo", None, None), TEST_TIMEOUT).is_err());
        drop(a);
        empty_registry();
    }

    #[test]
    fn real_helper_late_a_reply_is_inert_and_b_keeps_its_own_stream() {
        let _guard = serial_test();
        let a = start("late", "A");
        // Hold only the reaper's child handle to let a genuine delayed reply
        // arrive AFTER timeout. IO itself does not need this mutex.
        let slot = a.slot.clone();
        let held = slot.child.lock().unwrap();
        assert!(a.usb_send(&[11]).is_err());
        let b = start("echo", "B");
        assert_eq!(b.usb_send(&[22]).unwrap()[0], 22);
        std::thread::sleep(Duration::from_millis(700));
        assert!(a.usb_send(&[33]).is_err());
        assert_eq!(b.usb_send(&[44]).unwrap()[0], 44);
        drop(held);
        drop(a);
        drop(b);
        empty_registry();
    }

    #[test]
    fn real_backend_timeout_retires_stream_and_keeps_slot_until_child_reaped() {
        let _guard = serial_test();
        let a = start("backend-timeout", "A");
        let slot = a.slot.clone();
        // Prevent killing/reaping only; the actual error frame and late reply
        // still travel over real child pipes. The IPC worker may terminate.
        let held = slot.child.lock().unwrap();
        let started = Instant::now();
        let error = a.usb_send(&[11]).unwrap_err().to_string();
        assert!(error.contains("Bluetooth Vial timeout"));
        assert!(started.elapsed() < TEST_TIMEOUT);
        await_condition(|| slot.worker_joined.load(Ordering::Acquire));
        assert!(!slot.reaped.load(Ordering::Acquire));
        assert!(!a.is_available());
        assert!(HidProxy::start(&device("A"), spec("echo", None, None), TEST_TIMEOUT).is_err());
        let b = start("echo", "B");
        let c = start("echo", "C");
        assert_eq!(c.usb_send(&[45]).unwrap()[0], 45);
        std::thread::sleep(Duration::from_millis(650));
        assert!(a.usb_send(&[33]).is_err());
        assert_eq!(b.usb_send(&[44]).unwrap()[0], 44);
        drop(held);
        drop(a);
        drop(b);
        drop(c);
        empty_registry();
        assert!(slot.reaped.load(Ordering::Acquire));
    }

    #[test]
    fn real_helper_shared_output_and_queries_use_one_ipc_owner() {
        let _guard = serial_test();
        let proxy = start("echo", "A");
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(Arc::new(proxy)),
        };
        let output = hid.shared_output().unwrap();
        output.write_output_report(&[0xac, 1]).unwrap();
        assert_eq!(hid.usb_send(&[7]).unwrap()[0], 7);
        assert_eq!(REGISTRY.lock().unwrap().entries.len(), 1);
        drop(hid);
        assert!(!output.is_available());
        assert!(output.write_output_report(&[1]).is_err());
        empty_registry();
    }

    #[test]
    fn real_helper_bridge_retirement_releases_endpoint_while_desktop_query_is_blocked() {
        let _guard = serial_test();
        let proxy = start("echo", "A");
        let slot = proxy.slot.clone();
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(Arc::new(proxy)),
        };
        let (mut bridge, release_query, query_finished) =
            crate::qmk_hid_host::test_bridge_holding_transport(hid);
        let started = Instant::now();
        bridge.stop();
        assert!(started.elapsed() < Duration::from_millis(100));
        // No empty_registry() or sleep before replacement: a real UI handoff
        // races the reaper and must await retirement within its startup bound.
        let replacement = start("echo", "A");
        assert!(slot.reaped.load(Ordering::Acquire));
        assert!(!query_finished.load(Ordering::Acquire));
        // The stale desktop-query thread still owns its old HidDevice, but its
        // revoked proxy no longer reserves A or consumes the global helper cap.
        assert_eq!(replacement.usb_send(&[42]).unwrap()[0], 42);
        release_query.send(()).unwrap();
        await_condition(|| query_finished.load(Ordering::Acquire));
        drop(replacement);
        empty_registry();
    }

    fn host_target() -> Device {
        let mut target = device("A");
        // Presence check only; all traffic uses the supplied real fake-helper proxy.
        target.path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/hid_proxy.rs")
            .to_string_lossy()
            .into_owned();
        target
    }

    fn clock_mode() -> crate::qmk_hid_host::HostDataMode {
        crate::qmk_hid_host::HostDataMode {
            time: true,
            ..Default::default()
        }
    }

    fn output_commands(slot: &Slot) -> Vec<u8> {
        slot.completed_output
            .lock()
            .unwrap()
            .iter()
            .map(|p| p[0])
            .collect()
    }

    #[test]
    fn combined_automatic_selected_automatic_handoff_requeries_each_dedicated_owner() {
        use crate::qmk_hid_host::{
            supports_extended_host_protocol, test_start_bridge, HostProtocol,
        };
        let _guard = serial_test();
        let first = Arc::new(start("host-extended", "A"));
        let first_slot = first.slot.clone();
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(first.clone()),
        };
        let mut automatic = test_start_bridge(
            host_target(),
            clock_mode(),
            None,
            Some(hid),
            HostProtocol::Discover,
            || None,
        );
        await_condition(|| output_commands(&first_slot).contains(&0xAF));
        assert_eq!(
            first_slot
                .submitted
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.starts_with("fe09"))
                .count(),
            1
        );
        let before = Instant::now();
        automatic.stop();
        assert!(before.elapsed() < Duration::from_millis(100));
        // No pre-wait: replacement admission waits for real reaping/IPC retirement.
        let selected = Arc::new(start("host-extended", "A"));
        assert!(first_slot.reaped.load(Ordering::Acquire));
        assert!(!first.is_available());
        let keyboard = HidDevice {
            backend: super::super::HidBackend::Proxy(selected.clone()),
        };
        let supported = supports_extended_host_protocol(&keyboard.query_qmk_settings().unwrap());
        assert!(supported);
        let mut bridge = test_start_bridge(
            host_target(),
            clock_mode(),
            keyboard.shared_output(),
            None,
            HostProtocol::Selected(supported),
            || None,
        );
        await_condition(|| output_commands(&selected.slot).contains(&0xAF));
        assert_eq!(
            selected
                .slot
                .submitted
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.starts_with("fe09"))
                .count(),
            1,
            "shared consumer must not renegotiate"
        );
        bridge.stop();
        await_condition(|| {
            selected
                .slot
                .completed_output
                .lock()
                .unwrap()
                .last()
                .is_some_and(|p| p.starts_with(&[0xBA, 0]))
        });
        assert!(
            selected.is_available(),
            "shared stop retired selected transport"
        );
        assert_eq!(keyboard.usb_send(&[77]).unwrap()[0], 77);
        drop(keyboard);
        drop(selected);
        drop(bridge);
        empty_registry();
        let last = Arc::new(start("host-extended", "A"));
        let last_slot = last.slot.clone();
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(last.clone()),
        };
        let mut again = test_start_bridge(
            host_target(),
            clock_mode(),
            None,
            Some(hid),
            HostProtocol::Discover,
            || None,
        );
        await_condition(|| output_commands(&last_slot).contains(&0xAF));
        assert_eq!(
            last_slot
                .submitted
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.starts_with("fe09"))
                .count(),
            1
        );
        again.stop();
        empty_registry();
        assert!(!last.is_available());
    }

    #[test]
    fn combined_stop_during_discovery_retires_before_query_can_finish() {
        use crate::qmk_hid_host::{test_start_bridge, HostProtocol};
        let _guard = serial_test();
        let mut proxy = start("discovery-block", "A");
        proxy.timeout = Some(Duration::from_secs(5));
        let proxy = Arc::new(proxy);
        let slot = proxy.slot.clone();
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(proxy.clone()),
        };
        let mut bridge = test_start_bridge(
            host_target(),
            clock_mode(),
            None,
            Some(hid),
            HostProtocol::Discover,
            || panic!("clock bridge queried media"),
        );
        await_condition(|| {
            slot.submitted
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.starts_with("fe09"))
        });
        let before = Instant::now();
        bridge.stop();
        assert!(
            before.elapsed() < Duration::from_millis(100),
            "stop waited for discovery"
        );
        let replacement = start("echo", "A");
        assert!(
            before.elapsed() < Duration::from_secs(3),
            "replacement waited for old five-second query deadline"
        );
        assert!(slot.reaped.load(Ordering::Acquire));
        assert!(!proxy.is_available());
        assert!(
            output_commands(&slot).is_empty(),
            "unconfirmed discovery emitted host data"
        );
        assert_eq!(replacement.usb_send(&[42]).unwrap()[0], 42);
        drop(replacement);
        empty_registry();
    }

    #[test]
    fn combined_retired_selected_proxy_is_fail_closed_even_while_strong_owner_survives() {
        let _guard = serial_test();
        let proxy = Arc::new(start("echo", "A"));
        let hid = HidDevice {
            backend: super::super::HidBackend::Proxy(proxy.clone()),
        };
        let output = hid.shared_output().unwrap();
        proxy.retirement_handle().retire();
        empty_registry();
        assert!(!output.is_available());
        assert!(output.write_output_report(&[0xBA, 1]).is_err());
        let error =
            crate::qmk_hid_host::test_open_selected_owner(&host_target(), &output).unwrap_err();
        assert!(error.to_string().contains("owner is no longer available"));
        assert!(
            REGISTRY.lock().unwrap().entries.is_empty(),
            "expired shared owner was reopened"
        );
        assert!(hid.usb_send(&[7]).is_err());
    }

    #[test]
    fn combined_shared_generation_fencing_and_final_shutdown_use_one_proxy_owner() {
        use crate::qmk_hid_host::{test_start_bridge, HostDataMode, HostProtocol};
        let _guard = serial_test();
        let proxy = Arc::new(start("echo", "A"));
        let slot = proxy.slot.clone();
        // Two wrappers of the SAME physical proxy must share the ordering domain.
        let first = HidDevice {
            backend: super::super::HidBackend::Proxy(proxy.clone()),
        };
        let second = HidDevice {
            backend: super::super::HidBackend::Proxy(proxy.clone()),
        };
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        struct Finished(mpsc::Sender<()>);
        impl Drop for Finished {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let finished = Finished(done_tx);
        let mut old = test_start_bridge(
            host_target(),
            HostDataMode {
                media: true,
                ..clock_mode()
            },
            first.shared_output(),
            None,
            HostProtocol::Selected(true),
            move || {
                let _keep_alive = &finished;
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Some(("old artist".into(), "old title".into()))
            },
        );
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let before = output_commands(&slot).len();
        old.stop();
        let mut next = test_start_bridge(
            host_target(),
            clock_mode(),
            second.shared_output(),
            None,
            HostProtocol::Selected(true),
            || None,
        );
        await_condition(|| output_commands(&slot)[before..].contains(&0xAF));
        let reports = output_commands(&slot);
        assert_eq!(&reports[before..], &[0xAD, 0xAE, 0xBA, 0xAA, 0xAF]);
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            output_commands(&slot),
            reports,
            "retired host consumer wrote after successor AF"
        );
        assert!(proxy.is_available());
        assert_eq!(first.usb_send(&[11]).unwrap()[0], 11);
        assert_eq!(second.usb_send(&[12]).unwrap()[0], 12);
        next.stop();
        await_condition(|| {
            slot.completed_output
                .lock()
                .unwrap()
                .last()
                .is_some_and(|p| p.starts_with(&[0xBA, 0]))
        });
        assert!(
            proxy.is_available(),
            "final shared shutdown revoked keyboard"
        );
        drop(first);
        drop(second);
        drop(proxy);
        drop(old);
        drop(next);
        empty_registry();
    }

    #[test]
    fn real_helper_repeated_timeouts_do_not_accumulate_threads_or_slots() {
        let _guard = serial_test();
        for _ in 0..8 {
            assert!(HidProxy::start(
                &device("A"),
                spec("startup-silence", None, None),
                Duration::from_millis(40)
            )
            .is_err());
            empty_registry();
        }
        let registry = REGISTRY.lock().unwrap();
        assert!(registry.reaper_started);
        assert!(registry.entries.is_empty());
    }
}
