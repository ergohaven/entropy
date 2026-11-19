//! Guided UF2 updates for explicitly reviewed Rynk keyboard profiles.
use anyhow::{bail, ensure, Context, Result};
use crate::rynk::{
    client::{RynkBoard, RynkClient, RynkTimeouts},
    link::{PhysicalId, RynkLink},
    session::RynkSession,
    transport,
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
#[cfg(debug_assertions)]
pub(crate) mod actions_test;
mod packages;
#[path = "firmware_flash/platform.rs"]
mod platform;
pub(crate) mod profiles;
pub(crate) use profiles::{profile, Part, Role};

const MAX_DOWNLOAD: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct UsbTarget {
    pub port: String,
    pub vendor: u16,
    pub product: u16,
    pub mounts: Vec<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct Prepared {
    pub bytes: Vec<u8>,
    pub origin: UsbTarget,
    pub version: String,
    pub part: Part,
    pub remaining: Vec<(Part, Vec<u8>)>,
    pub keyboard_uid: [u8; 8],
}

impl Prepared {
    pub(crate) fn next(&self) -> Option<Self> {
        let ((part, bytes), remaining) = self.remaining.split_first()?;
        Some(Self {
            bytes: bytes.clone(),
            origin: self.origin.clone(),
            version: self.version.clone(),
            part: *part,
            remaining: remaining.to_vec(),
            keyboard_uid: self.keyboard_uid,
        })
    }
}

#[derive(Clone)]
pub(crate) enum Stage {
    EnteringBootloader,
    WaitingComponent,
    Unlocking(Vec<(u8, u8)>),
    WaitingBootloader,
    Writing,
    WaitingReconnect,
    WaitingDisconnect,
}

pub(crate) enum Event {
    Progress(Stage),
    Finished(Result<Outcome, String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Verified,
    TransferredUnverified,
}

pub(crate) fn supported(device: &crate::device::Device) -> bool {
    semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .is_ok_and(|v| v >= semver::Version::new(0, 6, 0))
        && device.vendor_id == 0xe126
        && runtime_product(device.product_id)
        && device.firmware == crate::firmware::FirmwareProtocol::Rynk
        && !device.is_bluetooth_transport()
}

fn runtime_product(pid: u16) -> bool {
    profile(pid).is_some()
}

pub(crate) fn compatible_version(version: &str) -> bool {
    semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
        .is_ok_and(|v| v >= semver::Version::new(0, 3, 0) && v.pre.is_empty() && v.build.is_empty())
}

fn validate_asset(pid: u16, version: &str, name: &str, address: &str) -> Result<()> {
    let profile = profile(pid).context("Unsupported Rynk keyboard model")?;
    validate_profile_asset(profile, version, name, address)
}

fn validate_profile_asset(
    profile: profiles::Profile,
    version: &str,
    name: &str,
    address: &str,
) -> Result<()> {
    ensure!(
        compatible_version(version),
        "Firmware updates require RMK 0.3.0 or later"
    );
    ensure!(
        name == format!("{}-{version}.zip", profile.archive),
        "Firmware does not match this Rynk model and topology"
    );
    ensure!(
        address == format!("https://github.com/ergohaven/rmk/releases/download/{version}/{name}"),
        "Unexpected firmware download address"
    );
    Ok(())
}

#[cfg(test)]
fn validate_uf2(bytes: &[u8]) -> Result<()> {
    validate_uf2_for(bytes, profiles::PHENOM_CENTRAL)
}

fn validate_uf2_for(bytes: &[u8], part: Part) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_DOWNLOAD && bytes.len().is_multiple_of(512),
        "Invalid UF2 length"
    );
    let count = bytes.len() / 512;
    let mut seen = vec![false; count];
    let mut addresses = std::collections::BTreeSet::new();
    let (family, start, end) = part.layout.limits();
    for block in bytes.chunks_exact(512) {
        let word = |offset| u32::from_le_bytes(block[offset..offset + 4].try_into().unwrap());
        ensure!(
            word(0) == 0x0a324655 && word(4) == 0x9e5d5157 && word(508) == 0x0ab16f30,
            "Invalid UF2 block"
        );
        ensure!(
            word(8) == 0x2000 && word(28) == family,
            "UF2 does not match the selected MCU"
        );
        ensure!(
            word(16) == 256 && word(12).is_multiple_of(256) && (start..end).contains(&word(12)),
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
        addresses.first() == Some(&start),
        "UF2 is missing its boot image"
    );
    Ok(())
}

fn validate_checksum(bytes: &[u8], size: usize, digest: &str) -> Result<()> {
    ensure!(
        bytes.len() == size && format!("sha256:{:x}", Sha256::digest(bytes)) == digest,
        "Firmware checksum or length does not match the release"
    );
    Ok(())
}

fn require_newer_version(current: Option<&str>, expected: &str) -> Result<()> {
    let current = current.context("Keyboard does not report a firmware version")?;
    ensure!(
        compatible_version(current),
        "Firmware updates require RMK 0.3.0 or later"
    );
    let current = semver::Version::parse(current).context("Invalid installed firmware version")?;
    let expected = semver::Version::parse(expected).context("Invalid proposed firmware version")?;
    ensure!(
        current < expected,
        "This target already has this firmware or a newer version"
    );
    Ok(())
}

fn find_origin(devices: &[UsbTarget], pid: u16) -> Result<UsbTarget> {
    // Bind the transfer to one physical supported keyboard, regardless of protocol.
    // Require one physical supported keyboard, including other model variants.
    let candidates: Vec<_> = devices
        .iter()
        .filter(|d| d.vendor == 0xe126 && runtime_product(d.product))
        .collect();
    ensure!(
        candidates.len() == 1,
        "Connect only one supported keyboard part by USB before updating"
    );
    let target = candidates[0];
    ensure!(
        target.product == pid && !target.port.is_empty(),
        "Selected keyboard USB port does not match"
    );
    Ok(target.clone())
}

fn bootloader_target(devices: &[UsbTarget], port: &str, part: Part) -> Result<Option<UsbTarget>> {
    ensure!(
        !devices
            .iter()
            .any(|d| d.vendor == 0xe126 && runtime_product(d.product) && d.port != port),
        "Disconnect other supported keyboard parts before updating"
    );
    let candidates: Vec<_> = devices.iter().filter(|d| d.port == port).collect();
    ensure!(candidates.len() <= 1, "USB port identity is ambiguous");
    match candidates.first() {
        Some(d)
            if (part.rp2040() && d.vendor == 0x2e8a && d.product == 3)
                || (!part.rp2040() && !d.mounts.is_empty()) =>
        {
            ensure!(
                d.mounts.len() <= 1,
                "Bootloader has multiple mounted volumes"
            );
            Ok((d.mounts.len() == 1).then(|| (*d).clone()))
        }
        Some(d) if (d.vendor == 0xe126 && runtime_product(d.product)) || !part.rp2040() => Ok(None),
        Some(_) => bail!("A different USB device appeared on the selected port"),
        None => Ok(None),
    }
}

pub(crate) fn prepare(
    device: &crate::device::Device,
    version: &str,
    name: &str,
    url: &str,
    keyboard_uid: [u8; 8],
    runtime_part: Part,
) -> Result<Prepared> {
    ensure!(
        supported(device),
        "Only supported Rynk keyboard profiles connected by USB can be flashed"
    );
    let origin = find_origin(&platform::snapshot()?, device.product_id)?;
    let mut images = load_images(device.product_id, version, name, url)?;
    let runtime_index = images
        .iter()
        .position(|(part, _)| *part == runtime_part)
        .context("USB component is not in this firmware package")?;
    let runtime_image = images.remove(runtime_index);
    images.push(runtime_image);
    let (part, bytes) = images.remove(0);
    let version = version.strip_prefix('v').unwrap_or(version);
    Ok(Prepared {
        bytes,
        origin,
        version: version.to_owned(),
        part,
        remaining: images,
        keyboard_uid,
    })
}

fn load_images(pid: u16, version: &str, name: &str, url: &str) -> Result<Vec<(Part, Vec<u8>)>> {
    #[cfg(debug_assertions)]
    if let Some(images) = actions_test::load_images(pid, version, name, url) {
        return images;
    }

    validate_asset(pid, version, name, url)?;
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
            "https://api.github.com/repos/ergohaven/rmk/releases/tags/{version}"
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
    validate_checksum(&bytes, asset.size, &digest)?;
    packages::unpack(&bytes, pid)
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

/// Own the selected Rynk session until bootloader entry; UI discovery is paused
/// until this worker finishes, so no reconnect worker can borrow the same half.
pub(crate) fn flash(
    prepared: Prepared,
    session: Option<RynkSession>,
    physical_reset: bool,
    events: &Sender<Event>,
) -> Result<Outcome> {
    let final_step = prepared.remaining.is_empty();
    ensure!(
        !final_step || session.is_some() || physical_reset,
        "Reconnect the original USB component first"
    );
    if let Some(session) = &session {
        let current = find_origin(&platform::snapshot()?, prepared.origin.product)?;
        ensure!(
            current.port == prepared.origin.port,
            "Keyboard moved to a different USB port; start again"
        );
        let info = session
            .link()
            .request::<rmk_types_rynk::protocol::rynk::ergohaven::GetErgohavenInfo>(())?;
        ensure!(
            info.keyboard_uid == prepared.keyboard_uid,
            "Keyboard definition changed; start the update again"
        );
        let expected = prepared.remaining.last().map_or(prepared.part, |(part, _)| *part);
        ensure!(
            profile(current.product).and_then(|profile| profile.runtime_part(session.link().board()))
                == Some(expected),
            "Reconnect the original USB component before restarting the update"
        );
        require_newer_version(Some(&info_version(&info)), &prepared.version)?;
        if final_step && !physical_reset {
            session.enter_bootloader()?;
        }
    }
    drop(session);
    let mut software_entry = final_step && !physical_reset;
    let mut wait_for_component = !final_step && !physical_reset;
    let _ = events.send(Event::Progress(if wait_for_component {
        Stage::WaitingComponent
    } else {
        Stage::WaitingBootloader
    }));
    let target = wait_for_usb(Duration::from_secs(90), platform::snapshot, |devices| {
        if let Some(target) = bootloader_target(devices, &prepared.origin.port, prepared.part)? {
            return Ok(Some(target));
        }
        if wait_for_component {
            if let Some(session) = maintenance_session(devices, &prepared)? {
                enter_component_bootloader(&session, &prepared, events)?;
                software_entry = true;
                wait_for_component = false;
                let _ = events.send(Event::Progress(Stage::WaitingBootloader));
            }
        }
        Ok(None)
    })
    .with_context(|| {
        if prepared.part.rp2040() {
            "Bootloader did not appear. Hold BOOT while reconnecting the requested half to USB"
        } else {
            "Bootloader did not appear. Reconnect the requested part and double-press Reset"
        }
    })?;
    let root = &target.mounts[0];
    let mut info = String::new();
    std::fs::File::open(root.join("INFO_UF2.TXT"))?
        .take(8192)
        .read_to_string(&mut info)?;
    validate_bootloader_info(&info, prepared.part)?;
    // Revalidation before opening the fixed filename stays strict, including
    // transient OS errors. Volume labels alone never authorize transfer.
    ensure!(
        bootloader_target(&platform::snapshot()?, &prepared.origin.port, prepared.part)?
            == Some(target.clone()),
        "Bootloader target changed before transfer"
    );
    let _ = events.send(Event::Progress(Stage::Writing));
    let write_result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("ENTROPY.UF2"))?;
        write_payload(&mut file, &prepared.bytes, |file| file.sync_all())?;
        Ok(())
    })();
    if !final_step && !software_entry {
        // A manual transfer may install maintenance support for the first time.
        // Give those images a chance to report their target and version.
        let _ = events.send(Event::Progress(Stage::WaitingDisconnect));
        wait_for_usb(Duration::from_secs(45), platform::snapshot, |devices| {
            Ok(bootloader_departed(devices, &target, prepared.part)?.then_some(()))
        })
        .context("Peripheral did not leave update mode after transfer")?;
        let _ = events.send(Event::Progress(Stage::WaitingReconnect));
        let fallback_at = Instant::now() + Duration::from_secs(8);
        let outcome = wait_for_usb(Duration::from_secs(11), platform::snapshot, |devices| {
            if verify_on_port(devices, &prepared, true)? {
                return Ok(Some(Outcome::Verified));
            }
            Ok((Instant::now() >= fallback_at).then_some(Outcome::TransferredUnverified))
        })?;
        if outcome == Outcome::TransferredUnverified {
            write_result.context("Peripheral transfer could not be confirmed; re-enter update mode and retry")?;
        }
        return Ok(outcome);
    }
    // A UF2 drive can disappear before flush returns. A fresh version change
    // from the sole Rynk board on the original port establishes success either way.
    let _ = events.send(Event::Progress(Stage::WaitingReconnect));
    let verification = wait_for_usb(Duration::from_secs(45), platform::snapshot, |devices| {
        Ok(verify_on_port(devices, &prepared, !final_step)?.then_some(()))
    });
    match verification {
        Ok(()) => Ok(Outcome::Verified),
        Err(error) => match write_result {
            Err(write) => bail!("Transfer failed ({write:#}) and firmware could not be verified: {error:#}. Re-enter bootloader mode and retry"),
            Ok(()) => bail!("Transfer finished, but firmware could not be verified: {error:#}. Reconnect the half before retrying"),
        }
    }
}

