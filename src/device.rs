use crate::firmware::FirmwareProtocol;

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
    pub firmware: FirmwareProtocol,
}

impl Device {
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
            path.contains("bth") || path.contains("bluetooth")
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
        let transport = if self.is_bluetooth_transport() {
            "Bluetooth"
        } else {
            "USB"
        };
        format!("{} ({transport})", display_name.trim())
    }
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
    pub fn scan_devices() -> Vec<Device> {
        let mut devices = Vec::new();

        #[cfg(target_os = "macos")]
        if crate::hid::macos_hid_scan_disabled_for_rosetta() {
            return devices;
        }

        #[cfg(target_os = "macos")]
        let _hid_lock = crate::hid::macos_hid_operation_lock();
        if let Ok(api) = hidapi::HidApi::new() {
            for info in api.device_list() {
                // Filter: Vial usage page 0xFF60, usage 0x61
                if info.usage_page() == 0xFF60 && info.usage() == 0x61 {
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
                        path: info.path().to_string_lossy().to_string(),
                        firmware: FirmwareProtocol::Vial,
                    });
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            deduplicate_kernel_bluetooth_devices(&mut devices);
            let bluez_devices = crate::linux_ble::scan_devices();
            merge_bluez_fallback_devices(&mut devices, bluez_devices);
        }

        devices
    }

    pub fn scan(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.devices = Self::scan_devices();
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
fn merge_bluez_fallback_devices(devices: &mut Vec<Device>, bluez_devices: Vec<Device>) {
    for bluez_device in bluez_devices {
        let bluez_identity = normalized_bluetooth_identity(&bluez_device.serial_number);
        let kernel_hid_available = !bluez_identity.is_empty()
            && devices.iter().any(|device| {
                device.is_bluetooth_transport()
                    && !device.uses_bluez_gatt_transport()
                    && normalized_bluetooth_identity(&device.serial_number) == bluez_identity
            });

        if kernel_hid_available {
            log::info!(
                "Using the Linux kernel HID transport for paired Bluetooth device {}",
                bluez_device.name
            );
        } else {
            devices.push(bluez_device);
        }
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
            firmware: FirmwareProtocol::Vial,
        }
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
    fn suffixes_usb_display_name() {
        let device = test_device("Usb", "IOService:/AppleUserUSBHostHIDDevice");

        assert_eq!(
            device.display_name_with_transport("Ergohaven K:04"),
            "Ergohaven K:04 (USB)"
        );
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
    fn prefers_kernel_hid_over_matching_bluez_gatt_device() {
        let mut kernel_hid = test_device("Bluetooth", "/dev/hidraw7");
        kernel_hid.serial_number = "c6:9e:29:c4:f4:c7".to_owned();
        let mut bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        bluez.serial_number = "C6:9E:29:C4:F4:C7".to_owned();
        let mut devices = vec![kernel_hid.clone()];

        merge_bluez_fallback_devices(&mut devices, vec![bluez]);

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].path, kernel_hid.path);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn keeps_bluez_gatt_when_kernel_hid_is_unavailable() {
        let bluez = test_device(
            "Bluetooth",
            "bluez-gatt:/org/bluez/hci0/dev_C6_9E_29_C4_F4_C7/service002a",
        );
        let mut devices = Vec::new();

        merge_bluez_fallback_devices(&mut devices, vec![bluez.clone()]);

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

        merge_bluez_fallback_devices(&mut devices, vec![bluez]);

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
