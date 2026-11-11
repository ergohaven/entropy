//! Filesystem boundary for the resident pre-import backup. No device mutation here.
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Capture before application storage can create any missing config components.
/// The existing prefix is a provisioned namespace boundary, not a claim about
/// durability of its own entry in its parent. Retain this descriptor on failed
/// creation/sync and explicit retry; never rediscover a nearer anchor afterward.
/// Across process restarts provisioning of that boundary remains an external
/// precondition. This is not a power-loss or adversarial-rename certification.
#[derive(Clone, Debug)]
pub(crate) struct BackupNamespace {
    configured_root: PathBuf,
    anchor: PathBuf,
    config_suffix: Vec<OsString>,
}

impl BackupNamespace {
    pub(crate) fn resolve(config_root: &Path) -> io::Result<Self> {
        let configured_root = if config_root.is_absolute() {
            config_root.to_path_buf()
        } else {
            std::env::current_dir()?.join(config_root)
        };
        if configured_root
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Backup storage cannot contain parent traversal",
            ));
        }
        let mut existing = configured_root.clone();
        let mut config_suffix = Vec::new();
        loop {
            match std::fs::metadata(&existing) {
                Ok(metadata) if metadata.is_dir() => break,
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "Backup storage prefix is not a directory",
                    ))
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let name = existing.file_name().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "No provisioned backup storage boundary",
                        )
                    })?;
                    config_suffix.push(name.to_os_string());
                    if !existing.pop() {
                        return Err(error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        config_suffix.reverse();
        Ok(Self {
            configured_root,
            anchor: existing.canonicalize()?,
            config_suffix,
        })
    }

    pub(crate) fn matches_config_root(&self, root: &Path) -> bool {
        let absolute = if root.is_absolute() {
            root.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(root)
        } else {
            return false;
        };
        absolute == self.configured_root
    }
}

/// Publication has completed the selected platform's file/directory policy.
#[derive(Debug)]
pub(super) struct PublishedBackup {
    pub(super) path: PathBuf,
}
impl PublishedBackup {
    pub(super) fn from_legacy_backup(path: PathBuf) -> Self {
        Self { path }
    }
}
impl AsRef<Path> for PublishedBackup {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

pub(super) fn publish_backup(
    namespace: &BackupNamespace,
    slug: &str,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<PublishedBackup> {
    #[cfg(target_os = "windows")]
    let fs = FilePublicationFs;
    #[cfg(not(target_os = "windows"))]
    let fs = RealFs;
    publish_backup_with(&fs, namespace, slug, write)
}

trait BackupFs {
    fn create_directory(&self, path: &Path) -> io::Result<()> {
        create_directory(path)
    }
    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        sync_directory(path)
    }
    fn sync_creation_parents(&self, parents: &[PathBuf]) -> io::Result<()> {
        for parent in parents.iter().rev() {
            self.sync_directory(parent)?;
        }
        Ok(())
    }
    fn sync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
    fn sync_published_backup(&self, dir: &Path, _path: &Path) -> io::Result<()> {
        self.sync_directory(dir)
    }
    fn publish(&self, file: tempfile::NamedTempFile, path: &Path) -> io::Result<()> {
        file.persist_noclobber(path)
            .map(|_| ())
            .map_err(|e| e.error)
    }
}

fn create_directory(path: &Path) -> io::Result<()> {
    match std::fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // Existing config-root aliases were resolved at capture. App-owned
            // suffix components must remain directories, never symlink escapes.
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                Ok(())
            } else {
                Err(error)
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(target_os = "windows"))]
struct RealFs;
#[cfg(not(target_os = "windows"))]
impl BackupFs for RealFs {}

/// Windows intentionally provides a file-level, non-administrative contract:
/// flush complete bytes, publish without replacement, then flush the published
/// file. This does NOT establish power-loss durability of newly created parent
/// entries. That limited namespace risk is accepted; no volume flush or UAC is
/// requested, and no reported filesystem error permits device mutation.
/// Tests on other hosts exercise this same policy, not native Windows APIs.
#[cfg(any(target_os = "windows", test))]
struct FilePublicationFs;
#[cfg(any(target_os = "windows", test))]
impl BackupFs for FilePublicationFs {
    fn sync_creation_parents(&self, _parents: &[PathBuf]) -> io::Result<()> {
        Ok(())
    }

    fn sync_published_backup(&self, _dir: &Path, path: &Path) -> io::Result<()> {
        File::options().write(true).open(path)?.sync_all()
    }
}

fn publish_backup_with(
    fs: &impl BackupFs,
    namespace: &BackupNamespace,
    slug: &str,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<PublishedBackup> {
    let mut dir = namespace.anchor.clone();
    let mut parents = Vec::new();
    for component in namespace
        .config_suffix
        .iter()
        .map(OsString::as_os_str)
        .chain([
            std::ffi::OsStr::new("entropy"),
            std::ffi::OsStr::new("backups"),
        ])
    {
        parents.push(dir.clone());
        dir.push(component);
        fs.create_directory(&dir)?;
    }
    // Unix retries the whole captured parent chain, including components left
    // by a failed attempt. Windows retains the documented file-only policy.
    fs.sync_creation_parents(&parents)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".pending-backup-")
        .tempfile_in(&dir)?;
    write(staged.as_file_mut())?;
    fs.sync_file(staged.as_file())?;
    let path = dir.join(format!(
        "auto-backup-{}-{}.entlayout",
        slug,
        staged.path().file_name().unwrap().to_string_lossy()
    ));
    fs.publish(staged, &path)?;
    // Every selected-platform operation must succeed before the caller may SET.
    fs.sync_published_backup(&dir, &path)?;
    Ok(PublishedBackup { path })
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Cannot establish import backup directory durability on this platform; import refused",
    ))
}

#[cfg(test)]
#[path = "entlayout_backup_tests.rs"]
mod tests;

#[cfg(all(test, not(any(unix, target_os = "windows"))))]
#[test]
fn unsupported_directory_durability_refuses_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let result = publish_backup(&namespace, "keyboard", |_| {
        panic!("must refuse before backup serialization or device mutation")
    });
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Unsupported);
}
