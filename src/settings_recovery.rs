use crate::app::portable_settings::{PortableSetting, PortableSettingId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DOCUMENT_VERSION: u32 = 1;
const MAX_SNAPSHOTS: usize = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RecoveryIdentity {
    pub(crate) vendor_id: u16,
    pub(crate) product_id: u16,
    pub(crate) vial_keyboard_id: String,
    pub(crate) serial: String,
}

impl RecoveryIdentity {
    pub(crate) fn new(
        vendor_id: u16,
        product_id: u16,
        vial_keyboard_id: impl Into<String>,
        serial: Option<&str>,
    ) -> Option<Self> {
        let vial_keyboard_id = vial_keyboard_id.into().trim().to_owned();
        let serial = serial?.trim().to_owned();
        if vial_keyboard_id.is_empty() || serial.is_empty() {
            return None;
        }
        Some(Self {
            vendor_id,
            product_id,
            vial_keyboard_id,
            serial,
        })
    }

    fn storage_key(&self) -> String {
        format!(
            "{:04x}-{:04x}-{}-{}",
            self.vendor_id,
            self.product_id,
            hex_component(&self.vial_keyboard_id),
            hex_component(&self.serial)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RecoveryFingerprint {
    pub(crate) firmware_version: Option<String>,
    pub(crate) schema_hash: Option<String>,
}

impl RecoveryFingerprint {
    pub(crate) fn new(
        firmware_version: Option<impl Into<String>>,
        schema_hash: Option<impl Into<String>>,
    ) -> Option<Self> {
        let firmware_version = normalize(firmware_version.map(Into::into));
        let schema_hash = normalize(schema_hash.map(Into::into));
        if firmware_version.is_none() && schema_hash.is_none() {
            return None;
        }
        Some(Self {
            firmware_version,
            schema_hash,
        })
    }

    fn is_valid(&self) -> bool {
        self.firmware_version
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || self
                .schema_hash
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

fn normalize(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrustSource {
    VerifiedWrite,
    Restore,
    Import,
    KeepCurrent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TrustedSnapshot {
    pub(crate) fingerprint: RecoveryFingerprint,
    pub(crate) trusted_at: u64,
    pub(crate) source: TrustSource,
    pub(crate) fields: BTreeMap<PortableSettingId, PortableSetting>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryFieldOutcome {
    Restored(PortableSettingId),
    Skipped {
        id: PortableSettingId,
        reason: String,
    },
    Failed {
        id: PortableSettingId,
        reason: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecoveryReport {
    pub(crate) outcomes: Vec<RecoveryFieldOutcome>,
}

impl RecoveryReport {
    pub(crate) fn restored_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RecoveryFieldOutcome::Restored(_)))
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryHistory {
    identity: RecoveryIdentity,
    snapshots: Vec<TrustedSnapshot>,
}

impl RecoveryHistory {
    pub(crate) fn new(identity: RecoveryIdentity) -> Self {
        Self {
            identity,
            snapshots: Vec::new(),
        }
    }

    pub(crate) fn identity(&self) -> &RecoveryIdentity {
        &self.identity
    }

    pub(crate) fn snapshots(&self) -> &[TrustedSnapshot] {
        &self.snapshots
    }

    pub(crate) fn apply_verified(
        &mut self,
        fingerprint: RecoveryFingerprint,
        trusted_at: u64,
        source: TrustSource,
        fields: impl IntoIterator<Item = PortableSetting>,
    ) -> bool {
        let fields: BTreeMap<_, _> = fields
            .into_iter()
            .map(|setting| (setting.id().clone(), setting))
            .collect();
        if fields.is_empty() {
            return false;
        }

        if let Some(index) = self
            .snapshots
            .iter()
            .position(|snapshot| snapshot.fingerprint == fingerprint)
        {
            let mut snapshot = self.snapshots.remove(index);
            match source {
                TrustSource::Restore | TrustSource::KeepCurrent => {
                    snapshot.fields = fields;
                }
                TrustSource::VerifiedWrite | TrustSource::Import => {
                    snapshot.fields.extend(fields);
                }
            }
            snapshot.trusted_at = trusted_at;
            snapshot.source = source;
            self.snapshots.insert(0, snapshot);
            return true;
        }

        self.snapshots.insert(
            0,
            TrustedSnapshot {
                fingerprint,
                trusted_at,
                source,
                fields,
            },
        );
        self.snapshots.truncate(MAX_SNAPSHOTS);
        true
    }
}

#[derive(Deserialize, Serialize)]
struct RecoveryDocument {
    version: u32,
    identity: RecoveryIdentity,
    snapshots: Vec<SnapshotDocument>,
}

#[derive(Deserialize, Serialize)]
struct SnapshotDocument {
    fingerprint: RecoveryFingerprint,
    trusted_at: u64,
    source: TrustSource,
    fields: Vec<PortableSetting>,
}

impl From<&RecoveryHistory> for RecoveryDocument {
    fn from(history: &RecoveryHistory) -> Self {
        Self {
            version: DOCUMENT_VERSION,
            identity: history.identity.clone(),
            snapshots: history
                .snapshots
                .iter()
                .map(|snapshot| SnapshotDocument {
                    fingerprint: snapshot.fingerprint.clone(),
                    trusted_at: snapshot.trusted_at,
                    source: snapshot.source,
                    fields: snapshot.fields.values().cloned().collect(),
                })
                .collect(),
        }
    }
}

pub(crate) struct RecoveryStore {
    root: PathBuf,
}

impl RecoveryStore {
    pub(crate) fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_owned(),
        }
    }

    pub(crate) fn path_for(&self, identity: &RecoveryIdentity) -> PathBuf {
        self.root.join(format!("{}.json", identity.storage_key()))
    }

    pub(crate) fn temp_path_for(&self, identity: &RecoveryIdentity) -> PathBuf {
        self.root
            .join(format!("{}.json.tmp", identity.storage_key()))
    }

    pub(crate) fn load(&self, identity: &RecoveryIdentity) -> io::Result<RecoveryHistory> {
        fs::create_dir_all(&self.root)?;
        remove_if_present(&self.temp_path_for(identity))?;
        let path = self.path_for(identity);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(RecoveryHistory::new(identity.clone()));
            }
            Err(error) => return Err(error),
        };

        let document: RecoveryDocument = match serde_json::from_slice(&bytes) {
            Ok(document) => document,
            Err(_) => {
                self.quarantine(&path)?;
                return Ok(RecoveryHistory::new(identity.clone()));
            }
        };
        let invalid_snapshot = document.snapshots.iter().any(|snapshot| {
            !snapshot.fingerprint.is_valid()
                || snapshot.fields.is_empty()
                || snapshot.fields.iter().any(|setting| !setting.is_valid())
        });
        if document.version != DOCUMENT_VERSION
            || document.identity != *identity
            || invalid_snapshot
        {
            self.quarantine(&path)?;
            return Ok(RecoveryHistory::new(identity.clone()));
        }

        Ok(RecoveryHistory {
            identity: document.identity,
            snapshots: document
                .snapshots
                .into_iter()
                .take(MAX_SNAPSHOTS)
                .map(|snapshot| TrustedSnapshot {
                    fingerprint: snapshot.fingerprint,
                    trusted_at: snapshot.trusted_at,
                    source: snapshot.source,
                    fields: snapshot
                        .fields
                        .into_iter()
                        .map(|setting| (setting.id().clone(), setting))
                        .collect(),
                })
                .collect(),
        })
    }

    pub(crate) fn save(&self, history: &RecoveryHistory) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let destination = self.path_for(history.identity());
        let temporary = self.temp_path_for(history.identity());
        remove_if_present(&temporary)?;

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, &RecoveryDocument::from(history))
            .map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);

        atomic_replace(&temporary, &destination)?;
        sync_parent(&destination)?;
        Ok(())
    }

    fn quarantine(&self, source: &Path) -> io::Result<PathBuf> {
        for suffix in 0.. {
            let marker = if suffix == 0 {
                "corrupt".to_owned()
            } else {
                format!("corrupt-{suffix}")
            };
            let destination = source.with_extension(format!("json.{marker}"));
            if !destination.exists() {
                fs::rename(source, &destination)?;
                return Ok(destination);
            }
        }
        unreachable!()
    }
}

fn hex_component(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::portable_settings::{
        known_qmk_setting, PortableSetting, PortableSettingId, PortableValue,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn identity() -> RecoveryIdentity {
        RecoveryIdentity::new(0xfeed, 0x6060, "vial-id", Some("SERIAL-1"))
            .expect("serial-backed identity")
    }

    fn fingerprint(value: &str) -> RecoveryFingerprint {
        RecoveryFingerprint::new(Some(value), Some("schema-a")).unwrap()
    }

    fn setting(qsid: u16, value: u64) -> PortableSetting {
        PortableSetting::new(
            known_qmk_setting(qsid).unwrap(),
            PortableValue::Unsigned(value),
        )
        .unwrap()
    }

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "entropy-recovery-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn recovery_identity_requires_nonempty_serial() {
        assert!(RecoveryIdentity::new(1, 2, "keyboard", None).is_none());
        assert!(RecoveryIdentity::new(1, 2, "keyboard", Some("  ")).is_none());
        assert!(RecoveryIdentity::new(1, 2, "keyboard", Some("serial")).is_some());
    }

    #[test]
    fn same_fingerprint_merges_fields_and_new_fingerprint_keeps_only_accepted_fields() {
        let mut history = RecoveryHistory::new(identity());
        history.apply_verified(
            fingerprint("fw-a"),
            10,
            TrustSource::VerifiedWrite,
            [setting(25, 200)],
        );
        history.apply_verified(
            fingerprint("fw-a"),
            11,
            TrustSource::VerifiedWrite,
            [setting(7, 1)],
        );
        assert_eq!(history.snapshots().len(), 1);
        assert_eq!(history.snapshots()[0].fields.len(), 2);

        history.apply_verified(
            fingerprint("fw-b"),
            12,
            TrustSource::Restore,
            [setting(25, 200)],
        );
        assert_eq!(history.snapshots().len(), 2);
        assert_eq!(history.snapshots()[0].fields.len(), 1);
        assert!(history.snapshots()[0]
            .fields
            .contains_key(&PortableSettingId::qmk(
                crate::app::portable_settings::PortableCategory::TapHold,
                "quick_tap_term",
                25,
                [],
            )));
        assert_eq!(history.snapshots()[1].fields.len(), 2);
    }

    #[test]
    fn full_capture_replaces_stale_fields_on_same_fingerprint() {
        let mut history = RecoveryHistory::new(identity());
        history.apply_verified(
            fingerprint("fw-a"),
            10,
            TrustSource::VerifiedWrite,
            [setting(25, 200), setting(7, 1)],
        );

        history.apply_verified(
            fingerprint("fw-a"),
            11,
            TrustSource::KeepCurrent,
            [setting(25, 175)],
        );

        assert_eq!(history.snapshots().len(), 1);
        assert_eq!(history.snapshots()[0].fields.len(), 1);
        assert_eq!(
            history.snapshots()[0]
                .fields
                .get(&PortableSettingId::qmk(
                    crate::app::portable_settings::PortableCategory::TapHold,
                    "quick_tap_term",
                    25,
                    [],
                ))
                .expect("retained setting")
                .value,
            PortableValue::Unsigned(175)
        );
    }

    #[test]
    fn full_capture_replaces_matching_non_newest_snapshot() {
        let mut history = RecoveryHistory::new(identity());
        history.apply_verified(
            fingerprint("fw-a"),
            10,
            TrustSource::VerifiedWrite,
            [setting(25, 200), setting(7, 1)],
        );
        history.apply_verified(
            fingerprint("fw-b"),
            11,
            TrustSource::KeepCurrent,
            [setting(25, 150)],
        );

        history.apply_verified(
            fingerprint("fw-a"),
            12,
            TrustSource::Restore,
            [setting(25, 175)],
        );

        assert_eq!(history.snapshots()[0].fingerprint, fingerprint("fw-a"));
        assert_eq!(history.snapshots()[0].fields.len(), 1);
        assert_eq!(history.snapshots().len(), 2);
    }

    #[test]
    fn history_rotates_newest_first_at_three_snapshots() {
        let mut history = RecoveryHistory::new(identity());
        for index in 0..4 {
            history.apply_verified(
                fingerprint(&format!("fw-{index}")),
                index,
                TrustSource::KeepCurrent,
                [setting(25, index)],
            );
        }
        assert_eq!(history.snapshots().len(), 3);
        assert_eq!(history.snapshots()[0].fingerprint, fingerprint("fw-3"));
        assert_eq!(history.snapshots()[2].fingerprint, fingerprint("fw-1"));
    }

    #[test]
    fn store_overwrites_atomically_and_round_trips_versioned_json() {
        let root = temp_root("overwrite");
        let store = RecoveryStore::new(&root);
        let mut history = RecoveryHistory::new(identity());
        history.apply_verified(
            fingerprint("fw-a"),
            1,
            TrustSource::KeepCurrent,
            [setting(25, 100)],
        );
        store.save(&history).unwrap();
        history.apply_verified(
            fingerprint("fw-a"),
            2,
            TrustSource::VerifiedWrite,
            [setting(25, 250)],
        );
        store.save(&history).unwrap();

        let loaded = store.load(&identity()).unwrap();
        assert_eq!(loaded, history);
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path_for(&identity())).unwrap()).unwrap();
        assert_eq!(json["version"], 1);
        assert!(!store.temp_path_for(&identity()).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_history_is_quarantined_and_stale_temp_is_removed() {
        let root = temp_root("corrupt");
        let store = RecoveryStore::new(&root);
        fs::write(store.path_for(&identity()), b"not json").unwrap();
        fs::write(store.temp_path_for(&identity()), b"stale").unwrap();

        let loaded = store.load(&identity()).unwrap();
        assert!(loaded.snapshots().is_empty());
        assert!(!store.temp_path_for(&identity()).exists());
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt"))
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn semantically_invalid_history_is_quarantined() {
        let root = temp_root("invalid");
        let store = RecoveryStore::new(&root);
        let mut history = RecoveryHistory::new(identity());
        history.apply_verified(
            fingerprint("fw-a"),
            1,
            TrustSource::KeepCurrent,
            [setting(25, 100)],
        );
        let mut document = serde_json::to_value(RecoveryDocument::from(&history)).unwrap();
        document["snapshots"][0]["fields"][0]["value"] =
            serde_json::json!({"kind": "unsigned", "value": 1001});
        fs::write(
            store.path_for(&identity()),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();

        let loaded = store.load(&identity()).unwrap();
        assert!(loaded.snapshots().is_empty());
        assert!(!store.path_for(&identity()).exists());
        fs::remove_dir_all(root).unwrap();
    }
}
