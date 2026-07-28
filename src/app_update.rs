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
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }

    let Some(segments) = url.path_segments() else {
        return false;
    };
    match segments.collect::<Vec<_>>().as_slice() {
        ["ergohaven", "entropy", "releases", "tag", tag] => !tag.is_empty(),
        ["ergohaven", "entropy", "releases", "download", tag, asset] => {
            !tag.is_empty() && !asset.is_empty()
        }
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusts_entropy_release_urls() {
        assert!(is_trusted_release_url(
            "https://github.com/ergohaven/entropy/releases/tag/v0.3.0"
        ));
        assert!(is_trusted_release_url(
            "https://github.com/ergohaven/entropy/releases/download/v0.3.0/Entropy-Windows-x86_64.exe"
        ));
        assert!(is_trusted_release_url(
            "https://github.com/ergohaven/entropy/releases/tag/release%2Fv0.3.0"
        ));
    }

    #[test]
    fn rejects_untrusted_update_urls() {
        for url in [
            "http://github.com/ergohaven/entropy/releases/tag/v0.3.0",
            "https://github.com.evil.invalid/ergohaven/entropy/releases/tag/v0.3.0",
            "https://github.com/ergohaven/another-repo/releases/tag/v0.3.0",
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
        let trusted_url = "https://github.com/ergohaven/entropy/releases/tag/v0.3.0";
        let mut launched_url = None;

        assert!(open_trusted_release_url(trusted_url, |url| {
            launched_url = Some(url.to_owned());
            true
        }));
        assert_eq!(launched_url.as_deref(), Some(trusted_url));
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
