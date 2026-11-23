//! Guided UF2 updates for supported QMK and RMK release profiles.
use anyhow::{bail, ensure, Context, Result};
use crate::app::{firmware_release_target_for_usb, FirmwareReleaseFamily};
use crate::device_session::DeviceSession;
use crate::hid::HidDevice;
use crate::rynk::{
    client::{RynkBoard, RynkClient, RynkTimeouts, RynkTransport},
    link::{LinkClient, PhysicalId, RynkLink},
    transport,
};
use rmk_types_rynk::protocol::rynk::command;
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
#[cfg(test)]
pub(crate) use profiles::profile;
pub(crate) use profiles::{Part, Role};

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
    pub profile: profiles::Profile,
    pub bytes: Vec<u8>,
    pub origin: UsbTarget,
    pub version: String,
    pub part: Part,
    pub remaining: Vec<(Part, Vec<u8>)>,
    pub keyboard_uid: [u8; 8],
    pub qmk_serial: Option<String>,
}

impl Prepared {
    pub(crate) fn bind_qmk_serial(&mut self, serial: &str) -> Result<()> {
        self.check_qmk_serial(serial)?;
        if !serial.is_empty() {
            self.qmk_serial = Some(serial.to_owned());
        }
        Ok(())
    }

    fn check_qmk_serial(&self, serial: &str) -> Result<()> {
        ensure!(
            self.qmk_serial.as_deref().is_none_or(|bound| bound == serial),
            "Reconnect the same QMK keyboard part before continuing"
        );
        Ok(())
    }

    pub(crate) fn next(&self) -> Option<Self> {
        let ((part, bytes), remaining) = self.remaining.split_first()?;
        Some(Self {
            profile: self.profile,
            bytes: bytes.clone(),
            origin: self.origin.clone(),
            version: self.version.clone(),
            part: *part,
            remaining: remaining.to_vec(),
            keyboard_uid: self.keyboard_uid,
            qmk_serial: None,
        })
    }
}

#[derive(Clone)]
pub(crate) enum Stage {
    CheckingConnection,
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
        && !device.is_bluetooth_transport()
        && match device.firmware {
            crate::firmware::FirmwareProtocol::Rynk => profiles::PROFILES
                .iter().any(|profile| profile.product_id == device.product_id),
            crate::firmware::FirmwareProtocol::Vial => crate::app::QMK_RELEASE_PROFILES
                .iter().any(|(pid, _, _)| *pid == device.product_id),
        }
}

pub(crate) fn runtime_product(pid: u16) -> bool {
    rynk_product(pid)
        || crate::app::QMK_RELEASE_PROFILES.iter().any(|(product, _, _)| *product == pid)
}

fn rynk_product(pid: u16) -> bool {
    profiles::PROFILES.iter().any(|profile| profile.product_id == pid)
}

pub(crate) fn qmk_version(version: &str) -> Option<semver::Version> {
    semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
        .ok()
        .filter(|v| v.major > 0 && v.pre.is_empty() && v.build.is_empty())
}

pub(crate) fn compatible_version(version: &str) -> bool {
    semver::Version::parse(version.strip_prefix('v').unwrap_or(version))
        .is_ok_and(|v| v >= semver::Version::new(0, 3, 0) && v.pre.is_empty() && v.build.is_empty())
}

#[cfg(test)]
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
    if profile.qmk {
        ensure!(qmk_version(version).is_some(), "Expected a stable QMK version");
        ensure!(
            name == format!("{version}_{}.uf2", profile.archive),
            "Firmware does not match this QMK model and revision"
        );
        ensure!(
            address == format!("https://github.com/ergohaven/vial-qmk/releases/download/{version}/{name}"),
            "Unexpected QMK firmware download address"
        );
        return Ok(());
    }
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
    profile: profiles::Profile,
    version: &str,
    name: &str,
    url: &str,
    keyboard_uid: [u8; 8],
    runtime_part: Part,
) -> Result<Prepared> {
    let origin = find_origin(&platform::snapshot()?, profile.product_id)?;
    let mut images = load_images(profile, version, name, url)?;
    if !profile.qmk {
        let runtime_index = images
            .iter()
            .position(|(part, _)| *part == runtime_part)
            .context("USB component is not in this firmware package")?;
        let runtime_image = images.remove(runtime_index);
        images.push(runtime_image);
    }
    let (part, bytes) = images.remove(0);
    let version = version.strip_prefix('v').unwrap_or(version);
    Ok(Prepared {
        profile,
        bytes,
        origin,
        version: version.to_owned(),
        part,
        remaining: images,
        keyboard_uid,
        qmk_serial: None,
    })
}