fn runtime_id(devices: &[UsbTarget], prepared: &Prepared) -> Result<Option<PhysicalId>> {
    if !devices
        .iter()
        .any(|d| d.vendor == 0xe126 && runtime_product(d.product))
    {
        return Ok(None);
    }
    let current = find_origin(devices, prepared.origin.product)?;
    ensure!(
        current.port == prepared.origin.port,
        "Keyboard moved to a different USB port"
    );
    let boards = match transport::scan_usb() {
        Ok(boards) => boards,
        Err(error) => {
            log::debug!("Firmware update enumeration: {error}");
            return Ok(None);
        }
    };
    let mut matches = boards
        .into_iter()
        .filter(|d| d.vendor_id == 0xe126 && runtime_product(d.product_id));
    let first = matches.next();
    ensure!(matches.next().is_none(), "Rynk keyboard identity is ambiguous");
    Ok(first.filter(|d| d.product_id == current.product).map(|d| d.id))
}

fn maintenance_session(devices: &[UsbTarget], prepared: &Prepared) -> Result<Option<RynkSession>> {
    let Some(id) = runtime_id(devices, prepared)? else {
        return Ok(None);
    };
    let Ok(link) = RynkLink::open(&id) else {
        return Ok(None);
    };
    let board = link.board();
    ensure!(
        board.info.keyboard_uid == prepared.keyboard_uid,
        "Connected USB component has a different keyboard definition"
    );
    // The original primary or the previous half may still be plugged in.
    if board.firmware_target != Some(prepared.part.target) {
        return Ok(None);
    }
    require_newer_version(Some(&info_version(&board.info)), &prepared.version)?;
    Ok(Some(RynkSession::new(link)))
}

