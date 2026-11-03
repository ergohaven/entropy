use super::tests::module_app;
use super::*;
use crate::app::{AltRepeatKeyEntry, AltRepeatKeyOptionsState, ComboEntry, KeyOverrideEntry};
use crate::keyboard::KeyBinding;
use crate::keycode_picker::TapDanceEntry;
use rmk_types::action::{Action, KeyAction};

/// A K:04 with every RMK-native capability, two layers and two slots of
/// each dynamic entry, all empty.
fn rmk_k04() -> EntropyApp {
    let mut app = module_app();
    app.layer_count = 2;
    let layout = app.layout.as_mut().unwrap();
    layout.layers = vec![vec![0u16.into(); layout.keys.len()]; 2];
    layout.encoder_layers = vec![vec![0; layout.encoders.len()]; 2];
    app.layer_names = vec![String::new(); 2];
    app.supported_qmk_settings.extend([2, 200, 201]);
    app.keycode_picker.supports_rmk_native_key_actions = true;
    app.keycode_picker.supports_rmk_native_combo_output = true;
    app.keycode_picker.supports_rmk_native_tap_dance_actions = true;
    app.supports_rmk_combo_layers = true;
    app.keycode_picker.macro_count = 2;
    app.keycode_picker.macro_texts = vec![Vec::new(); 2];
    app.keycode_picker.tap_dance_entries = vec![TapDanceEntry::default(); 2];
    app.combo_entries = vec![ComboEntry::default(); 2];
    app.combo_term = Some(50);
    app.key_override_entries = vec![KeyOverrideEntry::default(); 2];
    app.alt_repeat_entries = vec![AltRepeatKeyEntry::default(); 2];
    app.layout_options_value = Some(0);
    app
}

#[test]
fn import_into_rmk_keyboard_sends_the_same_vial_requests() {
    let symbol = KeyBinding::Rmk(KeyAction::Single(Action::User(0x90)));
    let mut source = rmk_k04();
    let layout = source.layout.as_mut().unwrap();
    layout.layers[0][0] = 0x04.into();
    layout.layers[0][1] = symbol;
    layout.layers[1][2] = 0x1e.into();
    if let Some(encoder) = layout.encoder_layers[0].first_mut() {
        *encoder = 0x80;
    }
    source.layer_names = vec!["Base".into(), "Fn".into()];
    source.keycode_picker.macro_texts = vec![b"ab".to_vec(), Vec::new()];
    source.combo_entries[0] = ComboEntry {
        keys: [0x04, 0x05, 0, 0],
        output: 0x06.into(),
        layer: None,
    };
    source.combo_entries[1] = ComboEntry {
        keys: [0x07, 0x08, 0, 0],
        output: symbol,
        layer: Some(1),
    };
    source.combo_term = Some(80);
    source.keycode_picker.tap_dance_entries[0] = TapDanceEntry {
        on_tap: 0x04.into(),
        on_hold: symbol,
        on_double_tap: 0x05.into(),
        on_tap_hold: 0.into(),
        tapping_term: 200,
    };
    source.key_override_entries[0] = KeyOverrideEntry {
        trigger: 0x04,
        replacement: 0x05,
        layers: 0xffff,
        trigger_mods: 0x02,
        negative_mod_mask: 0,
        suppressed_mods: 0x02,
        options: crate::app::KeyOverrideOptionsState::from_bits(0x80),
    };
    source.alt_repeat_entries[0] = AltRepeatKeyEntry {
        keycode: 0x04,
        alt_keycode: 0x05,
        allowed_mods: 0,
        options: AltRepeatKeyOptionsState::from_bits(0x08),
    };
    source.layout_options_value = Some(1);
    source.module_settings.values.insert(140, 1);
    let bundle: EntLayoutFile = serde_json::from_str(
        &serde_json::to_string(&source.entlayout_snapshot().unwrap()).unwrap(),
    )
    .unwrap();

    let mut target = rmk_k04();
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    crate::golden_frames::fake_rmk_keyboard(&recorder);
    *hid.native_capabilities.lock().unwrap() = Some(crate::rmk_native::RmkNativeCapabilities {
        version: 1,
        key_actions: true,
        combo_output: true,
        tap_dance_actions: true,
        combo_layers: true,
        ..Default::default()
    });
    target.hid_device = Some(hid.into());
    let (failures, _, _) = target.apply_entlayout(&bundle).unwrap();
    assert!(failures.is_empty(), "{failures:?}");
    crate::golden_frames::assert_vial_frames("import_rmk_k04", &recorder.requests());
}
