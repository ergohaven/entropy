//! Native RMK 0.3.0+ package validation, independent of keyboard model.
//! Peripheral images have no runtime USB HID through which to verify a version.
use super::*;

pub(super) fn unpack(bytes: &[u8], pid: u16) -> Result<Vec<(Part, Vec<u8>)>> {
    let profile = profile(pid).context("Unsupported Rynk keyboard model")?;
    unpack_profile(bytes, profile)
}

fn unpack_profile(bytes: &[u8], profile: profiles::Profile) -> Result<Vec<(Part, Vec<u8>)>> {
    let names: Vec<_> = profile
        .parts
        .iter()
        .map(|part| profile.image_name(*part))
        .collect();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    ensure!(
        archive.len() == names.len() + 2,
        "Unexpected files in firmware package"
    );
    let mut files = std::collections::BTreeMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_owned();
        let limit = if names.contains(&name) {
            MAX_DOWNLOAD
        } else {
            4096
        };
        ensure!(
            names.contains(&name) || matches!(name.as_str(), "BUILD_COMMIT.txt" | "SHA256SUMS.txt"),
            "Unexpected firmware package member: {name}"
        );
        ensure!(
            file.size() <= limit as u64 && !file.is_dir(),
            "Oversized firmware package member"
        );
        let mut contents = Vec::new();
        file.by_ref()
            .take((limit + 1) as u64)
            .read_to_end(&mut contents)?;
        ensure!(
            contents.len() <= limit && files.insert(name, contents).is_none(),
            "Oversized or duplicate firmware package member"
        );
    }
    let commit = std::str::from_utf8(&files["BUILD_COMMIT.txt"])?.trim();
    ensure!(
        commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid firmware build provenance"
    );
    let checksums = String::from_utf8(
        files
            .remove("SHA256SUMS.txt")
            .context("Missing package checksums")?,
    )?;
    let mut hashes = std::collections::BTreeMap::new();
    for line in checksums.lines() {
        let (hash, name) = line
            .split_once("  ")
            .context("Invalid package checksum list")?;
        ensure!(
            names.iter().any(|n| n == name) && hashes.insert(name, hash).is_none(),
            "Unexpected or duplicate package checksum"
        );
    }
    ensure!(
        hashes.len() == names.len(),
        "Incomplete package checksum list"
    );
    profile
        .parts
        .iter()
        .zip(names)
        .map(|(&part, name)| {
            let contents = files.remove(&name).context("Missing firmware image")?;
            ensure!(
                format!("{:x}", Sha256::digest(&contents)) == hashes[name.as_str()],
                "Firmware image checksum mismatch"
            );
            validate_uf2_for(&contents, part)?;
            Ok((part, contents))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use profiles::*;

    fn image(part: Part) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        for (offset, value) in [
            (0, 0x0a324655u32),
            (4, 0x9e5d5157),
            (8, 0x2000),
            (
                12,
                if part.rp2040() {
                    0x10000000
                } else if part == K04_DONGLE {
                    0x1000
                } else {
                    0x26000
                },
            ),
            (16, 256),
            (24, 1),
            (
                28,
                if part.rp2040() {
                    0xe48bff56
                } else {
                    0xada52840
                },
            ),
            (508, 0x0ab16f30),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[32] = part.role as u8;
        bytes[33..33 + part.suffix.len()].copy_from_slice(part.suffix.as_bytes());
        bytes
    }

    fn package(pid: u16, change: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
        package_for_profile(profile(pid).unwrap(), change)
    }

    fn package_for_profile(
        p: Profile,
        change: impl FnOnce(&mut Vec<(String, Vec<u8>)>),
    ) -> Vec<u8> {
        let mut files: Vec<_> = p
            .parts
            .iter()
            .map(|&part| (p.image_name(part), image(part)))
            .collect();
        let checksums: String = files
            .iter()
            .map(|(name, bytes)| format!("{:x}  {name}\n", Sha256::digest(bytes)))
            .collect();
        files.push(("SHA256SUMS.txt".into(), checksums.into_bytes()));
        files.push((
            "BUILD_COMMIT.txt".into(),
            b"347cf14f48cf994cb7f33ef2e9e026e59a7e2e4e\n".to_vec(),
        ));
        change(&mut files);
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, contents) in files {
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
            zip.write_all(&contents).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn firmware_flash_future_single_board_uses_profile_without_model_branches() {
        // Deliberately absent from the production registry. The same central
        // role works with either MCU and does not inherit the dongle's map.
        for p in [fixtures::SINGLE_RP2040, fixtures::SINGLE_NRF52840] {
            let name = "future-board-v0.3.1.zip";
            let url = format!("https://github.com/ergohaven/rmk/releases/download/v0.3.1/{name}");
            assert!(validate_profile_asset(p, "v0.3.1", name, &url).is_ok());
            assert!(validate_profile_asset(p, "v0.3.1", "phenom-v0.3.1.zip", &url).is_err());
            let bytes = package_for_profile(p, |_| {});
            let images = unpack_profile(&bytes, p).unwrap();
            assert_eq!(images, vec![(p.parts[0], image(p.parts[0]))]);
            assert!(!p.has_peripherals());
            assert!(!p.has_dongle());
            assert!(unpack(&bytes, p.product_id).is_err());
            let mut device = crate::device::test_usb_device("fixture", 1, p.product_id);
            device.firmware = crate::firmware::FirmwareProtocol::Rynk;
            assert!(!supported(&device));
        }
    }

    #[test]
    fn firmware_flash_k04_packages_preserve_model_topology_and_role_order() {
        for pid in (0x52..=0x54).chain(0x71..=0x76) {
            let images = unpack(&package(pid, |_| {}), pid).unwrap();
            assert_eq!(
                images.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                profile(pid).unwrap().parts
            );
            for (part, contents) in images {
                assert_eq!(contents, image(part));
            }
        }
        assert!(unpack(&package(0x74, |_| {}), 0x75).is_err());
        assert!(unpack(&package(0x52, |_| {}), 0x74).is_err());
        assert!(unpack(&package(0x52, |_| {}), 0x53).is_err());
        assert!(unpack(&package(0x71, |_| {}), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f[0].1[32] ^= 1), 0x74).is_err());
        // Same MCU and role, different physical halves: checksums must catch a
        // package whose left/right contents were exchanged after publication.
        assert_ne!(image(K04_LEFT_PERIPHERAL), image(K04_RIGHT_PERIPHERAL));
        assert!(unpack(
            &package(0x71, |files| {
                let left = files[0].1.clone();
                files[0].1 = files[1].1.clone();
                files[1].1 = left;
            }),
            0x71
        )
        .is_err());
        assert!(unpack(&package(0x74, |f| f[0].0 = "../left.uf2".into()), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f[0].1.resize(MAX_DOWNLOAD + 1, 0)), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f.pop().map(|_| ()).unwrap()), 0x74).is_err());
    }

    #[test]
    fn firmware_flash_k04_images_exclude_mbr_softdevice_storage_and_bootloader() {
        for part in [
            K04_LEFT_PERIPHERAL,
            K04_RIGHT_PERIPHERAL,
            K04_LEFT_CENTRAL,
            K04_DONGLE,
        ] {
            assert!(validate_uf2_for(&image(part), part).is_ok());
            assert!(validate_uf2_for(&image(part), PHENOM_CENTRAL).is_err());
            for address in [0, 0xcc000, 0xec000, 0xf4000, 0x10000000] {
                let mut bytes = image(part);
                bytes[12..16].copy_from_slice(&u32::to_le_bytes(address));
                assert!(
                    validate_uf2_for(&bytes, part).is_err(),
                    "{part:?} {address:x}"
                );
            }
        }
        assert!(validate_uf2_for(&image(K04_DONGLE), K04_LEFT_CENTRAL).is_err());
        assert!(validate_uf2_for(&image(K04_LEFT_CENTRAL), K04_DONGLE).is_err());
    }

    // Source-only release compatibility probe. Download the nine named stable
    // archives separately, then opt in with ENTROPY_RYNK_RELEASE_DIR.
    #[test]
    #[ignore = "requires published native RMK 0.3.0 archives"]
    fn firmware_flash_rynk_published_archives() {
        let root = std::env::var_os("ENTROPY_RYNK_RELEASE_DIR").expect("release fixture directory");
        for (pid, name) in [
            (0x52, "phenom-v0.3.0.zip"),
            (0x53, "phenom-mini-v0.3.0.zip"),
            (0x54, "phenom-micro-v0.3.0.zip"),
            (0x74, "k04-v0.3.0.zip"),
            (0x75, "k04-mini-v0.3.0.zip"),
            (0x76, "k04-micro-v0.3.0.zip"),
            (0x71, "k04-qube-v0.3.0.zip"),
            (0x72, "k04-mini-qube-v0.3.0.zip"),
            (0x73, "k04-micro-qube-v0.3.0.zip"),
        ] {
            let bytes = std::fs::read(PathBuf::from(&root).join(name)).unwrap();
            let images = unpack(&bytes, pid).unwrap();
            assert_eq!(images.len(), profile(pid).unwrap().parts.len());
        }
    }
}
