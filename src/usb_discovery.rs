//! Discovery while a Bluetooth owner is active. Never send VIA/Vial requests,
//! open a Bluetooth HID endpoint, or invoke the BlueZ discovery/cache pipeline.
use super::*;

#[cfg(target_os = "linux")]
pub(super) fn scan() -> Result<Vec<Device>, String> {
    static LAST: std::sync::Mutex<Option<LinuxSnapshot>> = std::sync::Mutex::new(None);
    let root = std::path::Path::new("/sys/class/hidraw");
    scan_unless_unchanged(root, &LAST, || {
        // Pinned hidapi linux-native-basic-udev enumerates sysfs metadata/report
        // descriptors only (no hidraw open/ioctl). Do not call scan_devices(): it
        // additionally enters BlueZ. Return USB endpoints only.
        let api = hidapi::HidApi::new().map_err(|e| format!("USB metadata scan failed: {e}"))?;
        validate_linux_snapshot(root, api
            .device_list()
            .filter(|i| {
                matches!(i.bus_type(), hidapi::BusType::Usb)
                    && i.usage_page() == 0xFF60
                    && i.usage() == 0x61
            })
            .map(|i| {
                let path = i.path().to_string_lossy().to_string();
                let mut d = Device {
                    name: i.product_string().unwrap_or("Unknown Keyboard").into(),
                    vendor_id: i.vendor_id(),
                    product_id: i.product_id(),
                    manufacturer: i.manufacturer_string().unwrap_or("").into(),
                    serial_number: i.serial_number().unwrap_or("").into(),
                    bus_type: "Usb".into(),
                    instance_token: device_instance_token(&path),
                    path,
                    firmware: FirmwareProtocol::Vial,
                };
                d.normalize_discovered_identity();
                d
            })
            .collect())
    })
}

/// A hidraw node: its name, its `device` link and the inode of its sysfs
/// directory, which is made anew whenever the node is.
#[cfg(target_os = "linux")]
type HidrawNode = (std::ffi::OsString, std::path::PathBuf, u64);

/// A complete scan and the hidraw nodes it was taken over.
#[cfg(target_os = "linux")]
struct LinuxSnapshot {
    nodes: Vec<HidrawNode>,
    devices: Vec<Device>,
}

// hidapi reads every sysfs attribute of every hidraw node and all its ancestors:
// about 100 ms of CPU a pass, every 2 s. A node's `device` link names its HID
// instance, which the kernel numbers anew on every registration, and a driver
// rebind that keeps the instance still makes the node anew, under a new inode.
// So while the nodes stay the same the last complete result still holds. Only
// a complete (validated) result is kept, and only one the nodes did not change
// under; anything else is retried in full.
#[cfg(target_os = "linux")]
fn scan_unless_unchanged(
    root: &std::path::Path,
    last: &std::sync::Mutex<Option<LinuxSnapshot>>,
    scan: impl FnOnce() -> Result<Vec<Device>, String>,
) -> Result<Vec<Device>, String> {
    let nodes = hidraw_nodes(root);
    if let (Some(nodes), Some(last)) = (&nodes, &*last.lock().unwrap_or_else(|e| e.into_inner())) {
        if last.nodes == *nodes {
            return Ok(last.devices.clone());
        }
    }
    let result = scan();
    // A node unbound or rebound while hidapi enumerated reads differently
    // after the scan than before it: what the scan saw is not kept.
    let unchanged = nodes.filter(|nodes| hidraw_nodes(root).as_ref() == Some(nodes));
    *last.lock().unwrap_or_else(|e| e.into_inner()) = match (unchanged, &result) {
        (Some(nodes), Ok(devices)) => Some(LinuxSnapshot {
            nodes,
            devices: devices.clone(),
        }),
        _ => None,
    };
    result
}

