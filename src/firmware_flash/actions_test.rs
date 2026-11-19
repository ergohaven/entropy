//! Temporary, opt-in firmware source for hardware review of RMK #79.
use super::*;

pub(crate) const VERSION: &str = "v0.3.1";
pub(crate) const RUN_URL: &str = "https://github.com/IgorArkhipov/rmk/actions/runs/44480578690";

pub(crate) struct Artifact {
    product: u16,
    prefix: &'static str,
    filename: &'static str,
    size: usize,
    digest: &'static str,
}

const ARTIFACTS: [Artifact; 2] = [
    Artifact {
        product: 0x0052,
        prefix: "phenom",
        filename: "phenom-rynk3.zip",
        size: 412_961,
        digest: "sha256:f80fd98619ba381f39e70be2e004489b2ad4d8039443fd37697decb2391b2fbe",
    },
    Artifact {
        product: 0x0074,
        prefix: "k04",
        filename: "k04_series_k04-rynk3.zip",
        size: 720_277,
        digest: "sha256:f228db29b87c626e6ecaef67423ef8e954026c4e2fd34cd0d92f2d208c471385",
    },
];

impl Artifact {
    pub(crate) fn release_name(&self) -> String {
        format!("{}-{VERSION}.zip", self.prefix)
    }

    fn path(&self) -> Option<PathBuf> {
        std::env::var_os("ENTROPY_PR275_RYNK3_DIR")
            .filter(|directory| !directory.is_empty())
            .map(|directory| PathBuf::from(directory).join(self.filename))
    }

    fn read_images(&self) -> Result<Vec<(Part, Vec<u8>)>> {
        let path = self.path().context("Set ENTROPY_PR275_RYNK3_DIR")?;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .with_context(|| format!("Cannot open {}", path.display()))?
            .take((MAX_DOWNLOAD + 1) as u64)
            .read_to_end(&mut bytes)?;
        validate_checksum(&bytes, self.size, self.digest)
            .context("Invalid RMK #79 Actions artifact")?;

        let profile = profile(self.product).context("Missing firmware profile")?;
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        ensure!(
            archive.len() == profile.parts.len(),
            "Unexpected files in the Actions archive"
        );
        profile
            .parts
            .iter()
            .map(|&part| {
                let name = profile.image_name(part);
                let entry = archive.by_name(&name)?;
                ensure!(
                    !entry.is_dir() && entry.size() <= MAX_DOWNLOAD as u64,
                    "Invalid Actions archive member: {name}"
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
}

pub(crate) fn artifact_for(prefix: &str) -> Option<&'static Artifact> {
    ARTIFACTS.iter().find(|artifact| {
        artifact.prefix == prefix && artifact.path().is_some_and(|path| path.is_file())
    })
}

pub(crate) fn load_images(
    pid: u16,
    version: &str,
    name: &str,
    url: &str,
) -> Option<Result<Vec<(Part, Vec<u8>)>>> {
    if url != RUN_URL {
        return None;
    }
    Some(
        ARTIFACTS
            .iter()
            .find(|artifact| {
                artifact.product == pid && version == VERSION && artifact.release_name() == name
            })
            .context("Actions artifact does not match this keyboard")
            .and_then(Artifact::read_images),
    )
}
