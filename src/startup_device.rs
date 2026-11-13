//! Device targeting shared by GUI startup and console export.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct StartupDevicePreference {
    pub(crate) identity: DeviceIdentity,
    pub(crate) name: String,
}

impl StartupDevicePreference {
    pub(crate) fn from_device(device: &Device) -> Self {
        Self {
            identity: device.stable_identity(),
            name: device.display_name_with_transport(&device.name),
        }
    }
}

#[derive(Clone)]
pub(super) enum StartupDeviceTarget {
    Filter(String),
    Preferred(StartupDevicePreference),
}

impl StartupDeviceTarget {
    pub(super) fn select(&self, devices: &[Device]) -> Option<usize> {
        match self {
            Self::Filter(filter) => select_device(devices, Some(filter)).ok(),
            Self::Preferred(preference) => {
                let mut matches = devices
                    .iter()
                    .enumerate()
                    .filter_map(|(i, device)| preference.identity.matches(device).then_some(i));
                let first = matches.next()?;
                matches.next().is_none().then_some(first)
            }
        }
    }
}

impl EntropyApp {
    /// The command line overrides the saved preference for this launch only.
    pub(crate) fn with_startup_device(mut self, filter: Option<String>) -> Self {
        if let Some(filter) = filter {
            self.startup_device = Some(StartupDeviceTarget::Filter(filter));
        }
        self
    }
}

/// `--device` matching: a hex `VID:PID`, the exact HID path, or a
/// case-insensitive fragment of the product name.
pub(super) fn device_matches(device: &Device, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return false;
    }
    if let Some((vendor, product)) = filter.split_once(':') {
        if let (Ok(vendor), Ok(product)) = (
            u16::from_str_radix(vendor.trim(), 16),
            u16::from_str_radix(product.trim(), 16),
        ) {
            if device.vendor_id == vendor && device.product_id == product {
                return true;
            }
        }
    }
    device.path == filter || device.name.to_lowercase().contains(&filter.to_lowercase())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NoDevice {
    Missing,
    Ambiguous(Vec<usize>),
}

