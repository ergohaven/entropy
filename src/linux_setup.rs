use std::path::{Path, PathBuf};

struct BundledLinuxFile {
    path: &'static str,
    bytes: &'static [u8],
    executable: bool,
}

const BUNDLED_LINUX_FILES: &[BundledLinuxFile] = &[
    BundledLinuxFile {
        path: "linux/ibus/install-user.sh",
        bytes: include_bytes!("../linux/ibus/install-user.sh"),
        executable: true,
    },
    BundledLinuxFile {
        path: "linux/ibus/uninstall-user.sh",
        bytes: include_bytes!("../linux/ibus/uninstall-user.sh"),
        executable: true,
    },
    BundledLinuxFile {
        path: "linux/ibus/entropy-ibus-engine",
        bytes: include_bytes!("../linux/ibus/entropy-ibus-engine"),
        executable: true,
    },
    BundledLinuxFile {
        path: "linux/ibus/entropy-universal-symbols.xml.in",
        bytes: include_bytes!("../linux/ibus/entropy-universal-symbols.xml.in"),
        executable: false,
    },
    BundledLinuxFile {
        path: "linux/udev/install-vial-rules.sh",
        bytes: include_bytes!("../linux/udev/install-vial-rules.sh"),
        executable: true,
    },
];

pub(crate) fn setup_script_path(script: &str) -> Option<PathBuf> {
    find_existing_resource(script).or_else(|| materialize_bundled_resource_group(script).ok())
}

pub(crate) fn bundled_ibus_engine_path() -> Option<PathBuf> {
    const ENGINE: &str = "linux/ibus/entropy-ibus-engine";
    find_existing_resource(ENGINE).or_else(|| materialize_bundled_resource_group(ENGINE).ok())
}

fn find_existing_resource(resource: &str) -> Option<PathBuf> {
    let relative = Path::new(resource);
    if relative.exists() {
        return Some(relative.to_path_buf());
    }
    if let Some(appdir) = std::env::var_os("APPDIR") {
        let path = PathBuf::from(appdir).join(resource);
        if path.exists() {
            return Some(path);
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .and_then(|dir| {
            for ancestor in dir.ancestors() {
                let path = ancestor.join(resource);
                if path.exists() {
                    return Some(path);
                }
            }
            None
        })
}

fn materialize_bundled_resource_group(resource: &str) -> std::io::Result<PathBuf> {
    let Some(target_file) = BUNDLED_LINUX_FILES
        .iter()
        .find(|file| file.path == resource)
    else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("unknown bundled Linux resource: {resource}"),
        ));
    };
    let Some(group_dir) = Path::new(target_file.path).parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("bundled Linux resource has no parent: {resource}"),
        ));
    };
    let root = bundled_resource_root();
    for file in BUNDLED_LINUX_FILES
        .iter()
        .filter(|file| Path::new(file.path).starts_with(group_dir))
    {
        write_bundled_file(&root, file)?;
    }
    Ok(root.join(resource))
}

fn bundled_resource_root() -> PathBuf {
    let cache_home = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".cache"))
        })
        .unwrap_or_else(std::env::temp_dir);
    cache_home
        .join("entropy/bundled")
        .join(env!("CARGO_PKG_VERSION"))
}

fn write_bundled_file(root: &Path, file: &BundledLinuxFile) -> std::io::Result<()> {
    let path = root.join(file.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::read(&path).ok().as_deref() != Some(file.bytes) {
        std::fs::write(&path, file.bytes)?;
    }
    if file.executable {
        set_executable(&path)?;
    }
    Ok(())
}

fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}
