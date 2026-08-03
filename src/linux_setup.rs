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

/// Where the IBus component describing the Entropy engine currently lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IbusComponentState {
    /// No component file mentions the Entropy engine.
    Missing,
    /// Installed into the user's data directory by `install-user.sh`.
    User,
    /// Registered outside the user's data directory — a distribution package or
    /// a declarative setup such as the NixOS module. Not ours to modify.
    System,
}

/// Identifier the component XML carries; see linux/ibus/entropy-universal-symbols.xml.in.
const IBUS_COMPONENT_NAME: &str = "org.freedesktop.IBus.Entropy";

pub(crate) fn ibus_component_state() -> IbusComponentState {
    ibus_component_state_in(
        user_ibus_component_dir().as_deref(),
        &system_ibus_component_dirs(),
    )
}

fn ibus_component_state_in(user_dir: Option<&Path>, system_dirs: &[PathBuf]) -> IbusComponentState {
    // A system registration wins: it keeps working regardless of what sits in
    // the user's directory, and the install/uninstall scripts must not touch it.
    if system_dirs.iter().any(|dir| dir_declares_entropy(dir)) {
        return IbusComponentState::System;
    }
    match user_dir {
        Some(dir) if dir_declares_entropy(dir) => IbusComponentState::User,
        _ => IbusComponentState::Missing,
    }
}

fn dir_declares_entropy(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.extension().is_some_and(|ext| ext == "xml")
            && std::fs::read_to_string(&path)
                .is_ok_and(|contents| contents.contains(IBUS_COMPONENT_NAME))
    })
}

fn user_ibus_component_dir() -> Option<PathBuf> {
    xdg_data_home().map(|data_home| data_home.join("ibus/component"))
}

/// Mirrors the lookup IBus itself performs: IBUS_COMPONENT_PATH wins outright
/// (ibus-with-plugins on NixOS pins it to a store path), otherwise every
/// XDG_DATA_DIRS entry is scanned, falling back to the XDG default.
fn system_ibus_component_dirs() -> Vec<PathBuf> {
    if let Some(paths) = std::env::var_os("IBUS_COMPONENT_PATH") {
        let user_dir = user_ibus_component_dir();
        return std::env::split_paths(&paths)
            .filter(|dir| Some(dir.as_path()) != user_dir.as_deref())
            .collect();
    }

    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    std::env::split_paths(&data_dirs)
        .map(|dir| dir.join("ibus/component"))
        .collect()
}

fn xdg_data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
}

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

    fn test_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "entropy-linux-setup-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_component(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("entropy-universal-symbols.xml"),
            include_str!("../linux/ibus/entropy-universal-symbols.xml.in"),
        )
        .unwrap();
    }

    #[test]
    fn component_state_is_missing_without_any_registration() {
        let root = test_dir("missing");
        let user = root.join("user");
        let system = root.join("system");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();

        assert_eq!(
            ibus_component_state_in(Some(&user), &[system]),
            IbusComponentState::Missing
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn component_state_reports_user_install() {
        let root = test_dir("user");
        let user = root.join("user");
        write_component(&user);

        assert_eq!(
            ibus_component_state_in(Some(&user), &[root.join("system")]),
            IbusComponentState::User
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn component_state_reports_system_registration() {
        let root = test_dir("system");
        let system = root.join("system");
        write_component(&system);

        assert_eq!(
            ibus_component_state_in(Some(&root.join("user")), &[system]),
            IbusComponentState::System
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn system_registration_wins_over_a_stale_user_copy() {
        let root = test_dir("both");
        let user = root.join("user");
        let system = root.join("system");
        write_component(&user);
        write_component(&system);

        assert_eq!(
            ibus_component_state_in(Some(&user), &[system]),
            IbusComponentState::System
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unrelated_components_do_not_count_as_entropy() {
        let root = test_dir("unrelated");
        let system = root.join("system");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(
            system.join("other.xml"),
            "<component><name>org.freedesktop.IBus.Other</name></component>",
        )
        .unwrap();

        assert_eq!(
            ibus_component_state_in(Some(&root.join("user")), &[system]),
            IbusComponentState::Missing
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
