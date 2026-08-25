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
pub(crate) mod hid_protocol;
use hid_protocol::*;

/// hidapi's macOS backend uses a process-global IOHIDManager. Concurrent
/// enumeration while another thread holds an open device can crash on macOS 26.
#[cfg(target_os = "macos")]
pub(crate) fn macos_hid_operation_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(target_os = "macos")]
pub(crate) fn initialize_macos_hid_on_main_thread() {
    if macos_hid_scan_disabled_for_rosetta() {
        return;
    }

    // hidapi's Darwin backend binds its global IOHIDManager to the first hid_init run loop.
    let _hid_lock = macos_hid_operation_lock();
    if let Err(error) = hidapi::HidApi::new() {
        log::warn!("macOS HID initialization failed: {error}");
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_hid_scan_disabled_for_rosetta() -> bool {
    macos_running_under_rosetta()
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_runtime_architecture_status() -> &'static str {
    if macos_running_under_rosetta() {
        "x86_64 translated by Rosetta on Apple Silicon"
    } else if cfg!(target_arch = "aarch64") {
        "native arm64 Apple Silicon"
    } else if cfg!(target_arch = "x86_64") {
        "native x86_64 Intel"
    } else {
        "native macOS architecture"
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_rosetta_hid_status_message() -> &'static str {
    "Entropy is running under Rosetta. Install the macOS arm64 build to enable HID access on Apple Silicon."
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
#[cfg(target_os = "linux")]
const LINUX_BLE_NOTIFICATION_PROBE_TIMEOUT_MS: i32 = 80;
#[cfg(target_os = "linux")]
const LINUX_BLE_UNCORRELATED_REPLY_SETTLE: Duration = Duration::from_millis(32);
#[cfg(target_os = "windows")]
const WINDOWS_HID_HELPER_USB_COMMAND_TIMEOUT: Duration = Duration::from_millis(1_500);
#[cfg(target_os = "windows")]
const WINDOWS_HID_HELPER_BLE_COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
#[cfg(target_os = "windows")]
const HID_PROXY_OUTPUT_PREFIX: &str = "output:";
const VIAL_GUI_RETRY_DELAY: Duration = Duration::from_millis(500);
const HID_OPEN_RETRIES: usize = 5;
const HID_OPEN_RETRY_DELAY: Duration = Duration::from_millis(250);
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const HID_REPORT_DESCRIPTOR_MAX: usize = 4_096;
#[cfg(target_os = "linux")]
const BLUETOOTH_HID_PLATFORM: &str = "Linux";
#[cfg(target_os = "macos")]
const BLUETOOTH_HID_PLATFORM: &str = "macOS";
#[cfg(target_os = "windows")]
const BLUETOOTH_HID_PLATFORM: &str = "Windows";

pub(crate) const MACOS_HID_INPUT_MONITORING_REQUIRED: &str =
    "macOS Input Monitoring permission is required for Bluetooth HID access. \
     Allow Entropy in System Settings → Privacy & Security → Input Monitoring, \
     then fully quit and reopen Entropy";

pub(crate) const fn is_supported_via_protocol(version: u16) -> bool {
    matches!(version, 9 | u16::MAX)
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct MacosHidInputMonitoringRequired;

#[cfg(target_os = "macos")]
impl std::fmt::Display for MacosHidInputMonitoringRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(MACOS_HID_INPUT_MONITORING_REQUIRED)
    }
}

#[cfg(target_os = "macos")]
impl std::error::Error for MacosHidInputMonitoringRequired {}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[derive(Debug)]
struct UnsafeBluetoothReportMap;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl std::fmt::Display for UnsafeBluetoothReportMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "Bluetooth firmware mixes unnumbered Vial data with numbered HID reports; \
             update the keyboard firmware before connecting",
        )
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl std::error::Error for UnsafeBluetoothReportMap {}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn is_unsafe_bluetooth_report_map(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.is::<UnsafeBluetoothReportMap>())
}

#[path = "hid_parse.rs"]
mod hid_parse;

#[path = "hid_dynamic.rs"]
mod hid_dynamic;

#[path = "hid_macros.rs"]
mod hid_macros;

#[path = "hid_keymap.rs"]
mod hid_keymap;
pub(crate) use hid_keymap::keycode_writeback_readback;

#[path = "hid_settings.rs"]
mod hid_settings;
pub(crate) use hid_settings::BatteryHalves;

#[path = "hid_vial.rs"]
mod hid_vial;

#[cfg(not(target_arch = "wasm32"))]
pub struct HidDevice {
    backend: HidBackend,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct TestHidRecorder {
    requests: std::sync::Arc<std::sync::Mutex<Vec<[u8; MSG_LEN]>>>,
}

#[cfg(test)]
impl TestHidRecorder {
    pub(crate) fn requests(&self) -> Vec<[u8; MSG_LEN]> {
        self.requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum TestHidFault {
    Disconnect,
    Timeout,
    WorkerPanic,
    IgnoreQmkSettingSet,
}

#[cfg(not(target_arch = "wasm32"))]
enum HidBackend {
    Local {
        device: hidapi::HidDevice,
        transport: HidTransport,
        write_framing: HidWriteFraming,
        path: Option<PathBuf>,
        input_report_polling: std::sync::atomic::AtomicBool,
    },
    #[cfg(target_os = "windows")]
    Proxy(std::sync::Arc<HidProxy>),
    #[cfg(target_os = "linux")]
    LinuxBle(crate::linux_ble::LinuxBleDevice),
    #[cfg(test)]
    Test {
        recorder: TestHidRecorder,
        combo: std::sync::Mutex<([u16; 4], u16)>,
        qmk_settings: std::sync::Mutex<std::collections::BTreeMap<u16, u16>>,
        fault_after_requests: std::sync::Mutex<Option<(usize, TestHidFault)>>,
    },
}

#[cfg(target_os = "linux")]
pub(crate) struct LinuxBluetoothHidWriter {
    device: hidapi::HidDevice,
    write_framing: HidWriteFraming,
    path: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
impl LinuxBluetoothHidWriter {
    pub(crate) fn open(device: &crate::device::Device) -> Result<Self> {
        let local = HidDevice::open_fresh_for_local(device)?;
        let HidBackend::Local {
            device,
            transport,
            write_framing,
            path,
            ..
        } = local.backend
        else {
            bail!("Linux Bluetooth HID writer did not open a local HID backend");
        };
        if !transport.is_bluetooth() {
            bail!("Linux Bluetooth HID writer opened a non-Bluetooth endpoint");
        }

        Ok(Self {
            device,
            write_framing,
            path,
        })
    }

    pub(crate) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        ensure_output_report_len(data)?;
        write_output_report_local(&self.device, self.write_framing, self.path.as_deref(), data)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HidTransport {
    Usb,
    Bluetooth,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HidWriteFraming {
    ReportIdPrefixed(u8),
    LinuxBluetoothUnnumbered,
}

#[cfg(not(target_arch = "wasm32"))]
impl HidWriteFraming {
    fn report_id(self) -> Option<u8> {
        match self {
            Self::ReportIdPrefixed(report_id) => Some(report_id),
            Self::LinuxBluetoothUnnumbered => None,
        }
    }
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
            #[cfg(target_os = "linux")]
            HidBackend::LinuxBle(_) => true,
            #[cfg(test)]
            HidBackend::Test { .. } => false,
        }
    }
}

#[cfg(target_os = "windows")]
struct HidProxy {
    request_lock: Mutex<()>,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    rx: Mutex<mpsc::Receiver<String>>,
    transport: HidTransport,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct SharedHidOutput {
    backend: SharedHidOutputBackend,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
enum SharedHidOutputBackend {
    #[cfg(target_os = "windows")]
    Proxy(std::sync::Weak<HidProxy>),
    #[cfg(test)]
    Test(TestHidRecorder),
    #[cfg(not(any(target_os = "windows", test)))]
    #[allow(dead_code)]
    Unavailable,
}

#[cfg(not(target_arch = "wasm32"))]
impl SharedHidOutput {
    pub(crate) fn is_available(&self) -> bool {
        match &self.backend {
            #[cfg(target_os = "windows")]
            SharedHidOutputBackend::Proxy(proxy) => proxy.strong_count() > 0,
            #[cfg(test)]
            SharedHidOutputBackend::Test(_) => true,
            #[cfg(not(any(target_os = "windows", test)))]
            SharedHidOutputBackend::Unavailable => false,
        }
    }

    pub(crate) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        ensure_output_report_len(data)?;
        match &self.backend {
            #[cfg(target_os = "windows")]
            SharedHidOutputBackend::Proxy(proxy) => proxy
                .upgrade()
                .context("Shared HID output owner is no longer available")?
                .write_output_report(data),
            #[cfg(test)]
            SharedHidOutputBackend::Test(recorder) => {
                record_test_output_report(recorder, data);
                Ok(())
            }
            #[cfg(not(any(target_os = "windows", test)))]
            SharedHidOutputBackend::Unavailable => {
                bail!("Shared HID output is unavailable on this platform")
            }
        }
    }
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
pub fn is_transport_disconnect_error_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("disconnected")
        || message.contains("broken pipe")
        || message.contains("pipe is being closed")
        || message.contains("the device is not connected")
        || message.contains("org.bluez.error.notconnected")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_transport_disconnect_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| is_transport_disconnect_error_message(&cause.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_disconnect_error_message(message: &str) -> bool {
    is_transport_disconnect_error_message(message) || {
        let message = message.to_ascii_lowercase();
        message.contains("device did not respond")
            || message.contains("hid helper timed out")
            || message.contains("failed to write hid helper request")
            || message.contains("failed to flush hid helper request")
            || message.contains("hid write failed")
            || message.contains("hid read failed")
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_disconnect_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| is_disconnect_error_message(&cause.to_string()))
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

    if device.is_bluetooth_transport() && !matches!(info.bus_type(), hidapi::BusType::Bluetooth) {
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
    #[cfg(test)]
    pub(crate) fn test_device() -> (Self, TestHidRecorder) {
        Self::test_device_with_fault_after_requests(None)
    }

    #[cfg(test)]
    pub(crate) fn test_device_with_fault_after_requests(
        fault_after_requests: Option<(usize, TestHidFault)>,
    ) -> (Self, TestHidRecorder) {
        let recorder = TestHidRecorder {
            requests: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        };
        let device = Self {
            backend: HidBackend::Test {
                recorder: recorder.clone(),
                combo: std::sync::Mutex::new(([0; 4], 0)),
                qmk_settings: std::sync::Mutex::new(std::collections::BTreeMap::new()),
                fault_after_requests: std::sync::Mutex::new(fault_after_requests),
            },
        };
        (device, recorder)
    }

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
                write_framing: HidWriteFraming::ReportIdPrefixed(0),
                path: Some(PathBuf::from(path)),
                input_report_polling: std::sync::atomic::AtomicBool::new(false),
            },
        })
    }

    pub fn open_fresh_for(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if device.uses_bluez_gatt_transport() {
            match crate::linux_ble::LinuxBleDevice::open(device) {
                Ok(bluez_device) => {
                    log::info!(
                        "Using direct BlueZ GATT for Bluetooth device {}",
                        device.name
                    );
                    return Ok(Self {
                        backend: HidBackend::LinuxBle(bluez_device),
                    });
                }
                Err(error) => {
                    log::warn!(
                        "Direct BlueZ GATT unavailable for {}: {error:#}; \
                         falling back to the Linux kernel HID transport",
                        device.name
                    );
                }
            }
            return Self::open_fresh_for_local(device)
                .context("Failed to open the Linux Bluetooth Vial transport");
        }

        #[cfg(target_os = "windows")]
        {
            return Self::open_proxy_for(device);
        }

        #[cfg(not(target_os = "windows"))]
        {
            Self::open_fresh_for_local(device)
        }
    }

    pub(crate) fn shared_output(&self) -> Option<SharedHidOutput> {
        match &self.backend {
            #[cfg(target_os = "windows")]
            HidBackend::Proxy(proxy) => Some(SharedHidOutput {
                backend: SharedHidOutputBackend::Proxy(std::sync::Arc::downgrade(proxy)),
            }),
            #[cfg(test)]
            HidBackend::Test { recorder, .. } => Some(SharedHidOutput {
                backend: SharedHidOutputBackend::Test(recorder.clone()),
            }),
            _ => None,
        }
    }

    fn open_fresh_for_local(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "macos")]
        prepare_macos_bluetooth_hid_access(device)?;

        let mut last_error = None;
        for attempt in 0..HID_OPEN_RETRIES {
            match Self::try_open_fresh_for(device) {
                Ok(device) => return Ok(device),
                Err(e) => {
                    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
                    if is_unsafe_bluetooth_report_map(&e) {
                        return Err(e);
                    }
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
            backend: HidBackend::Proxy(std::sync::Arc::new(HidProxy {
                request_lock: Mutex::new(()),
                child: Mutex::new(child),
                stdin: Mutex::new(stdin),
                rx: Mutex::new(rx),
                transport: device_transport(device),
            })),
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
                        let transport = device_transport(device);
                        let write_framing = detect_hid_write_framing(&hid_device, transport)?;
                        return Ok(Self {
                            backend: HidBackend::Local {
                                device: hid_device,
                                transport,
                                write_framing,
                                path: local_hid_path(device),
                                input_report_polling: std::sync::atomic::AtomicBool::new(false),
                            },
                        });
                    }
                    Err(e) => {
                        #[cfg(target_os = "macos")]
                        if macos_hid_open_not_permitted(&e) {
                            return Err(MacosHidInputMonitoringRequired.into());
                        }
                        log::debug!("direct HID path open failed, falling back to scan: {e}");
                    }
                }
            }
        }

        for info in api.device_list() {
            if !device_info_matches(info, device, true) {
                continue;
            }
            let path = PathBuf::from(info.path().to_string_lossy().into_owned());
            let hid_device = match info.open_device(&api) {
                Ok(device) => device,
                Err(error) => {
                    #[cfg(target_os = "macos")]
                    if macos_hid_open_not_permitted(&error) {
                        return Err(MacosHidInputMonitoringRequired.into());
                    }
                    return Err(error).context("Failed to open HID device");
                }
            };
            let transport = device_transport(device);
            let write_framing = detect_hid_write_framing(&hid_device, transport)?;
            return Ok(Self {
                backend: HidBackend::Local {
                    device: hid_device,
                    transport,
                    write_framing,
                    path: Some(path),
                    input_report_polling: std::sync::atomic::AtomicBool::new(false),
                },
            });
        }

        for info in api.device_list() {
            if !device_info_matches(info, device, false) {
                continue;
            }
            let path = PathBuf::from(info.path().to_string_lossy().into_owned());
            let hid_device = match info.open_device(&api) {
                Ok(device) => device,
                Err(error) => {
                    #[cfg(target_os = "macos")]
                    if macos_hid_open_not_permitted(&error) {
                        return Err(MacosHidInputMonitoringRequired.into());
                    }
                    return Err(error).context("Failed to open HID device");
                }
            };
            let transport = device_transport(device);
            let write_framing = detect_hid_write_framing(&hid_device, transport)?;
            return Ok(Self {
                backend: HidBackend::Local {
                    device: hid_device,
                    transport,
                    write_framing,
                    path: Some(path),
                    input_report_polling: std::sync::atomic::AtomicBool::new(false),
                },
            });
        }

        anyhow::bail!("HID device disappeared during reconnect")
    }

    /// Write one padded Vial Raw HID output report without waiting for a reply.
    ///
    /// Live host data is write-only, but it must use the same transport-specific
    /// report framing as normal Vial commands (notably report ID 5 over RMK BLE).
    pub(crate) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        ensure_output_report_len(data)?;

        match &self.backend {
            HidBackend::Local {
                device,
                write_framing,
                path,
                ..
            } => write_output_report_local(device, *write_framing, path.as_deref(), data),
            #[cfg(target_os = "windows")]
            HidBackend::Proxy(proxy) => proxy.write_output_report(data),
            #[cfg(target_os = "linux")]
            HidBackend::LinuxBle(device) => device.write_output_report(data),
            #[cfg(test)]
            HidBackend::Test { recorder, .. } => {
                record_test_output_report(recorder, data);
                Ok(())
            }
        }
    }

    /// Send exactly MSG_LEN bytes (with 0x00 report ID prepended), receive MSG_LEN bytes back.
    pub(crate) fn usb_send(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
        match &self.backend {
            HidBackend::Local {
                device,
                transport,
                write_framing,
                path,
                input_report_polling,
            } => usb_send_local(
                device,
                *transport,
                *write_framing,
                path.as_deref(),
                input_report_polling,
                data,
            ),
            #[cfg(target_os = "windows")]
            HidBackend::Proxy(proxy) => proxy.usb_send(data),
            #[cfg(target_os = "linux")]
            HidBackend::LinuxBle(device) => {
                device.send(data, |response| response_matches_command(data, response))
            }
            #[cfg(test)]
            HidBackend::Test {
                recorder,
                combo,
                qmk_settings,
                fault_after_requests,
            } => {
                let mut request = [0; MSG_LEN];
                let len = data.len().min(MSG_LEN);
                request[..len].copy_from_slice(&data[..len]);
                recorder
                    .requests
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(request);

                let fault = {
                    let mut pending = fault_after_requests
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    match pending.as_mut() {
                        Some((remaining, _)) if *remaining == 0 => pending.take(),
                        Some((remaining, _)) => {
                            *remaining -= 1;
                            None
                        }
                        None => None,
                    }
                };
                if let Some((_, fault)) = fault {
                    match fault {
                        TestHidFault::Disconnect => bail!("HID device disconnected"),
                        TestHidFault::Timeout => bail!("HID timeout — device did not respond"),
                        TestHidFault::WorkerPanic => panic!("test HID worker stopped"),
                        TestHidFault::IgnoreQmkSettingSet => {
                            if request[0] != CMD_VIA_VIAL_PREFIX
                                || request[1] != CMD_VIAL_QMK_SETTINGS_SET
                            {
                                bail!("test fault expected a QMK Settings SET request");
                            }
                            return Ok([0; MSG_LEN]);
                        }
                    }
                }

                let mut response = [0; MSG_LEN];
                match (request[0], request[1], request[2]) {
                    (CMD_VIA_MACRO_GET_BUFFER_SIZE, _, _) => {
                        response[1..3].copy_from_slice(&64u16.to_be_bytes());
                    }
                    (CMD_VIA_VIAL_PREFIX, CMD_VIAL_DYNAMIC_ENTRY_OP, DYNAMIC_VIAL_COMBO_SET) => {
                        let mut keys = [0; 4];
                        for (index, key) in keys.iter_mut().enumerate() {
                            let offset = 4 + index * 2;
                            *key = u16::from_le_bytes([request[offset], request[offset + 1]]);
                        }
                        let output = u16::from_le_bytes([request[12], request[13]]);
                        *combo.lock().unwrap_or_else(|error| error.into_inner()) = (keys, output);
                    }
                    (CMD_VIA_VIAL_PREFIX, CMD_VIAL_DYNAMIC_ENTRY_OP, DYNAMIC_VIAL_COMBO_GET) => {
                        let (keys, output) =
                            *combo.lock().unwrap_or_else(|error| error.into_inner());
                        for (index, key) in keys.iter().enumerate() {
                            let offset = 1 + index * 2;
                            response[offset..offset + 2].copy_from_slice(&key.to_le_bytes());
                        }
                        response[9..11].copy_from_slice(&output.to_le_bytes());
                    }
                    (CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_SET, _) => {
                        let qsid = u16::from_le_bytes([request[2], request[3]]);
                        let value = u16::from_le_bytes([request[4], request[5]]);
                        qmk_settings
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .insert(qsid, value);
                    }
                    (CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_GET, _) => {
                        let qsid = u16::from_le_bytes([request[2], request[3]]);
                        let value = qmk_settings
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .get(&qsid)
                            .copied()
                            .unwrap_or_default();
                        response[1..3].copy_from_slice(&value.to_le_bytes());
                    }
                    (
                        CMD_VIA_CUSTOM_GET_VALUE,
                        ERGOHAVEN_CUSTOM_NAMESPACE,
                        ERGOHAVEN_CUSTOM_BATTERY_HALVES,
                    ) => {
                        response[..3].copy_from_slice(&request[..3]);
                        response[3] = ERGOHAVEN_BATTERY_HALVES_VERSION;
                    }
                    _ => {}
                }
                Ok(response)
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn device_transport(device: &crate::device::Device) -> HidTransport {
    if device.is_bluetooth_transport() {
        HidTransport::Bluetooth
    } else {
        HidTransport::Usb
    }
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
fn ensure_output_report_len(data: &[u8]) -> Result<()> {
    if data.len() > MSG_LEN {
        bail!(
            "HID output report too long — {} bytes, max {} bytes",
            data.len(),
            MSG_LEN
        );
    }
    Ok(())
}

#[cfg(test)]
fn record_test_output_report(recorder: &TestHidRecorder, data: &[u8]) {
    let mut report = [0; MSG_LEN];
    report[..data.len()].copy_from_slice(data);
    recorder
        .requests
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(report);
}

#[cfg(not(target_arch = "wasm32"))]
fn write_output_report_local(
    device: &hidapi::HidDevice,
    write_framing: HidWriteFraming,
    path: Option<&Path>,
    data: &[u8],
) -> Result<()> {
    ensure_hid_path_present(path)?;

    let mut write_buf = [0u8; MSG_LEN + 1];
    write_buf[1..1 + data.len()].copy_from_slice(data);
    let write_frame = local_hid_write_frame(&mut write_buf, write_framing);
    let bytes_written = device
        .write(write_frame)
        .context("HID output report write failed")?;
    if bytes_written != write_frame.len() {
        bail!(
            "HID output report short write — wrote {} bytes, expected {} bytes",
            bytes_written,
            write_frame.len()
        );
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn is_optional_firmware_version_request(data: &[u8]) -> bool {
    data.starts_with(&[CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION])
}

#[cfg(not(target_arch = "wasm32"))]
fn is_optional_qmk_settings_query(data: &[u8]) -> bool {
    data.starts_with(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_QUERY])
}

#[cfg(not(target_arch = "wasm32"))]
fn is_keymap_read_request(data: &[u8]) -> bool {
    matches!(
        data.first(),
        Some(&CMD_VIA_KEYMAP_GET_BUFFER) | Some(&CMD_VIA_GET_KEYCODE)
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn is_optional_dynamic_entry_count_request(data: &[u8]) -> bool {
    data.starts_with(&[
        CMD_VIA_VIAL_PREFIX,
        CMD_VIAL_DYNAMIC_ENTRY_OP,
        DYNAMIC_VIAL_GET_NUM_ENTRIES,
    ])
}

#[cfg(not(target_arch = "wasm32"))]
fn usb_send_max_attempts(transport: HidTransport, data: &[u8]) -> usize {
    // Runtime firmware metadata and optional QMK-settings discovery both have
    // safe fallbacks, so an unsupported probe must not hold up the whole
    // connection retry budget.
    if transport.is_bluetooth()
        || is_optional_firmware_version_request(data)
        || is_optional_qmk_settings_query(data)
        || is_keymap_read_request(data)
        || crate::rmk_native::is_rmk_native_capabilities_request(data)
        || is_optional_dynamic_entry_count_request(data)
    {
        1
    } else {
        VIAL_GUI_USB_RETRIES
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn usb_send_local(
    device: &hidapi::HidDevice,
    transport: HidTransport,
    write_framing: HidWriteFraming,
    path: Option<&Path>,
    input_report_polling: &std::sync::atomic::AtomicBool,
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
    write_buf[1..1 + data.len()].copy_from_slice(data);
    let write_frame = local_hid_write_frame(&mut write_buf, write_framing);

    let read_timeout_ms = if transport.is_bluetooth() {
        WINDOWS_BLE_READ_TIMEOUT_MS
    } else {
        VIAL_GUI_READ_TIMEOUT_MS
    };

    let max_retries = usb_send_max_attempts(transport, data);

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

        match device.write(write_frame) {
            Ok(bytes_written) if bytes_written == write_frame.len() => {}
            Ok(bytes_written) => {
                last_error = Some(anyhow::anyhow!(
                    "HID short write — wrote {} bytes, expected {} bytes",
                    bytes_written,
                    write_frame.len()
                ));
                continue;
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("HID write failed: {e}"));
                continue;
            }
        }

        #[cfg(target_os = "linux")]
        if transport.is_bluetooth()
            && input_report_polling.load(std::sync::atomic::Ordering::Relaxed)
        {
            match read_response_via_input_report(device, write_framing, data, read_timeout_ms) {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        #[cfg(target_os = "linux")]
        if transport.is_bluetooth() {
            let notification_probe_error = match read_response(
                device,
                transport,
                write_framing,
                data,
                LINUX_BLE_NOTIFICATION_PROBE_TIMEOUT_MS,
            ) {
                Ok(response) => return Ok(response),
                Err(error) => error,
            };
            match read_response_via_input_report(device, write_framing, data, read_timeout_ms) {
                Ok(response) => {
                    input_report_polling.store(true, std::sync::atomic::Ordering::Relaxed);
                    static LOG_INPUT_REPORT_FALLBACK_ONCE: std::sync::Once = std::sync::Once::new();
                    LOG_INPUT_REPORT_FALLBACK_ONCE.call_once(|| {
                        log::info!(
                            "Linux Bluetooth HID notifications unavailable; \
                             using Get Input Report polling"
                        );
                    });
                    return Ok(response);
                }
                Err(input_report_error) => {
                    // Some HID stacks expose notifications but reject
                    // GET_REPORT. Give a slow notification the original full
                    // timeout before failing the command.
                    match read_response(device, transport, write_framing, data, read_timeout_ms) {
                        Ok(response) => return Ok(response),
                        Err(notification_error) => {
                            last_error = Some(anyhow::anyhow!(
                                "HID notification probe failed: {notification_probe_error}; \
                                 Get Input Report fallback failed: {input_report_error}; \
                                 full notification wait failed: {notification_error}"
                            ));
                            continue;
                        }
                    }
                }
            }
        }

        match read_response(device, transport, write_framing, data, read_timeout_ms) {
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
fn local_hid_write_frame(
    write_buf: &mut [u8; MSG_LEN + 1],
    write_framing: HidWriteFraming,
) -> &[u8] {
    match write_framing {
        HidWriteFraming::ReportIdPrefixed(report_id) => {
            write_buf[0] = report_id;
            write_buf
        }
        HidWriteFraming::LinuxBluetoothUnnumbered => &write_buf[1..],
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_hid_write_framing(
    device: &hidapi::HidDevice,
    transport: HidTransport,
) -> Result<HidWriteFraming> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if transport.is_bluetooth() {
        let mut descriptor = [0u8; HID_REPORT_DESCRIPTOR_MAX];
        let length = device
            .get_report_descriptor(&mut descriptor)
            .context("Failed to read the Bluetooth HID report descriptor")?;

        #[cfg(target_os = "linux")]
        let unnumbered_framing = HidWriteFraming::LinuxBluetoothUnnumbered;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let unnumbered_framing = HidWriteFraming::ReportIdPrefixed(0);

        let write_framing = bluetooth_hid_write_framing(&descriptor[..length], unnumbered_framing)?;
        match write_framing {
            HidWriteFraming::ReportIdPrefixed(report_id) if report_id != 0 => {
                log::info!(
                    "Using report-ID {} HOGP framing for {} Bluetooth HID",
                    report_id,
                    BLUETOOTH_HID_PLATFORM
                );
            }
            HidWriteFraming::ReportIdPrefixed(0) => {
                log::info!(
                    "Using unnumbered HOGP framing for {} Bluetooth HID",
                    BLUETOOTH_HID_PLATFORM
                );
            }
            HidWriteFraming::LinuxBluetoothUnnumbered => {
                log::info!(
                    "Using 32-byte unnumbered HOGP framing for {} Bluetooth HID",
                    BLUETOOTH_HID_PLATFORM
                );
            }
            HidWriteFraming::ReportIdPrefixed(_) => unreachable!(),
        }
        return Ok(write_framing);
    }

    let _ = (device, transport);
    Ok(HidWriteFraming::ReportIdPrefixed(0))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn bluetooth_hid_write_framing(
    descriptor: &[u8],
    unnumbered_framing: HidWriteFraming,
) -> Result<HidWriteFraming> {
    let layout = analyze_hid_report_descriptor(descriptor);

    if !layout.vial_collection_found {
        bail!("Bluetooth HID report descriptor has no Vial application collection");
    }
    if layout.vial_report_id_conflict {
        bail!("Bluetooth HID report descriptor assigns conflicting Vial report ids");
    }

    if let Some(report_id) = layout.vial_report_id {
        return Ok(HidWriteFraming::ReportIdPrefixed(report_id));
    }
    if layout.vial_uses_unnumbered_reports && layout.has_numbered_reports {
        return Err(UnsafeBluetoothReportMap.into());
    }
    if layout.vial_uses_unnumbered_reports {
        return Ok(unnumbered_framing);
    }

    bail!("Bluetooth HID report descriptor has no Vial input/output reports")
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, PartialEq, Eq)]
struct HidReportDescriptorLayout {
    has_numbered_reports: bool,
    vial_collection_found: bool,
    vial_report_id: Option<u8>,
    vial_uses_unnumbered_reports: bool,
    vial_report_id_conflict: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn analyze_hid_report_descriptor(descriptor: &[u8]) -> HidReportDescriptorLayout {
    let mut layout = HidReportDescriptorLayout::default();
    let mut offset = 0usize;
    let mut usage_page = 0u32;
    let mut local_usage = None;
    let mut report_id = 0u8;
    let mut global_stack = Vec::new();
    let mut collection_stack = Vec::new();

    while offset < descriptor.len() {
        let prefix = descriptor[offset];
        if prefix == 0xFE {
            if offset + 2 >= descriptor.len() {
                break;
            }
            let data_len = usize::from(descriptor[offset + 1]);
            let Some(next) = offset.checked_add(3 + data_len) else {
                break;
            };
            if next > descriptor.len() {
                break;
            }
            offset = next;
            continue;
        }

        let data_len = match prefix & 0x03 {
            0x03 => 4,
            size => usize::from(size),
        };
        let Some(next) = offset.checked_add(1 + data_len) else {
            break;
        };
        if next > descriptor.len() {
            break;
        }

        let item_type = (prefix >> 2) & 0x03;
        let item_tag = (prefix >> 4) & 0x0F;
        let value = descriptor[offset + 1..next]
            .iter()
            .enumerate()
            .fold(0u32, |value, (index, byte)| {
                value | (u32::from(*byte) << (index * 8))
            });

        match (item_type, item_tag) {
            // Global Usage Page
            (0x01, 0x00) => usage_page = value,
            // Global Report ID
            (0x01, 0x08) if data_len > 0 => {
                report_id = value as u8;
                if report_id != 0 {
                    layout.has_numbered_reports = true;
                }
            }
            // Global Push / Pop
            (0x01, 0x0A) => global_stack.push((usage_page, report_id)),
            (0x01, 0x0B) => {
                if let Some((saved_usage_page, saved_report_id)) = global_stack.pop() {
                    usage_page = saved_usage_page;
                    report_id = saved_report_id;
                }
            }
            // Local Usage
            (0x02, 0x00) => local_usage = Some(value),
            // Main Collection
            (0x00, 0x0A) => {
                let parent_is_vial = collection_stack.last().copied().unwrap_or(false);
                let is_vial = parent_is_vial
                    || (value == 0x01 && usage_page == 0xFF60 && local_usage == Some(0x61));
                if !parent_is_vial && is_vial {
                    layout.vial_collection_found = true;
                }
                collection_stack.push(is_vial);
                local_usage = None;
            }
            // Main End Collection
            (0x00, 0x0C) => {
                collection_stack.pop();
                local_usage = None;
            }
            // Main Input / Output / Feature
            (0x00, 0x08 | 0x09 | 0x0B) => {
                if collection_stack.last().copied().unwrap_or(false) {
                    if report_id == 0 {
                        layout.vial_uses_unnumbered_reports = true;
                        if layout.vial_report_id.is_some() {
                            layout.vial_report_id_conflict = true;
                        }
                    } else if let Some(existing) = layout.vial_report_id {
                        if existing != report_id {
                            layout.vial_report_id_conflict = true;
                        }
                    } else {
                        layout.vial_report_id = Some(report_id);
                        if layout.vial_uses_unnumbered_reports {
                            layout.vial_report_id_conflict = true;
                        }
                    }
                }
                local_usage = None;
            }
            // Local state is consumed by every other Main item.
            (0x00, _) => local_usage = None,
            _ => {}
        }

        offset = next;
    }

    layout
}

#[cfg(target_os = "linux")]
pub(crate) fn vial_report_id_from_hid_descriptor(descriptor: &[u8]) -> Option<u8> {
    let layout = analyze_hid_report_descriptor(descriptor);
    if !layout.vial_collection_found
        || layout.vial_report_id_conflict
        || (layout.vial_uses_unnumbered_reports && layout.has_numbered_reports)
    {
        return None;
    }

    layout
        .vial_report_id
        .or_else(|| layout.vial_uses_unnumbered_reports.then_some(0))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_response(
    device: &hidapi::HidDevice,
    transport: HidTransport,
    write_framing: HidWriteFraming,
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
        let resp = match decode_hid_response(&read_buf, bytes_read, write_framing) {
            Ok(resp) => resp,
            Err(e) => {
                last_error = Some(e);
                if transport.is_bluetooth() {
                    continue;
                }
                break;
            }
        };

        if response_matches_command(command, &resp) {
            return Ok(resp);
        }

        last_error = Some(anyhow::anyhow!(
            "HID stale or unrelated report for command {:02X}: {:02X?}",
            command.first().copied().unwrap_or(0),
            &resp[..command.len().clamp(3, 8)]
        ));
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HID timeout — device did not respond")))
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_hid_response(
    read_buf: &[u8; MSG_LEN + 1],
    bytes_read: usize,
    write_framing: HidWriteFraming,
) -> Result<[u8; MSG_LEN]> {
    let mut resp = [0u8; MSG_LEN];
    match (write_framing, bytes_read) {
        (HidWriteFraming::ReportIdPrefixed(expected), length) if length == MSG_LEN + 1 => {
            if read_buf[0] != expected {
                bail!(
                    "HID response has report id {}, expected {}",
                    read_buf[0],
                    expected
                );
            }
            resp.copy_from_slice(&read_buf[1..MSG_LEN + 1]);
        }
        (HidWriteFraming::ReportIdPrefixed(0), length) if length == MSG_LEN => {
            resp.copy_from_slice(&read_buf[..MSG_LEN]);
        }
        (HidWriteFraming::LinuxBluetoothUnnumbered, length) if length == MSG_LEN => {
            resp.copy_from_slice(&read_buf[..MSG_LEN]);
        }
        _ => {
            bail!(
                "HID invalid response length — read {} bytes for {:?}",
                bytes_read,
                write_framing
            );
        }
    }
    Ok(resp)
}

#[cfg(target_os = "linux")]
fn read_response_via_input_report(
    device: &hidapi::HidDevice,
    write_framing: HidWriteFraming,
    command: &[u8],
    timeout_ms: i32,
) -> Result<[u8; MSG_LEN]> {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms.max(1) as u64);

    // QMK_SETTINGS_GET and GET_ENCODER replies do not identify their request.
    // A single interval can therefore return the preceding value and shift a
    // sequence of settings (for example left module -> right module). Give
    // those commands the same four-interval freshness window as direct GATT.
    std::thread::sleep(linux_ble_input_report_settle(command));

    loop {
        let mut read_buf = [0u8; MSG_LEN + 1];
        read_buf[0] = write_framing.report_id().unwrap_or(0);
        let bytes_read = device
            .get_input_report(&mut read_buf)
            .map_err(|e| anyhow::anyhow!("HID Get Input Report failed: {e}"))?;
        let resp = decode_hid_response(&read_buf, bytes_read, write_framing)?;
        if response_matches_command(command, &resp) {
            return Ok(resp);
        }

        let stale_error = anyhow::anyhow!(
            "HID stale Get Input Report for command {:02X}: {:02X?}",
            command.first().copied().unwrap_or(0),
            &resp[..command.len().clamp(3, 8)]
        );

        if std::time::Instant::now() >= deadline {
            return Err(stale_error);
        }
        std::thread::sleep(WINDOWS_BLE_SETTLE_DELAY);
    }
}

#[cfg(target_os = "linux")]
fn linux_ble_input_report_settle(command: &[u8]) -> Duration {
    if vial_reply_is_uncorrelated(command) {
        LINUX_BLE_UNCORRELATED_REPLY_SETTLE
    } else {
        WINDOWS_BLE_SETTLE_DELAY
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_command(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    let Some(&cmd) = command.first() else {
        return false;
    };

    match cmd {
        CMD_VIA_GET_PROTOCOL_VERSION => {
            resp[0] == CMD_VIA_GET_PROTOCOL_VERSION
                && is_supported_via_protocol(u16::from_be_bytes([resp[1], resp[2]]))
        }
        CMD_VIA_GET_LAYER_COUNT => {
            resp[0] == CMD_VIA_GET_LAYER_COUNT && (1..=32).contains(&resp[1])
        }
        CMD_VIA_KEYMAP_GET_BUFFER | CMD_VIA_MACRO_GET_BUFFER => {
            command.len() >= 4 && resp[..4] == command[..4]
        }
        CMD_VIA_MACRO_GET_COUNT | CMD_VIA_MACRO_GET_BUFFER_SIZE => resp[0] == cmd,
        CMD_VIA_CUSTOM_GET_VALUE | CMD_VIA_CUSTOM_SET_VALUE
            if command.get(1) == Some(&ERGOHAVEN_CUSTOM_NAMESPACE) =>
        {
            crate::rmk_native::matches_rmk_native_response(command, resp).unwrap_or_else(|| {
                command.len() >= 3 && resp[0] == cmd && resp[1..3] == command[1..3]
            })
        }
        CMD_VIA_GET_KEYBOARD_VALUE => {
            command.len() >= 2
                && ((resp[0] == cmd && resp[1] == command[1])
                    || (is_optional_firmware_version_request(command)
                        && resp[0] == u8::MAX
                        && resp[1] == VIA_FIRMWARE_VERSION))
        }
        CMD_VIA_LIGHTING_GET_VALUE => command.len() >= 2 && resp[0] == cmd && resp[1] == command[1],
        CMD_VIA_GET_KEYCODE => command.len() >= 4 && resp[0] == cmd && resp[1..4] == command[1..4],
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
        CMD_VIAL_QMK_SETTINGS_QUERY => response_matches_qmk_settings_query(command, resp),
        CMD_VIAL_QMK_SETTINGS_GET => response_matches_qmk_settings_get(command, resp),
        CMD_VIAL_QMK_SETTINGS_SET => response_matches_qmk_settings_set(command, resp),
        CMD_VIAL_DYNAMIC_ENTRY_OP
        | CMD_VIAL_GET_ENCODER
        | CMD_VIAL_SET_ENCODER
        | CMD_VIAL_UNLOCK_START
        | CMD_VIAL_LOCK => true,
        _ => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn response_echoes_vial_command(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    command.len() >= 2
        && command.len() <= MSG_LEN
        && resp[1..command.len()] == command[1..]
        && resp[command.len()..].iter().all(|byte| *byte == 0)
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_qmk_settings_set(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    // RMK echoes the SET payload, while Vial/QMK implementations may return
    // only the success/error status byte.
    response_echoes_vial_command(command, resp)
        || (matches!(resp[0], 0 | u8::MAX) && resp[1..].iter().all(|byte| *byte == 0))
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_qmk_settings_get(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    let Some(qsid_bytes) = command.get(2..4) else {
        return false;
    };
    if response_echoes_vial_command(command, resp) {
        return true;
    }
    if resp[0] != 0 {
        return false;
    }

    let qsid = u16::from_le_bytes([qsid_bytes[0], qsid_bytes[1]]);
    if !(200..232).contains(&qsid) {
        return true;
    }

    let payload = &resp[1..];
    let Some(end) = payload.iter().position(|byte| *byte == 0) else {
        return false;
    };
    end <= 15 && std::str::from_utf8(&payload[..end]).is_ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn response_matches_qmk_settings_query(command: &[u8], resp: &[u8; MSG_LEN]) -> bool {
    // Older Vial-QMK builds echo unsupported vendor commands. Treat that echo
    // as a correlated terminal response so the optional settings probe can
    // fail immediately instead of consuming the full 20-attempt USB budget.
    if response_echoes_vial_command(command, resp) {
        return true;
    }

    let Some(qsid_bytes) = command.get(2..4) else {
        return false;
    };
    let cursor = u16::from_le_bytes([qsid_bytes[0], qsid_bytes[1]]);
    let mut reached_terminator = false;

    for chunk in resp.chunks_exact(2) {
        let qsid = u16::from_le_bytes([chunk[0], chunk[1]]);
        if qsid == u16::MAX {
            reached_terminator = true;
        } else if reached_terminator || qsid <= cursor {
            return false;
        }
    }

    true
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
        let _request_guard = self
            .request_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("HID helper request lock poisoned"))?;
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

    fn write_output_report(&self, data: &[u8]) -> Result<()> {
        let request = format!("{HID_PROXY_OUTPUT_PREFIX}{}", bytes_to_hex(data));
        let line = self.request(&request)?;
        let response: ProxyResponse =
            serde_json::from_str(&line).context("HID helper returned malformed response")?;
        if !response.ok {
            bail!(response
                .error
                .unwrap_or_else(|| "HID helper output report failed".to_owned()));
        }
        Ok(())
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
        let response = if let Some(encoded) = line.strip_prefix(HID_PROXY_OUTPUT_PREFIX) {
            match hex_to_bytes(encoded).and_then(|data| hid.write_output_report(&data)) {
                Ok(()) => ProxyResponse {
                    ok: true,
                    data: None,
                    error: None,
                },
                Err(e) => ProxyResponse {
                    ok: false,
                    data: None,
                    error: Some(e.to_string()),
                },
            }
        } else {
            match hex_to_bytes(line).and_then(|data| hid.usb_send(&data)) {
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
            }
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

#[cfg(target_os = "macos")]
fn prepare_macos_bluetooth_hid_access(device: &crate::device::Device) -> Result<()> {
    if !device.is_bluetooth_transport() || crate::smart_input::input_monitoring_access_granted() {
        return Ok(());
    }

    if crate::smart_input::request_input_monitoring_access() {
        return Ok(());
    }

    Err(MacosHidInputMonitoringRequired.into())
}

#[cfg(target_os = "macos")]
fn macos_hid_open_not_permitted(error: &hidapi::HidError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("0xe00002e2") || message.contains("not permitted")
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn write_only_output_report_uses_the_hid_transport_owner() {
        let (device, recorder) = HidDevice::test_device();

        device.write_output_report(&[0xAC, 1]).unwrap();

        let requests = recorder.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(&requests[0][..4], &[0xAC, 1, 0, 0]);
    }

    #[test]
    fn usb_hid_write_keeps_zero_report_id() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[1] = CMD_VIA_GET_PROTOCOL_VERSION;

        let frame = local_hid_write_frame(&mut buffer, HidWriteFraming::ReportIdPrefixed(0));

        assert_eq!(frame.len(), MSG_LEN + 1);
        assert_eq!(frame[0], 0);
        assert_eq!(frame[1], CMD_VIA_GET_PROTOCOL_VERSION);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_bluetooth_hid_write_omits_unnumbered_report_id() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[1] = CMD_VIA_GET_PROTOCOL_VERSION;
        buffer[2] = 0xA5;

        let frame = local_hid_write_frame(&mut buffer, HidWriteFraming::LinuxBluetoothUnnumbered);

        assert_eq!(frame.len(), MSG_LEN);
        assert_eq!(frame[0], CMD_VIA_GET_PROTOCOL_VERSION);
        assert_eq!(frame[1], 0xA5);
    }

    #[test]
    fn numbered_hid_write_uses_vial_report_id() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[1] = CMD_VIA_GET_PROTOCOL_VERSION;
        buffer[2] = 0xA5;

        let frame = local_hid_write_frame(&mut buffer, HidWriteFraming::ReportIdPrefixed(5));

        assert_eq!(frame.len(), MSG_LEN + 1);
        assert_eq!(frame[0], 5);
        assert_eq!(frame[1], CMD_VIA_GET_PROTOCOL_VERSION);
        assert_eq!(frame[2], 0xA5);
    }

    #[test]
    fn numbered_bluetooth_live_output_uses_vial_report_id() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[1] = 0xAC;
        buffer[2] = 1;

        let frame = local_hid_write_frame(&mut buffer, HidWriteFraming::ReportIdPrefixed(5));

        assert_eq!(&frame[..3], &[5, 0xAC, 1]);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[test]
    fn numbered_bluetooth_descriptor_selects_vial_report_id() {
        let descriptor = [
            0x06, 0x60, 0xFF, // Usage Page 0xFF60
            0x09, 0x61, // Usage 0x61
            0xA1, 0x01, // Application collection
            0x85, 0x05, // Report ID 5
            0x09, 0x62, // Usage input
            0x81, 0x02, // Input
            0x09, 0x63, // Usage output
            0x91, 0x02, // Output
            0xC0, // End collection
        ];

        assert_eq!(
            bluetooth_hid_write_framing(&descriptor, HidWriteFraming::ReportIdPrefixed(0)).unwrap(),
            HidWriteFraming::ReportIdPrefixed(5)
        );
    }

    #[test]
    fn detects_numbered_vial_collection_in_hid_descriptor() {
        let descriptor = [
            0x06, 0x60, 0xFF, // Usage Page 0xFF60
            0x09, 0x61, // Usage 0x61
            0xA1, 0x01, // Application collection
            0x85, 0x05, // Report ID 5
            0x09, 0x62, // Usage input
            0x81, 0x02, // Input
            0x09, 0x63, // Usage output
            0x91, 0x02, // Output
            0xC0, // End collection
        ];

        let layout = analyze_hid_report_descriptor(&descriptor);
        assert!(layout.has_numbered_reports);
        assert!(layout.vial_collection_found);
        assert_eq!(layout.vial_report_id, Some(5));
        assert!(!layout.vial_uses_unnumbered_reports);
        assert!(!layout.vial_report_id_conflict);
        #[cfg(target_os = "linux")]
        assert_eq!(vial_report_id_from_hid_descriptor(&descriptor), Some(5));
    }

    #[test]
    fn detects_unnumbered_vial_collection_in_hid_descriptor() {
        let descriptor = [
            0x06, 0x60, 0xFF, // Usage Page 0xFF60
            0x09, 0x61, // Usage 0x61
            0xA1, 0x01, // Application collection
            0x09, 0x62, // Usage input
            0x81, 0x02, // Input
            0x09, 0x63, // Usage output
            0x91, 0x02, // Output
            0xC0, // End collection
        ];

        let layout = analyze_hid_report_descriptor(&descriptor);
        assert!(!layout.has_numbered_reports);
        assert!(layout.vial_collection_found);
        assert_eq!(layout.vial_report_id, None);
        assert!(layout.vial_uses_unnumbered_reports);
        #[cfg(target_os = "linux")]
        assert_eq!(vial_report_id_from_hid_descriptor(&descriptor), Some(0));
    }

    #[test]
    fn detects_unsafe_unnumbered_vial_mixed_with_numbered_reports() {
        let descriptor = [
            0x06, 0x60, 0xFF, // Usage Page 0xFF60
            0x09, 0x61, // Usage 0x61
            0xA1, 0x01, // Vial application collection
            0x09, 0x62, // Usage input
            0x81, 0x02, // Unnumbered Input
            0x09, 0x63, // Usage output
            0x91, 0x02, // Unnumbered Output
            0xC0, // End collection
            0x05, 0x01, // Usage Page Generic Desktop
            0x09, 0x06, // Usage Keyboard
            0xA1, 0x01, // Keyboard application collection
            0x85, 0x01, // Report ID 1
            0x81, 0x00, // Input
            0xC0, // End collection
        ];

        let layout = analyze_hid_report_descriptor(&descriptor);
        assert!(layout.has_numbered_reports);
        assert!(layout.vial_collection_found);
        assert_eq!(layout.vial_report_id, None);
        assert!(layout.vial_uses_unnumbered_reports);
        #[cfg(target_os = "linux")]
        assert_eq!(vial_report_id_from_hid_descriptor(&descriptor), None);
    }

    #[test]
    fn ignores_report_id_bytes_inside_long_hid_items() {
        let descriptor = [
            0xFE, 0x02, 0x01, 0x85, 0x05, // Long item containing 0x85
            0x75, 0x08, // Report size 8
        ];

        assert_eq!(
            analyze_hid_report_descriptor(&descriptor),
            HidReportDescriptorLayout::default()
        );
    }

    #[test]
    fn hid_response_accepts_unnumbered_stream_report() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[0] = CMD_VIA_GET_PROTOCOL_VERSION;
        buffer[1] = 0;
        buffer[2] = 9;

        let response =
            decode_hid_response(&buffer, MSG_LEN, HidWriteFraming::LinuxBluetoothUnnumbered)
                .unwrap();

        assert_eq!(response[0], CMD_VIA_GET_PROTOCOL_VERSION);
        assert_eq!(&response[1..3], &[0, 9]);
    }

    #[test]
    fn hid_response_accepts_report_with_explicit_id() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[0] = 5;
        buffer[1] = CMD_VIA_GET_PROTOCOL_VERSION;
        buffer[2] = 0;
        buffer[3] = 9;

        let response =
            decode_hid_response(&buffer, MSG_LEN + 1, HidWriteFraming::ReportIdPrefixed(5))
                .unwrap();

        assert_eq!(response[0], CMD_VIA_GET_PROTOCOL_VERSION);
        assert_eq!(&response[1..3], &[0, 9]);
    }

    #[test]
    fn hid_response_rejects_mouse_report_id_for_vial_payload() {
        let mut buffer = [0u8; MSG_LEN + 1];
        buffer[0] = 2;
        buffer[1] = 3;

        assert!(
            decode_hid_response(&buffer, MSG_LEN + 1, HidWriteFraming::ReportIdPrefixed(5))
                .is_err()
        );
    }

    #[test]
    fn hid_response_rejects_invalid_length() {
        let buffer = [0u8; MSG_LEN + 1];

        assert!(decode_hid_response(
            &buffer,
            MSG_LEN - 1,
            HidWriteFraming::LinuxBluetoothUnnumbered,
        )
        .is_err());
    }

    #[test]
    fn optional_firmware_version_probe_uses_one_usb_attempt() {
        let command = [CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION];

        assert_eq!(usb_send_max_attempts(HidTransport::Usb, &command), 1);
    }

    #[test]
    fn optional_qmk_settings_query_uses_one_usb_attempt() {
        let command = [CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_QUERY, 0, 0];

        assert_eq!(usb_send_max_attempts(HidTransport::Usb, &command), 1);
    }

    #[test]
    fn optional_qmk_compatibility_probes_use_one_usb_attempt() {
        let rmk_capabilities = [
            CMD_VIA_CUSTOM_GET_VALUE,
            ERGOHAVEN_CUSTOM_NAMESPACE,
            0x02, // ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION_CAPS
        ];
        let dynamic_entry_counts = [
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_GET_NUM_ENTRIES,
        ];

        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &rmk_capabilities),
            1
        );
        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &dynamic_entry_counts),
            1
        );
    }

    #[test]
    fn keymap_reads_fail_fast_for_compatibility_fallback() {
        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &[CMD_VIA_KEYMAP_GET_BUFFER, 0, 0, 28]),
            1
        );
        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &[CMD_VIA_GET_KEYCODE, 0, 0, 0]),
            1
        );
    }

    #[test]
    fn mandatory_usb_request_keeps_full_retry_budget() {
        let command = [CMD_VIA_GET_PROTOCOL_VERSION];

        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &command),
            VIAL_GUI_USB_RETRIES
        );

        let native_key_action = [
            CMD_VIA_CUSTOM_GET_VALUE,
            ERGOHAVEN_CUSTOM_NAMESPACE,
            0x03, // ERGOHAVEN_CUSTOM_NATIVE_KEY_ACTION
        ];
        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &native_key_action),
            VIAL_GUI_USB_RETRIES
        );

        let dynamic_entry_read = [
            CMD_VIA_VIAL_PREFIX,
            CMD_VIAL_DYNAMIC_ENTRY_OP,
            DYNAMIC_VIAL_COMBO_GET,
            0,
        ];
        assert_eq!(
            usb_send_max_attempts(HidTransport::Usb, &dynamic_entry_read),
            VIAL_GUI_USB_RETRIES
        );
    }

    #[test]
    fn accepts_current_and_legacy_via_protocol_versions() {
        assert!(is_supported_via_protocol(9));
        assert!(is_supported_via_protocol(u16::MAX));
        assert!(!is_supported_via_protocol(0));
        assert!(!is_supported_via_protocol(8));
        assert!(!is_supported_via_protocol(10));

        let command = [CMD_VIA_GET_PROTOCOL_VERSION];
        let mut response = [0u8; MSG_LEN];
        response[..3].copy_from_slice(&[CMD_VIA_GET_PROTOCOL_VERSION, 0xFF, 0xFF]);
        assert!(response_matches_command(&command, &response));
    }

    #[test]
    fn qmk_settings_query_accepts_an_unsupported_command_echo() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_VIAL_PREFIX;
        command[1] = CMD_VIAL_QMK_SETTINGS_QUERY;
        let response = command;

        assert!(response_matches_qmk_settings_query(&command, &response));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn uncorrelated_linux_ble_gets_wait_for_a_fresh_input_report() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_VIAL_PREFIX;
        command[1] = CMD_VIAL_QMK_SETTINGS_GET;

        assert_eq!(
            linux_ble_input_report_settle(&command),
            LINUX_BLE_UNCORRELATED_REPLY_SETTLE
        );

        command[1] = CMD_VIAL_GET_DEFINITION;
        assert_eq!(
            linux_ble_input_report_settle(&command),
            WINDOWS_BLE_SETTLE_DELAY
        );
    }

    #[test]
    fn firmware_version_probe_accepts_successful_response() {
        let command = [CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION];
        let mut response = [0u8; MSG_LEN];
        response[..6].copy_from_slice(&[
            CMD_VIA_GET_KEYBOARD_VALUE,
            VIA_FIRMWARE_VERSION,
            0,
            4,
            0,
            5,
        ]);

        assert!(response_matches_command(&command, &response));
    }

    #[test]
    fn firmware_version_probe_accepts_matching_unhandled_response() {
        let command = [CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION];
        let mut response = [0u8; MSG_LEN];
        response[0] = u8::MAX;
        response[1] = VIA_FIRMWARE_VERSION;

        assert!(response_matches_command(&command, &response));
    }

    #[test]
    fn firmware_version_probe_rejects_unhandled_response_for_another_value() {
        let command = [CMD_VIA_GET_KEYBOARD_VALUE, VIA_FIRMWARE_VERSION];
        let mut response = [0u8; MSG_LEN];
        response[0] = u8::MAX;
        response[1] = VIA_SWITCH_MATRIX_STATE;

        assert!(!response_matches_command(&command, &response));
    }

    fn qmk_settings_command(subcommand: u8, qsid: u16) -> [u8; MSG_LEN] {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_VIAL_PREFIX;
        command[1] = subcommand;
        command[2..4].copy_from_slice(&qsid.to_le_bytes());
        command
    }

    #[test]
    fn qmk_settings_set_accepts_echoed_command_response() {
        let mut command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_SET, 300);
        command[4..6].copy_from_slice(&2048u16.to_le_bytes());
        let mut response = command;
        response[0] = 0;

        assert!(response_matches_command(&command, &response));
    }

    #[test]
    fn qmk_settings_set_accepts_status_only_response() {
        let mut command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_SET, 300);
        command[4..6].copy_from_slice(&2048u16.to_le_bytes());
        let success = [0u8; MSG_LEN];
        let mut error = [0u8; MSG_LEN];
        error[0] = u8::MAX;

        assert!(response_matches_command(&command, &success));
        assert!(response_matches_command(&command, &error));
    }

    #[test]
    fn qmk_settings_set_rejects_stale_get_response() {
        let mut command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_SET, 300);
        command[4] = 2;
        let mut stale_response = [0u8; MSG_LEN];
        stale_response[0] = 0;
        stale_response[1] = 2;
        stale_response[2..4].copy_from_slice(&300u16.to_le_bytes());

        assert!(!response_matches_command(&command, &stale_response));
    }

    #[test]
    fn qmk_settings_set_rejects_echo_for_another_qsid() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_SET, 300);
        let mut stale_response = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_SET, 301);
        stale_response[0] = 0;

        assert!(!response_matches_command(&command, &stale_response));
    }

    #[test]
    fn qmk_settings_get_accepts_success_and_echoed_error_shapes() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_GET, 300);
        let mut success = [0u8; MSG_LEN];
        success[0] = 0;
        success[1..3].copy_from_slice(&2048u16.to_le_bytes());
        let mut error = command;
        error[0] = u8::MAX;

        assert!(response_matches_command(&command, &success));
        assert!(response_matches_command(&command, &error));
    }

    #[test]
    fn qmk_settings_get_rejects_impossible_status_payload() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_GET, 300);
        let mut response = [0u8; MSG_LEN];
        response[0] = 0x7F;
        response[1] = 0x42;

        assert!(!response_matches_command(&command, &response));
    }

    #[test]
    fn layer_name_get_rejects_stale_encoder_payload() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_GET, 201);
        let mut stale_encoder = [0u8; MSG_LEN];
        stale_encoder[..4].copy_from_slice(&[0x00, 0xEA, 0x00, 0xE9]);

        assert!(!response_matches_command(&command, &stale_encoder));

        let mut valid_name = [0u8; MSG_LEN];
        valid_name[1..5].copy_from_slice(b"Nav\0");
        assert!(response_matches_command(&command, &valid_name));
    }

    #[test]
    fn qmk_settings_query_accepts_advancing_qsids_and_terminator() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_QUERY, 100);
        let mut response = [u8::MAX; MSG_LEN];
        response[0..2].copy_from_slice(&101u16.to_le_bytes());
        response[2..4].copy_from_slice(&300u16.to_le_bytes());

        assert!(response_matches_command(&command, &response));
    }

    #[test]
    fn qmk_settings_query_rejects_stale_nonadvancing_batch() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_QUERY, 300);
        let mut stale_response = [u8::MAX; MSG_LEN];
        stale_response[0..2].copy_from_slice(&101u16.to_le_bytes());
        stale_response[2..4].copy_from_slice(&300u16.to_le_bytes());

        assert!(!response_matches_command(&command, &stale_response));
    }

    #[test]
    fn qmk_settings_query_rejects_values_after_terminator() {
        let command = qmk_settings_command(CMD_VIAL_QMK_SETTINGS_QUERY, 100);
        let mut response = [u8::MAX; MSG_LEN];
        response[0..2].copy_from_slice(&101u16.to_le_bytes());
        response[4..6].copy_from_slice(&300u16.to_le_bytes());

        assert!(!response_matches_command(&command, &response));
    }

    #[test]
    fn native_action_scan_rejects_a_stale_flat_index() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = 0x04;
        command[3] = 0x01;
        command[4..6].copy_from_slice(&59u16.to_le_bytes());

        let mut stale_response = command;
        stale_response[4] = 0;
        stale_response[5..7].copy_from_slice(&58u16.to_le_bytes());

        assert!(!response_matches_command(&command, &stale_response));
    }

    #[test]
    fn native_dynamic_action_scan_rejects_a_stale_flat_index() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = 0x06;
        command[3] = 0x01;
        command[4..6].copy_from_slice(&17u16.to_le_bytes());

        let mut stale_response = command;
        stale_response[4] = 0;
        stale_response[5..7].copy_from_slice(&16u16.to_le_bytes());

        assert!(!response_matches_command(&command, &stale_response));
    }

    #[test]
    fn native_capabilities_accepts_exact_qmk_echo_as_unsupported() {
        let mut command = [0u8; MSG_LEN];
        command[0] = CMD_VIA_CUSTOM_GET_VALUE;
        command[1] = ERGOHAVEN_CUSTOM_NAMESPACE;
        command[2] = 0x02;

        assert!(response_matches_command(&command, &command));
    }
}
