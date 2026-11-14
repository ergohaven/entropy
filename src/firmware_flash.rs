//! Guided updates for explicitly identified Phenom USB keyboards.
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
#[path = "firmware_flash/platform.rs"]
mod platform;

const MAX_DOWNLOAD: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct UsbTarget {
    pub port: String,
    pub vendor: u16,
    pub product: u16,
    pub mounts: Vec<PathBuf>,
}

pub(crate) struct Prepared {
    pub bytes: Vec<u8>,
    pub origin: UsbTarget,
    pub version: String,
}

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    EnteringBootloader,
    WaitingBootloader,
    Writing,
    WaitingReconnect,
}

pub(crate) enum Event {
    Progress(Stage),
    Finished(Result<(), String>),
}

pub(crate) fn supported(device: &crate::device::Device) -> bool {
    device.vendor_id == 0xe126
        && matches!(device.product_id, 0x52..=0x54)
        && device.firmware == crate::firmware::FirmwareProtocol::Vial
        && !device.is_bluetooth_transport()
}

fn validate_asset(pid: u16, version: &str, name: &str, address: &str) -> Result<()> {
    let model = match pid {
        0x52 => "phenom",
        0x53 => "phenom_mini",
        0x54 => "phenom_micro",
        _ => bail!("Unsupported keyboard"),
    };
    ensure!(
        semver::Version::parse(version).is_ok(),
        "Invalid firmware version"
    );
    ensure!(
        name == format!("{version}_{model}_v1.uf2"),
        "Firmware does not match this Phenom model"
    );
    let expected =
        format!("https://github.com/ergohaven/vial-qmk/releases/download/{version}/{name}");
    ensure!(address == expected, "Unexpected firmware download address");
    Ok(())
}

fn validate_uf2(bytes: &[u8]) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_DOWNLOAD && bytes.len().is_multiple_of(512),
        "Invalid UF2 length"
    );
    let count = bytes.len() / 512;
    let mut seen = vec![false; count];
    let mut addresses = std::collections::BTreeSet::new();
    for block in bytes.chunks_exact(512) {
        let word = |offset| u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap());
        ensure!(
            word(0) == 0x0a324655 && word(4) == 0x9e5d5157 && word(508) == 0x0ab16f30,
            "Invalid UF2 block"
        );
        ensure!(
            word(8) == 0x2000 && word(28) == 0xe48bff56,
            "Firmware is not a flashable RP2040 UF2"
        );
        ensure!(
            word(16) == 256
                && word(12).is_multiple_of(256)
                && (0x10000000..0x10200000).contains(&word(12)),
            "Invalid UF2 flash address or payload"
        );
        let number = word(20) as usize;
        ensure!(
            word(24) as usize == count && number < count && !seen[number],
            "Incomplete or duplicate UF2 blocks"
        );
        ensure!(addresses.insert(word(12)), "Overlapping UF2 flash blocks");
        seen[number] = true;
    }
    ensure!(
        addresses.first() == Some(&0x10000000),
        "UF2 is missing its boot image"
    );
    Ok(())
}

fn validate_download(bytes: &[u8], size: usize, digest: &str) -> Result<()> {
    ensure!(
        bytes.len() == size && format!("sha256:{:x}", Sha256::digest(bytes)) == digest,
        "Firmware checksum or length does not match the release"
    );
    validate_uf2(bytes)
}

fn require_newer_version(current: Option<&str>, expected: &str) -> Result<()> {
    let current = current.context("Keyboard does not report a firmware version")?;
    let current = semver::Version::parse(current).context("Invalid installed firmware version")?;
    let expected = semver::Version::parse(expected).context("Invalid proposed firmware version")?;
    ensure!(
        current < expected,
        "This half already has this firmware or a newer version"
    );
    Ok(())
}

fn find_origin(devices: &[UsbTarget], pid: u16) -> Result<UsbTarget> {
    // Vial deliberately shares its runtime USB serial across keyboards.
    // Require one physical Phenom device, including the other model variants.
    let candidates: Vec<_> = devices
        .iter()
        .filter(|d| d.vendor == 0xe126 && matches!(d.product, 0x52..=0x54))
        .collect();
    ensure!(
        candidates.len() == 1,
        "Connect only one Phenom half by USB before updating"
    );
    let target = candidates[0];
    ensure!(
        target.product == pid && !target.port.is_empty(),
        "Selected Phenom USB port does not match"
    );
    Ok(target.clone())
}

