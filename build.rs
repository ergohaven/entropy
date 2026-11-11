use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

const WINDOWS_ICON_PATH: &str = "assets/entropy.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    record_source_revision();
    println!("cargo:rerun-if-changed={WINDOWS_ICON_PATH}");

    // rmk-types takes its constants, the combo, morse and bulk sizes among
    // them, from a keyboard config named here: a shell left from a firmware
    // build would give Entropy that keyboard's constants.
    for name in ["KEYBOARD_TOML_PATH", "VIAL_JSON_PATH"] {
        println!("cargo:rerun-if-env-changed={name}");
        assert!(
            env::var_os(name).is_none(),
            "{name} is set: rmk-types would build Entropy with that keyboard's constants; unset it"
        );
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    assert!(
        Path::new(WINDOWS_ICON_PATH).is_file(),
        "Windows icon file is missing: {WINDOWS_ICON_PATH}"
    );

    let mut res = winresource::WindowsResource::new();
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") && env::var_os("WINDRES").is_none()
    {
        if let Some(windres) = find_windres() {
            res.set_windres_path(windres.to_string_lossy().as_ref());
        }
    }

    res.set_icon(WINDOWS_ICON_PATH);
    res.set("ProductName", "Entropy");
    res.set("FileDescription", "Entropy");
    res.set("CompanyName", "Ergohaven");
    res.set("LegalCopyright", "© Ergohaven");

    res.compile().expect(
        "failed to embed Windows resources; refusing to build a Windows binary without an icon",
    );
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn record_source_revision() {
    // Source archives can be built inside another repository. Never embed
    // that enclosing repository's revision as Entropy's identity.
    let source_root =
        env::var_os("CARGO_MANIFEST_DIR").and_then(|p| PathBuf::from(p).canonicalize().ok());
    let git_root =
        git(&["rev-parse", "--show-toplevel"]).and_then(|p| PathBuf::from(p).canonicalize().ok());
    if source_root.is_none() || source_root != git_root {
        return;
    }
    // Watch both loose and packed refs, including linked worktrees. A commit
    // must invalidate this metadata even when none of the Rust sources changed.
    for name in [
        "HEAD".to_owned(),
        "packed-refs".to_owned(),
        "refs/tags".to_owned(),
    ]
    .into_iter()
    .chain(git(&["symbolic-ref", "-q", "HEAD"]))
    {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(commit) = git(&["rev-parse", "HEAD"])
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        println!("cargo:rustc-env=ENTROPY_BUILD_COMMIT={commit}");
        let release_tag = format!("v{}", env::var("CARGO_PKG_VERSION").unwrap_or_default());
        let tagged = git(&["tag", "--points-at", "HEAD"])
            .is_some_and(|tags| tags.lines().any(|tag| tag == release_tag));
        println!("cargo:rustc-env=ENTROPY_BUILD_DEVELOPMENT={}", !tagged);
    }
}

fn find_windres() -> Option<PathBuf> {
    [
        "x86_64-w64-mingw32-windres",
        "x86_64-w64-mingw32ucrt-windres",
        "windres",
    ]
    .into_iter()
    .find_map(find_in_path)
}

fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}
