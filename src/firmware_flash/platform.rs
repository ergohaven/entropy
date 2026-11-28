//! OS topology and mounted-volume discovery. All commands are read-only.
use super::UsbTarget;
use anyhow::{ensure, Result};
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn output(command: &mut Command) -> Result<Vec<u8>> {
    const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

    // Files keep large IORegistry output from filling a pipe while waiting for
    // process exit. A stuck OS utility must not leave an update worker forever.
    let stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    let mut child = command
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "USB topology utility failed ({status})");
            use std::io::{Read, Seek};
            let mut stdout = stdout;
            stdout.rewind()?;
            let mut bytes = Vec::new();
            stdout
                .take((MAX_OUTPUT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= MAX_OUTPUT_BYTES,
                "USB topology utility output exceeded 16 MiB"
            );
            return Ok(bytes);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("USB topology utility timed out");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Observe post-write USB re-enumeration without waiting for mounts on macOS.
pub(super) fn snapshot_usb() -> Result<Vec<UsbTarget>> {
    #[cfg(target_os = "macos")]
    {
        snapshot_macos(None)
    }
    #[cfg(not(target_os = "macos"))]
    {
        snapshot()
    }
}

#[cfg(target_os = "macos")]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    snapshot_macos(Some(macos_mount_point))
}

#[cfg(target_os = "macos")]
type MountLookup = fn(&str) -> Result<Option<PathBuf>>;

#[cfg(target_os = "macos")]
fn snapshot_macos(lookup_mount: Option<MountLookup>) -> Result<Vec<UsbTarget>> {
    // Use USB device subtrees from IOService to retain their disk descendants.
    let bytes = output(Command::new("/usr/sbin/ioreg").args([
        "-a",
        "-l",
        "-w",
        "0",
        "-p",
        "IOService",
        "-r",
        "-c",
        "IOUSBHostDevice",
    ]))?;
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes))?;
    let mut targets = Vec::new();
    collect_macos(&value, lookup_mount, &mut targets)?;
    Ok(targets)
}

#[cfg(target_os = "macos")]
fn collect_macos(
    value: &plist::Value,
    lookup_mount: Option<MountLookup>,
    targets: &mut Vec<UsbTarget>,
) -> Result<()> {
    if let Some(array) = value.as_array() {
        for child in array {
            collect_macos(child, lookup_mount, targets)?;
        }
    } else if let Some(dict) = value.as_dictionary() {
        let int = |key| dict.get(key).and_then(plist::Value::as_unsigned_integer);
        // USB interfaces inherit the device's VID, PID and location. Exclude
        // them as targets while still visiting their storage/HID children below.
        let vendor = int("idVendor").filter(|_| !dict.contains_key("bInterfaceNumber"));
        if let (Some(vendor), Some(product), Some(location)) =
            (vendor, int("idProduct"), int("locationID"))
        {
            let mut target = UsbTarget {
                port: format!("macos:{location:08x}"),
                vendor: vendor.try_into()?,
                product: product.try_into()?,
                mounts: Vec::new(),
            };
            if let (Some(lookup_mount), Some(children)) =
                (lookup_mount, dict.get("IORegistryEntryChildren"))
            {
                let mut disks = std::collections::BTreeSet::new();
                collect_bsd_names(children, &mut disks);
                for disk in disks {
                    if let Some(path) = lookup_mount(&disk)? {
                        if !target.mounts.contains(&path) {
                            target.mounts.push(path);
                        }
                    }
                }
            }
            // After a write, unmounting does not prove the bootloader left. Keep
            // unknown occupants too, so callers can reject ambiguous topology.
            if lookup_mount.is_none()
                || (vendor == 0xe126 && super::runtime_product(product.try_into()?))
                || (vendor == 0x2e8a && product == 3)
                || !target.mounts.is_empty()
            {
                targets.push(target);
            }
        }
        if let Some(children) = dict.get("IORegistryEntryChildren") {
            collect_macos(children, lookup_mount, targets)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_mount_point(disk: &str) -> Result<Option<PathBuf>> {
    let bytes = output(Command::new("/usr/sbin/diskutil").args(["info", "-plist", disk]))?;
    let info = plist::Value::from_reader(std::io::Cursor::new(bytes))?;
    Ok(info
        .as_dictionary()
        .and_then(|d| d.get("MountPoint"))
        .and_then(plist::Value::as_string)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from))
}

#[cfg(target_os = "macos")]
fn collect_bsd_names(value: &plist::Value, names: &mut std::collections::BTreeSet<String>) {
    if let Some(array) = value.as_array() {
        for child in array {
            collect_bsd_names(child, names);
        }
    }
    if let Some(dict) = value.as_dictionary() {
        // A hub's descendants belong to their own physical USB ports.
        if dict.contains_key("idVendor") && !dict.contains_key("bInterfaceNumber") {
            return;
        }
        if let Some(name) = dict.get("BSD Name").and_then(plist::Value::as_string) {
            names.insert(name.into());
        }
        if let Some(children) = dict.get("IORegistryEntryChildren") {
            collect_bsd_names(children, names);
        }
    }
}

#[cfg(target_os = "linux")]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    let mut targets = Vec::new();
    let mounts = std::fs::read_to_string("/proc/self/mountinfo")?;
    for entry in std::fs::read_dir("/sys/bus/usb/devices")? {
        let entry = entry?;
        let path = entry.path();
        let hex = |name| {
            std::fs::read_to_string(path.join(name))
                .ok()
                .and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
        };
        let (Some(vendor), Some(product)) = (hex("idVendor"), hex("idProduct")) else {
            continue;
        };
        let physical = path.canonicalize()?;
        let mut target = UsbTarget {
            port: format!("linux:{}", entry.file_name().to_string_lossy()),
            vendor,
            product,
            mounts: Vec::new(),
        };
        {
            for line in mounts.lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() < 6 {
                    continue;
                }
                if std::fs::canonicalize(PathBuf::from("/sys/dev/block").join(fields[2])).is_ok_and(
                    |p| {
                        p.ancestors()
                            .find(|ancestor| ancestor.join("idVendor").is_file())
                            == Some(physical.as_path())
                    },
                ) {
                    let mount = unescape_mount(fields[4])?;
                    if !target.mounts.contains(&mount) {
                        target.mounts.push(mount);
                    }
                }
            }
        }
        if (vendor == 0xe126 && super::runtime_product(product))
            || (vendor == 0x2e8a && product == 3)
            || !target.mounts.is_empty()
        {
            targets.push(target);
        }
    }
    Ok(targets)
}