fn bootloader_target(devices: &[UsbTarget], port: &str) -> Result<Option<UsbTarget>> {
    ensure!(
        !devices
            .iter()
            .any(|d| d.vendor == 0xe126 && matches!(d.product, 0x52..=0x54) && d.port != port),
        "Disconnect other Phenom USB halves before updating"
    );
    let candidates: Vec<_> = devices.iter().filter(|d| d.port == port).collect();
    ensure!(candidates.len() <= 1, "USB port identity is ambiguous");
    match candidates.first() {
        Some(d) if d.vendor == 0x2e8a && d.product == 3 => {
            ensure!(
                d.mounts.len() <= 1,
                "Bootloader has multiple mounted volumes"
            );
            Ok((d.mounts.len() == 1).then(|| (*d).clone()))
        }
        Some(d) if d.vendor == 0xe126 => Ok(None),
        Some(_) => bail!("A different USB device appeared on the selected port"),
        None => Ok(None),
    }
}

pub(crate) fn prepare(
    device: &crate::device::Device,
    version: &str,
    name: &str,
    url: &str,
) -> Result<Prepared> {
    ensure!(
        supported(device),
        "Only Phenom keyboards connected by USB are supported"
    );
    validate_asset(device.product_id, version, name, url)?;
    let origin = find_origin(&platform::snapshot()?, device.product_id)?;
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
        digest: Option<String>,
        size: usize,
    }
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        prerelease: bool,
        assets: Vec<Asset>,
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let release: Release = agent
        .get(&format!(
            "https://api.github.com/repos/ergohaven/vial-qmk/releases/tags/{version}"
        ))
        .set("User-Agent", "Entropy")
        .call()?
        .into_json()?;
    ensure!(
        release.tag_name == version && !release.draft && !release.prerelease,
        "Release is not stable"
    );
    let mut matching = release.assets.into_iter().filter(|a| a.name == name);
    let asset = matching
        .next()
        .context("Firmware release asset is missing")?;
    ensure!(
        matching.next().is_none()
            && asset.browser_download_url == url
            && asset.size <= MAX_DOWNLOAD,
        "Unexpected firmware asset metadata"
    );
    let digest = asset
        .digest
        .context("Release has no SHA-256 digest; automatic flashing is unavailable")?;
    let mut bytes = Vec::new();
    agent
        .get(url)
        .call()?
        .into_reader()
        .take((MAX_DOWNLOAD + 1) as u64)
        .read_to_end(&mut bytes)?;
    validate_download(&bytes, asset.size, &digest)?;
    Ok(Prepared {
        bytes,
        origin,
        version: version.to_owned(),
    })
}