/// Every hidraw node, sorted; `None` when any is unreadable.
#[cfg(target_os = "linux")]
fn hidraw_nodes(root: &std::path::Path) -> Option<Vec<HidrawNode>> {
    use std::os::unix::fs::MetadataExt;

    let mut nodes = std::fs::read_dir(root)
        .ok()?
        .map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            Some((
                entry.file_name(),
                std::fs::read_link(path.join("device")).ok()?,
                std::fs::metadata(&path).ok()?.ino(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    nodes.sort();
    Some(nodes)
}

#[cfg(target_os = "linux")]
#[path = "linux_usb_metadata.rs"]
mod linux_metadata;

// hidapi 2.6.6 swallows class enumeration, identity and descriptor errors.
// Its Ok result authorizes removals only after independent sysfs validation.
#[cfg(target_os = "linux")]
fn validate_linux_snapshot(root: &std::path::Path, devices: Vec<Device>) -> Result<Vec<Device>, String> {
    linux_metadata::validate(root, devices)
}

// Stop before an adapter's USB ancestor: Bluetooth peripherals often sit under
// an USB Bluetooth radio. Only USB VID/PID ancestors prove a USB peripheral.
#[cfg(any(target_os = "windows", test))]
fn windows_usb_ancestry(ids: impl IntoIterator<Item = String>) -> bool {
    for id in ids {
        let id = id.to_ascii_uppercase();
        if id.starts_with("BTH") || id.starts_with("BLUETOOTH\\") {
            return false;
        }
        if id.starts_with("USB\\VID_") {
            return true;
        }
    }
    false
}

#[cfg(target_os = "windows")]
pub(super) fn scan() -> Result<Vec<Device>, String> {
    windows_usb::scan()
}

#[cfg(target_os = "macos")]
pub(super) fn scan() -> Result<Vec<Device>, String> {
    super::macos_usb_discovery::scan()
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub(super) fn scan() -> Result<Vec<Device>, String> {
    Err("USB-only discovery is not available on this platform; Bluetooth protection remains enabled".into())
}

#[cfg(target_os = "windows")]
mod windows_usb {
    use super::*;
    use std::{
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Devices::{DeviceAndDriverInstallation::*, HumanInterfaceDevice::*, Properties::*},
        Foundation::*,
        Storage::FileSystem::*,
    };
    fn text(w: &[u16]) -> String {
        String::from_utf16_lossy(&w[..w.iter().position(|c| *c == 0).unwrap_or(w.len())])
    }
    // Registry/Config Manager calls only. No HID handle exists until USB ancestry
    // has been established, unlike hidapi's generic Windows enumeration.
    unsafe fn is_usb(interface: *const u16) -> Result<bool, String> {
        let mut kind = 0;
        let mut id = [0u16; 512];
        let mut bytes = size_of_val(&id) as u32;
        if CM_Get_Device_Interface_PropertyW(
            interface,
            &DEVPKEY_Device_InstanceId,
            &mut kind,
            id.as_mut_ptr().cast(),
            &mut bytes,
            0,
        ) != CR_SUCCESS
            || kind != DEVPROP_TYPE_STRING
        {
            return Err("USB ancestry instance lookup incomplete".into());
        }
        let mut node = 0;
        if CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS {
            return Err("USB ancestry node lookup incomplete".into());
        }
        let mut ids = Vec::new();
        for _ in 0..32 {
            let mut count = 0;
            if CM_Get_Device_ID_Size(&mut count, node, 0) != CR_SUCCESS || count > 32768 {
                return Err("USB ancestry ID size unavailable".into());
            }
            let mut id = vec![0u16; count as usize + 1];
            if CM_Get_Device_IDW(node, id.as_mut_ptr(), id.len() as u32, 0) != CR_SUCCESS {
                return Err("USB ancestry ID unavailable".into());
            }
            let id = text(&id);
            let upper = id.to_ascii_uppercase();
            ids.push(id);
            if upper.starts_with("BTH")
                || upper.starts_with("BLUETOOTH\\")
                || upper.starts_with("USB\\VID_")
            {
                return Ok(windows_usb_ancestry(ids));
            }
            // Only an observed root terminates a non-USB ancestry normally.
            // A failed parent query on any other node is an incomplete scan.
            if upper.starts_with("HTREE\\ROOT\\") {
                return Ok(false);
            }
            let mut parent = 0;
            if CM_Get_Parent(&mut parent, node, 0) != CR_SUCCESS {
                return Err("USB ancestry parent lookup incomplete".into());
            }
            node = parent;
        }
        Err("USB ancestry depth exceeded".into())
    }
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    struct Preparsed(PHIDP_PREPARSED_DATA);
    impl Drop for Preparsed {
        fn drop(&mut self) {
            unsafe {
                HidD_FreePreparsedData(self.0);
            }
        }
    }
    unsafe fn string(
        handle: HANDLE,
        get: unsafe extern "system" fn(HANDLE, *mut std::ffi::c_void, u32) -> bool,
    ) -> Result<String, String> {
        let mut buffer = [0u16; 256];
        if get(
            handle,
            buffer.as_mut_ptr().cast(),
            size_of_val(&buffer) as u32,
        ) {
            Ok(text(&buffer))
        } else {
            Err(format!(
                "USB identity string unavailable: {}",
                GetLastError()
            ))
        }
    }
    unsafe fn usb_info(path: &[u16]) -> Result<Option<Device>, String> {
        if !is_usb(path.as_ptr())? {
            return Ok(None);
        }
        // Metadata-only, shared handle; never send/read reports or firmware data.
        let handle = CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!("USB metadata open incomplete: {}", GetLastError()));
        }
        let handle = Handle(handle);
        let mut pp = 0;
        if !HidD_GetPreparsedData(handle.0, &mut pp) {
            return Err("USB capabilities unavailable".into());
        }
        let pp = Preparsed(pp);
        let mut caps: HIDP_CAPS = zeroed();
        if HidP_GetCaps(pp.0, &mut caps) != HIDP_STATUS_SUCCESS {
            return Err("USB capabilities incomplete".into());
        }
        if caps.UsagePage != 0xFF60 || caps.Usage != 0x61 {
            return Ok(None);
        }
        let mut attrs: HIDD_ATTRIBUTES = zeroed();
        attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
        if !HidD_GetAttributes(handle.0, &mut attrs) {
            return Err("USB attributes unavailable".into());
        }
        let path = text(path);
        let product = string(handle.0, HidD_GetProductString)?;
        let mut d = Device {
            name: if product.is_empty() {
                "Unknown Keyboard".into()
            } else {
                product
            },
            vendor_id: attrs.VendorID,
            product_id: attrs.ProductID,
            manufacturer: string(handle.0, HidD_GetManufacturerString)?,
            serial_number: string(handle.0, HidD_GetSerialNumberString)?,
            bus_type: "Usb".into(),
            instance_token: device_instance_token(&path),
            path,
            firmware: FirmwareProtocol::Vial,
        };
        d.normalize_discovered_identity();
        Ok(Some(d))
    }
    pub(super) fn scan() -> Result<Vec<Device>, String> {
        unsafe {
            let mut guid = zeroed();
            HidD_GetHidGuid(&mut guid);
            for _ in 0..3 {
                let mut len = 0;
                let status = CM_Get_Device_Interface_List_SizeW(
                    &mut len,
                    &guid,
                    null(),
                    CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
                );
                if status != CR_SUCCESS || len > 16 * 1024 * 1024 {
                    return Err(format!("USB interface-list size failed: {status}"));
                }
                let mut list = vec![0u16; len.max(1) as usize];
                let status = CM_Get_Device_Interface_ListW(
                    &guid,
                    null(),
                    list.as_mut_ptr(),
                    list.len() as u32,
                    CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
                );
                if status == CR_BUFFER_SMALL {
                    continue;
                }
                if status != CR_SUCCESS {
                    return Err(format!("USB interface-list read failed: {status}"));
                }
                let mut devices = Vec::new();
                for entry in list.split_inclusive(|c| *c == 0) {
                    if entry.len() <= 1 {
                        break;
                    }
                    if let Some(device) = usb_info(entry)? {
                        devices.push(device);
                    }
                }
                return Ok(devices);
            }
            Err("USB interface list changed repeatedly; retaining previous inventory".into())
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
pub(super) fn linux_fixture_snapshot(case: u8) -> Result<Vec<Device>, String> {
    use std::{fs, os::unix::{fs::{symlink, MetadataExt}}};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("class/hidraw");
    let usb = temp.path().join("devices/usb1/1-1");
    let hid = usb.join("1-1:1.0/0003:FFFF:FFFF.0001");
    let raw = root.join("hidraw0");
    fs::create_dir_all(&raw).unwrap(); fs::create_dir_all(&hid).unwrap();
    // Match the validator's canonical sysfs identity, including when TMPDIR
    // is an inherited /proc/self/fd directory inside the no-hardware sandbox.
    let hid = fs::canonicalize(hid).unwrap();
    symlink(&hid, raw.join("device")).unwrap();
    fs::write(hid.join("uevent"), "HID_ID=0003:0000FFFF:0000FFFF\nHID_NAME=Fixture keyboard\nHID_UNIQ=\n").unwrap();
    fs::write(hid.join("report_descriptor"), [0x06, 0x60, 0xff, 0x09, 0x61, 0xa1, 0x01, 0xc0]).unwrap();
    for (file, value) in [("idVendor", "ffff\n"), ("idProduct", "ffff\n"), ("product", "Fixture keyboard\n"), ("manufacturer", "Fixture\n")] {
        fs::write(usb.join(file), value).unwrap();
    }
    let mut usb_descriptor = [18, 1, 0, 2, 0, 0, 0, 64, 0xff, 0xff, 0xff, 0xff, 0, 1, 1, 2, 0, 1];
    fs::write(usb.join("descriptors"), usb_descriptor).unwrap();
    let metadata = fs::metadata(&hid).unwrap();
    let mut devices = vec![Device {
        name: "Fixture keyboard".into(), manufacturer: "Fixture".into(), serial_number: "".into(),
        vendor_id: 0xffff, product_id: 0xffff, bus_type: "Usb".into(), path: "/dev/hidraw0".into(),
        instance_token: format!("sysfs:{}:{}:{}", hid.display(), metadata.dev(), metadata.ino()),
        firmware: FirmwareProtocol::Vial,
    }];
    match case {
        0 => {},
        1 => { return validate_linux_snapshot(&root.join("missing"), vec![]); },
        2 => { fs::remove_dir_all(&hid).unwrap(); },
        3 => { fs::remove_file(hid.join("uevent")).unwrap(); },
        4 => { fs::write(hid.join("uevent"), "HID_ID=broken\n").unwrap(); },
        5 => { fs::remove_file(hid.join("report_descriptor")).unwrap(); },
        6 => { fs::write(hid.join("report_descriptor"), [0x06, 0x60]).unwrap(); },
        7 => { fs::remove_file(usb.join("manufacturer")).unwrap(); },
        8 => { fs::remove_file(usb.join("product")).unwrap(); },
        9 => { fs::write(hid.join("uevent"), "HID_ID=0003:0000FFFF:0000FFFF\nHID_NAME=Fixture keyboard\n").unwrap(); },
        10 => { devices.clear(); }, // hidapi swallowed endpoint/descriptor failure
        11 => { devices[0].manufacturer.clear(); },
        12 => { fs::remove_dir_all(&raw).unwrap(); devices.clear(); },
        13 => { // Bluetooth bus stops before descriptor/USB ancestors.
            fs::write(hid.join("uevent"), "HID_ID=0005:0000FFFF:0000FFFF\n").unwrap();
            fs::remove_file(hid.join("report_descriptor")).unwrap(); devices.clear();
        },
        14 => { fs::write(hid.join("report_descriptor"), [0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0xc0]).unwrap(); devices.clear(); },
        15 => { devices[0].instance_token.clear(); },
        16 => { fs::remove_file(usb.join("idVendor")).unwrap(); },
        17 => { devices[0].serial_number = "invented".into(); },
        18..=20 => { // USB index zero proves legitimate omitted identity strings.
            if case != 19 {
                usb_descriptor[14] = 0;
                fs::remove_file(usb.join("manufacturer")).unwrap();
                devices[0].manufacturer.clear();
            }
            if case != 18 {
                usb_descriptor[15] = 0;
                fs::remove_file(usb.join("product")).unwrap();
                devices[0].name = "Unknown Keyboard".into();
            }
            fs::write(usb.join("descriptors"), usb_descriptor).unwrap();
        },
        21 => { fs::write(usb.join("product"), "\n").unwrap(); devices[0].name.clear(); },
        22 => { fs::remove_file(usb.join("descriptors")).unwrap(); },
        23 => { fs::write(usb.join("descriptors"), &usb_descriptor[..17]).unwrap(); },
        24..=27 => {
            let index = match case { 24 => 0, 25 => 1, 26 => 8, _ => 10 };
            usb_descriptor[index] = 0;
            fs::write(usb.join("descriptors"), usb_descriptor).unwrap();
        },
        28..=29 => {
            let field = if case == 28 { "manufacturer" } else { "product" };
            fs::remove_file(usb.join(field)).unwrap();
            fs::create_dir(usb.join(field)).unwrap();
        },
        30 => { fs::write(usb.join("product"), [0xff]).unwrap(); },
        31..=32 => { // An index-zero claim contradicting a present sysfs string.
            usb_descriptor[if case == 31 { 14 } else { 15 }] = 0;
            fs::write(usb.join("descriptors"), usb_descriptor).unwrap();
        },
        _ => panic!("unknown Linux metadata fixture"),
    }
    validate_linux_snapshot(&root, devices)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_metadata_accepts_descriptor_proven_omitted_identity_strings() {
        for case in 18..=20 {
            let devices = linux_fixture_snapshot(case).unwrap();
            assert_eq!(devices.len(), 1);
            assert_eq!(devices[0].manufacturer, if case == 19 { "Fixture" } else { "" });
            assert_eq!(devices[0].name, if case == 18 { "Fixture keyboard" } else { "Unknown Keyboard" });
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_metadata_preserves_present_empty_product() {
        let devices = linux_fixture_snapshot(21).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "");
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_metadata_rejects_invalid_usb_identity_evidence() {
        for case in 22..=32 {
            assert!(linux_fixture_snapshot(case).is_err(), "invalid USB identity evidence case {case}");
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_metadata_rejects_incomplete_or_degraded_snapshots() {
        for case in 1..=17 {
            if [12, 13, 14].contains(&case) { continue; }
            assert!(linux_fixture_snapshot(case).is_err(), "incomplete Linux metadata case {case} must not authorize removal");
        }
        assert_eq!(linux_fixture_snapshot(0).unwrap().len(), 1);
        for case in [12, 13, 14] { assert!(linux_fixture_snapshot(case).unwrap().is_empty()); }
    }

    #[cfg(target_os = "linux")]
    fn link_node(root: &std::path::Path, name: &str, instance: &str) {
        let node = root.join(name);
        let _ = std::fs::remove_dir_all(&node);
        std::fs::create_dir_all(&node).unwrap();
        std::os::unix::fs::symlink(format!("../../../{instance}"), node.join("device")).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn fixture_device(path: &str) -> Device {
        Device {
            name: "Fixture keyboard".into(),
            manufacturer: "Fixture".into(),
            serial_number: "".into(),
            vendor_id: 0xffff,
            product_id: 0xffff,
            bus_type: "Usb".into(),
            path: path.into(),
            instance_token: format!("sysfs:{path}"),
            firmware: FirmwareProtocol::Vial,
        }
    }

    /// Runs one discovery pass; returns the paths found and whether it scanned.
    #[cfg(target_os = "linux")]
    fn pass(
        root: &std::path::Path,
        last: &std::sync::Mutex<Option<LinuxSnapshot>>,
        result: Result<Vec<Device>, String>,
    ) -> (Result<Vec<String>, String>, bool) {
        let mut scanned = false;
        let found = scan_unless_unchanged(root, last, || {
            scanned = true;
            result
        });
        (
            found.map(|devices| devices.into_iter().map(|d| d.path).collect()),
            scanned,
        )
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_scan_reuses_its_result_while_the_hidraw_nodes_stay_the_same() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let last = std::sync::Mutex::new(None);
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
        let found = Ok(vec![fixture_device("/dev/hidraw0")]);

        assert_eq!(
            pass(root, &last, found.clone()),
            (Ok(vec!["/dev/hidraw0".into()]), true)
        );
        assert_eq!(
            pass(root, &last, Ok(vec![])),
            (Ok(vec!["/dev/hidraw0".into()]), false)
        );

        link_node(root, "hidraw1", "0003:FFFF:FFFF.0002");
        assert!(
            pass(root, &last, found.clone()).1,
            "an added node is scanned"
        );
        assert!(!pass(root, &last, Ok(vec![])).1);

        std::fs::remove_dir_all(root.join("hidraw1")).unwrap();
        assert!(
            pass(root, &last, found.clone()).1,
            "a removed node is scanned"
        );

        // Re-enumerated under the same node name: a new HID instance.
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0003");
        assert_eq!(pass(root, &last, Ok(vec![])), (Ok(vec![]), true));
    }

    /// Makes the node `name` anew under the same HID instance, as a driver
    /// rebind does. The new directory exists before the old one goes.
    #[cfg(target_os = "linux")]
    fn rebind_node(root: &std::path::Path, name: &str, instance: &str) {
        let staged = root.join(format!("{name}.rebound"));
        std::fs::create_dir_all(&staged).unwrap();
        std::os::unix::fs::symlink(format!("../../../{instance}"), staged.join("device")).unwrap();
        std::fs::remove_dir_all(root.join(name)).unwrap();
        std::fs::rename(&staged, root.join(name)).unwrap();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_scan_keeps_nothing_a_rebind_ran_under() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let keyboard = Ok(vec![fixture_device("/dev/hidraw0")]);
        let found = || (Ok(vec!["/dev/hidraw0".to_owned()]), true);

        // Unbound while hidapi enumerated, then bound again to the same HID
        // instance under the same node name: its link reads as before.
        let last = std::sync::Mutex::new(None);
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
        let unbound = scan_unless_unchanged(root, &last, || {
            std::fs::remove_dir_all(root.join("hidraw0")).unwrap();
            Ok(vec![])
        });
        assert!(unbound.is_ok_and(|devices| devices.is_empty()));
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
        assert_eq!(pass(root, &last, keyboard.clone()), found());

        // Unbound and bound again, both while hidapi enumerated.
        let last = std::sync::Mutex::new(None);
        let rebound = scan_unless_unchanged(root, &last, || {
            rebind_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
            Ok(vec![])
        });
        assert!(rebound.is_ok_and(|devices| devices.is_empty()));
        assert_eq!(pass(root, &last, keyboard), found());
        assert!(!pass(root, &last, Ok(vec![])).1, "a steady node is kept");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_scan_retries_an_incomplete_snapshot_in_full() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let last = std::sync::Mutex::new(None);
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
        let incomplete = Err("USB metadata snapshot incomplete".to_owned());

        assert_eq!(
            pass(root, &last, incomplete.clone()),
            (Err("USB metadata snapshot incomplete".into()), true)
        );
        assert_eq!(
            pass(root, &last, incomplete),
            (Err("USB metadata snapshot incomplete".into()), true)
        );
        assert!(pass(root, &last, Ok(vec![fixture_device("/dev/hidraw0")])).1);
        assert!(!pass(root, &last, Ok(vec![])).1);

        // A failure after a good pass drops the kept result too.
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0002");
        assert!(pass(root, &last, Err("gone".into())).1);
        link_node(root, "hidraw0", "0003:FFFF:FFFF.0001");
        assert!(
            pass(root, &last, Ok(vec![])).1,
            "nothing is kept after a failure"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_usb_scan_runs_every_time_sysfs_cannot_be_read() {
        let temp = tempfile::tempdir().unwrap();
        let last = std::sync::Mutex::new(None);
        let missing = temp.path().join("missing");
        for _ in 0..2 {
            assert!(pass(&missing, &last, Ok(vec![])).1);
        }
        // A node without its device link proves nothing either.
        std::fs::create_dir_all(temp.path().join("hidraw0")).unwrap();
        for _ in 0..2 {
            assert!(pass(temp.path(), &last, Ok(vec![])).1);
        }
    }

    #[test]
    fn usb_hotplug_windows_filters_bluetooth_before_any_hid_open() {
        let cases: &[(&[&str], bool)] = &[
            (
                &["HID\\VID_FEED&PID_0001", "USB\\VID_FEED&PID_0001&MI_01"],
                true,
            ),
            (
                &[
                    "HID\\VID_FEED&PID_0001",
                    "BTHLEDEVICE\\{fixture}",
                    "USB\\VID_8087&PID_0033",
                ],
                false,
            ),
            (
                &["HID\\fixture", "BTHENUM\\fixture", "USB\\VID_8087&PID_0033"],
                false,
            ),
            (&["HID\\fixture", "ACPI\\fixture"], false),
            (&[], false),
            (&["hid\\fixture", "usb\\vid_feed&pid_0001"], true),
        ];
        for (ids, expected) in cases {
            assert_eq!(
                windows_usb_ancestry(ids.iter().map(|s| s.to_string())),
                *expected
            );
        }
    }
}
