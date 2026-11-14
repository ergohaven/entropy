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
            stdout.take(16 * 1024 * 1024).read_to_end(&mut bytes)?;
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

#[cfg(target_os = "macos")]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    let bytes =
        output(Command::new("/usr/sbin/ioreg").args(["-a", "-l", "-w", "0", "-p", "IOService"]))?;
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes))?;
    let mut targets = Vec::new();
    collect_macos(&value, &mut targets)?;
    Ok(targets)
}

#[cfg(target_os = "macos")]
fn collect_macos(value: &plist::Value, targets: &mut Vec<UsbTarget>) -> Result<()> {
    if let Some(array) = value.as_array() {
        for child in array {
            collect_macos(child, targets)?;
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
            if let Some(children) = dict.get("IORegistryEntryChildren") {
                let mut disks = std::collections::BTreeSet::new();
                collect_bsd_names(children, &mut disks);
                for disk in disks {
                    let bytes =
                        output(Command::new("/usr/sbin/diskutil").args(["info", "-plist", &disk]))?;
                    let info = plist::Value::from_reader(std::io::Cursor::new(bytes))?;
                    if let Some(mount) = info
                        .as_dictionary()
                        .and_then(|d| d.get("MountPoint"))
                        .and_then(plist::Value::as_string)
                        .filter(|s| !s.is_empty())
                    {
                        let path = PathBuf::from(mount);
                        if !target.mounts.contains(&path) {
                            target.mounts.push(path);
                        }
                    }
                }
            }
            if (vendor == 0xe126 && super::runtime_product(product.try_into()?))
                || (vendor == 0x2e8a && product == 3)
                || !target.mounts.is_empty()
            {
                targets.push(target);
            }
        }
        if let Some(children) = dict.get("IORegistryEntryChildren") {
            collect_macos(children, targets)?;
        }
    }
    Ok(())
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

#[cfg(target_os = "windows")]
pub(super) fn snapshot() -> Result<Vec<UsbTarget>> {
    let bytes = output(Command::new("powershell.exe").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        include_str!("windows_topology.ps1"),
    ]))?;
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
            collect_macos(&value, &mut result).unwrap();
            assert_eq!(result.len(), 1, "VID {vendor:04x} PID {product:04x}");
            assert_eq!(result[0].port, "macos:000004d2");
            assert_eq!(result[0].product, product);
        }
    }
}
