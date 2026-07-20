use crate::firmware::FirmwareProtocol;

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
    pub fn is_bluetooth_transport(&self) -> bool {
        self.bus_type.eq_ignore_ascii_case("bluetooth") || {
            let path = self.path.to_ascii_lowercase();
            path.contains("bth") || path.contains("bluetooth")
        }
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
    pub fn scan_devices() -> anyhow::Result<Vec<Device>> {
        let mut devices = Vec::new();

        #[cfg(target_os = "macos")]
        if crate::hid::macos_hid_scan_disabled_for_rosetta() {
            return Ok(devices);
        }

        #[cfg(target_os = "macos")]
        let _hid_lock = crate::hid::macos_hid_operation_lock();
        let api =
            hidapi::HidApi::new().map_err(|error| anyhow::anyhow!("HID scan failed: {error}"))?;
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

        Ok(devices)
    }

    pub fn scan(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match Self::scan_devices() {
                Ok(devices) => self.devices = devices,
                Err(error) => log::warn!("device scan failed: {error}"),
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
}