fn enter_component_bootloader(
    session: &RynkSession,
    prepared: &Prepared,
    events: &Sender<Event>,
) -> Result<()> {
    session.lock()?;
    let result = (|| {
        unlock_component(session, events)?;
        let devices = platform::snapshot()?;
        ensure!(
            runtime_id(&devices, prepared)?.as_ref() == Some(session.link().id()),
            "USB component changed during unlock"
        );
        session.enter_bootloader()
    })();
    if result.is_err() {
        let _ = session.lock();
    }
    result
}

fn unlock_component(session: &RynkSession, events: &Sender<Event>) -> Result<()> {
    let (_, _, keys) = session.unlock_status()?;
    ensure!(!keys.is_empty(), "Component did not report physical unlock keys");
    let _ = events.send(Event::Progress(Stage::Unlocking(keys)));
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        let (unlocked, unlocking, _) = session.unlock_poll()?;
        if unlocked && !unlocking {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Physical unlock timed out. Reconnect the requested part and try again")
}

fn verify_on_port(devices: &[UsbTarget], prepared: &Prepared, require_target: bool) -> Result<bool> {
    let Some(id) = runtime_id(devices, prepared)? else {
        return Ok(false);
    };
    // Reopen the transport after reboot so no cached information can verify a write.
    let Ok(transport) = transport::open(&id) else {
        return Ok(false);
    };
    let Ok((_, board)) = RynkClient::connect(transport, RynkTimeouts::default()) else {
        return Ok(false);
    };
    if require_target && board.info.revision == 2 {
        return Ok(false);
    }
    verify_returned_board(&board, prepared, require_target)?;
    Ok(true)
}

fn verify_returned_board(board: &RynkBoard, prepared: &Prepared, require_target: bool) -> Result<()> {
    verify_returned_info(&board.info, prepared.keyboard_uid, &prepared.version)?;
    if require_target || board.info.revision >= 3 {
        ensure!(
            board.firmware_target == Some(prepared.part.target),
            "A different firmware component returned after the update"
        );
    }
    Ok(())
}

// A complete host write alone does not establish that cached bytes reached USB.
fn write_payload<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    sync: impl FnOnce(&mut W) -> std::io::Result<()>,
) -> Result<()> {
    writer.write_all(bytes)?;
    Ok(sync(writer)?)
}

