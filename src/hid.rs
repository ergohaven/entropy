/// Vial protocol implementation over HID.
/// Based on vial-gui Python source: protocol/keyboard_comm.py
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "hid_proxy.rs"]
mod hid_proxy;
pub use hid_proxy::run_hid_proxy_if_requested;
use hid_proxy::HidProxy;
pub(crate) use hid_proxy::{claim_open_target, HidRetirement};

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
// Firmware 4.0.6 performs the flash update synchronously before acknowledging
// SLOT_COMMIT. It remains enumerated, but regularly exceeds the generic USB
// response window; timing out here incorrectly tears down the whole connection.
const PICTOGRAM_SLOT_COMMIT_READ_TIMEOUT_MS: i32 = 2_500;
const WINDOWS_BLE_READ_TIMEOUT_MS: i32 = 2_500;
const WINDOWS_BLE_READ_SLICE_MS: i32 = 250;
const WINDOWS_BLE_SETTLE_DELAY: Duration = Duration::from_millis(12);
#[cfg(target_os = "linux")]
const LINUX_BLE_NOTIFICATION_PROBE_TIMEOUT_MS: i32 = 80;
#[cfg(target_os = "linux")]
const LINUX_BLE_UNCORRELATED_REPLY_SETTLE: Duration = Duration::from_millis(32);
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
    host_output: std::sync::Arc<crate::qmk_hid_host::HostOutputOwner>,
    pictogram_backup_directory: std::sync::Arc<std::sync::Mutex<Option<PathBuf>>>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<[u8; MSG_LEN]>>>,
    responses: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<[u8; MSG_LEN]>>>,
}

#[cfg(test)]
impl TestHidRecorder {
    // Per scripted physical owner; survives moving the HID device into its worker.
    pub(crate) fn set_pictogram_backup_directory(&self, directory: PathBuf) {
        *self.pictogram_backup_directory.lock().unwrap() = Some(directory);
    }

    pub(crate) fn respond_with(&self, responses: impl IntoIterator<Item = [u8; MSG_LEN]>) {
        self.responses.lock().unwrap().extend(responses);
    }

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
            HidBackend::Proxy(proxy) => proxy.is_bluetooth_transport(),
            #[cfg(target_os = "linux")]
            HidBackend::LinuxBle(_) => true,
            #[cfg(test)]
            HidBackend::Test { .. } => false,
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn macos_hid_operation_lock(&self) -> Option<std::sync::MutexGuard<'static, ()>> {
        #[cfg(test)]
        if matches!(&self.backend, HidBackend::Test { .. }) {
            return None;
        }

        if matches!(&self.backend, HidBackend::Proxy(_)) {
            return None;
        }
        Some(macos_hid_operation_lock())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct SharedHidOutput {
    backend: SharedHidOutputBackend,
    host_output: std::sync::Arc<crate::qmk_hid_host::HostOutputOwner>,
    host_lease: Option<crate::qmk_hid_host::HostOutputLease>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
enum SharedHidOutputBackend {
    Proxy(std::sync::Weak<HidProxy>),
    #[cfg(test)]
    Test(TestHidRecorder),
}

#[cfg(not(target_arch = "wasm32"))]
impl SharedHidOutput {
    pub(crate) fn shares_owner_with(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.host_output, &other.host_output)
    }

    #[cfg(test)]
    pub(crate) fn test_expired_proxy_owner() -> Self {
        Self {
            host_output: Default::default(),
            host_lease: None,
            backend: SharedHidOutputBackend::Proxy(std::sync::Weak::new()),
        }
    }

    pub(crate) fn is_available(&self) -> bool {
        match &self.backend {
            SharedHidOutputBackend::Proxy(proxy) => {
                proxy.upgrade().is_some_and(|proxy| proxy.is_available())
            }
            #[cfg(test)]
            SharedHidOutputBackend::Test(_) => true,
        }
    }

    pub(crate) fn for_host_bridge(
        &self,
        mode: crate::qmk_hid_host::HostDataMode,
        extended: bool,
    ) -> Self {
        let mut output = self.clone();
        output.host_lease = Some(self.host_output.claim(mode, extended));
        output
    }

    pub(crate) fn host_session_is_current(&self) -> bool {
        self.host_lease
            .as_ref()
            .is_none_or(|lease| lease.is_current())
    }

    pub(crate) fn write_host_shutdown(&self, payloads: &[Vec<u8>]) -> Result<()> {
        if let Some(lease) = self.host_lease.as_ref() {
            return lease.shutdown(|payload| self.write_output_report_unordered(payload));
        }
        for payload in payloads {
            self.write_output_report_unordered(payload)?;
        }
        Ok(())
    }

    pub(crate) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        if let Some(lease) = self.host_lease.as_ref() {
            return lease.write(data, |payload| self.write_output_report_unordered(payload));
        }
        self.write_output_report_unordered(data)
    }

