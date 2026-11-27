#[test]
fn phenom_timeout20_slider_actual_descriptor_set_reload_and_old_firmware_gate() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::{NodeT, Queryable};
    for extended in [false, true] {
        let mut app = phenom_auto_app();
        let json: serde_json::Value = serde_json::from_str(if extended {
            include_str!("../../tests/fixtures/phenom-approved-timeout20-normal.json")
        } else {
            include_str!("../../tests/fixtures/phenom-qmk-settings-a39f2668.json")
        })
        .unwrap();
        app.module_settings =
            EntropyApp::module_settings_from_definition(&json, &(120..400).collect::<Vec<_>>());
        let group_idx = app
            .module_settings
            .groups
            .iter()
            .position(|g| g.kind == ModuleSettingsGroupKind::AutoLayer)
            .unwrap();
        let field_idx = app.module_settings.groups[group_idx]
            .fields
            .iter()
            .position(|f| f.qsid == 324)
            .unwrap();
        let field = &app.module_settings.groups[group_idx].fields[field_idx];
        let variants = field.variants.clone();
        assert_eq!(variants.len(), if extended { 20 } else { 14 });
        assert_eq!(
            variants.last().unwrap(),
            if extended { "3000 ms" } else { "1500 ms" }
        );
        assert!(app.phenom_auto_timeout_field(ModuleSettingsGroupKind::AutoLayer, field));
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        app.hid_device = Some(hid.into());
        app.module_settings
            .set_value(324, (variants.len() - 1) as u16);
        let mut h = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .build_ui_state(
                move |ui, app: &mut EntropyApp| {
                    app.draw_module_settings_field_row(
                        ui, group_idx, field_idx, 700.0, 48.0, false,
                    );
                },
                app,
            );
        h.run();
        assert_eq!(
            h.get_by_role(Role::Slider)
                .accesskit_node()
                .max_numeric_value(),
            Some((variants.len() - 1) as f64)
        );
        for index in 0..variants.len() {
            let start = recorder.requests().len();
            h.get_by_role(Role::Slider)
                .set_value_accesskit(index as f64);
            h.run_steps(1);
            drain_hid_writes(h.state_mut(), &egui::Context::default());
            let requests = recorder.requests();
            assert_eq!(requests.len(), start + 1);
            assert_eq!(requests[start][2..5], [68, 1, index as u8]);
            let read = EntropyApp::read_module_settings(
                &json,
                &[324],
                h.state()
                    .hid_device
                    .as_ref()
                    .unwrap()
                    .require_vial()
                    .unwrap(),
            );
            assert_eq!(read.value(324), index as u16);
        }
        let rect = h.get_by_role(Role::Slider).rect();
        for (pos, index) in [
            (rect.left_center(), 0),
            (rect.right_center(), variants.len() - 1),
        ] {
            h.drag_at(pos);
            h.run_steps(1);
            h.drop_at(pos);
            h.run_steps(1);
            drain_hid_writes(h.state_mut(), &egui::Context::default());
            assert_eq!(h.state().module_settings.value(324), index as u16);
        }
    }
}
