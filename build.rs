use std::{
    env,
    path::{Path, PathBuf},
};

const WINDOWS_ICON_PATH: &str = "assets/entropy.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={WINDOWS_ICON_PATH}");

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
