//! Published RMK K:04 Vial packages. Roles follow the six release profiles;
//! peripheral images have no runtime USB HID through which to verify a version.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Phenom,
    LeftPeripheral,
    RightPeripheral,
    LeftCentral,
    Dongle,
}

impl Part {
    pub(crate) fn peripheral(self) -> bool {
        matches!(self, Self::LeftPeripheral | Self::RightPeripheral)
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::LeftPeripheral | Self::LeftCentral => "left",
            Self::RightPeripheral => "right",
            Self::Dongle => "dongle",
            Self::Phenom => unreachable!(),
        }
    }
}

pub(crate) struct Profile {
    pub archive: &'static str,
    output: &'static str,
    pub parts: &'static [Part],
}

pub(crate) fn profile(pid: u16) -> Option<Profile> {
    let (archive, output, qube) = match pid {
        0x71 => ("k04-qube", "k04_qube_k04", true),
        0x72 => ("k04-mini-qube", "k04_qube_mini", true),
        0x73 => ("k04-micro-qube", "k04_qube_micro", true),
        0x74 => ("k04", "k04_series_k04", false),
        0x75 => ("k04-mini", "k04_series_mini", false),
        0x76 => ("k04-micro", "k04_series_micro", false),
        _ => return None,
    };
    Some(Profile {
        archive,
        output,
        // Keep the normal USB device on old firmware until the final step.
        parts: if qube {
            &[Part::LeftPeripheral, Part::RightPeripheral, Part::Dongle]
        } else {
            &[Part::RightPeripheral, Part::LeftCentral]
        },
    })
}

pub(super) fn unpack(bytes: &[u8], pid: u16) -> Result<Vec<(Part, Vec<u8>)>> {
    let profile = profile(pid).context("Unsupported K:04 model")?;
    let names: Vec<_> = profile
        .parts
        .iter()
        .map(|part| format!("{}_{}.uf2", profile.output, part.suffix()))
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

    fn image(part: Part) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        for (offset, value) in [
            (0, 0x0a324655u32),
            (4, 0x9e5d5157),
            (8, 0x2000),
            (
                12,
                if part == Part::Dongle {
                    0x1000
                } else {
                    0x26000
                },
            ),
            (16, 256),
            (24, 1),
            (28, 0xada52840),
            (508, 0x0ab16f30),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[32] = part as u8;
        bytes
    }

    fn package(pid: u16, change: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
        let p = profile(pid).unwrap();
        let mut files: Vec<_> = p
            .parts
            .iter()
            .map(|&part| (format!("{}_{}.uf2", p.output, part.suffix()), image(part)))
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
    fn firmware_flash_k04_packages_preserve_model_topology_and_role_order() {
        for pid in 0x71..=0x76 {
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
        assert!(unpack(&package(0x71, |_| {}), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f[0].1[32] ^= 1), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f[0].0 = "../left.uf2".into()), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f[0].1.resize(MAX_DOWNLOAD + 1, 0)), 0x74).is_err());
        assert!(unpack(&package(0x74, |f| f.pop().map(|_| ()).unwrap()), 0x74).is_err());
    }

    #[test]
    fn firmware_flash_k04_images_exclude_mbr_softdevice_storage_and_bootloader() {
        for part in [
            Part::LeftPeripheral,
            Part::RightPeripheral,
            Part::LeftCentral,
            Part::Dongle,
        ] {
            assert!(validate_uf2_for(&image(part), part).is_ok());
            assert!(validate_uf2_for(&image(part), Part::Phenom).is_err());
            for address in [0, 0xcc000, 0xec000, 0xf4000, 0x10000000] {
                let mut bytes = image(part);
                bytes[12..16].copy_from_slice(&u32::to_le_bytes(address));
                assert!(
                    validate_uf2_for(&bytes, part).is_err(),
                    "{part:?} {address:x}"
                );
            }
        }
        assert!(validate_uf2_for(&image(Part::Dongle), Part::LeftCentral).is_err());
        assert!(validate_uf2_for(&image(Part::LeftCentral), Part::Dongle).is_err());
    }

    // Source-only release compatibility probe. Download the six named stable
    // archives separately, then opt in with ENTROPY_K04_RELEASE_DIR.
    #[test]
    #[ignore = "requires downloaded official K:04 release archives"]
    fn firmware_flash_k04_published_archives() {
        let root = std::env::var_os("ENTROPY_K04_RELEASE_DIR").expect("release fixture directory");
        for (pid, name) in [
            (0x74, "k04-v0.2.0.zip"),
            (0x75, "k04-mini-v0.2.0.zip"),
            (0x76, "k04-micro-v0.2.0.zip"),
            (0x71, "k04-qube-v0.2.0.zip"),
            (0x72, "k04-mini-qube-v0.2.0.zip"),
            (0x73, "k04-micro-qube-v0.2.0.zip"),
        ] {
            let bytes = std::fs::read(PathBuf::from(&root).join(name)).unwrap();
            let images = unpack(&bytes, pid).unwrap();
            assert_eq!(images.len(), profile(pid).unwrap().parts.len());
        }
    }
}
