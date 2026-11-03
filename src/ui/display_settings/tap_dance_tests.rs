//! Characterization of the reported Linux Tap Dance/t1 Choose path.
//! Actual App::ui, pointer clicks and HID workers; only HID transport is scripted.
//! Passing these does not reproduce or certify the physical AppImage/macOS GUI.
use super::*;
use crate::app::vial_hid_task::{VialHidOperation, VialHidTaskStart};

#[path = "batch_preflight_tests.rs"]
mod batch_preflight_tests;

fn tap_dance_app() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
    let (mut app, ctx, recorder) = app(true);
    let mut device = app.device_manager.devices()[0].clone();
    device.name = "Macropad".into();
    device.vendor_id = 0xE126;
    device.product_id = 0x0042;
    assert!(device.is_ergohaven_display_macropad());
    app.device_manager.replace_devices(vec![device]);
    app.keycode_picker.tap_dance_entries = vec![crate::keycode_picker::TapDanceEntry::default()];
    app.keycode_picker.tap_dance_names = vec!["t1".into()];
    app.display_settings.pictograms.selected_kind = PictogramKind::TapDance;
    app.restore_pictogram_editor_from_device();
    click_label(&mut app, &ctx, "Пиктограммы");
    let output = frame(&mut app, &ctx, vec![]);
    assert!(rendered_texts(&output).iter().any(|text| text == "t1"));
    assert!(app.display_settings.pictograms.source_levels.is_empty());
    (app, ctx, recorder)
}

#[test]
fn tap_dance_t1_blank_editor_choose_opens_and_recovers_after_real_slot_save() {
    let (mut app, ctx, recorder) = tap_dance_app();
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
    egui::Popup::close_all(&ctx);
    recorder.respond_with(test_pictogram_upload_responses(true, 0));
    let bitmap = [0xA5; PICTOGRAM_BYTES];
    assert!(app.assign_selected_pictogram(&ctx, Some(&bitmap)));
    assert!(app.display_settings.pictograms.saving);
    click_label(&mut app, &ctx, "Выбрать");
    assert!(
        !egui::Popup::is_any_open(&ctx),
        "unsafe write must block Choose"
    );
    finish(&mut app, &ctx);
    assert!(!app.display_settings.pictograms.busy());
    assert_eq!(
        app.display_settings
            .pictograms
            .library
            .bitmap(PictogramKind::TapDance, 0),
        Some(bitmap.as_slice())
    );
    assert!(!app
        .display_settings
        .pictograms
        .library
        .has(PictogramKind::Macro, 0));
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
}

#[test]
fn tap_dance_choose_known_support_read_error_allows_explicit_retry() {
    for error in ["status 4"] {
        let (mut app, ctx, recorder) = tap_dance_app();
        let mut read_error = [0; 32];
        read_error[0] = 0xC0;
        read_error[1] = 4;
        recorder.respond_with([read_error]);
        app.display_settings.pictograms.loaded = false;
        app.start_pictogram_load(&ctx);
        assert!(app.display_settings.pictograms.loading);
        click_label(&mut app, &ctx, "Выбрать");
        assert!(!egui::Popup::is_any_open(&ctx));
        finish(&mut app, &ctx);
        assert!(!app.vial_hid_task_blocks_user_action());
        assert!(!app.display_settings.pictograms.busy());
        assert_eq!(app.display_settings.pictograms.supported, Some(true));
        assert!(!app.display_settings.pictograms.loaded);
        assert!(app
            .display_settings
            .pictograms
            .load_failure
            .as_ref()
            .unwrap()
            .1
            .contains(error));
        let requests = recorder.requests().len();
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(
            recorder.requests().len(),
            requests,
            "failed reads must not loop per frame"
        );
        recorder.respond_by(|_| None);
        recorder.respond_with(test_pictogram_read_responses(&PictogramLibrary::default()));
        click_label(&mut app, &ctx, "Выбрать");
        assert!(
            app.vial_hid_task_active(),
            "Choose must initiate explicit read retry"
        );
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "unconfirmed snapshot cannot offer assignments"
        );
        finish(&mut app, &ctx);
        assert!(app.display_settings.pictograms.loaded);
        click_label(&mut app, &ctx, "Выбрать");
        assert!(egui::Popup::is_any_open(&ctx));
    }
}

