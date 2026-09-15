use crate::firmware::FirmwareProtocol;

const ERGOHAVEN_VENDOR_ID: u16 = 0xE126;
const K04_QUBE_PRODUCT_ID_START: u16 = 0x0071;
const K04_QUBE_PRODUCT_ID_END: u16 = 0x0073;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceIdentity {
    vendor_id: u16,
    product_id: u16,
    serial_number: String,
    manufacturer: String,
    product_name: String,
    bluetooth: bool,
}

impl DeviceIdentity {
    pub(crate) fn matches(&self, device: &Device) -> bool {
        if self.bluetooth != device.is_bluetooth_transport()
            || self.vendor_id != device.vendor_id
            || self.product_id != device.product_id
        {
            return false;
        }

        let serial_number = normalized_device_identity(&device.serial_number);
        if !self.serial_number.is_empty() {
            return self.serial_number == serial_number;
        }

        serial_number.is_empty()
            && self.manufacturer == normalized_device_identity(&device.manufacturer)
            && self.product_name == normalized_device_identity(&device.name)
    }
}

/// Represents a connected Vial/HID keyboard device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: String,
    pub serial_number: String,
    #[serde(default)]
    pub bus_type: String,
    /// HID path used by Vial.
    pub path: String,
    /// Identifies one concrete OS enumeration of this HID endpoint. On Linux
    /// `/dev/hidrawN` can be reused after a quick unplug/replug, so the path
    /// alone is not enough to decide that the existing open handle is alive.
    #[serde(default)]
    pub(crate) instance_token: String,
    pub firmware: FirmwareProtocol,
}

impl Device {
    /// Conservative physical reservation policy shared by the UI and transport.
    /// Endpoint/alias equality always wins, even if enumeration metadata changed.
    /// Missing serials do not prove that two same-model keyboards are distinct.
    pub(crate) fn may_share_physical_device(&self, other: &Device) -> bool {
        if (!self.path.is_empty() && self.path.eq_ignore_ascii_case(&other.path))
            || (!self.instance_token.is_empty() && self.instance_token == other.instance_token)
        {
            return true;
        }
        let left = self.physical_aliases();
        let right = other.physical_aliases();
        if left.iter().any(|alias| right.contains(alias)) {
            return true;
        }
        if !left.is_empty() && !right.is_empty() {
            // USB manufacturer serials and Bluetooth addresses are different
            // namespaces: disagreement across transports does not by itself
            // prove that a dual-mode keyboard is a different physical device.
            if self.is_bluetooth_transport() != other.is_bluetooth_transport() {
                return true;
            }
            return false;
        }
        // Bluetooth service metadata may be absent or disagree with hidraw's
        // VID/PID. Without addresses it cannot prove physical separation.
        if self.is_bluetooth_transport() || other.is_bluetooth_transport() {
            return true;
        }
        self.vendor_id == 0
            || other.vendor_id == 0
            || self.product_id == 0
            || other.product_id == 0
            || (self.vendor_id == other.vendor_id && self.product_id == other.product_id)
    }

    fn physical_aliases(&self) -> Vec<String> {
        let mut aliases = Vec::new();
        let serial = normalized_device_identity(&self.serial_number);
        if !serial.is_empty() {
            aliases.push(serial);
        }
        if let Some(address) = bluez_path_address(&self.path) {
            if !aliases.contains(&address) {
                aliases.push(address);
            }
        }
        aliases
    }

