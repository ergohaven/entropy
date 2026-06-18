/// Vial protocol implementation over HID.
/// Based on vial-gui Python source: protocol/keyboard_comm.py
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::io::{BufRead, BufReader, Write};

#[cfg(target_os = "windows")]
use std::process::{Child, ChildStdin, Command, Stdio};

#[cfg(target_os = "windows")]
use std::sync::{mpsc, Mutex};

#[path = "hid_protocol.rs"]
mod hid_protocol;
use hid_protocol::*;

/// hidapi's macOS backend uses a process-global IOHIDManager. Concurrent
/// enumeration while another thread holds an open device can crash on macOS 26.
#[cfg(target_os = "macos")]
pub(crate) fn macos_hid_operation_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_hid_scan_disabled_for_rosetta() -> bool {
    macos_running_under_rosetta()
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn macos_running_under_rosetta() -> bool {
    use std::os::raw::{c_char, c_int, c_void};

    extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }

    let mut translated = 0i32;
    let mut len = std::mem::size_of_val(&translated);
    let rc = unsafe {
        sysctlbyname(
            b"sysctl.proc_translated\0".as_ptr().cast(),
            (&mut translated as *mut i32).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };

    rc == 0 && translated == 1
}

#[cfg(all(target_os = "macos", not(target_arch = "x86_64")))]
fn macos_running_under_rosetta() -> bool {
    false
}

const VIAL_GUI_USB_RETRIES: usize = 20;
const VIAL_GUI_READ_TIMEOUT_MS: i32 = 500;
const WINDOWS_BLE_READ_TIMEOUT_MS: i32 = 2_500;
const WINDOWS_BLE_READ_SLICE_MS: i32 = 250;
const WINDOWS_BLE_SETTLE_DELAY: Duration = Duration::from_millis(12);
#[cfg(target_os = "windows")]
const WINDOWS_HID_HELPER_USB_COMMAND_TIMEOUT: Duration = Duration::from_millis(1_500);
#[cfg(target_os = "windows")]
const WINDOWS_HID_HELPER_BLE_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const VIAL_GUI_RETRY_DELAY: Duration = Duration::from_millis(500);
const HID_OPEN_RETRIES: usize = 5;
const HID_OPEN_RETRY_DELAY: Duration = Duration::from_millis(250);

#[path = "hid_parse.rs"]
mod hid_parse;

#[path = "hid_dynamic.rs"]
mod hid_dynamic;

#[path = "hid_macros.rs"]
mod hid_macros;

#[path = "hid_keymap.rs"]
mod hid_keymap;

#[path = "hid_settings.rs"]
mod hid_settings;

#[path = "hid_vial.rs"]
mod hid_vial;

#[cfg(not(target_arch = "wasm32"))]
pub struct HidDevice {
    backend: HidBackend,
}

#[cfg(not(target_arch = "wasm32"))]
enum HidBackend {
    Local {
        device: hidapi::HidDevice,
        transport: HidTransport,
        path: Option<PathBuf>,
    },
    #[cfg(target_os = "windows")]
    Proxy(HidProxy),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HidTransport {
    Usb,
    Bluetooth,
}

#[cfg(not(target_arch = "wasm32"))]
impl HidTransport {
    fn is_bluetooth(self) -> bool {
        matches!(self, Self::Bluetooth)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    pub fn is_bluetooth_transport(&self) -> bool {
        match &self.backend {
            HidBackend::Local { transport, .. } => transport.is_bluetooth(),
            #[cfg(target_os = "windows")]
            HidBackend::Proxy(proxy) => proxy.is_bluetooth_transport(),
        }
    }
}

#[cfg(target_os = "windows")]
struct HidProxy {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    rx: Mutex<mpsc::Receiver<String>>,
    transport: HidTransport,
}

#[cfg(target_os = "windows")]
#[derive(serde::Serialize, serde::Deserialize)]
struct ProxyResponse {
    ok: bool,
    data: Option<String>,
    error: Option<String>,
}

#[cfg(target_os = "windows")]
impl Drop for HidProxy {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_disconnect_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let message = cause.to_string().to_ascii_lowercase();
        message.contains("disconnected")
            || message.contains("device did not respond")
            || message.contains("hid helper timed out")
            || message.contains("failed to write hid helper request")
            || message.contains("failed to flush hid helper request")
            || message.contains("hid write failed")
            || message.contains("hid read failed")
            || message.contains("broken pipe")
            || message.contains("pipe is being closed")
            || message.contains("the device is not connected")
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn device_info_matches(
    info: &hidapi::DeviceInfo,
    device: &crate::device::Device,
    strict_identity: bool,
) -> bool {
    if info.usage_page() != 0xFF60
        || info.usage() != 0x61
        || info.vendor_id() != device.vendor_id
        || info.product_id() != device.product_id
    {
        return false;
    }

    if !strict_identity {
        return true;
    }

    let serial_matches = !device.serial_number.is_empty()
        && info
            .serial_number()
            .map(|serial| serial == device.serial_number)
            .unwrap_or(false);
    let product_matches = info
        .product_string()
        .map(|product| product == device.name)
        .unwrap_or(false);
    let manufacturer_matches = device.manufacturer.is_empty()
        || info
            .manufacturer_string()
            .map(|manufacturer| manufacturer == device.manufacturer)
            .unwrap_or(false);

    serial_matches || (product_matches && manufacturer_matches)
}

#[cfg(not(target_arch = "wasm32"))]
impl HidDevice {
    pub fn open(path: &str) -> Result<Self> {
        #[cfg(target_os = "macos")]
        let _hid_lock = macos_hid_operation_lock();
        let api = hidapi::HidApi::new().context("Failed to init hidapi")?;
        let device = api
            .open_path(&std::ffi::CString::new(path)?)
            .context("Failed to open HID device")?;
        Ok(Self {
            backend: HidBackend::Local {
                device,
                transport: HidTransport::Usb,
                path: Some(PathBuf::from(path)),
            },
        })
    }

    pub fn open_fresh_for(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "windows")]
        {
            return Self::open_proxy_for(device);
        }

        #[cfg(not(target_os = "windows"))]
        {
            Self::open_fresh_for_local(device)
        }
    }

    fn open_fresh_for_local(device: &crate::device::Device) -> Result<Self> {
        let mut last_error = None;
        for attempt in 0..HID_OPEN_RETRIES {
            match Self::try_open_fresh_for(device) {
                Ok(device) => return Ok(device),
                Err(e) => {
                    last_error = Some(e);
                    if attempt + 1 < HID_OPEN_RETRIES {
                        std::thread::sleep(HID_OPEN_RETRY_DELAY);
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("unable to open the device")))
    }

    #[cfg(target_os = "windows")]
    fn open_proxy_for(device: &crate::device::Device) -> Result<Self> {
        let exe = std::env::current_exe().context("Failed to find Entropy executable")?;
        let device_json =
            serde_json::to_string(device).context("Failed to serialize HID device")?;
        let mut child = Command::new(exe)
            .arg("--entropy-hid-proxy")
            .arg(device_json)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start HID helper")?;

        let stdin = child.stdin.take().context("HID helper stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("HID helper stdout unavailable")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let ready_line = match rx.recv_timeout(Duration::from_secs(12)) {
            Ok(line) => line,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("HID helper timed out while opening device");
            }
        };
        let ready: ProxyResponse = serde_json::from_str(&ready_line)
            .context("HID helper returned malformed startup response")?;
        if !ready.ok {
            let _ = child.kill();
            let _ = child.wait();
            bail!(ready
                .error
                .unwrap_or_else(|| "HID helper failed to open device".to_owned()));
        }

        Ok(Self {
            backend: HidBackend::Proxy(HidProxy {
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                rx: Mutex::new(rx),
                transport: device_transport(device),
            }),
        })
    }

    fn try_open_fresh_for(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "macos")]
        let _hid_lock = macos_hid_operation_lock();
        let api = hidapi::HidApi::new().context("Failed to init hidapi")?;

        if !device.path.is_empty() {
            if let Ok(path) = std::ffi::CString::new(device.path.as_str()) {
                match api.open_path(&path) {
                    Ok(hid_device) => {
                        return Ok(Self {
                            backend: HidBackend::Local {
                                device: hid_device,
                                transport: device_transport(device),
                                path: local_hid_path(device),
                            },
                        });
                    }
                    Err(e) => {
                        log::debug!("direct HID path open failed, falling back to scan: {e}");
                    }
                }
            }
        }

        for info in api.device_list() {
            if !device_info_matches(info, device, true) {
                continue;
            }
            return info
                .open_device(&api)
                .map(|hid_device| Self {
                    backend: HidBackend::Local {
                        device: hid_device,
                        transport: device_transport(device),
                        path: local_hid_path(device),
                    },
                })
                .context("Failed to open HID device");
        }

        for info in api.device_list() {
            if !device_info_matches(info, device, false) {
                continue;
            }
            return info
                .open_device(&api)
                .map(|hid_device| Self {
                    backend: HidBackend::Local {
                        device: hid_device,
                        transport: device_transport(device),
                        path: local_hid_path(device),
                    },
                })
                .context("Failed to open HID device");
        }

        anyhow::bail!("HID device disappeared during reconnect")
    }

    /// Send exactly MSG_LEN bytes (with 0x00 report ID prepended), receive MSG_LEN bytes back.
    fn usb_send(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
        match &self.backend {
            HidBackend::Local {
                device,
                transport,
                path,
            } => usb_send_local(device, *transport, path.as_deref(), data),
            #[cfg(target_os = "windows")]
            HidBackend::Proxy(proxy) => proxy.usb_send(data),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn device_transport(device: &crate::device::Device) -> HidTransport {
    #[cfg(target_os = "windows")]
    {
        if device.is_bluetooth_transport() {
            return HidTransport::Bluetooth;
        }
    }
    HidTransport::Usb
}

#[cfg(not(target_arch = "wasm32"))]
fn local_hid_path(device: &crate::device::Device) -> Option<PathBuf> {
    (!device.path.is_empty()).then(|| PathBuf::from(&device.path))
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_hid_path_present(path: Option<&Path>) -> Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(path) = path {
        if !path.exists() {
            bail!("HID device disconnected");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = path;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn usb_send_local(
    device: &hidapi::HidDevice,
    transport: HidTransport,
    path: Option<&Path>,
    data: &[u8],
) -> Result<[u8; MSG_LEN]> {
    ensure_hid_path_present(path)?;

    if data.len() > MSG_LEN {
        bail!(
            "HID command too long — {} bytes, max {} bytes",
            data.len(),
            MSG_LEN
        );
    }

    let mut write_buf = [0u8; MSG_LEN + 1];
    write_buf[0] = 0x00; // hidapi report ID, exactly like vial-gui
    write_buf[1..1 + data.len()].copy_from_slice(data);

    let read_timeout_ms = if transport.is_bluetooth() {
        WINDOWS_BLE_READ_TIMEOUT_MS
    } else {
        VIAL_GUI_READ_TIMEOUT_MS
    };

    let max_retries = if transport.is_bluetooth() {
        1
    } else {
        VIAL_GUI_USB_RETRIES
    };

    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 0..max_retries {
        ensure_hid_path_present(path)?;

        if attempt > 0 {
            std::thread::sleep(if transport.is_bluetooth() {
                WINDOWS_BLE_SETTLE_DELAY
            } else {
                VIAL_GUI_RETRY_DELAY
            });
            ensure_hid_path_present(path)?;
        }

        if transport.is_bluetooth() {
            drain_pending_reports(device);
        }

        match device.write(&write_buf) {
            Ok(bytes_written) if bytes_written == write_buf.len() => {}
            Ok(bytes_written) => {
                last_error = Some(anyhow::anyhow!(
                    "HID short write — wrote {} bytes, expected {} bytes",
                    bytes_written,
                    write_buf.len()
                ));
                continue;
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("HID write failed: {e}"));
                continue;
            }
        }

        match read_response(device, transport, data, read_timeout_ms) {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("failed to communicate with the device")))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_response(
    device: &hidapi::HidDevice,
    transport: HidTransport,
    command: &[u8],
    timeout_ms: i32,
) -> Result<[u8; MSG_LEN]> {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.max(1) as u64);
    let mut last_error: Option<anyhow::Error> = None;

    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }

        let remaining_ms = deadline.saturating_duration_since(now).as_millis().max(1) as i32;
        let read_timeout = if transport.is_bluetooth() {
            remaining_ms.min(WINDOWS_BLE_READ_SLICE_MS)
        } else {
            remaining_ms
        };

        let mut read_buf = [0u8; MSG_LEN + 1];
        let bytes_read = match device.read_timeout(&mut read_buf, read_timeout) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                return Err(anyhow::anyhow!("HID read failed: {e}"));
            }
        };

        if bytes_read == 0 {
            last_error = Some(anyhow::anyhow!("HID timeout — device did not respond"));
            continue;
        }
        if bytes_read != MSG_LEN && bytes_read != MSG_LEN + 1 {
            last_error = Some(anyhow::anyhow!(
                "HID invalid response length — read {} bytes, expected {} or {} bytes",
                bytes_read,
                MSG_LEN,
                MSG_LEN + 1
            ));
            if transport.is_bluetooth() {
                continue;
            }
            break;
        }

        let mut resp = [0u8; MSG_LEN];
        if bytes_read == MSG_LEN + 1 {
            resp.copy_from_slice(&read_buf[1..MSG_LEN + 1]);
        } else {
            resp.copy_from_slice(&read_buf[..MSG_LEN]);
        }

        if !transport.is_bluetooth() || response_matches_command(command, &resp) {
            return Ok(resp);
        }

        last_error = Some(anyhow::anyhow!(
            "HID stale or unrelated BLE report for command {:02X}: {:02X?}",
            command.first().copied().unwrap_or(0),
            &resp[..command.len().min(8).max(3)]
        ));
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HID timeout — device did not respond")))
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_command(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    let Some(&cmd) = command.first() else {
        return false;
    };

    match cmd {
        CMD_VIA_GET_PROTOCOL_VERSION => {
            resp[0] == CMD_VIA_GET_PROTOCOL_VERSION
                && matches!(u16::from_be_bytes([resp[1], resp[2]]), 9 | 0xFFFF)
        }
        CMD_VIA_GET_LAYER_COUNT => {
            resp[0] == CMD_VIA_GET_LAYER_COUNT && (1..=32).contains(&resp[1])
        }
        CMD_VIA_KEYMAP_GET_BUFFER | CMD_VIA_MACRO_GET_BUFFER => {
            command.len() >= 4 && resp[..4] == command[..4]
        }
        CMD_VIA_MACRO_GET_COUNT | CMD_VIA_MACRO_GET_BUFFER_SIZE => resp[0] == cmd,
        CMD_VIA_GET_KEYBOARD_VALUE | CMD_VIA_LIGHTING_GET_VALUE => {
            command.len() >= 2 && resp[0] == cmd && resp[1] == command[1]
        }
        CMD_VIA_SET_KEYBOARD_VALUE
        | CMD_VIA_SET_KEYCODE
        | CMD_VIA_LIGHTING_SET_VALUE
        | CMD_VIA_LIGHTING_SAVE
        | CMD_VIA_MACRO_SET_BUFFER => resp[0] == cmd,
        CMD_VIA_VIAL_PREFIX => response_matches_vial_command(command, resp),
        _ => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_vial_command(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    let Some(&subcommand) = command.get(1) else {
        return false;
    };

    match subcommand {
        CMD_VIAL_GET_KEYBOARD_ID => {
            let vial_protocol = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
            let keyboard_id = u64::from_le_bytes([
                resp[4], resp[5], resp[6], resp[7], resp[8], resp[9], resp[10], resp[11],
            ]);
            vial_protocol <= 6 && keyboard_id != 0 && keyboard_id != u64::MAX
        }
        CMD_VIAL_GET_SIZE => {
            let size = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
            (1..=2_000_000).contains(&size)
        }
        CMD_VIAL_GET_DEFINITION => {
            let block = command
                .get(2..6)
                .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .unwrap_or(0);
            block != 0 || resp.starts_with(&[0xFD, b'7', b'z', b'X', b'Z']) || resp[0] == 0x5D
        }
        CMD_VIAL_GET_UNLOCK_STATUS => matches!(resp[0], 0 | 1) && matches!(resp[1], 0 | 1),
        CMD_VIAL_UNLOCK_POLL => matches!(resp[0], 0 | 1) && matches!(resp[1], 0 | 1),
        CMD_VIAL_QMK_SETTINGS_GET
        | CMD_VIAL_QMK_SETTINGS_SET
        | CMD_VIAL_DYNAMIC_ENTRY_OP
        | CMD_VIAL_GET_ENCODER
        | CMD_VIAL_SET_ENCODER
        | CMD_VIAL_QMK_SETTINGS_QUERY
        | CMD_VIAL_UNLOCK_START
        | CMD_VIAL_LOCK => true,
        _ => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn drain_pending_reports(device: &hidapi::HidDevice) {
    let mut read_buf = [0u8; MSG_LEN + 1];
    for _ in 0..16 {
        match device.read_timeout(&mut read_buf, 0) {
            Ok(0) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

#[cfg(target_os = "windows")]
impl HidProxy {
    fn is_bluetooth_transport(&self) -> bool {
        self.transport.is_bluetooth()
    }

    fn command_timeout(&self) -> Duration {
        if self.transport.is_bluetooth() {
            WINDOWS_HID_HELPER_BLE_COMMAND_TIMEOUT
        } else {
            WINDOWS_HID_HELPER_USB_COMMAND_TIMEOUT
        }
    }

    fn kill_child(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.try_wait();
        }
    }

    fn request(&self, request: &str) -> Result<String> {
        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|_| anyhow::anyhow!("HID helper stdin lock poisoned"))?;
            writeln!(stdin, "{request}").context("Failed to write HID helper request")?;
            stdin
                .flush()
                .context("Failed to flush HID helper request")?;
        }

        let rx = self
            .rx
            .lock()
            .map_err(|_| anyhow::anyhow!("HID helper receiver lock poisoned"))?;
        match rx.recv_timeout(self.command_timeout()) {
            Ok(line) => Ok(line),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.kill_child();
                bail!("HID helper timed out during command");
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("HID helper disconnected during command");
            }
        }
    }

    fn usb_send(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
        if data.len() > MSG_LEN {
            bail!(
                "HID command too long — {} bytes, max {} bytes",
                data.len(),
                MSG_LEN
            );
        }

        let line = self.request(&bytes_to_hex(data))?;
        let response: ProxyResponse =
            serde_json::from_str(&line).context("HID helper returned malformed response")?;
        if !response.ok {
            bail!(response
                .error
                .unwrap_or_else(|| "HID helper command failed".to_owned()));
        }

        let bytes = hex_to_bytes(
            response
                .data
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("HID helper response missing data"))?,
        )?;
        if bytes.len() != MSG_LEN {
            bail!(
                "HID helper invalid response length — {} bytes, expected {}",
                bytes.len(),
                MSG_LEN
            );
        }
        let mut out = [0u8; MSG_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }
}

#[cfg(target_os = "windows")]
pub fn run_hid_proxy_if_requested() -> bool {
    let mut args = std::env::args();
    let _exe = args.next();
    if args.next().as_deref() != Some("--entropy-hid-proxy") {
        return false;
    }

    let result = (|| -> Result<()> {
        let device_json = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing HID helper device argument"))?;
        let device: crate::device::Device = serde_json::from_str(&device_json)
            .context("Failed to parse HID helper device argument")?;
        run_hid_proxy(device)
    })();

    if let Err(e) = result {
        let response = serde_json::to_string(&ProxyResponse {
            ok: false,
            data: None,
            error: Some(e.to_string()),
        })
        .unwrap_or_else(|_| {
            "{\"ok\":false,\"data\":null,\"error\":\"HID helper failed\"}".to_owned()
        });
        let _ = writeln!(std::io::stdout(), "{}", response);
        let _ = std::io::stdout().flush();
    }
    true
}

#[cfg(target_os = "windows")]
fn run_hid_proxy(device: crate::device::Device) -> Result<()> {
    let hid = HidDevice::open_fresh_for_local(&device)?;
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string(&ProxyResponse {
            ok: true,
            data: None,
            error: None,
        })?
    )?;
    std::io::stdout().flush()?;

    for line in BufReader::new(std::io::stdin()).lines() {
        let line = line?;
        let line = line.trim();
        let response = match hex_to_bytes(line).and_then(|data| hid.usb_send(&data)) {
            Ok(data) => ProxyResponse {
                ok: true,
                data: Some(bytes_to_hex(&data)),
                error: None,
            },
            Err(e) => ProxyResponse {
                ok: false,
                data: None,
                error: Some(e.to_string()),
            },
        };
        writeln!(std::io::stdout(), "{}", serde_json::to_string(&response)?)?;
        std::io::stdout().flush()?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn bytes_to_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for &byte in data {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

#[cfg(target_os = "windows")]
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        bail!("invalid hex length");
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

#[cfg(target_os = "windows")]
fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hex digit"),
    }
}
