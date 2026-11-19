//! Temporary Actions artifact source for the PR #275 hardware test.
use super::*;

pub(crate) const PROFILE: &str = "phenom";
pub(crate) const VERSION: &str = "v0.3.1";
pub(crate) const ARCHIVE: &str = "phenom.zip";
pub(crate) const RUN_URL: &str = "https://github.com/IgorArkhipov/rmk/actions/runs/44478096879";
const PRODUCT_ID: u16 = 0x0052;
// The outer Actions artifact digest authenticates both UF2 members.
const SIZE: usize = 416_526;
const DIGEST: &str = "sha256:5d26e79c3ea210af377772f4ca8df0d268d7159f59909c690a20668c14544270";

pub(crate) fn archive_path() -> Option<PathBuf> {
    std::env::var_os("ENTROPY_PR275_PHENOM_ZIP")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

pub(crate) fn load_images(
    pid: u16,
    version: &str,
    name: &str,
    url: &str,
) -> Option<Result<Vec<(Part, Vec<u8>)>>> {
    if (pid, version, name, url) != (PRODUCT_ID, VERSION, ARCHIVE, RUN_URL) {
        return None;
    }
    archive_path().map(|path| read_archive(&path))
}

fn read_archive(path: &std::path::Path) -> Result<Vec<(Part, Vec<u8>)>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .context("Cannot open the PR #275 Phenom Actions archive")?
        .take((MAX_DOWNLOAD + 1) as u64)
        .read_to_end(&mut bytes)?;
    validate_checksum(&bytes, SIZE, DIGEST).context("Invalid PR #275 Phenom Actions archive")?;

    let profile = profile(PRODUCT_ID).context("Missing Phenom profile")?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    ensure!(
        archive.len() == profile.parts.len(),
        "Unexpected files in the Phenom Actions archive"
    );
    profile
        .parts
        .iter()
        .map(|&part| {
            let name = profile.image_name(part);
            let entry = archive.by_name(&name)?;
            ensure!(
                !entry.is_dir() && entry.size() <= MAX_DOWNLOAD as u64,
                "Invalid Phenom Actions archive member: {name}"
            );
            let mut image = Vec::new();
            entry
                .take((MAX_DOWNLOAD + 1) as u64)
                .read_to_end(&mut image)?;
            validate_uf2_for(&image, part)?;
            Ok((part, image))
        })
        .collect()
}