    /// Strong identity for an actual open, deliberately stricter than the
    /// conservative reservation overlap policy. Product names are not identity.
    pub(crate) fn permits_hid_target(&self, actual: &Device) -> bool {
        #[cfg(target_os = "linux")]
        if actual.path.starts_with("/dev/hidraw")
            && (!actual.instance_token.starts_with("sysfs:")
                || (self.path == actual.path && !self.instance_token.starts_with("sysfs:")))
        {
            return false;
        }
        let bluetooth = self.is_bluetooth_transport();
        if bluetooth != actual.is_bluetooth_transport() {
            return false;
        }
        let expected_address = self.bluetooth_address();
        let actual_address = actual.bluetooth_address();
        let same_address =
            bluetooth && expected_address.is_some() && expected_address == actual_address;
        // BlueZ's Modalias may be absent or differ from the kernel's IDs. Only
        // a proven matching Bluetooth address can override that metadata, never
        // a product/manufacturer string or a missing serial.
        if (self.vendor_id != actual.vendor_id || self.product_id != actual.product_id)
            && !same_address
        {
            return false;
        }
        let serial_matches = if bluetooth {
            !self.serial_number.trim().is_empty()
                && normalized_device_identity(&self.serial_number)
                    == normalized_device_identity(&actual.serial_number)
        } else {
            !self.serial_number.trim().is_empty() && self.serial_number == actual.serial_number
        };
        if !self.serial_number.trim().is_empty() && !serial_matches && !same_address {
            return false;
        }
        // Reject internally contradictory cached/service identities as well.
        for description in [self, actual] {
            if bluez_path_address(&description.path).is_some()
                && description.bluetooth_address().is_none()
            {
                return false;
            }
        }
        if bluez_path_address(&self.path).is_some() && !same_address {
            return false;
        }
        let same_path = self.path == actual.path;
        if same_path
            && !self.instance_token.is_empty()
            && self.instance_token != actual.instance_token
        {
            return false;
        }
        // Serial-less fallback by VID/PID/name is unsafe. A path-encoded owning
        // Bluetooth address is strong evidence, not a serial-less substitution.
        if !serial_matches && !same_address {
            return same_path
                && !self.path.is_empty()
                && (!cfg!(target_os = "linux") || !self.instance_token.is_empty());
        }
        true
    }

    fn bluetooth_address(&self) -> Option<String> {
        if !self.is_bluetooth_transport() {
            return None;
        }
        let path_address = bluez_path_address(&self.path);
        let serial = normalized_device_identity(&self.serial_number);
        if let Some(path_address) = path_address {
            return (serial.is_empty() || serial == path_address).then_some(path_address);
        }
        (serial.len() == 12 && serial.bytes().all(|b| b.is_ascii_hexdigit())).then_some(serial)
    }

    pub(crate) fn stable_identity(&self) -> DeviceIdentity {
        DeviceIdentity {
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            serial_number: normalized_device_identity(&self.serial_number),
            manufacturer: normalized_device_identity(&self.manufacturer),
            product_name: normalized_device_identity(&self.name),
            bluetooth: self.is_bluetooth_transport(),
        }
    }

    pub fn is_bluetooth_transport(&self) -> bool {
        self.bus_type.eq_ignore_ascii_case("bluetooth") || {
            let path = self.path.to_ascii_lowercase();
            path.contains("bth") || path.contains("bluetooth") || path.starts_with("bluez-gatt:")
        }
    }

    #[cfg(target_os = "linux")]
    pub fn uses_bluez_gatt_transport(&self) -> bool {
        crate::linux_ble::is_bluez_gatt_path(&self.path)
    }

    pub fn display_name_cache_key(&self) -> String {
        format!(
            "{}\x1f{:04x}\x1f{:04x}\x1f{}\x1f{}\x1f{}",
            self.path,
            self.vendor_id,
            self.product_id,
            self.manufacturer,
            self.serial_number,
            self.name
        )
    }

    pub fn display_name_with_transport(&self, display_name: &str) -> String {
        let display_name = display_name.trim();
        if self.is_bluetooth_transport() {
            format!("{display_name} (Bluetooth)")
        } else if self.is_k04_qube() {
            display_name.to_owned()
        } else {
            format!("{display_name} (USB)")
        }
    }

