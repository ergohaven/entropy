#[test]
fn phenom_slider_alignment_real_page_matches_text_reference() {
    use crate::i18n::Language::{Russian, English};
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    for lang in [Russian, English] { for width in [400.0, 600.0, 1400.0] {
        let mut app = phenom_auto_app();
        app.app_settings.language = lang;
        let (hid, recorder) = crate::hid::HidDevice::test_device(); app.hid_device = Some(hid.into());
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(width, 1800.0))
            .with_accessibility_check(false).allow_missing_glyphs().build_ui_state(
                |ui, app: &mut EntropyApp| app.draw_module_settings_page(ui, ui.max_rect()), app);
        for side in 0..=1 { for module in [1, 2] {
            h.state_mut().module_settings.set_selected_module_group(side);
            h.state_mut().module_settings.set_value(372 + side as u16, module);
            h.state_mut().module_settings.set_value(134 + side as u16, 3);
            h.run();
            let reference = h.get_all_by_role(Role::Slider).map(|n| n.rect()).min_by(|a,b| a.top().total_cmp(&b.top())).unwrap();
            let slower = crate::i18n::tr_catalog(lang, "modules_settings.speed_slower");
            let faster = crate::i18n::tr_catalog(lang, "modules_settings.speed_faster");
            let left = h.get_by_label(slower).rect(); let right = h.get_by_label(faster).rect();
            if width >= 600.0 {
                // Recorded from the frozen original production Text page, not
                // recalculated using the new shared layout helper.
                let old_start = if lang == Russian { 345.2 } else { 314.7 } + (width - 600.0) / 2.0;
                assert!((reference.left()-old_start).abs() < 0.1, "preserve original Text start: {reference:?} expected {old_start}");
                assert!((reference.right()-(old_start+110.0)).abs() < 0.1, "preserve original Text end");
            }
            assert!((left.center().y-reference.center().y).abs() < 0.1, "left baseline centered");
            assert!((right.center().y-reference.center().y).abs() < 0.1, "right baseline centered");
            println!("reference {lang:?} width={width} side={side} module={module}: slider={reference:?} left={left:?} right={right:?}");
            for mode in [1, 2, 3, 0] {
                h.state_mut().module_settings.set_value(134 + side as u16, mode); h.run();
                let before = h.state().module_settings.values.clone();
                for node in h.get_all_by_role(Role::Slider) {
                    let rect = node.rect();
                    assert!((rect.left()-reference.left()).abs() < 0.1 && (rect.right()-reference.right()).abs() < 0.1,
                        "{lang:?} width={width} side={side} module={module} mode={mode}: {rect:?} != Text {reference:?}");
                    assert!(rect.left() >= 0.0 && rect.right() <= width, "track inside viewport");
                }
                let timeout = h.state().module_settings.groups.iter().find(|g| g.kind == ModuleSettingsGroupKind::AutoLayer).unwrap();
                let timeout_field = timeout.fields.iter().find(|f| f.qsid == 324).unwrap();
                let shown = &timeout_field.variants[h.state().module_settings.value(324) as usize];
                let numeric = h.get_by_label(shown).rect();
                let timeout_track = h.get_all_by_role(Role::Slider).map(|n| n.rect()).max_by(|a,b| a.top().total_cmp(&b.top())).unwrap();
                assert!((numeric.left()-right.left()).abs() < 0.1, "numeric caption in right column");
                assert!((numeric.center().y-timeout_track.center().y).abs() < 0.1, "numeric baseline centered");
                assert!(numeric.right() <= width, "numeric caption inside viewport");
                if mode != 0 {
                    let key = match mode { 1 => "modules_settings.speed_more_precise", 2 => "modules_settings.speed_smoother", _ => "modules_settings.speed_slower" };
                    let l = h.get_by_label(crate::i18n::tr_catalog(lang, key)).rect();
                    let r = h.get_by_label(faster).rect();
                    let track = h.get_all_by_role(Role::Slider).map(|n| n.rect()).min_by(|a,b| a.top().total_cmp(&b.top())).unwrap();
                    assert!((l.right()-left.right()).abs() < 0.1, "left captions end at same edge: {l:?} {left:?}");
                    assert!((r.left()-right.left()).abs() < 0.1, "right captions start at same edge");
                    assert!(((l.center().y-track.center().y)-(left.center().y-reference.center().y)).abs() < 0.1);
                    assert!(((r.center().y-track.center().y)-(right.center().y-reference.center().y)).abs() < 0.1);
                    assert!(l.left() >= 0.0 && r.right() <= width, "captions inside viewport");
                }
                assert_eq!(h.state().module_settings.values, before);
                assert!(recorder.requests().is_empty(), "rendering never writes");
            }
        }}
    }}
}
