// Focused filesystem/migration boundary; records are a retained production-C FULL.
fn archive_record() -> crate::qmk_hid_host::RecoveredRecord {
    let bridge = crate::qmk_hid_host::QmkHidHostBridge::test_inert(
        crate::device::test_usb_device("archive", 1, 0x00b2), Default::default(), None,
        crate::qmk_hid_host::HostProtocol::Selected(false));
    bridge.test_set_device_inventory_for_host_id("host-draft");
    bridge.resident_recoveries().remove(0)
}
fn archive_settings() -> AppSettings {
    let mut settings = AppSettings::default();
    let mut library = crate::application_layouts::DeviceApplicationLayouts::default();
    library.layouts.get_mut("default").unwrap().layers = vec![vec![4; 62]; 16];
    let mut draft = library.layouts["default"].clone();
    draft.id = "host-draft".into(); draft.name = "Keep this draft".into();
    draft.executable = "editor".into(); draft.layers[15][61] = 6;
    draft.user_edit_revision = 10;
    library.layouts.insert(draft.id.clone(), draft);
    library.normalize(); settings.application_layouts.insert("sm62".into(), library);
    settings
}
#[test]
fn host_managed_migration_durable_complete_backup_before_adoption_and_per_half_receipts() {
    let _storage = ScopedTempConfig::new();
    let mut settings = archive_settings();
    try_save_app_settings(&settings).unwrap();
    let previous_disk = std::fs::read(app_settings_path()).unwrap();
    let previous = settings.clone(); let record = archive_record();
    let mut device = crate::device::test_usb_device("left-usb", 1, 0x00b2);
    device.serial_number = "left-nor".into();
    let backup = migrate_legacy_archive(&mut settings, "sm62", &device, &[record.clone()], None).unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&backup).unwrap()).unwrap();
    assert_eq!(value["records"][0]["raw"], serde_json::to_value(&record.raw).unwrap());
    assert_eq!(value["records"][0]["identity_crc"], serde_json::to_value(record.identity_crc.to_vec()).unwrap());
    assert_eq!(value["previous_host_settings"], serde_json::to_value(&previous).unwrap());
    assert_eq!(value["previous_settings_file_bytes"], serde_json::to_value(previous_disk).unwrap());
    let library = &settings.application_layouts["sm62"];
    assert_eq!(library.layouts["host-draft"], previous.application_layouts["sm62"].layouts["host-draft"]);
    let (id, raw) = library.device_raw_profiles.iter().next().unwrap();
    assert_eq!(raw, &record.raw); assert_ne!(id, "host-draft");
    assert_eq!(library.snapshot_for_layout(&library.layouts[id]).device_raw.as_ref(), Some(&record.raw));
    let reloaded: AppSettings = serde_json::from_slice(&std::fs::read(app_settings_path()).unwrap()).unwrap();
    assert_eq!(reloaded.application_layouts, settings.application_layouts);
    assert!(migrate_legacy_archive(&mut settings, "sm62", &device, &[record.clone()], None).unwrap().is_none());
    device.path = "right-usb".into(); device.serial_number = "right-nor".into();
    assert!(migrate_legacy_archive(&mut settings, "sm62", &device, &[record], None).unwrap().is_some());
    assert_eq!(settings.application_layouts["sm62"].host_archive_migrations.len(), 2);
}
#[test]
fn host_managed_migration_failed_backup_or_settings_save_never_confirms_or_changes_library() {
    for fail_backup in [true, false] {
        let _storage = ScopedTempConfig::new(); let mut settings = archive_settings();
        let before = serde_json::to_value(&settings).unwrap();
        let device = crate::device::test_usb_device("failing-archive", 1, 0x00b2);
        if fail_backup { std::fs::write(app_settings_path().with_file_name("resident100-recovery"), b"not a directory").unwrap(); }
        else { std::fs::write(app_settings_path(), b"invalid settings must not be overwritten").unwrap(); }
        assert!(migrate_legacy_archive(&mut settings, "sm62", &device, &[archive_record()], None).is_err());
        assert_eq!(serde_json::to_value(&settings).unwrap(), before);
        assert!(settings.application_layouts["sm62"].host_archive_migrations.is_empty());
    }
}
#[test]
fn host_managed_migration_pending_delete_and_partial_records_remain_preserved() {
    let _storage = ScopedTempConfig::new(); let mut settings = archive_settings();
    let library = settings.application_layouts.get_mut("sm62").unwrap();
    let draft = library.layouts.remove("host-draft").unwrap();
    library.resident_deleted.insert("host-draft".into(), (7, draft));
    let before = library.resident_deleted.clone();
    let device = crate::device::test_usb_device("deleted-archive", 1, 0x00b2);
    let mut r = archive_record(); r.validation = Some("partial".into());
    assert!(migrate_legacy_archive(&mut settings, "sm62", &device, &[r], None).is_err());
    assert!(settings.application_layouts["sm62"].host_archive_migrations.is_empty());
    migrate_legacy_archive(&mut settings, "sm62", &device, &[archive_record()], None).unwrap();
    let library = &settings.application_layouts["sm62"];
    assert_eq!(library.resident_deleted, before);
    assert_eq!(library.layouts.len(), 1);
}