#[test]
fn tap_dance_choose_initial_transient_query_failure_allows_safe_explicit_retry() {
    let (mut app, ctx, recorder) = tap_dance_app();
    app.display_settings.pictograms.supported = None;
    app.display_settings.pictograms.loaded = false;
    let mut read_error = [0; 32];
    read_error[0] = 0xC0;
    read_error[1] = 4;
    recorder.respond_with([read_error]);
    app.start_pictogram_load(&ctx);
    finish(&mut app, &ctx);
    assert_eq!(
        app.display_settings.pictograms.supported, None,
        "a failed first QUERY cannot prove unsupported firmware"
    );
    assert!(!app.display_settings.pictograms.loaded);
    assert!(!app.display_settings.pictograms.busy());
    assert!(!app.vial_hid_task_blocks_user_action());
    assert!(app
        .display_settings
        .pictograms
        .load_failure
        .as_ref()
        .unwrap()
        .1
        .contains("status 4"));
    let requests = recorder.requests().len();
    for _ in 0..3 {
        frame(&mut app, &ctx, vec![]);
    }
    assert_eq!(
        recorder.requests().len(),
        requests,
        "no automatic retry loop"
    );
    assert!(!app.assign_selected_pictogram(&ctx, Some(&[0xA5; PICTOGRAM_BYTES])));
    assert_eq!(
        recorder.requests().len(),
        requests,
        "unconfirmed storage must not write"
    );
    recorder.respond_by(|_| None);
    recorder.respond_with(test_pictogram_read_responses(&PictogramLibrary::default()));
    click_label(&mut app, &ctx, "Выбрать");
    assert!(app.vial_hid_task_active(), "Choose should retry the QUERY");
    assert!(
        !egui::Popup::is_any_open(&ctx),
        "no assignments before confirmed read"
    );
    finish(&mut app, &ctx);
    assert_eq!(app.display_settings.pictograms.supported, Some(true));
    assert!(app.display_settings.pictograms.loaded);
    assert!(app.display_settings.pictograms.load_failure.is_none());
    assert!(
        recorder.requests()[requests..]
            .iter()
            .all(|request| matches!(request[0], 0xC0 | 0xC1 | 0xCA | 0xCB)),
        "retry must only read"
    );
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
    assert!(rendered_texts(&frame(&mut app, &ctx, vec![]))
        .iter()
        .any(|text| text == "t1"));
}

#[test]
fn tap_dance_choose_explicitly_unsupported_query_stays_disabled() {
    for response in [[0; 32], {
        let mut incompatible = [0; 32];
        incompatible[0] = 0xC0;
        incompatible[2] = 0xFF;
        incompatible
    }] {
        let (mut app, ctx, recorder) = tap_dance_app();
        app.display_settings.pictograms.supported = None;
        app.display_settings.pictograms.loaded = false;
        recorder.respond_with([response]);
        app.start_pictogram_load(&ctx);
        finish(&mut app, &ctx);
        assert_eq!(app.display_settings.pictograms.supported, Some(false));
        assert!(!app.display_settings.pictograms.loaded);
        let requests = recorder.requests().len();
        click_label(&mut app, &ctx, "Выбрать");
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(!app.vial_hid_task_active());
        assert!(!app.assign_selected_pictogram(&ctx, Some(&[0xA5; PICTOGRAM_BYTES])));
        assert_eq!(recorder.requests().len(), requests);
    }
}

#[test]
fn tap_dance_timeout_clears_connection_instead_of_retaining_a_busy_choose() {
    let (mut app, ctx, _) = tap_dance_app();
    let (hid, _) = crate::hid::HidDevice::test_device_with_fault_after_requests(Some((
        0,
        crate::hid::TestHidFault::Timeout,
    )));
    app.hid_device = Some(hid.into());
    app.start_pictogram_load(&ctx);
    finish(&mut app, &ctx);
    assert!(!app.vial_hid_task_blocks_user_action());
    assert!(!app.display_settings.pictograms.busy());
    assert!(app.hid_device.is_none());
    assert!(app.layout.is_none());
}