fn load_images(profile: profiles::Profile, version: &str, name: &str, url: &str) -> Result<Vec<(Part, Vec<u8>)>> {
    #[cfg(debug_assertions)]
    if !profile.qmk {
        if let Some(images) = actions_test::load_images(profile, version, name, url) {
            return images;
        }
    }

    validate_profile_asset(profile, version, name, url)?;
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
    let repo = if profile.qmk { "vial-qmk" } else { "rmk" };
    let release: Release = agent
        .get(&format!(
            "https://api.github.com/repos/ergohaven/{repo}/releases/tags/{version}"
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
    if profile.qmk {
        validate_uf2_for(&bytes, profile.parts[0])?;
        Ok(profile.parts.iter().map(|part| (*part, bytes.clone())).collect())
    } else {
        packages::unpack_profile(&bytes, profile)
    }
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

/// Own the selected USB session until bootloader entry; UI discovery is paused
/// until this worker finishes, so no reconnect worker can borrow the same half.
pub(crate) fn flash(
    prepared: Prepared,
    session: Option<DeviceSession>,
    physical_reset: bool,
    events: &Sender<Event>,
) -> Result<Outcome> {
    let final_step = prepared.remaining.is_empty();
    ensure!(
        !final_step || session.is_some() || physical_reset,
        "Reconnect the original USB component first"
    );
    if prepared.profile.qmk {
        ensure!(!physical_reset, "QMK updates require the active Vial connection");
        let hid = session.as_ref().and_then(DeviceSession::as_vial)
            .context("Connect and select the requested QMK keyboard part")?;
        confirm_original_port(&prepared)?;
        let device = scan_qmk_usb(&prepared)?.context("Could not confirm the active QMK USB device")?;
        let current = qmk_identity(hid, &prepared, &device)?;
        let current = qmk_version(&current).context("Invalid installed QMK version")?;
        let expected = qmk_version(&prepared.version).context("Invalid proposed QMK version")?;
        ensure!(current < expected, "This target already has this firmware or a newer version");
        confirm_original_port(&prepared)?;
        let _ = events.send(Event::Progress(Stage::EnteringBootloader));
        hid.enter_qmk_bootloader()?;
    } else if let Some(session) = &session {
        let session = session.as_rynk().context("RMK updates require a Rynk connection")?;
        let current = find_origin(&platform::snapshot()?, prepared.origin.product)
            .context("Could not confirm the original USB connection before transfer")?;
        ensure!(
            current.port == prepared.origin.port,
            "Keyboard moved to a different USB port; start again"
        );
        let info = session
            .link()
            .request::<rmk_types_rynk::protocol::rynk::ergohaven::GetErgohavenInfo>(())
            .context("Could not read the originally connected component before transfer")?;
        ensure!(
            info.keyboard_uid == prepared.keyboard_uid,
            "Keyboard definition changed; start the update again"
        );
        let expected = prepared.remaining.last().map_or(prepared.part, |(part, _)| *part);
        ensure!(
            prepared.profile.runtime_part(session.link().board()) == Some(expected),
            "Reconnect the original USB component before restarting the update"
        );
        check_session_topology(session.link(), prepared.profile, info.definition_hash)?;
        require_newer_version(Some(&info_version(&info)), &prepared.version)?;
        if final_step && !physical_reset {
            let _ = events.send(Event::Progress(Stage::EnteringBootloader));
            session.enter_bootloader()?;
        }
    }
    drop(session);
    let mut software_entry = prepared.profile.qmk || (final_step && !physical_reset);
    let mut wait_for_component = !prepared.profile.qmk && !final_step && !physical_reset;
    let _ = events.send(Event::Progress(if wait_for_component {
        log::info!("Ready for the requested firmware component on {}", prepared.origin.port);
        Stage::WaitingComponent
    } else {
        Stage::WaitingBootloader
    }));
    let target = wait_for_usb(Duration::from_secs(90), platform::snapshot, |devices| {
        if let Some(target) = bootloader_target(devices, &prepared.origin.port, prepared.part)? {
            return Ok(Some(target));
        }
        if wait_for_component {
            if let Some((id, mut client)) = maintenance_session(devices, &prepared)? {
                enter_component_bootloader(&id, &mut client, &prepared, events)?;
                software_entry = true;
                wait_for_component = false;
                let _ = events.send(Event::Progress(Stage::WaitingBootloader));
            }
        }
        Ok(None)
    })
    .with_context(|| {
        if prepared.profile.qmk {
            "QMK bootloader did not appear on the original USB port"
        } else if prepared.part.rp2040() {
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
    match &write_result {
        Ok(()) => log::info!("UF2 write and sync completed on {}", target.port),
        Err(error) => log::warn!("UF2 write or sync did not complete on {}: {error:#}", target.port),
    }
    let verification = if !final_step && !software_entry {
        wait_for_manual_peripheral(&target, &prepared, write_result.is_ok(), events)
    } else {
        let _ = events.send(Event::Progress(Stage::WaitingReconnect));
        wait_for_version(&prepared, Duration::from_secs(45))
    };
    match (verification, write_result) {
        // A UF2 drive can disappear before flush returns. The fresh version
        // and identity checks establish success even if the OS reported an I/O error.
        (Ok(outcome), _) => Ok(outcome),
        (Err(error), Err(write)) => bail!(
            "UF2 write or sync did not complete ({write:#}); firmware could not be verified: {error:#}"
        ),
        (Err(error), Ok(())) => Err(error.context("Firmware could not be verified after writing")),
    }
}

fn wait_for_manual_peripheral(
    target: &UsbTarget,
    prepared: &Prepared,
    write_completed: bool,
    events: &Sender<Event>,
) -> Result<Outcome> {
    let _ = events.send(Event::Progress(Stage::WaitingDisconnect));
    // Do not ask the OS about the mount that is in the process of disappearing.
    // A snapshot with the runtime already back is also evidence of departure;
    // we need not observe the empty-port interval between the two modes.
    wait_for_usb(Duration::from_secs(45), platform::snapshot_usb, |devices| {
        Ok(bootloader_departed(devices, target, prepared.part)?.then_some(()))
    })
    .context("Peripheral did not leave update mode after transfer")?;
    log::info!("UF2 bootloader no longer enumerated on {}", target.port);
    let _ = events.send(Event::Progress(Stage::WaitingReconnect));
    let mut departed_since = Some(Instant::now());
    wait_for_usb(Duration::from_secs(11), platform::snapshot_usb, |devices| {
        peripheral_return_outcome(
            &mut departed_since,
            Instant::now(),
            write_completed,
            bootloader_departed(devices, target, prepared.part)?,
            || verify_on_port(devices, prepared, true),
        )
    })
    .context("Peripheral did not return with a verifiable firmware version")
}

fn peripheral_return_outcome(
    departed_since: &mut Option<Instant>,
    now: Instant,
    write_completed: bool,
    bootloader_gone: bool,
    verify: impl FnOnce() -> Result<bool>,
) -> Result<Option<Outcome>> {
    if !bootloader_gone {
        *departed_since = None;
        return Ok(None);
    }
    if verify()? {
        return Ok(Some(Outcome::Verified));
    }
    // Legacy peripherals may not report a version. Require both a clean
    // host write and departure before offering unverified completion.
    let since = departed_since.get_or_insert(now);
    Ok((write_completed && now.duration_since(*since) >= Duration::from_secs(8))
        .then_some(Outcome::TransferredUnverified))
}

fn wait_for_version(prepared: &Prepared, timeout: Duration) -> Result<Outcome> {
    wait_for_usb(timeout, platform::snapshot_usb, |devices| {
        Ok(verify_on_port(devices, prepared, !prepared.remaining.is_empty())?
            .then_some(Outcome::Verified))
    })
}

/// Recheck a prepared target after a timeout without unlocking or rewriting it.
pub(crate) fn verify_after_reconnect(prepared: &Prepared) -> Result<Outcome> {
    wait_for_version(prepared, Duration::from_secs(20))
        .context("The requested target and firmware version could not be verified after reconnecting")
}

fn runtime_id(devices: &[UsbTarget], prepared: &Prepared) -> Result<Option<PhysicalId>> {
    if !devices
        .iter()
        .any(|d| d.vendor == 0xe126 && rynk_product(d.product))
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
        .filter(|d| d.vendor_id == 0xe126 && rynk_product(d.product_id));
    let first = matches.next();
    ensure!(matches.next().is_none(), "Rynk keyboard identity is ambiguous");
    Ok(first.filter(|d| d.product_id == current.product).map(|d| d.id))
}

fn maintenance_session(
    devices: &[UsbTarget],
    prepared: &Prepared,
) -> Result<Option<(PhysicalId, LinkClient)>> {
    let Some(id) = runtime_id(devices, prepared)? else {
        return Ok(None);
    };
    // PhysicalId names the port, not its current occupant. A retained editor
    // or host-data link may still contain the previous half's handshake.
    let (mut client, board) = match transport::open(&id)
        .map_err(crate::rynk::client::RynkClientError::from)
        .and_then(|transport| RynkClient::connect(transport, RynkTimeouts::default()))
    {
        Ok(connected) => connected,
        Err(error) => {
            log::debug!("Could not open a fresh firmware maintenance connection: {error}");
            return Ok(None);
        }
    };
    ensure!(
        board.info.keyboard_uid == prepared.keyboard_uid,
        "Connected USB component has a different keyboard definition"
    );
    // The original primary or the previous half may still be plugged in.
    if board.firmware_target != Some(prepared.part.target) {
        return Ok(None);
    }
    check_topology(&mut client, prepared.profile, board.info.definition_hash)?;
    require_newer_version(Some(&info_version(&board.info)), &prepared.version)?;
    log::info!("Opened a fresh Rynk link to the requested firmware component on {}", prepared.origin.port);
    Ok(Some((id, client)))
}

fn enter_component_bootloader(
    id: &PhysicalId,
    client: &mut RynkClient<impl RynkTransport>,
    prepared: &Prepared,
    events: &Sender<Event>,
) -> Result<()> {
    client.request::<command::Lock>(&())
        .context("Could not reset the requested component's unlock state")?;
    let result = (|| {
        unlock_component(client, events).context("Could not unlock the requested component")?;
        let devices = platform::snapshot()?;
        ensure!(
            runtime_id(&devices, prepared)?.as_ref() == Some(id),
            "USB component changed during unlock"
        );
        // Keep using the claimed transport; a new occupant on this port
        // cannot inherit the connection or its physical unlock.
        let status = client.request::<command::GetLockStatus>(&())?;
        ensure!(!status.locked && !status.unlocking, "Unlock the keyboard before entering update mode");
        client.enter_bootloader().context("Could not request USB bootloader entry")?;
        log::info!("Requested bootloader entry over Rynk after physical unlock on {}", prepared.origin.port);
        Ok(())
    })();
    if result.is_err() {
        let _ = client.request::<command::Lock>(&());
    }
    result
}

fn unlock_component(client: &mut RynkClient<impl RynkTransport>, events: &Sender<Event>) -> Result<()> {
    let status = client.request::<command::GetLockStatus>(&())
        .context("Could not read physical unlock keys")?;
    ensure!(!status.key_positions.is_empty(), "Component did not report physical unlock keys");
    let _ = events.send(Event::Progress(Stage::Unlocking(status.key_positions.to_vec())));
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        let status = client.request::<command::UnlockPoll>(&())?;
        if !status.locked && !status.unlocking {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Physical unlock timed out. Reconnect the requested part and try again")
}

fn confirm_original_port(prepared: &Prepared) -> Result<()> {
    let current = find_origin(&platform::snapshot_usb()?, prepared.origin.product)?;
    ensure!(current.port == prepared.origin.port, "Keyboard moved to a different USB port");
    Ok(())
}

fn qmk_identity(hid: &HidDevice, prepared: &Prepared, device: &crate::device::Device) -> Result<String> {
    prepared.check_qmk_serial(&device.serial_number)?;
    let (_, uid) = hid.get_keyboard_id()?;
    ensure!(
        uid != 0 && uid.to_le_bytes() == prepared.keyboard_uid,
        "Connected QMK device has a different keyboard identity"
    );
    let version = hid.get_firmware_version()?.context("Keyboard did not report a firmware version")?;
    let definition = hid.get_layout_json()?;
    let native = hid.get_rmk_native_capabilities()?;
    let target = firmware_release_target_for_usb(
        prepared.origin.vendor, prepared.origin.product, Some(&version), &definition,
        false, native.version != 0, Some(&device.name),
    ).context("Connected QMK firmware revision could not be identified")?;
    ensure!(
        target.family == FirmwareReleaseFamily::Qmk && target.asset_prefix == prepared.profile.archive,
        "Connected keyboard is running a different firmware family or revision"
    );
    Ok(version)
}

fn scan_qmk_usb(prepared: &Prepared) -> Result<Option<crate::device::Device>> {
    let Ok(devices) = crate::device::DeviceManager::scan_usb_devices() else {
        return Ok(None);
    };
    let mut matches = devices.into_iter().filter(|device| {
        device.vendor_id == prepared.origin.vendor && device.product_id == prepared.origin.product
            && device.firmware == crate::firmware::FirmwareProtocol::Vial
    });
    let first = matches.next();
    ensure!(matches.next().is_none(), "QMK keyboard identity is ambiguous");
    Ok(first)
}

fn verify_qmk_on_port(devices: &[UsbTarget], prepared: &Prepared) -> Result<bool> {
    let Ok(current) = find_origin(devices, prepared.origin.product) else {
        return Ok(false);
    };
    if current.port != prepared.origin.port {
        return Ok(false);
    }
    let Ok(Some(device)) = scan_qmk_usb(prepared) else {
        return Ok(false);
    };
    let Ok(hid) = HidDevice::open_fresh_for(&device) else {
        return Ok(false);
    };
    let Ok(version) = qmk_identity(&hid, prepared, &device) else {
        return Ok(false);
    };
    Ok(qmk_version(&version).is_some_and(|version| {
        qmk_version(&prepared.version) == Some(version)
    }))
}

fn verify_on_port(devices: &[UsbTarget], prepared: &Prepared, require_target: bool) -> Result<bool> {
    if prepared.profile.qmk {
        return verify_qmk_on_port(devices, prepared);
    }
    let Some(id) = runtime_id(devices, prepared)? else {
        return Ok(false);
    };
    // Reopen the transport after reboot so no cached information can verify a write.
    let Ok(transport) = transport::open(&id) else {
        return Ok(false);
    };
    let Ok((mut client, board)) = RynkClient::connect(transport, RynkTimeouts::default()) else {
        return Ok(false);
    };
    if require_target && board.info.revision == 2 {
        return Ok(false);
    }
    verify_returned_board(&board, prepared, require_target)?;
    check_topology(&mut client, prepared.profile, board.info.definition_hash)?;
    Ok(true)
}

fn check_session_topology(link: &RynkLink, profile: profiles::Profile, hash: u32) -> Result<()> {
    if profile.shares_product_id() {
        let definition = link.run(move |client| client.read_board_definition(hash))?;
        check_definition_archive(&definition, profile.archive)?;
    }
    Ok(())
}

fn check_topology(
    client: &mut RynkClient<impl RynkTransport>,
    profile: profiles::Profile,
    hash: u32,
) -> Result<()> {
    if profile.shares_product_id() {
        check_definition_archive(&client.read_board_definition(hash)?, profile.archive)?;
    }
    Ok(())
}

fn check_definition_archive(definition: &[u8], expected: &str) -> Result<()> {
    let definition = crate::hid::decode_vial_definition(definition)?;
    ensure!(
        definition.pointer("/entropy/firmwareUpdate/asset")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|asset| asset.trim().eq_ignore_ascii_case(expected)),
        "Connected USB component belongs to a different firmware topology"
    );
    Ok(())
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
    fn firmware_flash_qmk_retry_keeps_serial_until_the_next_part() {
        let profile = qmk_profile(0x52, "phenom_v1").unwrap();
        let mut prepared = Prepared {
            profile,
            bytes: Vec::new(),
            origin: UsbTarget { port: "port".into(), vendor: 0xe126, product: 0x52, mounts: Vec::new() },
            version: "4.2.0".into(),
            part: profile.parts[0],
            remaining: vec![(profile.parts[1], Vec::new())],
            keyboard_uid: [1; 8],
            qmk_serial: None,
        };
        prepared.bind_qmk_serial("first-half").unwrap();
        assert!(prepared.bind_qmk_serial("other-keyboard").is_err());
        assert!(prepared.check_qmk_serial("").is_err());
        assert!(prepared.check_qmk_serial("first-half").is_ok());
        prepared.next().unwrap().bind_qmk_serial("second-half").unwrap();
    }

    #[test]
    fn firmware_flash_classic_requires_the_topology_in_the_fresh_definition() {
        use crate::rynk::client::{fnv1a, testing::{handshake, info, ok, quick, FakeBoard}};
        use rmk_types_rynk::protocol::rynk::{Cmd, LayoutChunk};
        for (asset, same_topology) in [(Some("velvet-qube"), true), (Some("velvet"), false), (None, false)] {
            let json = serde_json::json!({"entropy": {"firmwareUpdate": {"asset": asset}}});
            let mut definition = Vec::new();
            lzma_rs::xz_compress(&mut json.to_string().as_bytes(), &mut definition).unwrap();
            let hash = fnv1a(&definition);
            let fake = FakeBoard::new(move |header, payload| {
                vec![match header.cmd {
                    Cmd::GetErgohavenInfo => {
                        let mut info = info(2);
                        info.definition_hash = hash;
                        ok(header, info)
                    }
                    Cmd::GetBoardDefinition => {
                        let offset = postcard::from_bytes::<u32>(payload).unwrap() as usize;
                        ok(header, LayoutChunk {
                            total_len: definition.len() as u32,
                            bytes: definition[offset..].try_into().unwrap(),
                        })
                    }
                    _ => handshake(header, 512).unwrap(),
                }]
            });
            let (mut client, board) = RynkClient::connect(fake.transport(), quick()).unwrap();
            let expected = profile_for_archive(0xbe, "velvet-qube").unwrap();
            assert_eq!(check_topology(&mut client, expected, board.info.definition_hash).is_ok(), same_topology);
        }
    }

    #[test]
    fn firmware_flash_revision_three_readback_must_match_the_physical_image() {
        use crate::rynk::client::testing::{capabilities, info};
        let mut info = info(3);
        info.firmware.major = 0;
        let prepared = Prepared {
            profile: profile(0x52).unwrap(),
            bytes: Vec::new(),
            origin: UsbTarget { port: "port".into(), vendor: 0xe126, product: 0x52, mounts: Vec::new() },
            version: "0.3.1".into(),
            part: PHENOM_PERIPHERAL,
            remaining: Vec::new(),
            keyboard_uid: info.keyboard_uid,
            qmk_serial: None,
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
    fn firmware_flash_can_verify_an_immediate_runtime_return_after_sync_error() {
        let boot = usb("port", 0x2e8a, 3, Some("/boot"));
        let runtime = usb("port", 0xe126, 0x52, None);
        let now = Instant::now();
        let gone = bootloader_departed(&[runtime], &boot, PHENOM_PERIPHERAL).unwrap();
        let outcome = peripheral_return_outcome(&mut None, now, false, gone, || Ok(true));
        assert_eq!(outcome.unwrap(), Some(Outcome::Verified));
    }

    #[test]
    fn firmware_flash_bootloader_return_resets_unverified_fallback() {
        let start = Instant::now();
        let mut departed_since = Some(start);
        let mut observe = |seconds, write_completed, gone| {
            peripheral_return_outcome(
                &mut departed_since,
                start + Duration::from_secs(seconds),
                write_completed,
                gone,
                || {
                    assert!(gone, "do not probe runtime while bootloader is present");
                    Ok(false)
                },
            )
            .unwrap()
        };
        assert_eq!(observe(7, true, false), None);
        assert_eq!(observe(8, true, true), None);
        assert_eq!(observe(16, false, true), None);
        assert_eq!(observe(16, true, true), Some(Outcome::TransferredUnverified));
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
        let mut device = crate::device::test_usb_device("port", 1, 0x52);
        device.firmware = crate::firmware::FirmwareProtocol::Rynk;
        assert!(supported(&device));
        device.bus_type = "Bluetooth".into();
        assert!(!supported(&device));
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
        assert!(!supported(&crate::device::test_usb_device("port", 1, 0x89)));
    }

    #[test]
    fn firmware_flash_qmk_accepts_only_exact_published_asset() {
        for &(pid, asset, split) in crate::app::QMK_RELEASE_PROFILES {
            let profile = profiles::qmk_profile(pid, asset).unwrap();
            let name = format!("4.2.0_{asset}.uf2");
            let url = format!("https://github.com/ergohaven/vial-qmk/releases/download/4.2.0/{name}");
            assert_eq!(profile.parts.len(), if split { 2 } else { 1 });
            assert!(validate_profile_asset(profile, "4.2.0", &name, &url).is_ok());
            assert!(validate_profile_asset(profile, "4.2.0", "4.2.0_wrong.uf2", &url).is_err());
            assert!(validate_profile_asset(profile, "4.2.0", &name, &url.replace("vial-qmk", "rmk")).is_err());
        }
        let layout = profiles::qmk_profile(0xb2, "sm62_v1").unwrap().parts[0].layout;
        assert_eq!(layout.limits().2, 0x101f2000);
        assert!(profiles::qmk_profile(0xb2, "sm30_v1").is_none());
    }

    #[test]
    fn firmware_flash_qmk_bootloader_requires_completed_physical_unlock() {
        for (unlocked, in_progress, physical_keys, acknowledged) in [
            (false, false, true, true),
            (true, true, true, true),
            (true, false, false, true),
            (true, false, true, false),
            (true, false, true, true),
        ] {
            let (hid, recorder) = HidDevice::test_device();
            let mut status = [0xff; 32];
            status[0] = u8::from(unlocked);
            status[1] = u8::from(in_progress);
            if physical_keys {
                status[2..6].copy_from_slice(&[1, 4, 1, 5]);
            }
            let mut ack = [0; 32];
            if acknowledged {
                ack[0] = 0x0b;
            }
            recorder.respond_with([status, ack]);
            let can_enter = unlocked && !in_progress && physical_keys;
            assert_eq!(hid.enter_qmk_bootloader().is_ok(), can_enter && acknowledged);
            let commands = recorder.requests();
            assert_eq!(commands[0][..2], [0xfe, 5]);
            assert_eq!(commands.iter().filter(|cmd| cmd[0] == 0x0b).count(), usize::from(can_enter));
        }
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
