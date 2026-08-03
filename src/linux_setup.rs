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

/// Where the IBus component describing the Entropy engine is registered.
///
/// The two are independent: a machine can carry a declarative registration and
/// a leftover copy from `install-user.sh` at the same time, and the setup
/// screen has to offer removal of the latter without pretending it can touch
/// the former.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct IbusRegistration {
    /// Installed into the user's data directory by `install-user.sh`.
    pub(crate) user: bool,
    /// Registered outside the user's data directory — a distribution package or
    /// a declarative setup such as the NixOS module. Not ours to modify.
    pub(crate) system: bool,
}

/// Identifier the component XML carries; see linux/ibus/entropy-universal-symbols.xml.in.
const IBUS_COMPONENT_NAME: &str = "org.freedesktop.IBus.Entropy";

/// Scans the component directories. Hits the filesystem, so callers cache the
/// result rather than asking once per frame.
pub(crate) fn ibus_registration() -> IbusRegistration {
    let user_dir = user_ibus_component_dir();
    ibus_registration_in(
        user_dir.as_deref(),
        &system_ibus_component_dirs(
            std::env::var_os("IBUS_COMPONENT_PATH").as_deref(),
            std::env::var_os("XDG_DATA_DIRS").as_deref(),
            user_dir.as_deref(),
        ),
    )
}

fn ibus_registration_in(user_dir: Option<&Path>, system_dirs: &[PathBuf]) -> IbusRegistration {
    IbusRegistration {
        user: user_dir.is_some_and(dir_declares_entropy),
        system: system_dirs.iter().any(|dir| dir_declares_entropy(dir)),
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
///
/// The user's own directory is filtered out of both: XDG_DATA_DIRS routinely
/// carries paths inside $HOME (profile directories, for one), and a user
/// install showing up as a system registration would hide the very action that
/// removes it.
fn system_ibus_component_dirs(
    component_path: Option<&std::ffi::OsStr>,
    data_dirs: Option<&std::ffi::OsStr>,
    user_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let dirs: Vec<PathBuf> = match component_path {
        Some(paths) => std::env::split_paths(paths).collect(),
        None => {
            let data_dirs = data_dirs
                .filter(|dirs| !dirs.is_empty())
                .unwrap_or_else(|| std::ffi::OsStr::new("/usr/local/share:/usr/share"));
            std::env::split_paths(data_dirs)
                .map(|dir| dir.join("ibus/component"))
                .collect()
        }
    };
    dirs.into_iter()
        .filter(|dir| Some(dir.as_path()) != user_dir)
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
    fn registration_is_empty_without_any_component() {
        let root = test_dir("missing");
        let user = root.join("user");
        let system = root.join("system");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();

        assert_eq!(
            ibus_registration_in(Some(&user), &[system]),
            IbusRegistration {
                user: false,
                system: false
            }
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn registration_reports_a_user_install() {
        let root = test_dir("user");
        let user = root.join("user");
        write_component(&user);

        assert_eq!(
            ibus_registration_in(Some(&user), &[root.join("system")]),
            IbusRegistration {
                user: true,
                system: false
            }
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn registration_reports_a_system_install() {
        let root = test_dir("system");
        let system = root.join("system");
        write_component(&system);

        assert_eq!(
            ibus_registration_in(Some(&root.join("user")), &[system]),
            IbusRegistration {
                user: false,
                system: true
            }
        );
        std::fs::remove_dir_all(&root).ok();
    }

    // Both can be present at once, and the user copy stays visible so that the
    // uninstall action for it does not disappear.
    #[test]
    fn registration_reports_both_copies_independently() {
        let root = test_dir("both");
        let user = root.join("user");
        let system = root.join("system");
        write_component(&user);
        write_component(&system);

        assert_eq!(
            ibus_registration_in(Some(&user), &[system]),
            IbusRegistration {
                user: true,
                system: true
            }
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
            ibus_registration_in(Some(&root.join("user")), &[system]),
            IbusRegistration::default()
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn ibus_component_path_takes_precedence_over_data_dirs() {
        let dirs = system_ibus_component_dirs(
            Some(std::ffi::OsStr::new("/pinned/component")),
            Some(std::ffi::OsStr::new("/ignored")),
            None,
        );

        assert_eq!(dirs, vec![PathBuf::from("/pinned/component")]);
    }

    #[test]
    fn data_dirs_fall_back_to_the_xdg_default() {
        let dirs = system_ibus_component_dirs(None, None, None);

        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/usr/local/share/ibus/component"),
                PathBuf::from("/usr/share/ibus/component"),
            ]
        );
    }

    // XDG_DATA_DIRS regularly contains paths inside $HOME, and IBUS_COMPONENT_PATH
    // can be set to the user's own directory. Either way a user install must not
    // be mistaken for a system one.
    #[test]
    fn the_user_directory_is_never_treated_as_a_system_one() {
        let user = PathBuf::from("/home/someone/.local/share/ibus/component");

        let from_data_dirs = system_ibus_component_dirs(
            None,
            Some(std::ffi::OsStr::new(
                "/home/someone/.local/share:/usr/share",
            )),
            Some(&user),
        );
        assert_eq!(
            from_data_dirs,
            vec![PathBuf::from("/usr/share/ibus/component")]
        );

        let from_component_path = system_ibus_component_dirs(
            Some(std::ffi::OsStr::new(
                "/home/someone/.local/share/ibus/component:/usr/share/ibus/component",
            )),
            None,
            Some(&user),
        );
        assert_eq!(
            from_component_path,
            vec![PathBuf::from("/usr/share/ibus/component")]
        );
    }
}
