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

pub(crate) fn ibus_user_installation_is_current() -> bool {
    let Some(data_home) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
    else {
        return false;
    };

    ibus_user_installation_is_current_at(&data_home)
}

fn ibus_user_installation_is_current_at(data_home: &Path) -> bool {
    let engine_path = data_home.join("entropy/ibus/entropy-ibus-engine");
    let component_path = data_home.join("ibus/component/entropy-universal-symbols.xml");
    let expected_engine = include_bytes!("../linux/ibus/entropy-ibus-engine");
    let component_template = include_str!("../linux/ibus/entropy-universal-symbols.xml.in");
    let expected_component =
        component_template.replace("@ENGINE_PATH@", engine_path.to_string_lossy().as_ref());

    std::fs::read(&engine_path).is_ok_and(|installed| installed == expected_engine)
        && std::fs::read_to_string(component_path)
            .is_ok_and(|installed| installed == expected_component)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_ibus_component_registers_only_entropy_english_and_russian() {
        let component = std::str::from_utf8(
            BUNDLED_LINUX_FILES
                .iter()
                .find(|file| file.path == "linux/ibus/entropy-universal-symbols.xml.in")
                .unwrap()
                .bytes,
        )
        .unwrap();

        assert_eq!(component.matches("<engine>").count(), 2);
        assert_eq!(component.matches("<longname>Entropy</longname>").count(), 2);
        assert!(!component.contains("entropy-universal-symbols-gb"));
    }

    #[test]
    fn ibus_installation_state_requires_current_engine_and_component() {
        let unique = format!(
            "entropy-ibus-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let data_home = std::env::temp_dir().join(unique);
        let engine_path = data_home.join("entropy/ibus/entropy-ibus-engine");
        let component_path = data_home.join("ibus/component/entropy-universal-symbols.xml");
        std::fs::create_dir_all(engine_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(component_path.parent().unwrap()).unwrap();
        std::fs::write(
            &engine_path,
            include_bytes!("../linux/ibus/entropy-ibus-engine"),
        )
        .unwrap();
        let component = include_str!("../linux/ibus/entropy-universal-symbols.xml.in")
            .replace("@ENGINE_PATH@", engine_path.to_string_lossy().as_ref());
        std::fs::write(&component_path, component).unwrap();

        assert!(ibus_user_installation_is_current_at(&data_home));
        std::fs::write(&component_path, "stale component").unwrap();
        assert!(!ibus_user_installation_is_current_at(&data_home));

        std::fs::remove_dir_all(data_home).unwrap();
    }
}