fn bootloader_departed(devices: &[UsbTarget], target: &UsbTarget, part: Part) -> Result<bool> {
    bootloader_target(devices, &target.port, part)?;
    Ok(!devices
        .iter()
        .any(|d| d.port == target.port && d.vendor == target.vendor && d.product == target.product))
}

pub(crate) fn info_version(
    info: &rmk_types_rynk::protocol::rynk::ergohaven::ErgohavenInfo,
) -> String {
    format!(
        "{}.{}.{}",
        info.firmware.major, info.firmware.minor, info.firmware.patch
    )
}

fn verify_returned_info(
    info: &rmk_types_rynk::protocol::rynk::ergohaven::ErgohavenInfo,
    uid: [u8; 8],
    expected: &str,
) -> Result<()> {
    ensure!(
        info.keyboard_uid == uid,
        "A different keyboard definition returned after the update"
    );
    let version = info_version(info);
    ensure!(
        version == expected,
        "Keyboard returned with firmware {version}, expected {expected}"
    );
    Ok(())
}

fn validate_bootloader_info(info: &str, part: Part) -> Result<()> {
    let valid = if part.rp2040() {
        info.lines().any(|line| line.trim() == "Board-ID: RPI-RP2")
    } else {
        info.starts_with("UF2 Bootloader ")
            && info.lines().any(|line| {
                line.trim()
                    .strip_prefix("Board-ID: ")
                    .is_some_and(|id| id.starts_with("nRF52840-"))
            })
    };
    ensure!(valid, "USB volume is not the expected MCU's UF2 bootloader");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use profiles::*;

    #[test]
    fn firmware_flash_revision_three_readback_must_match_the_physical_image() {
        use crate::rynk::client::testing::{capabilities, info};
        let mut info = info(3);
        info.firmware.major = 0;
        let prepared = Prepared {
            bytes: Vec::new(),
            origin: UsbTarget { port: "port".into(), vendor: 0xe126, product: 0x52, mounts: Vec::new() },
            version: "0.3.1".into(),
            part: PHENOM_PERIPHERAL,
            remaining: Vec::new(),
            keyboard_uid: info.keyboard_uid,
        };
        let mut board = RynkBoard {
            info,
            capabilities: capabilities(512),
            firmware_target: Some(PHENOM_CENTRAL.target),
        };
        assert!(verify_returned_board(&board, &prepared, false).is_err());
        board.firmware_target = None;
        assert!(verify_returned_board(&board, &prepared, true).is_err());
        board.firmware_target = Some(prepared.part.target);
        assert!(verify_returned_board(&board, &prepared, true).is_ok());
    }

    #[test]
    fn firmware_flash_rynk_bootloader_requires_fresh_unlock_and_handles_reset_before_reply() {
        use crate::rynk::client::testing::{err, handshake, ok, FakeBoard};
        use crate::rynk::session::RynkSession;
        use rmk_types_rynk::protocol::rynk::{Cmd, LockStatus, RynkError};
        for (n, locked, unlocking, reject, unplug) in [
            (0, true, false, false, false),
            (1, false, true, false, false),
            (2, false, false, false, false),
            (3, false, false, true, false),
            (4, false, false, false, true),
            (5, false, false, false, false),
        ] {
            let board = FakeBoard::new(move |h, _| {
                if n == 5 && h.cmd == Cmd::BootloaderJump {
                    return vec![];
                }
                vec![match h.cmd {
                    Cmd::GetLockStatus => ok(
                        h,
                        LockStatus {
                            locked,
                            unlocking,
                            remaining_keys: 0,
                            key_positions: Default::default(),
                        },
                    ),
                    Cmd::BootloaderJump if reject => err(h, RynkError::Locked),
                    Cmd::BootloaderJump => ok(h, ()),
                    _ => handshake(h, 4128).unwrap(),
                }]
            });
            let link = crate::rynk::session::testing::link(&format!("usb:boot-test-{n}"), &board);
            let session = RynkSession::new(link);
            if unplug {
                board.unplug_on(Cmd::BootloaderJump);
            }
            let result = session.enter_bootloader();
            assert_eq!(result.is_ok(), !locked && !unlocking && !reject && !unplug);
            assert_eq!(
                board
                    .sent()
                    .iter()
                    .filter(|h| h.cmd == Cmd::BootloaderJump)
                    .count(),
                usize::from(!locked && !unlocking && !unplug)
            );
        }
    }

    #[test]
    fn firmware_flash_flush_loss_and_partial_writes_are_errors() {
        let mut complete = Vec::new();
        assert!(write_payload(&mut complete, b"firmware", |_| Err(
            std::io::ErrorKind::BrokenPipe.into()
        ))
        .is_err());
        assert_eq!(complete, b"firmware");
        let mut short = std::io::Cursor::new([0u8; 3]);
        assert!(write_payload(&mut short, b"firmware", |_| panic!(
            "a partial copy must not reach flush"
        ))
        .is_err());
    }

    #[test]
    fn firmware_flash_unmounted_bootloader_is_not_departure() {
        let boot = usb("port", 0x2e8a, 3, Some("/boot"));
        let mut unmounted = boot.clone();
        unmounted.mounts.clear();
        assert!(!bootloader_departed(&[unmounted], &boot, PHENOM_PERIPHERAL).unwrap());
        assert!(bootloader_departed(&[], &boot, PHENOM_PERIPHERAL).unwrap());
    }

    #[test]
    fn firmware_flash_rp2040_protects_settings_and_rejects_nordic_images() {
        let mut bytes = uf2();
        assert!(validate_uf2_for(&bytes, PHENOM_PERIPHERAL).is_ok());
        assert!(validate_uf2_for(&bytes, K04_LEFT_PERIPHERAL).is_err());
        bytes[12..16].copy_from_slice(&0x10100000u32.to_le_bytes());
        assert!(validate_uf2_for(&bytes, PHENOM_PERIPHERAL).is_err());
        bytes[12..16].copy_from_slice(&0x10000000u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&0xada52840u32.to_le_bytes());
        assert!(validate_uf2_for(&bytes, PHENOM_CENTRAL).is_err());
    }

    #[test]
    fn firmware_flash_rynk_verification_requires_expected_version_and_definition() {
        use crate::rynk::client::testing::info;
        use rmk_types_rynk::protocol::rynk::ergohaven::ERGOHAVEN_RYNK_REVISION;
        let mut returned = info(ERGOHAVEN_RYNK_REVISION);
        let uid = returned.keyboard_uid;
        assert!(verify_returned_info(&returned, uid, "2.3.1").is_ok());
        assert!(verify_returned_info(&returned, uid, "2.3.2").is_err());
        returned.keyboard_uid[0] ^= 1;
        assert!(verify_returned_info(&returned, uid, "2.3.1").is_err());
        for old in ["0.2.0", "0.3.0-beta.1", "0.3.0+build", "unknown", ""] {
            assert!(!compatible_version(old));
            assert!(require_newer_version(Some(old), "0.3.1").is_err());
        }
        assert!(compatible_version("0.3.0"));
        let mut legacy = crate::device::test_usb_device("port", 1, 0x52);
        assert!(!supported(&legacy));
        legacy.firmware = crate::firmware::FirmwareProtocol::Rynk;
        assert!(supported(&legacy));
        legacy.bus_type = "Bluetooth".into();
        assert!(!supported(&legacy));
    }

    #[test]
    fn firmware_flash_k04_admits_exact_usb_models_and_their_release_packages() {
        for (pid, model) in [
            (0x52, "phenom"),
            (0x53, "phenom-mini"),
            (0x54, "phenom-micro"),
            (0x71, "k04-qube"),
            (0x72, "k04-mini-qube"),
            (0x73, "k04-micro-qube"),
            (0x74, "k04"),
            (0x75, "k04-mini"),
            (0x76, "k04-micro"),
        ] {
            let mut device = crate::device::test_usb_device("port", 1, pid);
            device.firmware = crate::firmware::FirmwareProtocol::Rynk;
            assert!(supported(&device), "PID {pid:04x}");
            let name = format!("{model}-v0.3.0.zip");
            let url = format!("https://github.com/ergohaven/rmk/releases/download/v0.3.0/{name}");
            assert!(validate_asset(pid, "v0.3.0", &name, &url).is_ok());
            assert!(validate_asset(pid, "v0.3.0", "k03-v0.3.0.zip", &url).is_err());
        }
        assert!(!supported(&crate::device::test_usb_device("port", 1, 0x70)));
    }

    #[test]
    fn firmware_flash_download_requires_the_published_checksum_and_size() {
        let bytes = uf2();
        // Independent digest of the fixed UF2 fixture, rather than a digest
        // computed by the same validation code under test.
        let digest = "sha256:dc84cea9fbb141446b332ecc59c3c1d8ce9c5b1afce435651e63abeb1795eed5";
        assert!(validate_checksum(&bytes, 512, digest).is_ok());
        assert!(validate_checksum(&bytes, 511, digest).is_err());
        let mut changed = bytes;
        changed[32] ^= 1;
        assert!(validate_checksum(&changed, 512, digest).is_err());
        assert!(validate_checksum(&changed, 512, "sha256:invalid").is_err());
    }

    #[test]
    fn firmware_flash_same_version_cannot_mask_a_failed_transfer() {
        assert!(require_newer_version(Some("4.0.5"), "4.1.0").is_ok());
        for current in [Some("4.1.0"), Some("4.2.0"), Some("unknown"), None] {
            assert!(require_newer_version(current, "4.1.0").is_err());
        }
    }

    #[test]
    fn firmware_flash_k04_bootloader_is_port_bound_and_requires_nordic_board_info() {
        let boot = usb("port", 0x239a, 0x29, Some("/boot"));
        assert_eq!(
            bootloader_target(&[boot.clone()], "port", K04_RIGHT_PERIPHERAL).unwrap(),
            Some(boot.clone())
        );
        assert!(
            bootloader_target(&[boot.clone()], "other", K04_RIGHT_PERIPHERAL)
                .unwrap()
                .is_none()
        );
        assert!(bootloader_target(
            &[boot, usb("other", 0xe126, 0x74, None)],
            "port",
            K04_RIGHT_PERIPHERAL
        )
        .is_err());
        let info = "UF2 Bootloader 0.9.2\r\nModel: Test board\r\nBoard-ID: nRF52840-Test-revD\r\n";
        assert!(validate_bootloader_info(info, K04_RIGHT_PERIPHERAL).is_ok());
        assert!(validate_bootloader_info(info, PHENOM_CENTRAL).is_err());
        assert!(validate_bootloader_info("Board-ID: RPI-RP2", K04_LEFT_CENTRAL).is_err());
        assert!(validate_bootloader_info(&info.replace("52840", "52832"), K04_DONGLE).is_err());
        assert!(validate_bootloader_info("Model: nRF52840", K04_DONGLE).is_err());
        assert!(find_origin(
            &[usb("p", 0xe126, 0x74, None), usb("q", 0xe126, 0x71, None)],
            0x74
        )
        .is_err());
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
        assert!(bootloader_target(&[normal, boot.clone()], "port1", PHENOM_CENTRAL).is_err());
        assert_eq!(
            bootloader_target(&[boot.clone()], "port1", PHENOM_CENTRAL).unwrap(),
            Some(boot.clone())
        );
        assert_eq!(
            bootloader_target(
                &[usb("port2", 0x2e8a, 3, Some("/other"))],
                "port1",
                PHENOM_CENTRAL
            )
            .unwrap(),
            None
        );
        assert!(bootloader_target(&[boot.clone(), boot], "port1", PHENOM_CENTRAL).is_err());
    }

    #[test]
    fn firmware_flash_shared_serial_requires_one_physical_phenom_of_any_variant() {
        let normal = usb("selected", 0xe126, 0x52, None);
        let other = usb("other", 0xe126, 0x53, None);
        assert!(find_origin(&[normal, other.clone()], 0x52).is_err());
        let boot = usb("selected", 0x2e8a, 3, Some("/boot"));
        assert!(bootloader_target(&[boot, other], "selected", PHENOM_CENTRAL).is_err());
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
            |devices| bootloader_target(devices, "port", PHENOM_CENTRAL),
        )
        .unwrap();
        assert_eq!(target.mounts, vec![PathBuf::from("/boot")]);
        assert!(snapshots.is_empty());
        let boot = usb("port", 0x2e8a, 3, Some("/boot"));
        let error = wait_for_usb(
            Duration::from_secs(10),
            || Ok(vec![boot.clone(), boot.clone()]),
            |devices| bootloader_target(devices, "port", PHENOM_CENTRAL),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn firmware_flash_refuses_cross_model_assets_and_untrusted_urls() {
        let name = "phenom-v0.3.1.zip";
        let good = "https://github.com/ergohaven/rmk/releases/download/v0.3.1/phenom-v0.3.1.zip";
        assert!(validate_asset(0x52, "v0.3.1", name, good).is_ok());
        assert!(validate_asset(0x53, "v0.3.1", name, good).is_err());
        assert!(validate_asset(
            0x52,
            "v0.3.1",
            name,
            &good.replace("github.com", "example.com")
        )
        .is_err());
        assert!(validate_asset(0x52, "v0.3.1", name, &good.replace("v0.3.1/", "v0.3.0/")).is_err());
        assert!(validate_asset(0x52, "v0.2.0", "phenom-v0.2.0.zip", good).is_err());
        assert!(validate_asset(0x52, "4.1.0", "4.1.0_phenom_v1.uf2", good).is_err());
    }
}