/// The index of the one keyboard the request means.
pub(super) fn select_device(devices: &[Device], filter: Option<&str>) -> Result<usize, NoDevice> {
    let candidates: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| filter.is_none_or(|filter| device_matches(device, filter)))
        .map(|(index, _)| index)
        .collect();
    match candidates.as_slice() {
        [index] => Ok(*index),
        [] => Err(NoDevice::Missing),
        _ => Err(NoDevice::Ambiguous(candidates)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> Device {
        Device {
            name: name.into(),
            vendor_id: 0xe126,
            product_id: 0x0071,
            manufacturer: "Test".into(),
            serial_number: name.into(),
            bus_type: "Usb".into(),
            path: format!("/test/{name}"),
            instance_token: "original".into(),
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn app(
        filter: &str,
    ) -> (
        EntropyApp,
        mpsc::Receiver<(Device, mpsc::Sender<ConnectTaskMessage>)>,
    ) {
        let mut app = EntropyApp::new_inert_for_test().with_startup_device(Some(filter.into()));
        let (tx, rx) = mpsc::channel();
        app.test_connect_requests = Some(tx);
        #[cfg(target_os = "linux")]
        {
            app.test_device_scan_udev_access = Some(crate::app::app_settings_ui::UdevAccess {
                vial: true,
                rynk: true,
            });
        }
        (app, rx)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn startup_device_usb_target_requires_rules_even_when_ble_is_reachable() {
        use crate::app::app_settings_ui::UdevAccess;
        let (mut app, requests) = app("Phenom");
        app.test_device_scan_udev_access = Some(UdevAccess {
            vial: false,
            rynk: false,
        });
        let mut other = device("Macropad");
        other.bus_type = "Bluetooth".into();
        other.path = "bluez-gatt:/org/bluez/hci0/dev_00/service0020".into();
        let target = device("Phenom Mini");
        app.apply_device_scan_result(vec![other.clone(), target.clone()]);
        assert!(requests.try_recv().is_err());
        assert!(app.startup_target_needs_udev(UdevAccess {
            vial: false,
            rynk: false
        }));
        app.test_device_scan_udev_access = Some(UdevAccess {
            vial: true,
            rynk: true,
        });
        app.apply_device_scan_result(vec![other, target.clone()]);
        assert_eq!(requests.try_recv().unwrap().0.path, target.path);
    }

    #[test]
    fn startup_device_waits_for_target_then_connects_once() {
        let (mut app, requests) = app("Phenom");
        let other = device("Macropad");
        app.apply_device_scan_result(vec![]);
        app.apply_device_scan_result(vec![other.clone()]);
        assert!(requests.try_recv().is_err());
        let target = device("Phenom Mini");
        app.apply_device_scan_result(vec![other.clone(), target.clone()]);
        assert_eq!(requests.try_recv().unwrap().0.path, target.path);
        assert!(app.startup_device.is_none());
        app.apply_device_scan_result(vec![target, other]);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn startup_device_ambiguous_filter_keeps_picker_available() {
        let (mut app, requests) = app("Mini");
        app.apply_device_scan_result(vec![device("Phenom Mini"), device("K04 Mini")]);
        assert!(requests.try_recv().is_err());
        assert!(app.selected_device.is_none());
        assert_eq!(app.device_manager.devices().len(), 2);
        assert!(app.startup_device.is_some());
    }

    #[test]
    fn startup_device_manual_pick_cancels_waiting_target() {
        let (mut app, requests) = app("Phenom");
        let other = device("Macropad");
        app.apply_device_scan_result(vec![other.clone()]);
        app.connect_picked_device(0);
        assert_eq!(requests.try_recv().unwrap().0.path, other.path);
        assert!(app.startup_device.is_none());
        app.apply_device_scan_result(vec![device("Phenom Mini"), other]);
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn startup_device_preference_survives_storage_and_path_changes() {
        let mut target = device("Phenom Mini");
        let preference = StartupDevicePreference::from_device(&target);
        let mut settings = AppSettings::default();
        settings.preferred_startup_device = Some(preference.clone());
        let json = serde_json::to_string(&settings).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.preferred_startup_device, Some(preference.clone()));
        target.path = "/different/path".into();
        target.instance_token = "new".into();
        let saved = StartupDeviceTarget::Preferred(restored.preferred_startup_device.unwrap());
        assert_eq!(saved.select(&[device("Macropad"), target.clone()]), Some(1));
        assert_eq!(saved.select(&[target.clone(), target]), None);
        let legacy: AppSettings = serde_json::from_str(r#"{"language":"english"}"#).unwrap();
        assert!(legacy.preferred_startup_device.is_none());
    }

    #[test]
    fn startup_device_cli_overrides_preference_without_saving_it() {
        let saved = StartupDevicePreference::from_device(&device("Macropad"));
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.preferred_startup_device = Some(saved.clone());
        app.startup_device = Some(StartupDeviceTarget::Preferred(saved.clone()));
        let app = app.with_startup_device(Some("Phenom".into()));
        assert_eq!(
            app.startup_device
                .unwrap()
                .select(&[device("Macropad"), device("Phenom Mini")]),
            Some(1)
        );
        assert_eq!(app.app_settings.preferred_startup_device, Some(saved));
    }

    #[test]
    fn startup_device_filter_matches_name_vid_pid_and_exact_path() {
        let target = device("Phenom Mini");
        for filter in ["pHeNoM", "E126:0071", "e126:71", "/test/Phenom Mini"] {
            assert!(device_matches(&target, filter), "{filter}");
        }
        for filter in ["", "  ", "Macropad", "e126:0072", "/test/Phenom"] {
            // A path fragment is also a name fragment only if it occurs in the name.
            assert!(!device_matches(&target, filter), "{filter}");
        }
    }
}