#[cfg(any(target_os = "linux", test))]
fn unescape_mount(field: &str) -> Result<PathBuf> {
    let mut bytes = Vec::new();
    let raw = field.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\\' {
            ensure!(
                i + 3 < raw.len() && raw[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b)),
                "Invalid mount path escape"
            );
            let n = u16::from_str_radix(&field[i + 1..i + 4], 8)?;
            bytes.push(n.try_into()?);
            i += 4;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    Ok(PathBuf::from(String::from_utf8(bytes)?))
}

#[cfg(any(target_os = "windows", test))]
fn windows_topology_script() -> String {
    // Only compile-time u16 product IDs enter this script; no device-provided
    // strings or shell quoting. Use the same catalog as Linux and macOS.
    let products = super::runtime_products()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|product| product.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "$EntropyRuntimeProducts = @({products})\n{}",
        include_str!("windows_topology.ps1")
    )
}

#[cfg(target_os = "windows")]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    run_windows_topology("powershell.exe", &windows_topology_script())
}

#[cfg(any(target_os = "windows", all(test, target_os = "macos")))]
fn run_windows_topology(shell: &str, script: &str) -> Result<Vec<UsbTarget>> {
    let bytes =
        output(Command::new(shell).args(["-NoProfile", "-NonInteractive", "-Command", script]))?;
    let text = String::from_utf8(bytes)?;
    Ok(serde_json::from_str(
        text.trim_start_matches('\u{feff}').trim(),
    )?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    anyhow::bail!("Firmware flashing is unavailable on this OS")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn firmware_flash_rejects_oversized_topology_output() {
        let oversized_count = (16 * 1024 * 1024 + 1).to_string();
        let result =
            output(Command::new("/usr/bin/head").args(["-c", &oversized_count, "/dev/zero"]));
        let Err(error) = result else {
            panic!("oversized USB topology output should be rejected");
        };
        assert!(error.to_string().contains("exceeded 16 MiB"));
    }

    #[test]
    fn firmware_flash_windows_topology_uses_both_release_catalogs() {
        let script = windows_topology_script();
        let products: Vec<u16> = script
            .lines()
            .next()
            .unwrap()
            .strip_prefix("$EntropyRuntimeProducts = @(")
            .unwrap()
            .strip_suffix(')')
            .unwrap()
            .split(',')
            .map(|pid| pid.parse().unwrap())
            .collect();
        let expected: std::collections::BTreeSet<_> = super::super::profiles::PROFILES
            .iter()
            .map(|profile| profile.product_id)
            .chain(crate::app::QMK_RELEASE_PROFILES.iter().map(|&(pid, _, _)| pid))
            .collect();
        assert_eq!(products, expected.into_iter().collect::<Vec<_>>());
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[cfg_attr(target_os = "macos", ignore = "requires PowerShell (pwsh) on PATH")]
    #[test]
    fn firmware_flash_windows_finds_unmounted_qmk_macropad_as_origin() {
        // Exercise the actual PowerShell filter and JSON shape without hardware.
        let fake_pnp = r"
function Get-CimInstance { @() }
function Get-PnpDevice {
    param([switch]$PresentOnly)
    @(
        @{InstanceId='USB\VID_E126&PID_0042\MACROPAD'}
        @{InstanceId='USB\VID_E126&PID_FFFF\UNSUPPORTED'}
        @{InstanceId='USB\VID_2E8A&PID_0003\BOOT'}
    )
}
function Get-PnpDeviceProperty {
    param($InstanceId, $KeyName)
    if ($KeyName -ne 'DEVPKEY_Device_LocationPaths') { throw 'Unexpected PnP property' }
    switch ($InstanceId) {
        'USB\VID_E126&PID_0042\MACROPAD' { @{Data=@('PCIROOT(0)#USB(3)')} }
        'USB\VID_2E8A&PID_0003\BOOT' { @{Data=@('PCIROOT(0)#USB(4)')} }
        default { throw 'Unsupported USB device should not reach the port lookup' }
    }
}
";
        let shell = if cfg!(target_os = "windows") {
            "powershell.exe"
        } else {
            "pwsh"
        };
        let script = windows_topology_script();
        let targets = run_windows_topology(shell, &format!("{fake_pnp}\n{script}")).unwrap();
        let profile = super::super::profiles::qmk_profile(0x0042, "macropad_v3").unwrap();
        let origin = super::super::find_origin(&targets, profile.product_id).unwrap();
        assert_eq!(origin.port, "windows:PCIROOT(0)#USB(3)");
        assert!(origin.mounts.is_empty());
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|d| (d.vendor, d.product) == (0x2e8a, 3)));
    }

    #[test]
    fn firmware_flash_mount_paths_decode_kernel_escapes_without_shell_expansion() {
        assert_eq!(
            unescape_mount("/media/UF2\\040Drive").unwrap(),
            PathBuf::from("/media/UF2 Drive")
        );
        assert_eq!(
            unescape_mount("/media/$HOME").unwrap(),
            PathBuf::from("/media/$HOME")
        );
        assert!(unescape_mount("/media/\\777").is_err());
        assert!(unescape_mount("/media/\\0").is_err());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn firmware_flash_macos_counts_devices_once_when_interfaces_share_their_properties() {
        for (vendor, product) in [(0xe126, 0x52), (0x2e8a, 3)] {
            let xml = format!(
                r#"<?xml version="1.0"?><plist version="1.0"><array><dict>
                <key>idVendor</key><integer>{vendor}</integer>
                <key>idProduct</key><integer>{product}</integer>
                <key>locationID</key><integer>1234</integer>
                <key>USB Serial Number</key><string>shared-device-serial</string>
                <key>IORegistryEntryChildren</key><array><dict>
                    <key>bInterfaceNumber</key><integer>0</integer>
                    <key>idVendor</key><integer>{vendor}</integer>
                    <key>idProduct</key><integer>{product}</integer>
                    <key>locationID</key><integer>1234</integer>
                    <key>IORegistryEntryChildren</key><array><dict>
                        <key>SerialNumber</key><string>hid-child</string>
                    </dict></array>
                </dict></array>
            </dict></array></plist>"#
            );
            let value: plist::Value = plist::from_bytes(xml.as_bytes()).unwrap();
            let mut result = Vec::new();
            collect_macos(&value, Some(macos_mount_point), &mut result).unwrap();
            assert_eq!(result.len(), 1, "VID {vendor:04x} PID {product:04x}");
            assert_eq!(result[0].port, "macos:000004d2");
            assert_eq!(result[0].product, product);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn firmware_flash_macos_usb_only_keeps_unmounted_boot_and_unknown_devices() {
        let xml = r#"<?xml version="1.0"?><plist version="1.0"><array>
            <dict>
                <key>idVendor</key><integer>4660</integer>
                <key>idProduct</key><integer>22136</integer>
                <key>locationID</key><integer>1234</integer>
                <key>IORegistryEntryChildren</key><array><dict>
                    <key>bInterfaceNumber</key><integer>0</integer>
                    <key>idVendor</key><integer>4660</integer>
                    <key>idProduct</key><integer>22136</integer>
                    <key>locationID</key><integer>1234</integer>
                    <key>IORegistryEntryChildren</key><array><dict>
                        <key>BSD Name</key><string>disk123s1</string>
                    </dict></array>
                </dict></array>
            </dict>
            <dict>
                <key>idVendor</key><integer>17185</integer>
                <key>idProduct</key><integer>1</integer>
                <key>locationID</key><integer>1235</integer>
            </dict>
        </array></plist>"#;
        let value = plist::from_bytes(xml.as_bytes()).unwrap();
        let mut targets = Vec::new();
        // Admission must still consult the disk, and reject the arbitrary VID/PID
        // without a mount. Topology-only parsing below has no mount resolver.
        collect_macos(
            &value,
            Some(|disk| {
                assert_eq!(disk, "disk123s1");
                Ok(None)
            }),
            &mut targets,
        )
        .unwrap();
        assert!(targets.is_empty());
        let error = collect_macos(
            &value,
            Some(|_| anyhow::bail!("disk disappeared during mount lookup")),
            &mut targets,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "disk disappeared during mount lookup");

        collect_macos(&value, None, &mut targets).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].port, "macos:000004d2");
        assert_eq!((targets[0].vendor, targets[0].product), (0x1234, 0x5678));
        assert_eq!(targets[1].port, "macos:000004d3");
        assert_eq!((targets[1].vendor, targets[1].product), (0x4321, 1));
        assert!(targets.iter().all(|target| target.mounts.is_empty()));
    }
}
