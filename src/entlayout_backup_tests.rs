use super::*;
use std::cell::RefCell;
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Create(PathBuf),
    DirectorySync(PathBuf),
    Write,
    FileSync,
    Publish,
    PublicationBarrier,
    FirstSet,
}

#[derive(Default)]
struct RecordingFs {
    ops: RefCell<Vec<Op>>,
    fail: Option<Op>,
    collision: bool,
    destination: RefCell<Option<PathBuf>>,
}

impl RecordingFs {
    fn record(&self, op: Op) -> io::Result<()> {
        self.ops.borrow_mut().push(op.clone());
        if self.fail.as_ref() == Some(&op) {
            return Err(io::Error::other(format!("injected {op:?}")));
        }
        Ok(())
    }
}

impl BackupFs for RecordingFs {
    fn create_directory(&self, path: &Path) -> io::Result<()> {
        self.record(Op::Create(path.to_path_buf()))?;
        create_directory(path)
    }
    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        self.record(Op::DirectorySync(path.to_path_buf()))?;
        sync_directory(path)
    }
    fn sync_file(&self, file: &File) -> io::Result<()> {
        self.record(Op::FileSync)?;
        file.sync_all()
    }
    fn sync_published_backup(&self, dir: &Path, _path: &Path) -> io::Result<PublicationGuard> {
        self.record(Op::PublicationBarrier)?;
        self.sync_directory(dir)?;
        Ok(PublicationGuard::unprotected())
    }
    fn publish(&self, file: tempfile::NamedTempFile, path: &Path) -> io::Result<()> {
        self.record(Op::Publish)?;
        *self.destination.borrow_mut() = Some(path.to_path_buf());
        if self.collision {
            std::fs::write(path, b"earlier backup")?;
        }
        file.persist_noclobber(path)
            .map(|_| ())
            .map_err(|e| e.error)
    }
}

// Same production helper as the GUI writer: only filesystem operations are
// recorded/fault-injected. The caller's `?` gate is represented by FirstSet;
// this standalone seam does not execute the application or any real HID SET.
fn import_gate(fs: &RecordingFs, namespace: &BackupNamespace) -> io::Result<PathBuf> {
    let path = publish_backup_with(fs, namespace, "keyboard", |file| {
        fs.record(Op::Write)?;
        file.write_all(b"complete backup")
    })?;
    fs.record(Op::FirstSet)?;
    Ok(path.path)
}

fn position(ops: &[Op], op: Op) -> usize {
    ops.iter()
        .position(|actual| actual == &op)
        .unwrap_or_else(|| panic!("missing {op:?} in {ops:?}"))
}

#[test]
#[cfg(unix)]
fn new_directory_chain_is_synced_before_publication_and_first_set() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let dir = root.join("new-config/entropy/backups");
    let namespace = BackupNamespace::resolve(&root.join("new-config")).unwrap();
    let fs = RecordingFs::default();
    let path = import_gate(&fs, &namespace).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"complete backup");
    let ops = fs.ops.borrow();
    assert!(
        ops.iter().all(|op| match op {
            Op::DirectorySync(path) => path.starts_with(&namespace.anchor),
            _ => true,
        }),
        "visited a directory outside captured namespace: {ops:?}"
    );
    for parent in [
        &root,
        &root.join("new-config"),
        &root.join("new-config/entropy"),
    ] {
        assert!(
            position(&ops, Op::Create(dir.clone()))
                < position(&ops, Op::DirectorySync(parent.clone()))
        );
        assert!(position(&ops, Op::DirectorySync(parent.clone())) < position(&ops, Op::Write));
    }
    assert!(position(&ops, Op::Write) < position(&ops, Op::FileSync));
    assert!(position(&ops, Op::FileSync) < position(&ops, Op::Publish));
    assert!(position(&ops, Op::Publish) < position(&ops, Op::DirectorySync(dir)));
    assert!(
        position(
            &ops,
            Op::DirectorySync(root.join("new-config/entropy/backups"))
        ) < position(&ops, Op::FirstSet)
    );
}

