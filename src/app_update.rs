const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ergohaven/entropy/releases/latest";

#[derive(Clone, Debug)]
pub(crate) struct UpdateAsset {
    pub(crate) name: String,
    pub(crate) url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VersionRelation {
    UpdateAvailable,
    UpToDate,
    DevelopmentBuild,
}

#[derive(Clone, Debug)]
pub(crate) struct UpdateCheckResult {
    pub(crate) latest_version: String,
    pub(crate) release_url: String,
    pub(crate) platform_label: String,
    pub(crate) asset: Option<UpdateAsset>,
    pub(crate) relation: VersionRelation,
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateCheckOutcome {
    Success(UpdateCheckResult),
    Failed(String),
}

#[derive(Debug)]
pub(crate) enum UpdateCheckState {
    Idle,
    Checking {
        #[cfg(not(target_arch = "wasm32"))]
        receiver: std::sync::mpsc::Receiver<UpdateCheckOutcome>,
    },
    Ready(UpdateCheckResult),
    Failed(String),
}

impl Default for UpdateCheckState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(serde::Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

pub(crate) fn current_platform_label() -> String {
    format!("{} {}", normalized_os_label(), normalized_arch_label())
}

pub(crate) fn start_update_check() -> UpdateCheckState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match fetch_latest_release() {
                Ok(release) => UpdateCheckOutcome::Success(build_update_result(release)),
                Err(error) => UpdateCheckOutcome::Failed(error),
            };
            let _ = sender.send(outcome);
        });
        UpdateCheckState::Checking { receiver }
    }

    #[cfg(target_arch = "wasm32")]
    {
        UpdateCheckState::Failed("Update checks are not available in the web build".to_owned())
    }
}

pub(crate) fn poll_update_check(state: &mut UpdateCheckState) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let outcome = match state {
            UpdateCheckState::Checking { receiver } => receiver.try_recv().ok(),
            _ => None,
        };

        if let Some(outcome) = outcome {
            *state = match outcome {
                UpdateCheckOutcome::Success(result) => UpdateCheckState::Ready(result),
                UpdateCheckOutcome::Failed(error) => UpdateCheckState::Failed(error),
            };
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = state;
    }
}

pub(crate) fn update_available(state: &UpdateCheckState) -> bool {
    matches!(
        state,
        UpdateCheckState::Ready(UpdateCheckResult {
            relation: VersionRelation::UpdateAvailable,
            ..
        })
    )
}

pub(crate) fn open_url_in_browser(url: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        return std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "macos")]
    {
        return std::process::Command::new("open")
            .arg(url)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "linux")]
    {
        return std::process::Command::new("xdg-open")
            .arg(url)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = url;
        false
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_latest_release() -> Result<GitHubRelease, String> {
    ureq::get(GITHUB_LATEST_RELEASE_API)
        .set("User-Agent", concat!("Entropy/", env!("CARGO_PKG_VERSION")))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("GitHub request failed: {error}"))?
        .into_json::<GitHubRelease>()
        .map_err(|error| format!("GitHub response parse failed: {error}"))
}

fn build_update_result(release: GitHubRelease) -> UpdateCheckResult {
    let asset = release
        .assets
        .into_iter()
        .find(platform_asset_matches)
        .map(|asset| UpdateAsset {
            name: asset.name,
            url: asset.browser_download_url,
        });
    let relation = compare_versions(env!("CARGO_PKG_VERSION"), &release.tag_name);

    UpdateCheckResult {
        latest_version: release.tag_name,
        release_url: release.html_url,
        platform_label: current_platform_label(),
        asset,
        relation,
    }
}

fn platform_asset_matches(asset: &GitHubAsset) -> bool {
    let name = asset.name.to_ascii_lowercase();
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => name.ends_with(".appimage") && name.contains("x86_64"),
        ("windows", "x86_64") => {
            name.ends_with(".exe") && name.contains("windows") && name.contains("x86_64")
        }
        ("macos", "aarch64") => {
            name.ends_with(".dmg") && name.contains("macos") && name.contains("arm64")
        }
        ("macos", "x86_64") => {
            name.ends_with(".dmg") && name.contains("macos") && name.contains("x86_64")
        }
        _ => false,
    }
}

fn compare_versions(current: &str, latest: &str) -> VersionRelation {
    let current_parts = numeric_version_parts(current);
    let latest_parts = numeric_version_parts(latest);
    match current_parts.cmp(&latest_parts) {
        std::cmp::Ordering::Less => VersionRelation::UpdateAvailable,
        std::cmp::Ordering::Equal => VersionRelation::UpToDate,
        std::cmp::Ordering::Greater => VersionRelation::DevelopmentBuild,
    }
}

fn numeric_version_parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn normalized_os_label() -> &'static str {
    match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        other => other,
    }
}

fn normalized_arch_label() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        other => other,
    }
}