    fn write_output_report_unordered(&self, data: &[u8]) -> Result<()> {
        ensure_output_report_len(data)?;
        match &self.backend {
            SharedHidOutputBackend::Proxy(proxy) => proxy
                .upgrade()
                .context("Shared HID output owner is no longer available")?
                .write_output_report(data),
            #[cfg(test)]
            SharedHidOutputBackend::Test(recorder) => {
                record_test_output_report(recorder, data);
                Ok(())
            }
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
fn device_info_matches(info: &hidapi::DeviceInfo, device: &crate::device::Device) -> bool {
    info.usage_page() == 0xFF60
        && info.usage() == 0x61
        && device.permits_hid_target(&device_from_info(info))
}

fn device_from_info(info: &hidapi::DeviceInfo) -> crate::device::Device {
    let path = info.path().to_string_lossy().into_owned();
    crate::device::Device {
        name: info.product_string().unwrap_or_default().to_owned(),
        vendor_id: info.vendor_id(),
        product_id: info.product_id(),
        manufacturer: info.manufacturer_string().unwrap_or_default().to_owned(),
        serial_number: info.serial_number().unwrap_or_default().to_owned(),
        bus_type: format!("{:?}", info.bus_type()),
        instance_token: crate::device::device_instance_token(&path),
        path,
        firmware: crate::firmware::FirmwareProtocol::Vial,
    }
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
            host_output: Default::default(),
            pictogram_backup_directory: Default::default(),
            requests: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            responses: Default::default(),
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

    #[cfg(test)]
    pub(crate) fn test_pictogram_backup_directory(&self) -> Result<Option<PathBuf>> {
        if let HidBackend::Test { recorder, .. } = &self.backend {
            // Never let a scripted upload write into the real user's library.
            return recorder
                .pictogram_backup_directory
                .lock()
                .unwrap()
                .clone()
                .map(Some)
                .context("Scripted pictogram upload requires a per-test backup directory");
        }
        Ok(None)
    }

    pub fn open_fresh_for(device: &crate::device::Device) -> Result<Self> {
        Self::open_proxy_for(device)
    }

    // Called only by the helper on its main thread, never by the UI process.
    fn open_in_helper(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "linux")]
        if device.uses_bluez_gatt_transport() {
            match crate::linux_ble::LinuxBleDevice::open(device) {
                Ok(bluez_device) => {
                    return Ok(Self {
                        backend: HidBackend::LinuxBle(bluez_device),
                    })
                }
                Err(error) => log::warn!(
                    "Direct BlueZ GATT unavailable: {error:#}; trying validated kernel HID"
                ),
            }
        }
        Self::open_fresh_for_local(device)
    }

    /// Only dedicated owners may use this handle. A shared-output consumer must
    /// not revoke the selected keyboard's transport when its own bridge stops.
    pub(crate) fn retirement_handle(&self) -> Option<HidRetirement> {
        match &self.backend {
            HidBackend::Proxy(proxy) => Some(proxy.retirement_handle()),
            _ => None,
        }
    }

    pub(crate) fn shared_output(&self) -> Option<SharedHidOutput> {
        match &self.backend {
            HidBackend::Proxy(proxy) => Some(SharedHidOutput {
                host_output: proxy.host_output.clone(),
                host_lease: None,
                backend: SharedHidOutputBackend::Proxy(std::sync::Arc::downgrade(proxy)),
            }),
            #[cfg(test)]
            HidBackend::Test { recorder, .. } => Some(SharedHidOutput {
                host_output: recorder.host_output.clone(),
                host_lease: None,
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

    fn open_proxy_for(device: &crate::device::Device) -> Result<Self> {
        Ok(Self {
            backend: HidBackend::Proxy(std::sync::Arc::new(HidProxy::open(device)?)),
        })
    }

    fn try_open_fresh_for(device: &crate::device::Device) -> Result<Self> {
        #[cfg(target_os = "macos")]
        let _hid_lock = macos_hid_operation_lock();
        let api = hidapi::HidApi::new().context("Failed to init hidapi")?;
        // Direct and fallback paths use the exact same eligibility checks.
        // Never open a stale path before validating the current enumeration.
        let mut candidates: Vec<_> = api
            .device_list()
            .filter(|info| device_info_matches(info, device))
            .collect();
        candidates.sort_by_key(|info| info.path().to_string_lossy() != device.path);
        for info in candidates {
            let before = device_from_info(info);
            claim_open_target(&before)?;
            let hid_device = match info.open_device(&api) {
                Ok(opened) => opened,
                Err(error) => {
                    #[cfg(target_os = "macos")]
                    if macos_hid_open_not_permitted(&error) {
                        return Err(MacosHidInputMonitoringRequired.into());
                    }
                    log::debug!("Validated HID open failed: {error}");
                    continue;
                }
            };
            let opened_info = hid_device
                .get_device_info()
                .context("Failed to validate opened HID identity")?;
            let after = device_from_info(&opened_info);
            // Linux get_device_info() returns the first collection on a
            // multi-collection hidraw node, not necessarily its Vial collection.
            // Vial usage was checked in enumeration above; validate the actual
            // handle's physical identity here without rejecting that layout.
            if !device.permits_hid_target(&after)
                || before.path != after.path
                || before.instance_token != after.instance_token
            {
                bail!("HID device disconnected or endpoint identity changed during open");
            }
            let transport = device_transport(device);
            let write_framing = detect_hid_write_framing(&hid_device, transport)?;
            return Ok(Self {
                backend: HidBackend::Local {
                    device: hid_device,
                    transport,
                    write_framing,
                    path: Some(PathBuf::from(after.path)),
                    input_report_polling: std::sync::atomic::AtomicBool::new(false),
                },
            });
        }
        bail!("HID device disconnected or no identity-safe endpoint remains")
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
        let trace = log::log_enabled!(log::Level::Debug)
            .then(|| display_diagnostic_request(data))
            .flatten();
        let Some(trace) = trace else {
            return self.usb_send_untraced(data);
        };
        let started = std::time::Instant::now();
        log::debug!("Display HID request: {trace}");
        let result = self.usb_send_untraced(data);
        match &result {
            Ok(response) => log::debug!(
                "Display HID response: {trace} elapsed_ms={} {}",
                started.elapsed().as_millis(),
                display_diagnostic_response(data, response),
            ),
            Err(error) => log::debug!(
                "Display HID error: {trace} elapsed_ms={} error={error:#}",
                started.elapsed().as_millis(),
            ),
        }
        result
    }

    fn usb_send_untraced(&self, data: &[u8]) -> Result<[u8; MSG_LEN]> {
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

                if let Some(response) = recorder.responses.lock().unwrap().pop_front() {
                    return Ok(response);
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
fn is_qmk_settings_get(data: &[u8]) -> bool {
    data.starts_with(&[CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_GET])
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
fn usb_read_timeout_ms(transport: HidTransport, data: &[u8]) -> i32 {
    if transport.is_bluetooth() {
        WINDOWS_BLE_READ_TIMEOUT_MS
    } else if data.first() == Some(&0xC9) {
        PICTOGRAM_SLOT_COMMIT_READ_TIMEOUT_MS
    } else {
        VIAL_GUI_READ_TIMEOUT_MS
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn usb_send_max_attempts(transport: HidTransport, data: &[u8]) -> usize {
    // Runtime firmware metadata and optional QMK-settings discovery both have
    // safe fallbacks, so an unsupported probe must not hold up the whole
    // connection retry budget.
    if transport.is_bluetooth()
        || is_optional_firmware_version_request(data)
        || is_optional_qmk_settings_query(data)
        || is_qmk_settings_get(data)
        || is_keymap_read_request(data)
        || crate::rmk_native::is_rmk_native_capabilities_request(data)
        || is_optional_dynamic_entry_count_request(data)
        || data.first().is_some_and(|command| {
            // Pictogram BEGIN/DATA/COMMIT are not idempotent: replay can
            // restart storage, fail sequence checks, or reject a committed upload.
            // Safe QUERY/READ commands retain the normal retry budget.
            // A missing write ACK is an uncertain result, never permission to resend.
            (0xB0..=0xB7).contains(command)
                || matches!(command, 0xC2 | 0xC3 | 0xC4 | 0xC7 | 0xC8 | 0xC9)
                || (0xD0..=0xD5).contains(command)
        })
    {
        1
    } else {
        VIAL_GUI_USB_RETRIES
    }
}

/// Trace only display-transfer metadata and unlock control, never keymaps,
/// macro contents, image bytes or the physical unlock key coordinates.
#[cfg(not(target_arch = "wasm32"))]
fn display_diagnostic_request(data: &[u8]) -> Option<String> {
    let command = *data.first()?;
    if (0xC0..=0xCB).contains(&command) {
        let mut trace = format!("opcode=0x{command:02X} bytes={}", data.len());
        if matches!(command, 0xC3 | 0xC8) && data.len() >= 3 {
            trace.push_str(&format!(
                " sequence={}",
                u16::from_le_bytes([data[1], data[2]])
            ));
        } else if command == 0xC7 && data.len() >= 4 {
            trace.push_str(&format!(
                " kind={} slot={}",
                data[1],
                u16::from_le_bytes([data[2], data[3]])
            ));
        }
        Some(trace)
    } else if command == CMD_VIA_VIAL_PREFIX
        && data
            .get(1)
            .is_some_and(|subcommand| (0x05..=0x08).contains(subcommand))
    {
        Some(format!(
            "opcode=0xFE subcommand=0x{:02X} bytes={}",
            data[1],
            data.len()
        ))
    } else {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn display_diagnostic_response(request: &[u8], response: &[u8; MSG_LEN]) -> String {
    let mut trace = format!("reply=0x{:02X} byte1={}", response[0], response[1]);
    if request.first() == Some(&0xC0) && response[0] == 0xC0 {
        trace.push_str(&format!(
            " format={} valid={} slot_protocol={} write_format={}",
            response[2], response[3], response[16], response[17]
        ));
    } else if request.starts_with(&[CMD_VIA_VIAL_PREFIX, 0x07]) {
        trace.push_str(&format!(" counter={}", response[2]));
    }
    trace
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

    let read_timeout_ms = usb_read_timeout_ms(transport, data);

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
        // Keep reading for this command within the original deadline when a
        // delayed response from another pictogram command arrives. Never resend.
        0xC0..=0xCB => resp[0] == cmd,
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn display_diagnostics_exclude_keymaps_macros_and_pixel_payloads() {
        for request in [
            vec![],
            vec![0x04, 7, 8],
            vec![0x0F, 1, 2],
            vec![0xFE, 0x0D, 3],
        ] {
            assert!(display_diagnostic_request(&request).is_none());
        }
        let mut packet = [0x41; MSG_LEN];
        packet[..3].copy_from_slice(&[0xC8, 2, 0]);
        assert_eq!(
            display_diagnostic_request(&packet).unwrap(),
            "opcode=0xC8 bytes=32 sequence=2"
        );
        packet[..4].copy_from_slice(&[0xC7, 1, 24, 0]);
        assert_eq!(
            display_diagnostic_request(&packet).unwrap(),
            "opcode=0xC7 bytes=32 kind=1 slot=24"
        );
        // C1/CB responses contain user pixels after the two protocol bytes.
        packet[..2].copy_from_slice(&[0xCB, 0]);
        assert_eq!(
            display_diagnostic_response(&[0xCB], &packet),
            "reply=0xCB byte1=0"
        );
        // FE05 contains physical unlock key coordinates after its flags.
        packet[..2].copy_from_slice(&[0, 1]);
        assert_eq!(
            display_diagnostic_response(&[0xFE, 5], &packet),
            "reply=0x00 byte1=1"
        );
    }

    #[test]
    fn display_diagnostics_include_capabilities_and_rejected_reply_prefix() {
        let mut response = [0; MSG_LEN];
        response[0] = 0xC0;
        response[2] = 4;
        response[3] = 1;
        response[16] = 2;
        response[17] = 4;
        assert_eq!(
            display_diagnostic_response(&[0xC0], &response),
            "reply=0xC0 byte1=0 format=4 valid=1 slot_protocol=2 write_format=4"
        );
        response[..2].copy_from_slice(&[0xC9, 7]);
        assert_eq!(
            display_diagnostic_response(&[0xC0], &response),
            "reply=0xC9 byte1=7"
        );
        assert_eq!(
            display_diagnostic_request(&[0xFE, 8]).unwrap(),
            "opcode=0xFE subcommand=0x08 bytes=2"
        );
    }

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
    fn qmk_settings_reads_and_standby_session_use_one_usb_attempt() {
        let qmk_get = [CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_GET, 0, 0];

        assert_eq!(usb_send_max_attempts(HidTransport::Usb, &qmk_get), 1);
        assert_eq!(usb_send_max_attempts(HidTransport::Usb, &[0xB6, 1]), 1);
        assert_eq!(usb_send_max_attempts(HidTransport::Usb, &[0xB7, 100, 0]), 1);
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
    fn only_pictogram_slot_commit_gets_the_extended_usb_response_window() {
        assert_eq!(
            usb_read_timeout_ms(HidTransport::Usb, &[0xC9]),
            PICTOGRAM_SLOT_COMMIT_READ_TIMEOUT_MS
        );
        for command in [0xC0, 0xC7, 0xC8, 0xCA, 0xCB] {
            assert_eq!(
                usb_read_timeout_ms(HidTransport::Usb, &[command]),
                VIAL_GUI_READ_TIMEOUT_MS,
                "unexpected extended timeout for opcode 0x{command:02X}"
            );
        }
        assert_eq!(
            usb_read_timeout_ms(HidTransport::Bluetooth, &[0xC9]),
            WINDOWS_BLE_READ_TIMEOUT_MS
        );
    }

    #[test]
    fn pictogram_writes_are_one_shot_but_safe_reads_keep_retries() {
        for command in [0xC2, 0xC3, 0xC4, 0xC7, 0xC8, 0xC9] {
            assert_eq!(
                usb_send_max_attempts(HidTransport::Usb, &[command]),
                1,
                "non-idempotent pictogram opcode 0x{command:02X} retried"
            );
        }
        for command in [0xC0, 0xC1, 0xCA, 0xCB] {
            assert_eq!(
                usb_send_max_attempts(HidTransport::Usb, &[command]),
                VIAL_GUI_USB_RETRIES,
                "safe pictogram opcode 0x{command:02X} lost its retry budget"
            );
        }
    }

    #[test]
    fn pictogram_response_requires_the_current_opcode() {
        let mut response = [0u8; MSG_LEN];
        response[0] = 0xC4;
        assert!(!response_matches_command(&[0xC9], &response));
        response[0] = 0xC9;
        assert!(response_matches_command(&[0xC9], &response));
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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_hid_skips_macos_operation_lock() {
        let (hid, _) = HidDevice::test_device();

        assert!(hid.macos_hid_operation_lock().is_none());
    }
}
