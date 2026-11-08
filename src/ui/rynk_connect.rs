//! Connecting to a Rynk board: the same [`ConnectResult`] the Vial connect
//! builds, read through the board's link.
//!
//! The board hands out the Vial definition, setting ids and values Vial
//! carries, so the layout and settings load through the Vial path's own code.
//! Keys, macros and dynamic entries are read as Rynk types and shown the way
//! the editors hold them. A USB board loads in full here; nothing is deferred.

use super::device_connect_task::{
    layer_name_sync_updates, normalize_reported_layer_count, use_device_name_for_unnamed_layout,
};
use super::*;
use crate::rynk::client::{fnv1a, RynkClientError};
use crate::rynk::dynamic::{DynamicLoss, DynamicSlot};
use crate::rynk::link::{PhysicalId, RynkLink};
use crate::rynk::session::RynkSession;
use crate::rynk::{dynamic, keycode};
use rmk_types_rynk::protocol::rynk::ergohaven::caps;

impl EntropyApp {
    pub(super) fn connect_rynk(
        dev: &Device,
        language: crate::i18n::Language,
        progress: &dyn Fn(&str) -> Result<(), String>,
    ) -> Result<ConnectResult, String> {
        let rynk_failed = |what: &str, error: RynkClientError| {
            format!("{what} failed: {}", error.describe(language))
        };
        progress("Opening Rynk link…")?;
        log::info!("Opening Rynk board {} at {}", dev.name, dev.path);
        let link = RynkLink::open(&PhysicalId(dev.path.clone())).map_err(|error| {
            log::warn!(
                "Rynk board {} at {} did not open: {error}",
                dev.name,
                dev.path
            );
            rynk_failed("Open", error)
        })?;
        let session = RynkSession::new(link);
        let info = session.info().clone();
        let capabilities = *session.capabilities();

        progress("Reading keyboard definition…")?;
        let definition = board_definition(&session, rynk_cache_dir().as_deref())
            .map_err(|error| rynk_failed("Definition read", error))?;
        let json = crate::hid::decode_vial_definition(&definition)
            .map_err(failed(language, "Definition decode"))?;

        progress("Querying settings…")?;
        let supported_qmk_settings = session
            .setting_keys()
            .map_err(failed(language, "Setting list read"))?;
        // A failure here only costs time: each read below then asks on its own
        // and reports what fails, as before.
        if let Err(error) = session.read_settings_ahead(&supported_qmk_settings) {
            log::warn!("Rynk settings read ahead failed: {error:#}");
        }
        let has_qmk_setting = |qsid: u16| supported_qmk_settings.contains(&qsid);

        progress("Parsing keyboard layout…")?;
        let mut layout = KeyboardLayout::from_vial_json(&json)
            .map_err(|error| format!("Layout parse failed: {error}"))?;
        layout.firmware = FirmwareProtocol::Rynk;
        // The definition names the host data the screen shows; the firmware
        // says which of it SetHostData takes. The volume it shows nowhere.
        // Universal Symbols follow the host layout whether a screen shows it
        // or not, as over Vial.
        let takes = |cap| info.caps & cap != 0;
        let live = &mut layout.live_features;
        live.extended_host_protocol = false;
        live.time &= takes(caps::HOST_TIME);
        live.volume = false;
        live.layout =
            (live.layout || takes(caps::UNIVERSAL_SYMBOLS)) && takes(caps::HOST_KEYBOARD_LAYOUT);
        live.media &= takes(caps::HOST_MEDIA);
        use_device_name_for_unnamed_layout(&mut layout, &dev.name);
        if (layout.rows, layout.cols)
            != (
                capabilities.num_rows as usize,
                capabilities.num_cols as usize,
            )
        {
            return Err(format!(
                "Definition matrix {}x{} differs from the board's {}x{}",
                layout.rows, layout.cols, capabilities.num_rows, capabilities.num_cols
            ));
        }
        let layer_count = normalize_reported_layer_count(capabilities.num_layers as usize);
        layout.layers =
            vec![vec![crate::keyboard::KeyBinding::default(); layout.keys.len()]; layer_count];

        progress("Reading keymap…")?;
        let keymap = session
            .keymap(layer_count as u8)
            .map_err(failed(language, "Keymap read"))?;
        for (layer, actions) in keymap.iter().enumerate() {
            for (index, key) in layout.keys.iter().enumerate() {
                let at = key.row as usize * layout.cols + key.col as usize;
                if let Some(&action) = actions.get(at) {
                    layout.layers[layer][index] = keycode::key_binding(action);
                }
            }
        }

        if layout.layer_names.len() < layer_count {
            let start = layout.layer_names.len();
            layout
                .layer_names
                .extend((start..layer_count).map(|layer| layer.to_string()));
        }
        layout.layer_names.truncate(layer_count);
        let mut current_firmware_layer_names = vec![None; layer_count];
        let mut layer_names_from_firmware = vec![false; layer_count];
        for (layer, current) in current_firmware_layer_names.iter_mut().enumerate() {
            let qsid = 200 + layer as u16;
            if !has_qmk_setting(qsid) {
                continue;
            }
            let name = session
                .get_qmk_setting_string(qsid)
                .map_err(failed(language, "Layer name read"))?;
            *current = Some(name.clone());
            layout.layer_names[layer] = name;
            layer_names_from_firmware[layer] = true;
        }
        let saved = load_saved_layer_names(&dev.name);
        layout.layer_names = super::device_connect_apply::resolve_layer_names(
            &layout.layer_names,
            &layer_names_from_firmware,
            saved.as_deref(),
            layer_count,
        );
        let layer_name_updates = layer_name_sync_updates(
            &layout.layer_names,
            &current_firmware_layer_names,
            &supported_qmk_settings,
        );
        if !layer_name_updates.is_empty() {
            progress("Syncing layer names…")?;
            for (qsid, name) in layer_name_updates {
                let value = crate::hid::qmk_setting_string_value(&name);
                if let Err(error) = session.set_setting(qsid, value) {
                    log::warn!("Rynk layer name sync failed for qsid {qsid}: {error:#}");
                }
            }
        }

        let mut read_losses = Vec::new();
        if !layout.encoders.is_empty() {
            progress("Reading encoders…")?;
            layout.encoder_layers = vec![vec![0u16; layout.encoders.len()]; layer_count];
            let encoders = session
                .encoders(layer_count as u8, layout.encoder_count() as u8)
                .map_err(failed(language, "Encoder read"))?;
            for (layer, actions) in encoders.into_iter().enumerate() {
                let keycodes: Vec<_> = actions
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        shown(
                            &mut read_losses,
                            DynamicSlot::Encoder { layer: layer as u8 },
                            index as u8,
                            dynamic::encoder_keycodes(action),
                        )
                    })
                    .collect();
                for (visual, encoder) in layout.encoders.iter().enumerate() {
                    if let Some([ccw, cw]) = keycodes.get(encoder.encoder_idx as usize) {
                        layout.encoder_layers[layer][visual] =
                            if encoder.direction == 0 { *ccw } else { *cw };
                    }
                }
            }
        }

        // A board without the option serves no value for it.
        let layout_options_value =
            if layout.layout_options.is_empty() || info.caps & caps::LAYOUT_OPTIONS == 0 {
                None
            } else {
                Some(
                    session
                        .layout_options()
                        .map_err(failed(language, "Layout options read"))?,
                )
            };

        progress("Reading macros…")?;
        let macro_count = capabilities.max_macros;
        let macro_memory_bytes = capabilities.macro_space_size;
        let macro_buffer = session
            .macro_buffer(macro_memory_bytes, macro_count)
            .map_err(failed(language, "Macro read"))?;
        let macro_texts = crate::hid::HidDevice::parse_macros(&macro_buffer, macro_count);

        progress("Reading combos…")?;
        let combo_entries: Vec<ComboEntry> = session
            .read_combos()
            .map_err(failed(language, "Combo read"))?
            .iter()
            .enumerate()
            .map(|(index, combo)| {
                shown(
                    &mut read_losses,
                    DynamicSlot::Combo,
                    index as u8,
                    dynamic::combo_entry(combo),
                )
            })
            .collect();
        progress("Reading tap dance entries…")?;
        let tap_dance_entries: Vec<crate::keycode_picker::TapDanceEntry> = session
            .read_morses()
            .map_err(failed(language, "Tap dance read"))?
            .iter()
            .enumerate()
            .map(|(index, morse)| {
                shown(
                    &mut read_losses,
                    DynamicSlot::TapDance,
                    index as u8,
                    dynamic::tap_dance_entry(morse),
                )
            })
            .collect();
        progress("Reading key overrides…")?;
        let key_override_entries: Vec<KeyOverrideEntry> = session
            .read_forks()
            .map_err(failed(language, "Key override read"))?
            .iter()
            .enumerate()
            .map(|(index, fork)| {
                shown(
                    &mut read_losses,
                    DynamicSlot::KeyOverride,
                    index as u8,
                    dynamic::key_override_entry(fork),
                )
            })
            .collect();

        progress("Reading QMK settings values…")?;
        let behavior_settings = Self::read_behavior_settings(&supported_qmk_settings, &session);
        let touchpad_settings =
            Self::read_touchpad_settings(&json, &supported_qmk_settings, &session);
        let bluetooth_settings =
            Self::read_bluetooth_settings(&json, &supported_qmk_settings, &session);
        progress("Reading module settings…")?;
        let module_settings = Self::read_module_settings(&json, &supported_qmk_settings, &session);
        let layer_led_settings =
            Self::read_layer_led_settings(&json, &supported_qmk_settings, layer_count, &session);
        session.forget_settings_read_ahead();

        let supports_battery_halves = info.caps & caps::BATTERY_HALVES != 0;
        let battery_halves = if supports_battery_halves {
            match session.battery_halves() {
                Ok(halves) => Some(halves),
                Err(error) => {
                    log::warn!("Rynk battery read failed: {error:#}");
                    None
                }
            }
        } else {
            None
        };

        // As RMK's Vial connect reported them: caps word on, no layer lock or
        // alt repeat; Repeat is there when its keycode maps.
        let vial_features = VialFeatureSupport {
            caps_word: true,
            layer_lock: false,
            persistent_default_layer: true,
            repeat_key: keycode::vial_keycode_action(QK_REPEAT_KEY).is_some(),
            alt_repeat_key: false,
        };
        let keyboard_id = u64::from_le_bytes(info.keyboard_uid);
        let firmware = info.firmware;
        let firmware_version = format!("{}.{}.{}", firmware.major, firmware.minor, firmware.patch);
        // The board's release, as over Vial, but its Rynk package: the
        // definition names the asset, and a dongle shows host time and media.
        let host_screen = caps::HOST_TIME | caps::HOST_MEDIA;
        let firmware_update_target = crate::app::rynk_firmware_release_target(
            dev.vendor_id,
            dev.product_id,
            Some(&firmware_version),
            &json,
            info.caps & host_screen == host_screen,
        );
        let about_info = DeviceAboutInfo {
            manufacturer: dev.manufacturer.clone(),
            product: dev.name.clone(),
            vendor_id: dev.vendor_id,
            product_id: dev.product_id,
            path: dev.path.clone(),
            firmware_version: Some(firmware_version),
            firmware_update_target,
            supports_battery_halves,
            battery_halves,
            keyboard_id,
            macro_entries: macro_texts.len(),
            macro_memory_bytes: Some(macro_memory_bytes),
            supports_macro_delays: !macro_texts.is_empty(),
            supports_macro_ext_keycodes: true,
            tap_dance_entries: tap_dance_entries.len(),
            combo_entries: combo_entries.len(),
            key_override_entries: key_override_entries.len(),
            caps_word: vial_features.caps_word,
            layer_lock: vial_features.layer_lock,
            qmk_settings: !supported_qmk_settings.is_empty(),
            rynk: Some(RynkAboutInfo {
                revision: info.revision,
                caps: info.caps,
            }),
            ..Default::default()
        };
        let vial_unlock_status = match session.unlock_status() {
            Ok((unlocked, in_progress, keys)) => Some((unlocked && !in_progress, keys)),
            Err(error) => {
                log::warn!("Rynk lock status read failed: {error:#}");
                None
            }
        };

        progress("Applying keyboard layout…")?;
        Ok(ConnectResult {
            device_name: dev.name.clone(),
            keyboard_id,
            vial_unlock_status,
            hid_device: Some(session.into()),
            about_info,
            layer_names_from_firmware,
            macro_texts,
            // Rynk carries every key action, combo output, morse action and
            // 16-bit macro keycode; symbols and Russian letters are per board.
            supports_macro_ext_keycodes: true,
            supports_rmk_native_key_actions: true,
            supports_universal_numero: false,
            supports_universal_symbols: info.caps & caps::UNIVERSAL_SYMBOLS != 0,
            supports_universal_russian_letters: info.caps & caps::RUSSIAN_LETTERS != 0,
            supports_rmk_native_combo_output: true,
            supports_rmk_native_tap_dance_actions: true,
            supports_rmk_combo_layers: true,
            macro_ext_keycodes_disabled_reason: None,
            tap_dance_entries,
            combo_entries,
            combo_term: behavior_settings.combo_term,
            auto_shift_options: behavior_settings.auto_shift_options,
            auto_shift_timeout: behavior_settings.auto_shift_timeout,
            mouse_keys_settings: behavior_settings.mouse_keys,
            touchpad_settings,
            bluetooth_settings,
            module_settings,
            tap_hold_settings: behavior_settings.tap_hold,
            magic_settings: behavior_settings.magic,
            one_shot_settings: behavior_settings.one_shot,
            grave_escape_settings: behavior_settings.grave_escape,
            layer_led_settings,
            rgb_settings: RgbSettingsState::default(),
            display_settings: DisplaySettingsState::default(),
            layout_options_value,
            key_override_entries,
            alt_repeat_entries: Vec::new(),
            vial_features,
            layout,
            layer_count,
            supported_qmk_settings,
            deferred_load: DeferredDeviceLoadState::complete(layer_count),
            rynk_read_losses: read_losses,
        })
    }
}

