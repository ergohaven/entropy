//! Development downloads come only from successful builds of upstream rynk.
//! GitHub serves artifact archives to signed-in users through their browser.
use super::*;

const BUILDS_API: &str = "https://api.github.com/repos/ergohaven/entropy/actions/workflows/build.yml/runs?branch=rynk&status=success&per_page=30";

#[derive(serde::Deserialize)]
struct Builds {
    workflow_runs: Vec<Build>,
}

#[derive(serde::Deserialize)]
struct Repository {
    full_name: String,
}

#[derive(serde::Deserialize)]
struct Build {
    id: u64,
    event: String,
    status: String,
    conclusion: Option<String>,
    head_branch: Option<String>,
    head_sha: String,
    head_repository: Option<Repository>,
}

impl Build {
    fn eligible(&self) -> bool {
        self.id > 0
            && matches!(self.event.as_str(), "push" | "workflow_dispatch")
            && self.status == "completed"
            && self.conclusion.as_deref() == Some("success")
            && self.head_branch.as_deref() == Some("rynk")
            && self
                .head_repository
                .as_ref()
                .is_some_and(|repo| repo.full_name == "ergohaven/entropy")
            && self.head_sha.len() == 40
            && self.head_sha.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

#[derive(serde::Deserialize)]
struct Artifacts {
    artifacts: Vec<Artifact>,
}

#[derive(serde::Deserialize)]
struct Artifact {
    id: u64,
    name: String,
    expired: bool,
}

fn artifact_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("entropy-macos-arm64"),
        ("macos", "x86_64") => Some("entropy-macos-x86_64"),
        ("windows", "x86_64") => Some("entropy-windows"),
        ("linux", "x86_64") => Some("entropy-linux-packages"),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn fetch_latest_build() -> Result<UpdateCheckResult, String> {
    let builds: Builds = fetch_github_json(BUILDS_API)?;
    let expected = artifact_name(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| "Development builds are not available for this platform".to_owned())?;
    select_build(
        builds,
        expected,
        |id| {
            fetch_github_json(&format!(
            "https://api.github.com/repos/ergohaven/entropy/actions/runs/{id}/artifacts?per_page=100"
        ))
        },
        |latest| {
            current_build_commit()
                .and_then(|current| compare_source_commits(current, latest).ok())
                // Unknown or unpublished local sources cannot prove a newer build.
                .unwrap_or(VersionRelation::Unknown)
        },
    )
}

fn select_build(
    builds: Builds,
    expected: &str,
    mut artifacts: impl FnMut(u64) -> Result<Artifacts, String>,
    relation: impl FnOnce(&str) -> VersionRelation,
) -> Result<UpdateCheckResult, String> {
    // Bound API usage even when recent runs reused validation without binaries.
    for build in builds
        .workflow_runs
        .into_iter()
        .filter(Build::eligible)
        .take(5)
    {
        let Some(artifact) = artifacts(build.id)?
            .artifacts
            .into_iter()
            .find(|a| a.id > 0 && !a.expired && a.name == expected)
        else {
            // CI can reuse validation without producing downloadable binaries.
            continue;
        };
        let release_url = format!(
            "https://github.com/ergohaven/entropy/actions/runs/{}",
            build.id
        );
        return Ok(UpdateCheckResult {
            relation: relation(&build.head_sha),
            latest_version: format!("rynk {}", &build.head_sha[..8]),
            platform_label: current_platform_label(),
            asset: UpdateAssetState::Available(UpdateAsset {
                name: format!("{}.zip", artifact.name),
                url: format!("{release_url}/artifacts/{}", artifact.id),
            }),
            release_url,
        });
    }
    Err(
        "No published rynk download for this platform in the five latest successful builds"
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(id: u64) -> serde_json::Value {
        serde_json::json!({
            "id": id, "event": "workflow_dispatch", "status": "completed",
            "conclusion": "success", "head_branch": "rynk",
            "head_sha": "8f7311271e5f15b5305f30e59982b4e861ffd74b",
            "head_repository": {"full_name": "ergohaven/entropy"}
        })
    }

    #[test]
    fn only_successful_upstream_branch_builds_are_eligible() {
        for (field, value) in [
            ("event", serde_json::json!("pull_request")),
            ("status", serde_json::json!("in_progress")),
            ("conclusion", serde_json::json!("failure")),
            ("head_branch", serde_json::json!("main")),
            ("head_branch", serde_json::Value::Null),
            ("head_repository", serde_json::Value::Null),
            ("head_sha", serde_json::json!("invalid")),
            (
                "head_repository",
                serde_json::json!({"full_name":"someone/entropy"}),
            ),
        ] {
            let mut bad = build(1);
            bad[field] = value;
            let bad: Build = serde_json::from_value(bad).unwrap();
            assert!(!bad.eligible(), "{field}");
        }
        assert!(serde_json::from_value::<Build>(build(1))
            .unwrap()
            .eligible());
    }

    #[test]
    fn skips_missing_or_expired_artifacts_and_selects_the_platform_download() {
        let builds: Builds = serde_json::from_value(serde_json::json!({
            "workflow_runs": [build(3), build(2), build(1)]
        }))
        .unwrap();
        let result = select_build(
            builds,
            "entropy-macos-arm64",
            |id| {
                Ok(Artifacts {
                    artifacts: match id {
                        3 => vec![],
                        _ => vec![Artifact {
                            id: 42,
                            name: "entropy-macos-arm64".into(),
                            expired: id == 2,
                        }],
                    },
                })
            },
            |_| VersionRelation::UpToDate,
        )
        .unwrap();
        assert_eq!(result.latest_version, "rynk 8f731127");
        assert_eq!(result.relation, VersionRelation::UpToDate);
        assert_eq!(
            result.downloadable_asset().unwrap().url,
            "https://github.com/ergohaven/entropy/actions/runs/1/artifacts/42"
        );
        assert!(is_trusted_release_url(&result.release_url));
        assert!(is_trusted_release_url(
            &result.downloadable_asset().unwrap().url
        ));
        assert_eq!(
            artifact_name("linux", "x86_64"),
            Some("entropy-linux-packages")
        );
        assert_eq!(
            artifact_name("macos", "aarch64"),
            Some("entropy-macos-arm64")
        );
        assert_eq!(artifact_name("windows", "aarch64"), None);
    }

    #[test]
    fn deleted_fork_metadata_does_not_discard_an_upstream_download() {
        let mut deleted = build(2);
        deleted["head_branch"] = serde_json::Value::Null;
        deleted["head_repository"] = serde_json::Value::Null;
        let builds: Builds = serde_json::from_value(serde_json::json!({
            "workflow_runs": [deleted, build(1)]
        }))
        .unwrap();
        let result = select_build(
            builds,
            "entropy-macos-arm64",
            |id| {
                assert_eq!(id, 1);
                Ok(Artifacts {
                    artifacts: vec![Artifact {
                        id: 7,
                        name: "entropy-macos-arm64".into(),
                        expired: false,
                    }],
                })
            },
            |_| VersionRelation::Unknown,
        )
        .unwrap();
        assert_eq!(result.relation, VersionRelation::Unknown);
        assert!(result.downloadable_asset().is_some());
    }

    #[test]
    fn stops_after_five_artifact_lookups_without_a_download() {
        let builds: Builds = serde_json::from_value(serde_json::json!({
            "workflow_runs": (1..=10).map(build).collect::<Vec<_>>()
        }))
        .unwrap();
        let mut count = 0;
        assert!(select_build(
            builds,
            "entropy-macos-arm64",
            |_| {
                count += 1;
                Ok(Artifacts { artifacts: vec![] })
            },
            |_| panic!("no download")
        )
        .is_err());
        assert_eq!(count, 5);
    }

    #[test]
    fn never_substitutes_a_fork_pr_artifact_or_another_platform() {
        let mut pr = build(2);
        pr["event"] = serde_json::json!("pull_request");
        let builds: Builds = serde_json::from_value(serde_json::json!({
            "workflow_runs": [pr, build(1)]
        }))
        .unwrap();
        let result = select_build(
            builds,
            "entropy-macos-arm64",
            |id| {
                assert_eq!(id, 1);
                Ok(Artifacts {
                    artifacts: vec![Artifact {
                        id: 1,
                        name: "entropy-macos-x86_64".into(),
                        expired: false,
                    }],
                })
            },
            |_| panic!("no downloadable build"),
        );
        assert!(result.is_err());
    }
}