    fn is_k04_qube(&self) -> bool {
        self.vendor_id == ERGOHAVEN_VENDOR_ID
            && (K04_QUBE_PRODUCT_ID_START..=K04_QUBE_PRODUCT_ID_END).contains(&self.product_id)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn device_instance_token(path: &str) -> String {
    use std::os::unix::fs::MetadataExt;

    // The devtmpfs node can reuse both `/dev/hidrawN` and its device number
    // after a quick replug. The sysfs HID instance path ends in a kernel
    // enumeration id (for example `.0007`) and therefore changes for every
    // concrete USB attachment.
    let sysfs_device = std::path::Path::new(path).file_name().map(|name| {
        std::path::Path::new("/sys/class/hidraw")
            .join(name)
            .join("device")
    });
    if let Some(sysfs_device) = sysfs_device {
        if let Ok(canonical) = std::fs::canonicalize(&sysfs_device) {
            if let Ok(metadata) = std::fs::metadata(&canonical) {
                return format!(
                    "sysfs:{}:{}:{}",
                    canonical.display(),
                    metadata.dev(),
                    metadata.ino()
                );
            }
            return format!("sysfs:{}", canonical.display());
        }
    }

    // A devtmpfs path/device number is not a concrete hidraw enumeration.
    // Missing sysfs identity must remain unavailable rather than becoming the
    // path itself (which can be immediately reused by another attachment).
    if path.starts_with("/dev/hidraw") {
        return String::new();
    }
    std::fs::metadata(path)
        .map(|metadata| {
            format!(
                "dev:{}:{}:{}",
                metadata.dev(),
                metadata.ino(),
                metadata.rdev()
            )
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn device_instance_token(path: &str) -> String {
    path.to_owned()
}

/// Address alias encoded in BlueZ's owning Device1 object path. This is
/// platform independent so imported/cached device descriptions compare alike.
pub(crate) fn bluez_path_address(path: &str) -> Option<String> {
    let path = path.strip_prefix("bluez-gatt:").unwrap_or(path);
    let owner = path.split('/').find_map(|part| part.strip_prefix("dev_"))?;
    let parts: Vec<_> = owner.split('_').collect();
    if parts.len() != 6
        || parts
            .iter()
            .any(|p| p.len() != 2 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return None;
    }
    Some(parts.concat().to_ascii_lowercase())
}

fn normalized_device_identity(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Scans for connected Vial HID keyboard devices.
pub struct DeviceManager {
    devices: Vec<Device>,
}

impl DeviceManager {
    #[cfg(test)]
    pub(crate) fn empty_for_test() -> Self {
        Self { devices: vec![] }
    }

    pub fn new() -> Self {
        #[cfg(not(target_os = "macos"))]
        {
            let mut manager = Self { devices: vec![] };
            manager.scan();
            manager
        }

        // Do not scan here on macOS: hidapi pumps the run loop during enumeration,
        // which can re-enter winit while its event handler is still active.
        #[cfg(target_os = "macos")]
        {
            Self { devices: vec![] }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn scan_devices() -> Result<Vec<Device>, String> {
        let mut devices = Vec::new();

        #[cfg(target_os = "macos")]
        if crate::hid::macos_hid_scan_disabled_for_rosetta() {
            return Ok(devices);
        }

        #[cfg(target_os = "macos")]
        let _hid_lock = crate::hid::macos_hid_operation_lock();
        let api = hidapi::HidApi::new().map_err(|error| format!("HID scan failed: {error}"))?;
        for info in api.device_list() {
            // Filter: Vial usage page 0xFF60, usage 0x61
            if info.usage_page() == 0xFF60 && info.usage() == 0x61 {
                let path = info.path().to_string_lossy().to_string();
                devices.push(Device {
                    name: info
                        .product_string()
                        .unwrap_or("Unknown Keyboard")
                        .to_string(),
                    vendor_id: info.vendor_id(),
                    product_id: info.product_id(),
                    manufacturer: info.manufacturer_string().unwrap_or("").to_string(),
                    serial_number: info.serial_number().unwrap_or("").to_string(),
                    bus_type: format!("{:?}", info.bus_type()),
                    instance_token: device_instance_token(&path),
                    path,
                    firmware: FirmwareProtocol::Vial,
                });
            }
        }

        #[cfg(target_os = "linux")]
        {
            deduplicate_kernel_bluetooth_devices(&mut devices);
            let bluez_devices = crate::linux_ble::scan_devices_cached_nonblocking();
            merge_bluez_vial_devices(&mut devices, bluez_devices);
        }

        Ok(devices)
    }

    pub fn scan(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match Self::scan_devices() {
                Ok(devices) => self.devices = devices,
                Err(error) => log::warn!("{error}"),
            }
        }

        log::info!("Found {} Vial device(s)", self.devices.len());
    }

    pub fn replace_devices(&mut self, devices: Vec<Device>) {
        self.devices = devices;
        log::info!("Found {} Vial device(s)", self.devices.len());
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }
}

#[cfg(target_os = "linux")]
fn normalized_bluetooth_identity(value: &str) -> String {
    normalized_device_identity(value)
}

#[cfg(target_os = "linux")]
fn deduplicate_kernel_bluetooth_devices(devices: &mut Vec<Device>) {
    let mut seen = std::collections::HashSet::new();
    devices.retain(|device| {
        if !device.is_bluetooth_transport() || device.uses_bluez_gatt_transport() {
            return true;
        }
        let identity = normalized_bluetooth_identity(&device.serial_number);
        identity.is_empty() || seen.insert((identity, device.vendor_id, device.product_id))
    });
}

#[cfg(target_os = "linux")]
fn merge_bluez_vial_devices(devices: &mut Vec<Device>, bluez_devices: Vec<Device>) {
    for mut bluez_device in bluez_devices {
        let bluez_identity = normalized_bluetooth_identity(&bluez_device.serial_number);
        let matching_kernel_hid = if bluez_identity.is_empty() {
            None
        } else {
            devices
                .iter()
                .find(|device| {
                    device.is_bluetooth_transport()
                        && !device.uses_bluez_gatt_transport()
                        && normalized_bluetooth_identity(&device.serial_number) == bluez_identity
                })
                .cloned()
        };

        if let Some(kernel_hid) = matching_kernel_hid {
            // BlueZ can expose the resolved GATT service one scan before its
            // Modalias metadata, and that metadata can use a different product
            // id from the kernel HID endpoint. The matching Bluetooth address
            // proves both endpoints belong to the same keyboard, so keep the
            // kernel HID identity unconditionally. Otherwise an in-flight
            // reconnect stops recognizing the keyboard when transport switches.
            bluez_device.vendor_id = kernel_hid.vendor_id;
            bluez_device.product_id = kernel_hid.product_id;
            if bluez_device.manufacturer.trim().is_empty() {
                bluez_device.manufacturer = kernel_hid.manufacturer;
            }
            devices.retain(|device| {
                !device.is_bluetooth_transport()
                    || device.uses_bluez_gatt_transport()
                    || normalized_bluetooth_identity(&device.serial_number) != bluez_identity
            });
            log::info!(
                "Using direct BlueZ GATT for paired Bluetooth device {}",
                bluez_device.name
            );
        }
        devices.push(bluez_device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device(bus_type: &str, path: &str) -> Device {
        Device {
            name: "Test Keyboard".to_owned(),
            vendor_id: 0x1209,
            product_id: 0x2327,
            manufacturer: "Entropy".to_owned(),
            serial_number: "serial".to_owned(),
            bus_type: bus_type.to_owned(),
            path: path.to_owned(),
            instance_token: path.to_owned(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    #[test]
    fn physical_reservation_policy_is_conservative_and_symmetric() {
        let a = test_device("Usb", "endpoint-A");
        let mut b = a.clone();
        b.path = "endpoint-B".into();
        b.instance_token = b.path.clone();
        assert!(a.may_share_physical_device(&b));
        b.serial_number = "different-serial".into();
        assert!(!a.may_share_physical_device(&b));
        assert!(!b.may_share_physical_device(&a));
        b.path = a.path.clone();
        assert!(a.may_share_physical_device(&b));
        b.path = "endpoint-B".into();
        b.serial_number.clear();
        assert!(a.may_share_physical_device(&b));
        assert!(b.may_share_physical_device(&a));
        b.name = "Not identity evidence".into();
        assert!(a.may_share_physical_device(&b));
    }

    #[test]
    fn physical_reservation_covers_bluez_address_and_hidraw_aliases() {
        let mut a = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0010",
        );
        a.serial_number.clear();
        let mut b = test_device("Bluetooth", "/dev/hidraw9");
        b.serial_number = "aa:bb:cc:dd:ee:ff".into();
        b.product_id += 1;
        assert!(a.may_share_physical_device(&b));
        assert!(b.may_share_physical_device(&a));
        b.serial_number = "11:22:33:44:55:66".into();
        assert!(!a.may_share_physical_device(&b));
        b.serial_number.clear();
        assert!(a.may_share_physical_device(&b));
    }

    #[test]
    fn strict_open_never_substitutes_same_product_different_serial() {
        let a = test_device("Usb", "endpoint-A");
        let mut b = a.clone();
        b.serial_number = "different-serial".into();
        assert!(!a.permits_hid_target(&b));
        b.path = "fallback".into();
        b.instance_token = b.path.clone();
        assert!(!a.permits_hid_target(&b));
        b.serial_number = a.serial_number.clone();
        assert!(a.permits_hid_target(&b));
        b.product_id += 1;
        assert!(!a.permits_hid_target(&b));
    }

    #[test]
    fn strict_open_rejects_reused_instance_and_serial_less_fallback() {
        let mut a = test_device("Usb", "endpoint-A");
        let mut b = a.clone();
        b.instance_token = "reused-enumeration".into();
        assert!(!a.permits_hid_target(&b));
        a.serial_number.clear();
        b = a.clone();
        assert!(a.permits_hid_target(&b));
        b.path = "fallback".into();
        b.instance_token = "fallback-instance".into();
        assert!(!a.permits_hid_target(&b));
    }

    #[test]
    fn strict_bluetooth_fallback_requires_owning_address() {
        let mut a = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0010",
        );
        a.serial_number = "AA:BB:CC:DD:EE:FF".into();
        let mut b = a.clone();
        b.path = "/dev/hidraw9".into();
        b.instance_token = "sysfs:/fake/kernel.0001".into();
        b.serial_number = "aa-bb-cc-dd-ee-ff".into();
        assert!(a.permits_hid_target(&b));
        b.serial_number = "11:22:33:44:55:66".into();
        assert!(!a.permits_hid_target(&b));
        a.serial_number = b.serial_number.clone();
        assert!(!a.permits_hid_target(&b));
    }

    #[test]
    fn strong_bluez_address_survives_missing_or_different_modalias() {
        let mut expected = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0010",
        );
        expected.serial_number.clear();
        expected.vendor_id = 0;
        expected.product_id = 0;
        let mut kernel = test_device("Bluetooth", "/dev/hidraw7");
        kernel.serial_number = "AA:BB:CC:DD:EE:FF".into();
        kernel.instance_token = "sysfs:/fake/kernel.0002".into();
        assert!(expected.permits_hid_target(&kernel));
        kernel.serial_number = "11:22:33:44:55:66".into();
        assert!(!expected.permits_hid_target(&kernel));
    }

    #[test]
    fn usb_serial_and_bluetooth_address_are_not_proof_of_distinct_hardware() {
        let mut usb = test_device("Usb", "usb-endpoint");
        usb.serial_number = "manufacturer-serial".into();
        let mut bluetooth = test_device("Bluetooth", "bluetooth-endpoint");
        bluetooth.serial_number = "AA:BB:CC:DD:EE:FF".into();
        assert!(usb.may_share_physical_device(&bluetooth));
        assert!(bluetooth.may_share_physical_device(&usb));
    }

    #[test]
    fn hex_usb_serial_is_not_bluetooth_address_provenance() {
        let mut usb = test_device("Usb", "usb-endpoint");
        usb.serial_number = "123456789abc".into();
        let mut bluetooth = test_device("Bluetooth", "bluetooth-endpoint");
        bluetooth.serial_number = "AA:BB:CC:DD:EE:FF".into();
        assert!(usb.may_share_physical_device(&bluetooth));
        assert!(bluetooth.may_share_physical_device(&usb));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_or_path_only_hidraw_instance_fails_closed() {
        let mut expected = test_device("Usb", "/dev/hidraw999999999");
        assert!(device_instance_token(&expected.path).is_empty());
        assert!(!expected.permits_hid_target(&expected));
        expected.instance_token.clear();
        assert!(!expected.permits_hid_target(&expected));
        expected.instance_token = "sysfs:/fake/kernel.0001".into();
        assert!(expected.permits_hid_target(&expected));
        let mut reused = expected.clone();
        reused.instance_token = "sysfs:/fake/kernel.0002".into();
        assert!(!expected.permits_hid_target(&reused));
    }

    #[test]
    fn detects_bluetooth_from_bus_type() {
        let device = test_device("Bluetooth", "IOService:/AppleUserHIDDevice");

        assert!(device.is_bluetooth_transport());
    }

    #[test]
    fn detects_bluetooth_from_path_hint() {
        let device = test_device("Unknown", "IOService:/AppleBluetoothHIDKeyboard");

        assert!(device.is_bluetooth_transport());
    }

    #[test]
    fn leaves_usb_transport_unmarked() {
        let device = test_device("Usb", "IOService:/AppleUserUSBHostHIDDevice");

        assert!(!device.is_bluetooth_transport());
    }

    #[test]
    fn suffixes_regular_usb_display_name() {
        let mut device = test_device("Usb", "IOService:/AppleUserUSBHostHIDDevice");
        device.vendor_id = ERGOHAVEN_VENDOR_ID;
        device.product_id = K04_QUBE_PRODUCT_ID_END + 1;

        assert_eq!(
            device.display_name_with_transport("Ergohaven K:04"),
            "Ergohaven K:04 (USB)"
        );
    }

    #[test]
    fn leaves_all_k04_qube_usb_display_names_unmarked() {
        for product_id in K04_QUBE_PRODUCT_ID_START..=K04_QUBE_PRODUCT_ID_END {
            let mut device = test_device("Usb", "IOService:/AppleUserUSBHostHIDDevice");
            device.vendor_id = ERGOHAVEN_VENDOR_ID;
            device.product_id = product_id;

            assert_eq!(
                device.display_name_with_transport("Ergohaven K:04"),
                "Ergohaven K:04"
            );
        }
    }

    #[test]
    fn suffixes_bluetooth_display_name() {
        let device = test_device("Bluetooth", "/dev/hidraw7");

        assert_eq!(
            device.display_name_with_transport("Ergohaven K:04"),
            "Ergohaven K:04 (Bluetooth)"
        );
    }

    #[test]
    fn bluetooth_identity_survives_hid_path_changes() {
        let mut before = test_device("Bluetooth", "/dev/hidraw4");
        before.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut after = before.clone();
        after.path = "/dev/hidraw9".to_owned();
        after.serial_number = "c6-9e-29-c4-f4-c7".to_owned();

        assert!(before.stable_identity().matches(&after));
    }

    #[test]
    fn bluetooth_identity_rejects_a_different_serial_number() {
        let mut expected = test_device("Bluetooth", "/dev/hidraw4");
        expected.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut other = expected.clone();
        other.serial_number = "D7:AF:3A:D5:05:D8".to_owned();

        assert!(!expected.stable_identity().matches(&other));
    }

    #[test]
    fn serial_less_identity_requires_matching_product_metadata() {
        let mut expected = test_device("Bluetooth", "/dev/hidraw4");
        expected.serial_number.clear();
        let mut same_model = expected.clone();
        same_model.path = "/dev/hidraw9".to_owned();
        let mut other_model = same_model.clone();
        other_model.name = "Other Keyboard".to_owned();

        assert!(expected.stable_identity().matches(&same_model));
        assert!(!expected.stable_identity().matches(&other_model));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_bluez_gatt_transport_path() {
        let device = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF/service0010",
        );

        assert!(device.uses_bluez_gatt_transport());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn normalizes_bluez_and_hidraw_bluetooth_addresses_equally() {
        assert_eq!(
            normalized_bluetooth_identity("AA:BB:CC:DD:EE:FF"),
            normalized_bluetooth_identity("aa-bb-cc-dd-ee-ff")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn prefers_bluez_gatt_over_matching_kernel_hid_device() {
        let mut kernel_hid = test_device("Bluetooth", "/dev/hidraw7");
        kernel_hid.serial_number = "c6:9e:29:c4:f4:c7".to_owned();
        let mut bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        bluez.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut devices = vec![kernel_hid.clone()];

        merge_bluez_vial_devices(&mut devices, vec![bluez]);

        assert_eq!(devices.len(), 1);
        assert!(devices[0].uses_bluez_gatt_transport());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bluez_transport_keeps_kernel_identity_while_services_settle() {
        let mut kernel_hid = test_device("Bluetooth", "/dev/hidraw7");
        kernel_hid.serial_number = "c6:9e:29:c4:f4:c7".to_owned();
        let identity = kernel_hid.stable_identity();
        let mut bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        bluez.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        bluez.vendor_id = 0xE126;
        bluez.product_id = 0x0041;
        bluez.manufacturer.clear();
        let mut devices = vec![kernel_hid];

        merge_bluez_vial_devices(&mut devices, vec![bluez]);

        assert_eq!(devices.len(), 1);
        assert!(devices[0].uses_bluez_gatt_transport());
        assert_eq!(devices[0].vendor_id, 0x1209);
        assert_eq!(devices[0].product_id, 0x2327);
        assert_eq!(devices[0].manufacturer, "Entropy");
        assert!(identity.matches(&devices[0]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn keeps_bluez_gatt_when_kernel_hid_is_unavailable() {
        let bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        let mut devices = Vec::new();

        merge_bluez_vial_devices(&mut devices, vec![bluez.clone()]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, bluez.path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn usb_device_does_not_hide_bluez_gatt_fallback() {
        let mut usb = test_device("Usb", "/dev/hidraw2");
        usb.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        bluez.serial_number = usb.serial_number.clone();
        let mut devices = vec![usb];

        merge_bluez_vial_devices(&mut devices, vec![bluez]);

        assert_eq!(devices.len(), 2);
        assert!(devices[1].uses_bluez_gatt_transport());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn deduplicates_kernel_hid_collections_for_one_bluetooth_device() {
        let mut first = test_device("Bluetooth", "/dev/hidraw4");
        first.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut second = first.clone();
        second.path = "/dev/hidraw6".to_owned();
        let mut devices = vec![first.clone(), second];

        deduplicate_kernel_bluetooth_devices(&mut devices);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, first.path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn keeps_distinct_bluetooth_identities() {
        let mut first = test_device("Bluetooth", "/dev/hidraw4");
        first.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut second = first.clone();
        second.path = "/dev/hidraw6".to_owned();
        second.serial_number = "D7:AF:3A:D5:05:D8".to_owned();
        let mut devices = vec![first, second];

        deduplicate_kernel_bluetooth_devices(&mut devices);

        assert_eq!(devices.len(), 2);
    }
}