const QK_REPEAT_KEY: u16 = 0x7C79;

/// Where board definitions are kept by hash, beside the Vial cache. Tests
/// read their boards' definitions every time.
fn rynk_cache_dir() -> Option<std::path::PathBuf> {
    if cfg!(test) {
        return None;
    }
    let dir = dirs::config_dir()?.join("entropy").join("rynk_cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// The board's definition: the copy kept in `cache` under the hash the board
/// reports on every connect, or else read from the board and kept there. A
/// changed definition has another hash, and a copy that does not hash to its
/// name is read again.
fn board_definition(
    session: &RynkSession,
    cache: Option<&std::path::Path>,
) -> Result<Vec<u8>, RynkClientError> {
    let hash = session.info().definition_hash;
    let path = cache.map(|dir| dir.join(format!("definition_{hash:08x}.xz")));
    if let Some(kept) = path.as_ref().and_then(|path| std::fs::read(path).ok()) {
        if fnv1a(&kept) == hash {
            return Ok(kept);
        }
    }
    let definition = session
        .link()
        .run(move |client| client.read_board_definition(hash))?;
    if let Some(path) = path {
        if let Err(error) = keep(&path, &definition) {
            log::warn!("Rynk definition not kept at {}: {error}", path.display());
        }
    }
    Ok(definition)
}

/// Writes `bytes` to `path` through a file beside it, so a crash leaves no
/// half of a definition behind.
fn keep(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(bytes)?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// What the editor shows of an entry. What it cannot show goes to the report
/// after connect, since editing the entry replaces it.
fn shown<T>(
    losses: &mut Vec<(DynamicSlot, u8, DynamicLoss)>,
    slot: DynamicSlot,
    index: u8,
    (entry, lost): (T, Vec<DynamicLoss>),
) -> T {
    losses.extend(lost.into_iter().map(|loss| (slot, index, loss)));
    entry
}

/// What failed, and what the board's answer means when it gave one.
fn failed(language: crate::i18n::Language, what: &'static str) -> impl Fn(anyhow::Error) -> String {
    move |error| match error.downcast_ref::<RynkClientError>() {
        Some(rynk) => format!("{what} failed: {}", rynk.describe(language)),
        None => format!("{what} failed: {error:#}"),
    }
}

#[cfg(test)]
mod tests {
    use rmk_types_rynk::action::{Action, KeyAction};
    use rmk_types_rynk::combo::Combo;
    use rmk_types_rynk::fork::Fork;
    use rmk_types_rynk::keyboard_macros::MacroOp;
    use rmk_types_rynk::keycode::{HidKeyCode, KeyCode};
    use rmk_types_rynk::morse::{Morse, TAP};
    use rmk_types_rynk::protocol::rynk::ergohaven::{BatteryHalves, HostDataUpdate};

    use super::*;
    use crate::keyboard::KeyBinding;
    use crate::qmk_hid_host::rynk_bridge::testing::{
        await_updates, board as host_board, link as host_link, Desktop,
    };
    use crate::rynk::session::testing::{board, link, Stored};

    fn key(code: HidKeyCode) -> KeyAction {
        KeyAction::Single(Action::Key(KeyCode::Hid(code)))
    }

    fn tap_a() -> KeyAction {
        KeyAction::Tap(Action::Key(KeyCode::Hid(HidKeyCode::A)))
    }

    fn compressed(json: serde_json::Value) -> Vec<u8> {
        let text = json.to_string();
        let mut out = Vec::new();
        lzma_rs::xz_compress(&mut text.as_bytes(), &mut out).unwrap();
        out
    }

    /// A two-key board with two layers, one of everything, and a battery.
    fn stored() -> Stored {
        let mut stored = Stored::new(1, 2, 2);
        stored.definition = compressed(serde_json::json!({
            "name": "Rynk fixture",
            "matrix": {"rows": 1, "cols": 2},
            "layouts": {"keymap": [["0,0", "0,1"]]}
        }));
        stored.keyboard_uid = [1, 2, 3, 4, 5, 6, 7, 8];
        stored.caps = caps::BATTERY_HALVES | caps::UNIVERSAL_SYMBOLS;
        stored.battery = Some(BatteryHalves {
            mask: 0b01,
            left: 77,
            right: 0xFF,
        });
        // Layer 1, then layer 2; `Tap` has no keycode of its own.
        stored.keymap = vec![
            key(HidKeyCode::A),
            KeyAction::Tap(Action::Key(KeyCode::Hid(HidKeyCode::A))),
            key(HidKeyCode::B),
            KeyAction::No,
        ];
        stored.settings.insert(2, vec![200, 0]);
        stored.settings.insert(200, b"Base\0".to_vec());
        // Named as Entropy names it, so connect has no name to sync.
        stored.settings.insert(201, b"1\0".to_vec());
        stored.capabilities.max_combos = 2;
        stored.combos = vec![
            Combo::new(
                [key(HidKeyCode::A), key(HidKeyCode::B)],
                key(HidKeyCode::C),
                None,
            ),
            // A key the editor has no keycode for.
            Combo::new([tap_a(), key(HidKeyCode::B)], key(HidKeyCode::C), None),
        ];
        stored.capabilities.max_morse = 1;
        let mut morse = Morse::default();
        morse
            .put(TAP, Action::Key(KeyCode::Hid(HidKeyCode::A)))
            .unwrap();
        stored.morses = vec![morse];
        stored.capabilities.max_forks = 1;
        stored.forks = vec![Fork::empty()];
        stored.capabilities.max_macros = 1;
        stored.macros = vec![vec![MacroOp::Char(b'h'), MacroOp::Char(b'i')]];
        stored
    }

    fn rynk_device(path: &str) -> Device {
        Device {
            name: "Rynk fixture".into(),
            vendor_id: 0xE126,
            product_id: 0x0074,
            manufacturer: "Ergohaven".into(),
            serial_number: "rynk:010203".into(),
            bus_type: "Usb".into(),
            path: path.into(),
            instance_token: String::new(),
            firmware: FirmwareProtocol::Rynk,
        }
    }

    fn connect(path: &str) -> Result<ConnectResult, String> {
        EntropyApp::connect_rynk(&rynk_device(path), crate::i18n::Language::English, &|_| {
            Ok(())
        })
    }

    #[test]
    fn a_rynk_board_loads_in_full_without_a_write() {
        let (board, state) = board(stored());
        let _held = link("connect-full", &board);

        let result = connect("connect-full").unwrap();

        assert_eq!(result.layout.firmware, FirmwareProtocol::Rynk);
        assert_eq!(result.layer_count, 2);
        assert_eq!(result.layout.layers[0][0], KeyBinding::Vial(0x04));
        assert!(matches!(result.layout.layers[0][1], KeyBinding::Rynk(_)));
        assert_eq!(result.layout.layers[1][0], KeyBinding::Vial(0x05));
        assert_eq!(result.layout.layer_names[0], "Base");
        assert_eq!(result.layer_names_from_firmware, [true, true]);
        assert_eq!(result.combo_entries[0].keys, [0x04, 0x05, 0, 0]);
        assert_eq!(result.combo_entries[0].output, KeyBinding::Vial(0x06));
        assert_eq!(
            result.rynk_read_losses,
            [(DynamicSlot::Combo, 1, DynamicLoss::Key(tap_a()))],
            "what the editor cannot show is reported"
        );
        assert_eq!(result.tap_dance_entries[0].on_tap, KeyBinding::Vial(0x04));
        assert_eq!(result.key_override_entries.len(), 1);
        assert_eq!(result.macro_texts[0], b"hi");
        assert_eq!(result.combo_term, Some(200));
        assert!(result.supports_universal_symbols, "the board has symbols");
        let update = result.about_info.firmware_update_target.as_ref().unwrap();
        assert_eq!(update.asset_prefix, "k04-rynk", "the board's Rynk package");
        assert_eq!(update.current_version, "2.3.1");
        assert!(
            !result.supports_universal_russian_letters,
            "but not Russian letters"
        );
        assert_eq!(result.supported_qmk_settings, [2, 200, 201]);
        assert_eq!(
            result.keyboard_id,
            u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8])
        );
        assert_eq!(result.about_info.keyboard_id, result.keyboard_id);
        assert_eq!(
            result.about_info.rynk,
            Some(RynkAboutInfo {
                revision: rmk_types_rynk::protocol::rynk::ergohaven::ERGOHAVEN_RYNK_REVISION,
                caps: caps::BATTERY_HALVES | caps::UNIVERSAL_SYMBOLS,
            }),
            "About Device shows the board's revision and caps"
        );
        let battery = result.about_info.battery_halves.unwrap();
        assert_eq!((battery.left, battery.right), (Some(77), None));
        assert!(result
            .hid_device
            .as_ref()
            .is_some_and(crate::device_session::DeviceSession::is_rynk));
        assert!(
            !result.deferred_load.is_staged(),
            "nothing is left for later"
        );
        assert_eq!(state.lock().unwrap().writes(), [], "loading writes nothing");
    }

    #[test]
    fn a_board_that_takes_requests_back_to_back_loads_the_same() {
        let (one_by_one, _) = board(stored());
        let _held = link("connect-one-by-one", &one_by_one);
        let expected = connect("connect-one-by-one").unwrap();
        let (board, state) = board(stored());
        let board = board.pipelining();
        let _held = link("connect-pipelined", &board);

        let result = connect("connect-pipelined").unwrap();

        assert_eq!(result.layout.layers, expected.layout.layers);
        assert_eq!(result.layout.layer_names, expected.layout.layer_names);
        assert_eq!(result.combo_entries, expected.combo_entries);
        assert_eq!(result.tap_dance_entries, expected.tap_dance_entries);
        assert_eq!(result.key_override_entries, expected.key_override_entries);
        assert_eq!(result.macro_texts, expected.macro_texts);
        assert_eq!(result.combo_term, expected.combo_term);
        assert_eq!(result.rynk_read_losses, expected.rynk_read_losses);
        assert!(
            board.batches().contains(&3),
            "the three settings in one go: {:?}",
            board.batches()
        );
        let state = state.lock().unwrap();
        let settings = state
            .received
            .iter()
            .filter(|cmd| **cmd == rmk_types_rynk::protocol::rynk::Cmd::GetSetting)
            .count();
        assert_eq!(settings, 3, "each read once, ahead");
        assert_eq!(state.writes(), [], "loading writes nothing");
    }

    fn definition_reads(state: &std::sync::Mutex<Stored>) -> usize {
        let state = state.lock().unwrap();
        state
            .received
            .iter()
            .filter(|cmd| **cmd == rmk_types_rynk::protocol::rynk::Cmd::GetBoardDefinition)
            .count()
    }

    #[test]
    fn a_kept_definition_is_not_read_again() {
        let cache = tempfile::tempdir().unwrap();
        let (board, state) = board(stored());
        let session = RynkSession::new(link("definition-kept", &board));
        let definition = state.lock().unwrap().definition.clone();

        assert_eq!(
            board_definition(&session, Some(cache.path())).unwrap(),
            definition
        );
        assert_eq!(definition_reads(&state), 1);
        assert_eq!(
            board_definition(&session, Some(cache.path())).unwrap(),
            definition
        );
        assert_eq!(definition_reads(&state), 1, "the kept copy served");

        assert_eq!(board_definition(&session, None).unwrap(), definition);
        assert_eq!(definition_reads(&state), 2, "nowhere to keep it");
    }

    #[test]
    fn a_kept_definition_that_does_not_hash_to_its_name_is_read_again() {
        let cache = tempfile::tempdir().unwrap();
        let (board, state) = board(stored());
        let session = RynkSession::new(link("definition-damaged", &board));
        let definition = state.lock().unwrap().definition.clone();
        let kept = cache
            .path()
            .join(format!("definition_{:08x}.xz", fnv1a(&definition)));
        std::fs::write(&kept, b"half a definition").unwrap();

        assert_eq!(
            board_definition(&session, Some(cache.path())).unwrap(),
            definition
        );
        assert_eq!(definition_reads(&state), 1);
        assert_eq!(std::fs::read(&kept).unwrap(), definition, "kept whole now");
    }

    #[test]
    fn a_definition_of_another_matrix_is_refused() {
        let mut stored = stored();
        stored.capabilities.num_cols = 3;
        stored.keymap.resize(6, KeyAction::No);
        let (board, _) = board(stored);
        let _held = link("connect-matrix", &board);

        let Err(error) = connect("connect-matrix") else {
            panic!("a board whose matrix differs connected");
        };
        assert!(error.contains("differs"), "{error}");
    }

    /// The Rynk side of `universal_symbols_capability_enables_existing_layout_sync_bridge`:
    /// Phenom's definition names no host data, yet its Universal Symbols need
    /// the layout, so the board's caps turn the layout sync on.
    #[test]
    fn universal_symbols_cap_enables_the_layout_sync_bridge() {
        for (caps, layout) in [
            (caps::UNIVERSAL_SYMBOLS | caps::HOST_KEYBOARD_LAYOUT, true),
            (caps::UNIVERSAL_SYMBOLS, false),
            (caps::HOST_KEYBOARD_LAYOUT, false),
        ] {
            let mut stored = stored();
            stored.caps = caps;
            let (board, _) = board(stored);
            let path = format!("connect-layout-{caps:#x}");
            let _held = link(&path, &board);

            let result = connect(&path).unwrap();

            assert_eq!(result.layout.live_features.layout, layout, "{caps:#x}");
            let mode =
                EntropyApp::qmk_hid_host_mode_for(&result.layout, result.layout_options_value);
            assert_eq!(mode.layout, layout, "{caps:#x}");
            if !layout {
                continue;
            }
            // That mode's bridge gives such a board the desktop's layout.
            let desktop = Desktop::new();
            desktop.show(Some(1), None);
            let (host, received) = host_board(caps);
            let _bridge = desktop.bridge(host_link(&format!("{path}-host"), &host), mode);
            assert_eq!(
                await_updates(&received, 1),
                [HostDataUpdate::KeyboardLayout(1)]
            );
        }
    }

    #[test]
    fn picking_a_rynk_board_connects_it_through_the_connect_worker() {
        let (board, _) = board(stored());
        let _held = link("connect-worker", &board);
        let mut app = EntropyApp::new_inert_for_test();
        app.device_manager
            .replace_devices(vec![rynk_device("connect-worker")]);

        app.start_connect(0);
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(app.connect_state, ConnectState::Loading { .. }) {
            assert!(
                std::time::Instant::now() < deadline,
                "connect never finished"
            );
            app.poll_connect(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(app
            .hid_device
            .as_ref()
            .is_some_and(crate::device_session::DeviceSession::is_rynk));
        assert_eq!(app.firmware, FirmwareProtocol::Rynk);
        assert_eq!(app.combo_entries.len(), 2);
        assert_eq!(app.keycode_picker.macro_texts[0], b"hi");
        assert!(app.import_report_open, "the losses are shown after connect");
    }

    /// An app the fixture board under `path` is connected to.
    fn connected(path: &str) -> EntropyApp {
        let mut app = EntropyApp::new_inert_for_test();
        app.device_manager.replace_devices(vec![rynk_device(path)]);
        app.start_connect(0);
        finish_connect(&mut app);
        app
    }

    fn finish_connect(app: &mut EntropyApp) {
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(app.connect_state, ConnectState::Loading { .. }) {
            assert!(
                std::time::Instant::now() < deadline,
                "connect never finished"
            );
            app.poll_connect(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn an_import_the_board_drops_out_of_leaves_the_editor_as_it_was() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (board, state) = board(stored());
        let _held = link("import-dropped", &board);
        let mut app = connected("import-dropped");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("changed.entlayout");
        app.write_entlayout_export(&path);
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        // "bye", under a name of its own.
        file["data"]["macros"]["bytecode_base64"] = serde_json::json!(["Ynll"]);
        file["data"]["macros"]["names"] = serde_json::json!(["Farewell"]);
        std::fs::write(&path, file.to_string()).unwrap();

        board.unplug_on(rmk_types_rynk::protocol::rynk::Cmd::SetMacro);
        let error = app.import_entlayout_from_path(&path).unwrap_err();
        let lost = crate::i18n::tr_catalog(app.app_settings.language, "entlayout.connection_lost");
        assert!(error.to_string().contains(lost), "{error:#}");
        assert_eq!(app.keycode_picker.macro_texts[0], b"hi");
        assert_ne!(
            app.keycode_picker.macro_names.first().map(String::as_str),
            Some("Farewell")
        );
        assert_eq!(
            state.lock().unwrap().macros[0],
            [MacroOp::Char(b'h'), MacroOp::Char(b'i')]
        );
    }

    #[test]
    fn a_rynk_board_whose_link_broke_is_opened_again_while_plugged_in() {
        use rmk_types_rynk::protocol::rynk::{command, Cmd};

        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let (board, _) = board(stored());
        let first = link("link-broke", &board);
        let mut app = connected("link-broke");
        board.unplug_on(Cmd::GetCurrentLayer);
        assert!(first.request::<command::GetCurrentLayer>(()).is_err());
        assert!(!first.is_open());

        // The port lists the board as before; its next link is the one a
        // connect finds.
        let second = link("link-broke", &board);
        let devices = app.device_manager.devices().to_vec();
        app.apply_device_scan_result(devices);
        finish_connect(&mut app);
        let session = app
            .hid_device
            .as_ref()
            .and_then(crate::device_session::DeviceSession::as_rynk)
            .expect("the board is connected again");
        assert!(std::sync::Arc::ptr_eq(session.link(), &second));
    }

    #[test]
    fn a_locked_rynk_board_locks_its_matrix_but_not_its_edits() {
        let mut stored = stored();
        stored.lock = Some(rmk_types_rynk::protocol::rynk::LockStatus {
            locked: true,
            unlocking: false,
            remaining_keys: 1,
            key_positions: [(0, 1)].as_slice().try_into().unwrap(),
        });
        let (board, _) = board(stored);
        let _held = link("connect-locked", &board);
        let mut app = EntropyApp::new_inert_for_test();
        app.device_manager
            .replace_devices(vec![rynk_device("connect-locked")]);

        app.start_connect(0);
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while matches!(app.connect_state, ConnectState::Loading { .. }) {
            assert!(
                std::time::Instant::now() < deadline,
                "connect never finished"
            );
            app.poll_connect(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert!(app.is_matrix_locked(), "the matrix waits for an unlock");
        assert!(!app.is_vial_locked(), "edits do not");
        assert!(app.has_keyboard_lock());
    }
}
