const ENTROPY_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/ergohaven/entropy/releases/latest";
const ENTROPY_LATEST_RELEASE_PAGE: &str = "https://github.com/ergohaven/entropy/releases/latest";
const RMK_RELEASES_API: &str = "https://api.github.com/repos/ergohaven/rmk/releases?per_page=30";
const ERGOHAVEN_VENDOR_ID: u16 = 0xE126;

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateAssetState {
    Available(UpdateAsset),
    MissingForPlatform,
    MetadataUnavailable,
}

#[derive(Clone, Debug)]
pub(crate) struct UpdateCheckResult {
    pub(crate) latest_version: String,
    pub(crate) release_url: String,
    pub(crate) platform_label: String,
    pub(crate) asset: UpdateAssetState,
    pub(crate) relation: VersionRelation,
}

impl UpdateCheckResult {
    pub(crate) fn downloadable_asset(&self) -> Option<&UpdateAsset> {
        match &self.asset {
            UpdateAssetState::Available(asset) => Some(asset),
            UpdateAssetState::MissingForPlatform | UpdateAssetState::MetadataUnavailable => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FirmwareReleaseTarget {
    pub(crate) current_version: String,
    pub(crate) asset_prefix: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FirmwareUpdateCheckResult {
    pub(crate) latest_version: String,
    pub(crate) release_url: String,
    pub(crate) asset: UpdateAsset,
    pub(crate) relation: VersionRelation,
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateCheckOutcome {
    Success(UpdateCheckResult),
    Failed(String),
}

#[derive(Clone, Debug)]
pub(crate) enum FirmwareUpdateCheckOutcome {
    Success(FirmwareUpdateCheckResult),
    Unavailable,
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

#[derive(Debug)]
pub(crate) enum FirmwareUpdateCheckState {
    Unsupported,
    Checking {
        target: FirmwareReleaseTarget,
        #[cfg(not(target_arch = "wasm32"))]
        receiver: std::sync::mpsc::Receiver<FirmwareUpdateCheckOutcome>,
    },
    Ready {
        target: FirmwareReleaseTarget,
        result: FirmwareUpdateCheckResult,
    },
    Unavailable {
        target: FirmwareReleaseTarget,
    },
    Failed {
        target: FirmwareReleaseTarget,
        error: String,
    },
}

impl Default for FirmwareUpdateCheckState {
    fn default() -> Self {
        Self::Unsupported
    }
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(serde::Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

enum EntropyReleaseAssets {
    Known(Vec<GitHubAsset>),
    Unavailable,
}

struct LatestEntropyRelease {
    tag_name: String,
    html_url: String,
    assets: EntropyReleaseAssets,
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

pub(crate) fn start_firmware_update_check(
    target: Option<FirmwareReleaseTarget>,
) -> FirmwareUpdateCheckState {
    let Some(target) = target else {
        return FirmwareUpdateCheckState::Unsupported;
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let worker_target = target.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = match fetch_rmk_releases() {
                Ok(releases) => build_firmware_update_result(&worker_target, releases)
                    .map(FirmwareUpdateCheckOutcome::Success)
                    .unwrap_or(FirmwareUpdateCheckOutcome::Unavailable),
                Err(error) => FirmwareUpdateCheckOutcome::Failed(error),
            };
            let _ = sender.send(outcome);
        });
        FirmwareUpdateCheckState::Checking { target, receiver }
    }

    #[cfg(target_arch = "wasm32")]
    {
        FirmwareUpdateCheckState::Failed {
            target,
            error: "Update checks are not available in the web build".to_owned(),
        }
    }
}

pub(crate) fn ensure_firmware_update_check(
    state: &mut FirmwareUpdateCheckState,
    target: Option<FirmwareReleaseTarget>,
) {
    if firmware_update_target(state) == target.as_ref() {
        return;
    }
    *state = start_firmware_update_check(target);
}

pub(crate) fn retry_firmware_update_check(
    state: &FirmwareUpdateCheckState,
) -> FirmwareUpdateCheckState {
    start_firmware_update_check(firmware_update_target(state).cloned())
}

pub(crate) fn poll_update_check(state: &mut UpdateCheckState) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let outcome = match state {
            UpdateCheckState::Checking { receiver } => match receiver.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    *state = UpdateCheckState::Failed("Update check thread died".to_owned());
                    return;
                }
            },
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

pub(crate) fn poll_firmware_update_check(state: &mut FirmwareUpdateCheckState) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let outcome = match state {
            FirmwareUpdateCheckState::Checking { receiver, .. } => match receiver.try_recv() {
                Ok(outcome) => Some(outcome),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let target = firmware_update_target(state)
                        .expect("checking state always has a target")
                        .clone();
                    *state = FirmwareUpdateCheckState::Failed {
                        target,
                        error: "Update check thread died".to_owned(),
                    };
                    return;
                }
            },
            _ => None,
        };

        if let Some(outcome) = outcome {
            let target = firmware_update_target(state)
                .expect("checking state always has a target")
                .clone();
            *state = match outcome {
                FirmwareUpdateCheckOutcome::Success(result) => {
                    FirmwareUpdateCheckState::Ready { target, result }
                }
                FirmwareUpdateCheckOutcome::Unavailable => {
                    FirmwareUpdateCheckState::Unavailable { target }
                }
                FirmwareUpdateCheckOutcome::Failed(error) => {
                    FirmwareUpdateCheckState::Failed { target, error }
                }
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

pub(crate) fn firmware_update_available(state: &FirmwareUpdateCheckState) -> bool {
    matches!(
        state,
        FirmwareUpdateCheckState::Ready {
            result: FirmwareUpdateCheckResult {
                relation: VersionRelation::UpdateAvailable,
                ..
            },
            ..
        }
    )
}

pub(crate) fn firmware_update_check_pending(state: &FirmwareUpdateCheckState) -> bool {
    matches!(state, FirmwareUpdateCheckState::Checking { .. })
}

pub(crate) fn firmware_update_target(
    state: &FirmwareUpdateCheckState,
) -> Option<&FirmwareReleaseTarget> {
    match state {
        FirmwareUpdateCheckState::Unsupported => None,
        FirmwareUpdateCheckState::Checking { target, .. }
        | FirmwareUpdateCheckState::Ready { target, .. }
        | FirmwareUpdateCheckState::Unavailable { target }
        | FirmwareUpdateCheckState::Failed { target, .. } => Some(target),
    }
}

pub(crate) fn rmk_firmware_release_target(
    vendor_id: u16,
    product_id: u16,
    firmware_version: Option<&str>,
    vial_json: &serde_json::Value,
    legacy_qube: bool,
    native_rmk_marker: bool,
) -> Option<FirmwareReleaseTarget> {
    if vendor_id != ERGOHAVEN_VENDOR_ID {
        return None;
    }

    let current_version = firmware_version?.trim();
    if current_version.is_empty() {
        return None;
    }

    let explicit_asset = vial_json
        .pointer("/entropy/firmwareUpdate/asset")
        .and_then(serde_json::Value::as_str)
        .and_then(normalized_asset_prefix);
    let explicit_metadata_present = vial_json.pointer("/entropy/firmwareUpdate/asset").is_some();
    let firmware_name_is_rmk = vial_json
        .pointer("/firmware/name")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|name| name.to_ascii_lowercase().contains("rmk"));
    let legacy_zero_major = numeric_version_parts(current_version).first() == Some(&0);
    let catalog_asset = legacy_rmk_asset_prefix(product_id, legacy_qube);

    if explicit_metadata_present && explicit_asset.is_none() {
        return None;
    }
    if explicit_asset.is_none() && !(firmware_name_is_rmk || native_rmk_marker || legacy_zero_major)
    {
        return None;
    }

    Some(FirmwareReleaseTarget {
        current_version: current_version.to_owned(),
        asset_prefix: explicit_asset.or_else(|| catalog_asset.map(str::to_owned))?,
    })
}

fn legacy_rmk_asset_prefix(product_id: u16, qube: bool) -> Option<&'static str> {
    match (product_id, qube) {
        (0x0036, false) => Some("op36"),
        (0x0036, true) => Some("op36-qube"),
        (0x0044, false) => Some("imperial44"),
        (0x0044, true) => Some("imperial44-qube"),
        (0x0070, false) => Some("k03"),
        (0x0070, true) => Some("k03-qube"),
        (0x00BE, false) => Some("velvet"),
        (0x00BE, true) => Some("velvet-qube"),
        (0x0074, _) => Some("k04"),
        (0x0075, _) => Some("k04-mini"),
        (0x0076, _) => Some("k04-micro"),
        (0x0071, _) => Some("k04-qube"),
        (0x0072, _) => Some("k04-mini-qube"),
        (0x0073, _) => Some("k04-micro-qube"),
        _ => None,
    }
}

fn normalized_asset_prefix(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(&byte)
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..");
    valid.then_some(value)
}

pub(crate) fn open_url_in_browser(url: &str) -> bool {
    open_trusted_release_url(url, launch_url_in_browser)
}

fn open_trusted_release_url(url: &str, launch: impl FnOnce(&str) -> bool) -> bool {
    if !is_trusted_release_url(url) {
        log::warn!("Refusing to open an untrusted update URL");
        return false;
    }

    launch(url)
}

fn launch_url_in_browser(url: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let url = url
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        return result as usize > 32;
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

fn is_trusted_release_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !is_trusted_github_release_url(&url) {
        return false;
    }

    let Some(segments) = url.path_segments() else {
        return false;
    };
    match segments.collect::<Vec<_>>().as_slice() {
        ["ergohaven", repository @ ("entropy" | "rmk"), "releases", "tag", tag] => {
            !repository.is_empty() && !tag.is_empty()
        }
        ["ergohaven", repository @ ("entropy" | "rmk"), "releases", "download", tag, asset] => {
            !repository.is_empty() && !tag.is_empty() && !asset.is_empty()
        }
        _ => false,
    }
}

fn is_trusted_github_release_url(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_latest_release() -> Result<LatestEntropyRelease, String> {
    fetch_latest_release_with_fallback(
        || fetch_github_json(ENTROPY_LATEST_RELEASE_API).and_then(latest_entropy_release_from_api),
        fetch_latest_release_from_redirect,
    )
}

fn fetch_latest_release_with_fallback(
    fetch_from_api: impl FnOnce() -> Result<LatestEntropyRelease, String>,
    fetch_from_redirect: impl FnOnce() -> Result<LatestEntropyRelease, String>,
) -> Result<LatestEntropyRelease, String> {
    match fetch_from_api() {
        Ok(release) => Ok(release),
        Err(api_error) => fetch_from_redirect().map_err(|redirect_error| {
            format!("{api_error}; GitHub Releases fallback failed: {redirect_error}")
        }),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_latest_release_from_redirect() -> Result<LatestEntropyRelease, String> {
    let response = ureq::get(ENTROPY_LATEST_RELEASE_PAGE)
        .set("User-Agent", concat!("Entropy/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|error| format!("GitHub Releases request failed: {error}"))?;

    release_from_latest_release_redirect(response.get_url())
}

fn latest_entropy_release_from_api(release: GitHubRelease) -> Result<LatestEntropyRelease, String> {
    if parse_canonical_entropy_release_tag(&release.tag_name).is_none() {
        return Err("GitHub API did not return a canonical stable Entropy version".to_owned());
    }
    let url_tag = parse_entropy_release_url(&release.html_url)
        .map_err(|error| format!("GitHub API release URL was invalid: {error}"))?;
    if url_tag != release.tag_name {
        return Err("GitHub API release tag and URL did not match".to_owned());
    }

    Ok(LatestEntropyRelease {
        html_url: canonical_entropy_release_url(&release.tag_name),
        tag_name: release.tag_name,
        assets: EntropyReleaseAssets::Known(release.assets),
    })
}

fn release_from_latest_release_redirect(url: &str) -> Result<LatestEntropyRelease, String> {
    let tag_name = parse_entropy_release_url(url)?;

    Ok(LatestEntropyRelease {
        html_url: canonical_entropy_release_url(&tag_name),
        tag_name,
        assets: EntropyReleaseAssets::Unavailable,
    })
}

fn parse_entropy_release_url(url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url)
        .map_err(|error| format!("GitHub Releases redirect URL was invalid: {error}"))?;

    if !is_trusted_github_release_url(&parsed) {
        return Err("GitHub Releases redirect URL was not trusted".to_owned());
    }

    let Some(segments) = parsed.path_segments() else {
        return Err("GitHub Releases redirect URL had no path".to_owned());
    };
    let segments = segments.collect::<Vec<_>>();
    let ["ergohaven", "entropy", "releases", "tag", tag] = segments.as_slice() else {
        return Err("GitHub Releases redirect URL was not an Entropy release tag".to_owned());
    };
    if parse_canonical_entropy_release_tag(tag).is_none() {
        return Err(
            "GitHub Releases redirect URL did not contain a canonical Entropy version".to_owned(),
        );
    }

    Ok((*tag).to_owned())
}

fn parse_canonical_entropy_release_tag(tag: &str) -> Option<semver::Version> {
    let version = semver::Version::parse(tag.strip_prefix('v')?).ok()?;
    if !version.pre.is_empty() || !version.build.is_empty() || format!("v{version}") != tag {
        return None;
    }
    Some(version)
}

fn canonical_entropy_release_url(tag: &str) -> String {
    format!("https://github.com/ergohaven/entropy/releases/tag/{tag}")
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_rmk_releases() -> Result<Vec<GitHubRelease>, String> {
    fetch_github_json(RMK_RELEASES_API)
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_github_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    ureq::get(url)
        .set("User-Agent", concat!("Entropy/", env!("CARGO_PKG_VERSION")))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("GitHub request failed: {error}"))?
        .into_json::<T>()
        .map_err(|error| format!("GitHub response parse failed: {error}"))
}

fn build_update_result(latest: LatestEntropyRelease) -> UpdateCheckResult {
    let LatestEntropyRelease {
        tag_name,
        html_url,
        assets,
    } = latest;
    let asset = match assets {
        EntropyReleaseAssets::Known(assets) => assets
            .into_iter()
            .find(platform_asset_matches)
            .map(|asset| {
                UpdateAssetState::Available(UpdateAsset {
                    name: asset.name,
                    url: asset.browser_download_url,
                })
            })
            .unwrap_or(UpdateAssetState::MissingForPlatform),
        EntropyReleaseAssets::Unavailable => UpdateAssetState::MetadataUnavailable,
    };
    let relation = compare_entropy_versions(env!("CARGO_PKG_VERSION"), &tag_name);

    UpdateCheckResult {
        latest_version: tag_name,
        release_url: html_url,
        platform_label: current_platform_label(),
        asset,
        relation,
    }
}

fn build_firmware_update_result(
    target: &FirmwareReleaseTarget,
    releases: Vec<GitHubRelease>,
) -> Option<FirmwareUpdateCheckResult> {
    let (_, release, asset) = releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|mut release| {
            let expected_asset = format!("{}-{}.zip", target.asset_prefix, release.tag_name);
            let asset = release
                .assets
                .drain(..)
                .find(|asset| asset.name == expected_asset)?;
            Some((numeric_version_parts(&release.tag_name), release, asset))
        })
        .max_by(|left, right| left.0.cmp(&right.0))?;
    let relation = compare_firmware_versions(&target.current_version, &release.tag_name);

    Some(FirmwareUpdateCheckResult {
        latest_version: release.tag_name,
        release_url: release.html_url,
        asset: UpdateAsset {
            name: asset.name,
            url: asset.browser_download_url,
        },
        relation,
    })
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

fn compare_entropy_versions(current: &str, latest: &str) -> VersionRelation {
    let ordering = match (parse_semver(current), parse_semver(latest)) {
        (Some(current), Some(latest)) => current.cmp(&latest),
        _ => numeric_version_parts(current).cmp(&numeric_version_parts(latest)),
    };
    match ordering {
        std::cmp::Ordering::Less => VersionRelation::UpdateAvailable,
        std::cmp::Ordering::Equal => VersionRelation::UpToDate,
        std::cmp::Ordering::Greater => VersionRelation::DevelopmentBuild,
    }
}

fn compare_firmware_versions(current: &str, latest: &str) -> VersionRelation {
    match numeric_version_parts(current).cmp(&numeric_version_parts(latest)) {
        std::cmp::Ordering::Less => VersionRelation::UpdateAvailable,
        std::cmp::Ordering::Equal => VersionRelation::UpToDate,
        std::cmp::Ordering::Greater => VersionRelation::DevelopmentBuild,
    }
}

fn parse_semver(version: &str) -> Option<semver::Version> {
    let version = version.trim();
    let version = version.strip_prefix('v').unwrap_or(version);
    semver::Version::parse(version).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entropy_release(tag: &str, assets: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_owned(),
            html_url: format!("https://github.com/ergohaven/entropy/releases/tag/{tag}"),
            draft: false,
            prerelease: false,
            assets: assets
                .iter()
                .map(|name| GitHubAsset {
                    name: (*name).to_owned(),
                    browser_download_url: format!(
                        "https://github.com/ergohaven/entropy/releases/download/{tag}/{name}"
                    ),
                })
                .collect(),
        }
    }

    fn current_platform_asset_name() -> Option<&'static str> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some("entropy-v0.3.21-x86_64.AppImage"),
            ("windows", "x86_64") => Some("Entropy-Windows-v0.3.21-x86_64.exe"),
            ("macos", "aarch64") => Some("Entropy-macOS-v0.3.21-arm64.dmg"),
            ("macos", "x86_64") => Some("Entropy-macOS-v0.3.21-x86_64.dmg"),
            _ => None,
        }
    }

    fn release(tag: &str, draft: bool, prerelease: bool, assets: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_owned(),
            html_url: format!("https://github.com/ergohaven/rmk/releases/tag/{tag}"),
            draft,
            prerelease,
            assets: assets
                .iter()
                .map(|name| GitHubAsset {
                    name: (*name).to_owned(),
                    browser_download_url: format!(
                        "https://github.com/ergohaven/rmk/releases/download/{tag}/{name}"
                    ),
                })
                .collect(),
        }
    }

    #[test]
    fn builds_tag_only_release_from_latest_redirect() {
        let release = release_from_latest_release_redirect(
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.21",
        )
        .unwrap();

        assert_eq!(release.tag_name, "v0.3.21");
        assert_eq!(
            release.html_url,
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.21"
        );
        assert!(matches!(release.assets, EntropyReleaseAssets::Unavailable));
    }

    #[test]
    fn rejects_noncanonical_or_untrusted_latest_redirects() {
        for url in [
            "https://github.com/ergohaven/another-repo/releases/tag/v0.3.21",
            "https://github.com/ergohaven/entropy/releases/tag/release%2Fv0.3.21",
            "https://github.com/ergohaven/entropy/releases/tag/v0%2E3%2E21",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.21-rc.1",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.21+build.1",
            "https://github.com/ergohaven/entropy/releases/tag/v0.03.21",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3",
            "https://github.com/ergohaven/entropy/releases/tag/0.3.21",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.21?source=update",
            "https://github.com/ergohaven/entropy/releases/tag/",
        ] {
            assert!(release_from_latest_release_redirect(url).is_err(), "{url}");
        }
    }

    #[test]
    fn uses_api_release_with_complete_asset_metadata() {
        let mut fallback_requested = false;
        let latest = fetch_latest_release_with_fallback(
            || {
                latest_entropy_release_from_api(entropy_release(
                    "v0.3.21",
                    &["entropy-v0.3.21-x86_64.AppImage"],
                ))
            },
            || {
                fallback_requested = true;
                Err("fallback should not be requested".to_owned())
            },
        )
        .unwrap();

        assert_eq!(latest.tag_name, "v0.3.21");
        assert!(matches!(latest.assets, EntropyReleaseAssets::Known(_)));
        assert!(!fallback_requested);
    }

    #[test]
    fn rejects_api_release_when_tag_and_url_do_not_match() {
        let mut release = entropy_release("v0.3.21", &[]);
        release.html_url = "https://github.com/ergohaven/entropy/releases/tag/v0.3.20".to_owned();

        assert!(latest_entropy_release_from_api(release).is_err());
    }

    #[test]
    fn rejects_noncanonical_api_release_tags() {
        for tag in ["0.3.21", "v0.3.21-rc.1", "v0.3.21+build.1", "v0.03.21"] {
            assert!(latest_entropy_release_from_api(entropy_release(tag, &[])).is_err());
        }
    }

    #[test]
    fn complete_api_metadata_distinguishes_available_and_missing_assets() {
        let Some(asset_name) = current_platform_asset_name() else {
            return;
        };
        let available = build_update_result(
            latest_entropy_release_from_api(entropy_release("v0.3.21", &[asset_name])).unwrap(),
        );
        let missing = build_update_result(
            latest_entropy_release_from_api(entropy_release("v0.3.21", &[])).unwrap(),
        );

        assert!(matches!(
            &available.asset,
            UpdateAssetState::Available(UpdateAsset { name, .. }) if name == asset_name
        ));
        assert_eq!(
            available
                .downloadable_asset()
                .map(|asset| asset.name.as_str()),
            Some(asset_name)
        );
        assert_eq!(missing.asset, UpdateAssetState::MissingForPlatform);
        assert!(missing.downloadable_asset().is_none());
    }

    #[test]
    fn uses_tag_only_redirect_after_api_failure() {
        let latest = fetch_latest_release_with_fallback(
            || Err("GitHub request failed: rate limited".to_owned()),
            || {
                release_from_latest_release_redirect(
                    "https://github.com/ergohaven/entropy/releases/tag/v999.0.0",
                )
            },
        )
        .unwrap();
        let result = build_update_result(latest);

        assert_eq!(result.latest_version, "v999.0.0");
        assert_eq!(
            result.release_url,
            "https://github.com/ergohaven/entropy/releases/tag/v999.0.0"
        );
        assert_eq!(result.asset, UpdateAssetState::MetadataUnavailable);
        assert!(result.downloadable_asset().is_none());
        assert_eq!(result.relation, VersionRelation::UpdateAvailable);
    }

    #[test]
    fn reports_api_and_redirect_failures_together() {
        let result = fetch_latest_release_with_fallback(
            || Err("GitHub request failed: rate limited".to_owned()),
            || Err("GitHub Releases request failed: unavailable".to_owned()),
        );
        let Err(error) = result else {
            panic!("fallback should have failed");
        };

        assert_eq!(
            error,
            "GitHub request failed: rate limited; GitHub Releases fallback failed: GitHub Releases request failed: unavailable"
        );
    }

    #[test]
    fn compares_prerelease_versions_using_semver_precedence() {
        assert_eq!(
            compare_entropy_versions("0.3.21-rc.1", "v0.3.21"),
            VersionRelation::UpdateAvailable
        );
        assert_eq!(
            compare_entropy_versions("0.3.21", "v0.3.21-rc.1"),
            VersionRelation::DevelopmentBuild
        );
        assert_eq!(
            compare_entropy_versions("0.3.21", "v0.3.21"),
            VersionRelation::UpToDate
        );
    }

    #[test]
    fn trusts_entropy_and_rmk_release_urls() {
        for url in [
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.0",
            "https://github.com/ergohaven/entropy/releases/download/v0.3.0/Entropy-Windows-x86_64.exe",
            "https://github.com/ergohaven/entropy/releases/tag/release%2Fv0.3.0",
            "https://github.com/ergohaven/rmk/releases/tag/v0.1.6",
            "https://github.com/ergohaven/rmk/releases/download/v0.1.6/k04-v0.1.6.zip",
        ] {
            assert!(is_trusted_release_url(url), "{url}");
        }
    }

    #[test]
    fn rejects_untrusted_update_urls() {
        for url in [
            "http://github.com/ergohaven/entropy/releases/tag/v0.3.0",
            "https://github.com.evil.invalid/ergohaven/entropy/releases/tag/v0.3.0",
            "https://github.com/ergohaven/another-repo/releases/tag/v0.3.0",
            "https://github.com/attacker/rmk/releases/tag/v0.1.6",
            "https://github.com/ergohaven/entropy/issues",
            "file:///C:/Windows/System32/calc.exe",
            "https://github.com/ergohaven/entropy/releases/tag/",
            "https://github.com/ergohaven/entropy/releases/download/",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.0/extra",
            "https://github.com/ergohaven/entropy/releases/download/v0.3.0/nested/Entropy.exe",
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.0?source=update",
            "https://github.com/ergohaven/entropy/releases/download/../../../../attacker/repo/releases/download/v1/payload.exe",
            "https://github.com/ergohaven/entropy/releases/download/%2e%2e/%2e%2e/%2e%2e/%2e%2e/attacker/repo/releases/download/v1/payload.exe",
            "https://github.com/ergohaven/entropy/releases/download/..\\..\\..\\../attacker/repo/releases/download/v1/payload.exe",
        ] {
            assert!(!is_trusted_release_url(url), "{url}");
        }
    }

    #[test]
    fn refuses_untrusted_url_without_launching() {
        let mut launched = false;
        assert!(!open_trusted_release_url(
            "file:///C:/Windows/System32/calc.exe",
            |_| {
                launched = true;
                true
            }
        ));
        assert!(!launched);
    }

    #[test]
    fn forwards_trusted_url_to_launcher() {
        let trusted_url = "https://github.com/ergohaven/rmk/releases/tag/v0.1.6";
        let mut launched_url = None;

        assert!(open_trusted_release_url(trusted_url, |url| {
            launched_url = Some(url.to_owned());
            true
        }));
        assert_eq!(launched_url.as_deref(), Some(trusted_url));
    }

    #[test]
    fn maps_all_fourteen_production_profiles() {
        let profiles = [
            (0x0036, false, "op36"),
            (0x0044, false, "imperial44"),
            (0x0070, false, "k03"),
            (0x00BE, false, "velvet"),
            (0x0036, true, "op36-qube"),
            (0x0044, true, "imperial44-qube"),
            (0x0070, true, "k03-qube"),
            (0x00BE, true, "velvet-qube"),
            (0x0074, false, "k04"),
            (0x0075, false, "k04-mini"),
            (0x0076, false, "k04-micro"),
            (0x0071, true, "k04-qube"),
            (0x0072, true, "k04-mini-qube"),
            (0x0073, true, "k04-micro-qube"),
        ];

        for (product_id, qube, expected_asset) in profiles {
            let target = rmk_firmware_release_target(
                ERGOHAVEN_VENDOR_ID,
                product_id,
                Some("0.1.6"),
                &serde_json::json!({}),
                qube,
                false,
            )
            .unwrap();
            assert_eq!(target.asset_prefix, expected_asset);
        }
    }

    #[test]
    fn standalone_trackballs_are_not_rmk_release_targets() {
        for product_id in [0x00C1, 0x00C2, 0x00C3] {
            assert!(rmk_firmware_release_target(
                ERGOHAVEN_VENDOR_ID,
                product_id,
                Some("0.1.6"),
                &serde_json::json!({}),
                false,
                false,
            )
            .is_none());
        }
    }

    #[test]
    fn does_not_mistake_qmk_k03_for_legacy_rmk() {
        assert!(rmk_firmware_release_target(
            ERGOHAVEN_VENDOR_ID,
            0x0070,
            Some("4.0.5"),
            &serde_json::json!({"firmware": {"name": "QMK"}}),
            false,
            false,
        )
        .is_none());
    }

    #[test]
    fn explicit_metadata_recognizes_future_rmk_and_overrides_catalog() {
        let target = rmk_firmware_release_target(
            ERGOHAVEN_VENDOR_ID,
            0x0070,
            Some("1.2.3"),
            &serde_json::json!({
                "firmware": {"name": "RMK"},
                "entropy": {"firmwareUpdate": {"asset": "K03-Future"}}
            }),
            false,
            false,
        )
        .unwrap();

        assert_eq!(target.asset_prefix, "k03-future");
        assert_eq!(target.current_version, "1.2.3");
    }

    #[test]
    fn rejects_invalid_explicit_asset_prefix() {
        assert!(rmk_firmware_release_target(
            ERGOHAVEN_VENDOR_ID,
            0x0070,
            Some("1.2.3"),
            &serde_json::json!({
                "firmware": {"name": "RMK"},
                "entropy": {"firmwareUpdate": {"asset": "../payload"}}
            }),
            false,
            false,
        )
        .is_none());
    }

    #[test]
    fn selects_newest_stable_release_containing_exact_asset() {
        let target = FirmwareReleaseTarget {
            current_version: "0.1.3".to_owned(),
            asset_prefix: "k03".to_owned(),
        };
        let releases = vec![
            release("v0.1.6", false, false, &["k04-v0.1.6.zip"]),
            release("v0.1.5", false, true, &["k03-v0.1.5.zip"]),
            release("v0.1.2", false, false, &["k03-v0.1.2.zip"]),
            release("v0.1.4", false, false, &["k03-v0.1.4.zip"]),
            release("v0.1.7", true, false, &["k03-v0.1.7.zip"]),
        ];

        let result = build_firmware_update_result(&target, releases).unwrap();

        assert_eq!(result.latest_version, "v0.1.4");
        assert_eq!(result.asset.name, "k03-v0.1.4.zip");
        assert_eq!(result.relation, VersionRelation::UpdateAvailable);
    }

    #[test]
    fn reports_no_firmware_package_when_no_release_contains_asset() {
        let target = FirmwareReleaseTarget {
            current_version: "0.1.6".to_owned(),
            asset_prefix: "missing-package".to_owned(),
        };

        assert!(build_firmware_update_result(
            &target,
            vec![release("v0.1.6", false, false, &["k04-v0.1.6.zip"])]
        )
        .is_none());
    }

    #[test]
    fn ensure_keeps_completed_check_for_same_target() {
        let target = FirmwareReleaseTarget {
            current_version: "0.1.6".to_owned(),
            asset_prefix: "k04".to_owned(),
        };
        let result = FirmwareUpdateCheckResult {
            latest_version: "v0.1.6".to_owned(),
            release_url: "https://github.com/ergohaven/rmk/releases/tag/v0.1.6".to_owned(),
            asset: UpdateAsset {
                name: "k04-v0.1.6.zip".to_owned(),
                url: "https://github.com/ergohaven/rmk/releases/download/v0.1.6/k04-v0.1.6.zip"
                    .to_owned(),
            },
            relation: VersionRelation::UpToDate,
        };
        let mut state = FirmwareUpdateCheckState::Ready {
            target: target.clone(),
            result,
        };

        ensure_firmware_update_check(&mut state, Some(target));

        assert!(matches!(state, FirmwareUpdateCheckState::Ready { .. }));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn disconnected_firmware_worker_preserves_target_and_reports_failure() {
        let target = FirmwareReleaseTarget {
            current_version: "0.1.6".to_owned(),
            asset_prefix: "k04".to_owned(),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        let mut state = FirmwareUpdateCheckState::Checking {
            target: target.clone(),
            receiver,
        };

        poll_firmware_update_check(&mut state);

        assert!(matches!(
            state,
            FirmwareUpdateCheckState::Failed {
                target: ref failed_target,
                ref error,
            } if failed_target == &target && error == "Update check thread died"
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn missing_firmware_asset_finishes_as_unavailable() {
        let target = FirmwareReleaseTarget {
            current_version: "0.1.6".to_owned(),
            asset_prefix: "missing-package".to_owned(),
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(FirmwareUpdateCheckOutcome::Unavailable)
            .unwrap();
        let mut state = FirmwareUpdateCheckState::Checking {
            target: target.clone(),
            receiver,
        };

        poll_firmware_update_check(&mut state);

        assert!(matches!(
            state,
            FirmwareUpdateCheckState::Unavailable {
                target: ref unavailable_target,
            } if unavailable_target == &target
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn disconnected_update_worker_finishes_with_failure() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        let mut state = UpdateCheckState::Checking { receiver };

        poll_update_check(&mut state);

        assert!(matches!(
            state,
            UpdateCheckState::Failed(ref error) if error == "Update check thread died"
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn pending_update_worker_stays_checking() {
        let (_sender, receiver) = std::sync::mpsc::channel();
        let mut state = UpdateCheckState::Checking { receiver };

        poll_update_check(&mut state);

        assert!(matches!(state, UpdateCheckState::Checking { .. }));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn failed_update_outcome_finishes_with_reported_error() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(UpdateCheckOutcome::Failed("network unavailable".to_owned()))
            .unwrap();
        let mut state = UpdateCheckState::Checking { receiver };

        poll_update_check(&mut state);

        assert!(matches!(
            state,
            UpdateCheckState::Failed(ref error) if error == "network unavailable"
        ));
    }
}
