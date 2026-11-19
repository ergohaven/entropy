//! Temporary, opt-in firmware source for hardware review of RMK #79.
use super::*;

pub(crate) const VERSION: &str = "v0.3.1";
pub(crate) const RUN_URL: &str = "https://github.com/IgorArkhipov/rmk/actions/runs/44488954020";

pub(crate) struct Artifact {
    prefix: &'static str,
    filename: &'static str,
    size: usize,
    digest: &'static str,
}

const ARTIFACTS: &[Artifact] = &[
    Artifact {
        prefix: "phenom",
        filename: "phenom.zip",
        size: 412_731,
        digest: "sha256:eff5201635e11ab98001214d728d72f2fcd685cdaf313e5499e7fae86f269242",
    },
    Artifact {
        prefix: "phenom-mini",
        filename: "phenom_mini.zip",
        size: 412_814,
        digest: "sha256:60e3ce8a2a8375f3b7ac06b918c9c97b00e4c31c77b054ef7db7dd4813073ef9",
    },
    Artifact {
        prefix: "phenom-micro",
        filename: "phenom_micro.zip",
        size: 413_136,
        digest: "sha256:498717079de68f164a83d9027e0c156ece0d0ba6fcd574409049eec10db5c48e",
    },
    Artifact {
        prefix: "k04",
        filename: "k04_series_k04.zip",
        size: 720_740,
        digest: "sha256:51400a72101016d069d8bc84956c71dab26842b92a3224feca51c2117286bdaf",
    },
    Artifact {
        prefix: "k04-mini",
        filename: "k04_series_mini.zip",
        size: 720_368,
        digest: "sha256:c60fdfbeeecd6c78f8bc268f6ed7347c42c3c58c543a213dd8a23686c2b83ff9",
    },
    Artifact {
        prefix: "k04-micro",
        filename: "k04_series_micro.zip",
        size: 720_286,
        digest: "sha256:7b117e70d355e62c0dfe657709ffd85b0510de5961b1607b808b53c21e922791",
    },
    Artifact {
        prefix: "k04-qube",
        filename: "k04_qube_k04.zip",
        size: 1_059_371,
        digest: "sha256:cbdf30db89dae4281deaaa18f8bd6cfe21e13d0fecd54a9d847307fbcf81edba",
    },
    Artifact {
        prefix: "k04-mini-qube",
        filename: "k04_qube_mini.zip",
        size: 1_058_098,
        digest: "sha256:ad50793e93fa294fc3d0e214eb483030cdaf08ad9bb8fbcfe2274e1d82b289ca",
    },
    Artifact {
        prefix: "k04-micro-qube",
        filename: "k04_qube_micro.zip",
        size: 1_057_863,
        digest: "sha256:f2670f46fd01fe07a4c50c8079d3a5620248a05aae4bb4e53559378d87bd0a8e",
    },
    Artifact {
        prefix: "k03",
        filename: "k03.zip",
        size: 742_964,
        digest: "sha256:26732e69b4cf593ffb500da569af230914781474cf1091a347ccdda08cdffc49",
    },
    Artifact {
        prefix: "k03-qube",
        filename: "k03_qube.zip",
        size: 1_041_855,
        digest: "sha256:a7e5e2c8a9dbf1b8410bdff2c445359e3bd28761066d30ca7a534f7acfcb16c8",
    },
    Artifact {
        prefix: "imperial44",
        filename: "imperial44.zip",
        size: 741_752,
        digest: "sha256:78015b12064fae47067d32f33669bc0534de658ccd005b8eeb120d848a2e2fab",
    },
    Artifact {
        prefix: "imperial44-qube",
        filename: "imperial44_qube.zip",
        size: 1_040_953,
        digest: "sha256:b8dbd96fc059f977ab53f1ac76fd7e7b7a9e811868a34c9cd7b21dab86a382b8",
    },
    Artifact {
        prefix: "op36",
        filename: "op36.zip",
        size: 731_869,
        digest: "sha256:3c4b2d15380b77aca2d9c14cce7f4d4cc89276df347d3ac0a69abefdef7cf2c7",
    },
    Artifact {
        prefix: "op36-qube",
        filename: "op36_qube.zip",
        size: 1_029_199,
        digest: "sha256:a9bfd1ed08a834c288d5c92d108dfb705aa9ac2eb945018359a297fa668bdc9d",
    },
    Artifact {
        prefix: "velvet",
        filename: "velvet.zip",
        size: 761_950,
        digest: "sha256:cda5614267d5b4d497c53787169d8f6094d5b507b95ea485ba55fde201fe130e",
    },
    Artifact {
        prefix: "velvet-qube",
        filename: "velvet_qube.zip",
        size: 1_060_998,
        digest: "sha256:f9b1d6a3df97118a25ca545b9302e78229006493dcc8b737d6695051da74f97f",
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

    fn read_images(&self, profile: profiles::Profile) -> Result<Vec<(Part, Vec<u8>)>> {
        let path = self.path().context("Set ENTROPY_PR275_RYNK3_DIR")?;
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .with_context(|| format!("Cannot open {}", path.display()))?
            .take((MAX_DOWNLOAD + 1) as u64)
            .read_to_end(&mut bytes)?;
        validate_checksum(&bytes, self.size, self.digest)
            .context("Invalid RMK #79 Actions artifact")?;

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
    profile: profiles::Profile,
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
                artifact.prefix == profile.archive
                    && version == VERSION
                    && artifact.release_name() == name
            })
            .context("Actions artifact does not match this keyboard")
            .and_then(|artifact| artifact.read_images(profile)),
    )
}