#[test]
fn tap_dance_stale_deferred_result_cannot_overwrite_successor_or_leave_choose_busy() {
    let (mut app, ctx, recorder) = tap_dance_app();
    let context = std::sync::Arc::new(deferred_context());
    recorder.respond_by(|_| Some(Err("obsolete Tap Dance read rejected".into())));
    assert_eq!(
        app.start_vial_hid_operation(
            &ctx,
            VialHidOperation::Deferred(
                crate::app::device_deferred_load::DeferredLoadRequest::Section {
                    section: DeferredLoadSection::TapDance,
                    context,
                }
            )
        ),
        VialHidTaskStart::Started
    );
    app.connection_generation = app.connection_generation.wrapping_add(1);
    app.deferred_device_load = DeferredDeviceLoadState::staged(deferred_context());
    app.deferred_device_load
        .set_section_status(DeferredLoadSection::TapDance, DeferredLoadStatus::Loaded);
    let (successor_hid, _) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(successor_hid.into());
    app.display_settings.pictograms.editor_name = "Successor draft".into();
    finish(&mut app, &ctx);
    assert_eq!(
        app.deferred_device_load
            .section_status(DeferredLoadSection::TapDance),
        DeferredLoadStatus::Loaded
    );
    assert_eq!(
        app.display_settings.pictograms.editor_name,
        "Successor draft"
    );
    assert_eq!(app.display_settings.pictograms.supported, Some(true));
    assert!(app.hid_device.is_some());
    assert!(!app.vial_hid_task_blocks_user_action());
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
}

fn deferred_context() -> DeferredDeviceLoadContext {
    DeferredDeviceLoadContext {
        json: std::sync::Arc::new(serde_json::json!({})),
        supported_qmk_settings: std::sync::Arc::new(Vec::new()),
        definition_fingerprint: 7,
        layer_count: 1,
        rows: 1,
        cols: 1,
        encoder_count: 0,
        macro_count: 4,
        macro_memory_bytes: Some(64),
        tap_dance_count: 1,
        combo_count: 0,
        key_override_count: 0,
        alt_repeat_count: 0,
        modules_supported: false,
        touchpad_supported: false,
        bluetooth_supported: false,
        layer_leds_supported: false,
        rgb_supported: false,
        lighting_mode: None,
        supports_rmk_native_key_actions: false,
        supports_universal_numero: false,
        supports_universal_symbols: false,
        supports_universal_russian_letters: false,
        supports_rmk_native_combo_output: false,
        supports_rmk_native_tap_dance_actions: false,
        supports_rmk_combo_layers: false,
    }
}

#[test]
fn tap_dance_choose_deferred_read_failure_releases_hid_busy_without_clearing_slot() {
    let (mut app, ctx, recorder) = tap_dance_app();
    app.deferred_device_load = DeferredDeviceLoadState::staged(deferred_context());
    app.deferred_device_load
        .set_section_status(DeferredLoadSection::TapDance, DeferredLoadStatus::Loading);
    recorder.respond_by(|_| Some(Err("Tap Dance read rejected".into())));
    assert_eq!(
        app.start_vial_hid_operation(
            &ctx,
            VialHidOperation::Deferred(
                crate::app::device_deferred_load::DeferredLoadRequest::Section {
                    section: DeferredLoadSection::TapDance,
                    context: app.deferred_device_load.context.as_ref().unwrap().clone(),
                }
            )
        ),
        VialHidTaskStart::Started
    );
    assert!(app.vial_hid_task_blocks_user_action());
    click_label(&mut app, &ctx, "Выбрать");
    assert!(!egui::Popup::is_any_open(&ctx));
    finish(&mut app, &ctx);
    assert!(matches!(
        app.deferred_device_load
            .section_status(DeferredLoadSection::TapDance),
        DeferredLoadStatus::Failed(_)
    ));
    assert_eq!(app.keycode_picker.tap_dance_entries.len(), 1);
    assert!(!app.vial_hid_task_blocks_user_action());
    click_label(&mut app, &ctx, "Выбрать");
    assert!(egui::Popup::is_any_open(&ctx));
}