#[test]
#[cfg(unix)]
fn every_parent_sync_failure_refuses_before_write_or_first_set() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let namespace = BackupNamespace::resolve(&root.join("new-config")).unwrap();
    for parent in [
        &root,
        &root.join("new-config"),
        &root.join("new-config/entropy"),
    ] {
        let fs = RecordingFs {
            fail: Some(Op::DirectorySync(parent.clone())),
            ..Default::default()
        };
        let result = import_gate(&fs, &namespace);
        assert!(
            result.is_err(),
            "parent sync failure was ignored: {parent:?}"
        );
        let ops = fs.ops.borrow();
        assert!(!ops.contains(&Op::Write));
        assert!(!ops.contains(&Op::FirstSet));
    }
    // A previous failure may have left all components present. Explicit retry
    // still has to sync the parent entries, not assume existence is durability.
    let fs = RecordingFs::default();
    import_gate(&fs, &namespace).unwrap();
    assert!(fs.ops.borrow().contains(&Op::DirectorySync(root)));
}

#[test]
#[cfg(unix)]
fn existing_directory_and_earlier_backup_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let dir = root.path().canonicalize().unwrap().join("entropy/backups");
    std::fs::create_dir_all(&dir).unwrap();
    let earlier = dir.join("earlier.entlayout");
    std::fs::write(&earlier, b"earlier backup").unwrap();
    let path = publish_backup(&namespace, "keyboard", |file| {
        file.write_all(b"complete backup")
    })
    .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"complete backup");
    assert_eq!(std::fs::read(earlier).unwrap(), b"earlier backup");
}

#[test]
#[cfg(unix)]
fn filesystem_failures_refuse_without_first_set() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let dir = root.path().canonicalize().unwrap().join("entropy/backups");
    std::fs::create_dir_all(&dir).unwrap();
    let earlier = dir.join("earlier.entlayout");
    std::fs::write(&earlier, b"earlier backup").unwrap();
    for fail in [
        Op::Create(dir.clone()),
        Op::Write,
        Op::FileSync,
        Op::Publish,
        Op::DirectorySync(dir.clone()),
    ] {
        let fs = RecordingFs {
            fail: Some(fail.clone()),
            ..Default::default()
        };
        assert!(import_gate(&fs, &namespace).is_err(), "ignored {fail:?}");
        assert!(!fs.ops.borrow().contains(&Op::FirstSet));
        assert_eq!(std::fs::read(&earlier).unwrap(), b"earlier backup");
        if fail == Op::DirectorySync(dir.clone()) {
            // Publication already happened: retain the complete backup, but
            // refuse SET when its final directory sync failed.
            assert_eq!(
                std::fs::read(fs.destination.borrow().as_ref().unwrap()).unwrap(),
                b"complete backup"
            );
        } else {
            assert!(fs.destination.borrow().is_none());
        }
    }
}

#[test]
#[cfg(unix)]
fn real_create_error_refuses_without_first_set() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let blocked = root.path().join("entropy");
    std::fs::write(&blocked, b"keep").unwrap();
    let fs = RecordingFs::default();
    assert!(import_gate(&fs, &namespace).is_err());
    assert_eq!(std::fs::read(blocked).unwrap(), b"keep");
    assert!(!fs.ops.borrow().contains(&Op::FirstSet));
}

#[test]
fn failed_platform_publication_barrier_refuses_first_set_and_retains_backup() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let fs = RecordingFs {
        fail: Some(Op::PublicationBarrier),
        ..Default::default()
    };
    assert!(
        import_gate(&fs, &namespace).is_err(),
        "platform barrier was bypassed"
    );
    let ops = fs.ops.borrow();
    assert!(position(&ops, Op::Publish) < position(&ops, Op::PublicationBarrier));
    assert!(!ops.contains(&Op::FirstSet));
    assert_eq!(
        std::fs::read(fs.destination.borrow().as_ref().unwrap()).unwrap(),
        b"complete backup"
    );
}

#[test]
#[cfg(unix)]
fn existing_destination_is_never_clobbered() {
    let root = tempfile::tempdir().unwrap();
    let namespace = BackupNamespace::resolve(root.path()).unwrap();
    let fs = RecordingFs {
        collision: true,
        ..Default::default()
    };
    let error = import_gate(&fs, &namespace).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(
        std::fs::read(fs.destination.borrow().as_ref().unwrap()).unwrap(),
        b"earlier backup"
    );
    assert!(!fs.ops.borrow().contains(&Op::FirstSet));
}