fn wait_for_usb<T>(
    timeout: Duration,
    mut snapshot: impl FnMut() -> Result<Vec<UsbTarget>>,
    mut inspect: impl FnMut(&[UsbTarget]) -> Result<Option<T>>,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;
    loop {
        ensure!(
            Instant::now() < deadline,
            "USB transition timed out{}",
            last_error.map(|e| format!(": {e}")).unwrap_or_default()
        );
        match snapshot() {
            Ok(devices) => {
                if let Some(result) = inspect(&devices)? {
                    return Ok(result);
                }
            }
            // Enumeration can race removal of a sysfs node, Windows PnP
            // property, or macOS disk. This never authorizes a write.
            Err(error) => last_error = Some(error.to_string()),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Own the selected HID session until bootloader entry; UI discovery is paused
/// until this worker finishes, so no reconnect worker can borrow the same half.
pub(crate) fn flash(
    prepared: Prepared,
    hid: crate::hid::HidDevice,
    physical_reset: bool,
    events: &Sender<Event>,
) -> Result<()> {
    let current = find_origin(&platform::snapshot()?, prepared.origin.product)?;
    ensure!(
        current.port == prepared.origin.port,
        "Keyboard moved to a different USB port; start again"
    );
    // A second success must start from older firmware. The shared Vial serial
    // cannot identify halves, and an already updated half cannot count twice.
    require_newer_version(hid.get_firmware_version()?.as_deref(), &prepared.version)?;
    if !physical_reset {
        hid.enter_bootloader()?;
    }
    drop(hid);
    let _ = events.send(Event::Progress(Stage::WaitingBootloader));
    let target = wait_for_usb(Duration::from_secs(90), platform::snapshot, |devices| {
        bootloader_target(devices, &prepared.origin.port)
    })
    .context("Bootloader did not appear. Reconnect the half and try physical Reset")?;
    let root = &target.mounts[0];
    let mut info = String::new();
    std::fs::File::open(root.join("INFO_UF2.TXT"))?
        .take(8192)
        .read_to_string(&mut info)?;
    ensure!(
        info.lines().any(|line| line.trim() == "Board-ID: RPI-RP2"),
        "USB volume is not an RP2040 bootloader"
    );
    // Revalidation before opening the fixed filename stays strict, including
    // transient OS errors. Volume labels alone never authorize transfer.
    ensure!(
        bootloader_target(&platform::snapshot()?, &prepared.origin.port)? == Some(target.clone()),
        "Bootloader target changed before transfer"
    );
    let _ = events.send(Event::Progress(Stage::Writing));
    let write_result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("ENTROPY.UF2"))?;
        file.write_all(&prepared.bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    // A UF2 drive can disappear before flush returns. A fresh version change
    // from the sole Phenom on the original port establishes success either way.
    let _ = events.send(Event::Progress(Stage::WaitingReconnect));
    let verification = wait_for_usb(Duration::from_secs(45), platform::snapshot, |snapshot| {
        if !snapshot
            .iter()
            .any(|d| d.vendor == 0xe126 && matches!(d.product, 0x52..=0x54))
        {
            return Ok(None);
        }
        let current = find_origin(snapshot, prepared.origin.product)?;
        ensure!(
            current.port == prepared.origin.port,
            "Returning keyboard moved to another USB port"
        );
        let devices = match crate::device::DeviceManager::scan_usb_devices() {
            Ok(devices) => devices,
            Err(error) => {
                log::debug!("Firmware verification enumeration: {error}");
                return Ok(None);
            }
        };
        let matches: Vec<_> = devices.into_iter().filter(supported).collect();
        ensure!(
            matches.len() <= 1,
            "Returning keyboard identity is ambiguous; leave one Phenom half connected by USB"
        );
        if let Some(device) = matches.first().filter(|d| d.product_id == current.product) {
            if let Ok(hid) = crate::hid::HidDevice::open_fresh_for(device) {
                if let Ok(Some(version)) = hid.get_firmware_version() {
                    ensure!(
                        version == prepared.version,
                        "Keyboard returned with firmware {version}, expected {}",
                        prepared.version
                    );
                    return Ok(Some(()));
                }
            }
        }
        Ok(None)
    });
    match verification {
        Ok(()) => Ok(()),
        Err(error) => match write_result {
            Err(write) => bail!("Transfer failed ({write:#}) and firmware could not be verified: {error:#}. Re-enter bootloader mode and retry"),
            Ok(()) => bail!("Transfer finished, but firmware could not be verified: {error:#}. Reconnect the half before retrying"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_flash_download_requires_the_published_checksum_and_size() {
        let bytes = uf2();
        // Independent digest of the fixed UF2 fixture, rather than a digest
        // computed by the same validation code under test.
        let digest = "sha256:dc84cea9fbb141446b332ecc59c3c1d8ce9c5b1afce435651e63abeb1795eed5";
        assert!(validate_download(&bytes, 512, digest).is_ok());
        assert!(validate_download(&bytes, 511, digest).is_err());
        let mut changed = bytes;
        changed[32] ^= 1;
        assert!(validate_download(&changed, 512, digest).is_err());
        assert!(validate_download(&changed, 512, "sha256:invalid").is_err());
    }

    #[test]
    fn firmware_flash_same_version_cannot_mask_a_failed_transfer() {
        assert!(require_newer_version(Some("4.0.5"), "4.1.0").is_ok());
        for current in [Some("4.1.0"), Some("4.2.0"), Some("unknown"), None] {
            assert!(require_newer_version(current, "4.1.0").is_err());
        }
    }

    fn uf2() -> Vec<u8> {
        let mut bytes = vec![0; 512];
        for (offset, value) in [
            (0, 0x0a324655u32),
            (4, 0x9e5d5157),
            (8, 0x2000),
            (12, 0x10000000),
            (16, 256),
            (20, 0),
            (24, 1),
            (28, 0xe48bff56),
            (508, 0x0ab16f30),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn firmware_flash_rejects_invalid_uf2_before_any_device_write() {
        assert!(validate_uf2(&uf2()).is_ok());
        for offset in [0, 4, 8, 12, 16, 20, 24, 28, 508] {
            let mut bytes = uf2();
            bytes[offset..offset + 4].copy_from_slice(&0xffffffffu32.to_le_bytes());
            assert!(validate_uf2(&bytes).is_err(), "offset {offset}");
        }
        assert!(validate_uf2(&[]).is_err());
        assert!(validate_uf2(&uf2()[..511]).is_err());
        assert!(validate_uf2(&uf2().repeat(2)).is_err());
    }

    fn usb(port: &str, vid: u16, pid: u16, mount: Option<&str>) -> UsbTarget {
        UsbTarget {
            port: port.into(),
            vendor: vid,
            product: pid,
            mounts: mount.into_iter().map(std::path::PathBuf::from).collect(),
        }
    }

    #[test]
    fn firmware_flash_requires_the_original_port_and_one_bootloader_volume() {
        let normal = usb("port1", 0xe126, 0x52, None);
        assert_eq!(find_origin(&[normal.clone()], 0x52).unwrap().port, "port1");
        assert!(find_origin(&[normal.clone(), normal.clone()], 0x52).is_err());
        assert!(find_origin(&[normal.clone()], 0x53).is_err());
        let boot = usb("port1", 0x2e8a, 3, Some("/boot"));
        assert!(bootloader_target(&[normal, boot.clone()], "port1").is_err());
        assert_eq!(
            bootloader_target(&[boot.clone()], "port1").unwrap(),
            Some(boot.clone())
        );
        assert_eq!(
            bootloader_target(&[usb("port2", 0x2e8a, 3, Some("/other"))], "port1").unwrap(),
            None
        );
        assert!(bootloader_target(&[boot.clone(), boot], "port1").is_err());
    }

    #[test]
    fn firmware_flash_shared_serial_requires_one_physical_phenom_of_any_variant() {
        let normal = usb("selected", 0xe126, 0x52, None);
        let other = usb("other", 0xe126, 0x53, None);
        assert!(find_origin(&[normal, other.clone()], 0x52).is_err());
        let boot = usb("selected", 0x2e8a, 3, Some("/boot"));
        assert!(bootloader_target(&[boot, other], "selected").is_err());
    }

    #[test]
    fn firmware_flash_polling_retries_topology_errors_but_never_ambiguity() {
        let mut snapshots = std::collections::VecDeque::from([
            Err(anyhow::anyhow!("disk vanished during enumeration")),
            Ok(vec![usb("port", 0xe126, 0x52, None)]),
            Ok(vec![usb("port", 0x2e8a, 3, Some("/boot"))]),
        ]);
        let target = wait_for_usb(
            Duration::from_secs(10),
            || snapshots.pop_front().unwrap(),
            |devices| bootloader_target(devices, "port"),
        )
        .unwrap();
        assert_eq!(target.mounts, vec![PathBuf::from("/boot")]);
        assert!(snapshots.is_empty());
        let boot = usb("port", 0x2e8a, 3, Some("/boot"));
        let error = wait_for_usb(
            Duration::from_secs(10),
            || Ok(vec![boot.clone(), boot.clone()]),
            |devices| bootloader_target(devices, "port"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn firmware_flash_refuses_cross_model_assets_and_untrusted_urls() {
        let good =
            "https://github.com/ergohaven/vial-qmk/releases/download/4.1.0/4.1.0_phenom_v1.uf2";
        assert!(validate_asset(0x52, "4.1.0", "4.1.0_phenom_v1.uf2", good).is_ok());
        assert!(validate_asset(0x53, "4.1.0", "4.1.0_phenom_v1.uf2", good).is_err());
        assert!(validate_asset(
            0x52,
            "4.1.0",
            "4.1.0_phenom_v1.uf2",
            &good.replace("github.com", "example.com")
        )
        .is_err());
        assert!(validate_asset(
            0x52,
            "4.1.0",
            "4.1.0_phenom_v1.uf2",
            &good.replace("4.1.0/", "4.0.5/")
        )
        .is_err());
    }
}
