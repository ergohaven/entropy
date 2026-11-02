//! Combos, tap dances, key overrides and encoders: the editor's Vial-shaped
//! entries next to Rynk's `Combo`, `Morse`, `Fork` and `EncoderAction`.
//!
//! The mapping is the firmware's Vial one (`rmk/src/host/via/vial.rs`), so an
//! entry means on a Rynk board what it meant on a Vial RMK board. Where that
//! mapping would drop something without a word, writes fail and reads report it.

use std::fmt;

use rmk_types_rynk::action::{Action, EncoderAction, KeyAction};
use rmk_types_rynk::combo::Combo;
use rmk_types_rynk::fork::{Fork, ForkOptions, StateBits};
use rmk_types_rynk::modifier::ModifierCombination;
use rmk_types_rynk::morse::{Morse, MorsePattern, DOUBLE_TAP, HOLD, HOLD_AFTER_TAP, TAP};
use rmk_types_rynk::protocol::vial::VIAL_KEY_OVERRIDE_OPTION_BITS;

use super::keycode::{binding_action, exact_vial_keycode, key_binding, KeyBindingError};
use crate::app::{ComboEntry, KeyOverrideEntry, KeyOverrideOptionsState};
use crate::keyboard::KeyBinding;
use crate::keycode_picker::TapDanceEntry;

/// The tapping term Vial shows for a morse key without its own hold timeout.
const DEFAULT_TAPPING_TERM: u16 = 250;

/// Tap dance fields in Vial's order, with the morse pattern each one is.
const TAP_DANCE_PATTERNS: [MorsePattern; 4] = [TAP, HOLD, DOUBLE_TAP, HOLD_AFTER_TAP];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DynamicError {
    Binding(KeyBindingError),
    /// A tap dance field holds a key action that is not a single action, such
    /// as a tap-hold: a morse pattern runs one action.
    NotAnAction(KeyAction),
    TooManyComboKeys {
        max: usize,
    },
    TooManyMorsePatterns {
        max: usize,
    },
}

impl From<KeyBindingError> for DynamicError {
    fn from(error: KeyBindingError) -> Self {
        Self::Binding(error)
    }
}

impl fmt::Display for DynamicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => error.fmt(f),
            Self::NotAnAction(action) => write!(f, "{action:?} cannot run in a tap dance"),
            Self::TooManyComboKeys { max } => write!(f, "the board takes at most {max} combo keys"),
            Self::TooManyMorsePatterns { max } => {
                write!(f, "the board takes at most {max} patterns per tap dance")
            }
        }
    }
}

impl std::error::Error for DynamicError {}

/// What a Vial-shaped entry cannot show of the board's value. Writing the
/// entry back replaces the value, except for `MorsePattern`, which is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DynamicLoss {
    /// A combo key or a key override key with no keycode of its own.
    Key(KeyAction),
    /// A combo with more keys than the editor's four.
    ComboKeys(usize),
    /// A morse pattern a tap dance has no field for. Kept on write.
    MorsePattern(MorsePattern),
    /// A key override that outputs something other than its trigger when
    /// inactive.
    NegativeOutput(KeyAction),
    /// A key override that matches LED or mouse button state.
    StateMatch,
    /// A key override that feeds other key overrides.
    Bindable,
    /// A key override limited to layers above 15.
    Layers(u32),
}

/// Where a `DynamicLoss` was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DynamicSlot {
    Combo,
    TapDance,
    KeyOverride,
    Encoder { layer: u8 },
}

/// The warning for what the editor cannot show of the entries read from the
/// board, one line each, with what editing the entry does to it.
pub(crate) fn describe_dynamic_losses(
    language: crate::i18n::Language,
    losses: &[(DynamicSlot, u8, DynamicLoss)],
) -> String {
    let header = crate::i18n::tr_catalog(language, "rynk_read.losses");
    std::iter::once(header.to_owned())
        .chain(
            losses
                .iter()
                .map(|&(slot, index, loss)| describe_dynamic_loss(language, slot, index, loss)),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn describe_dynamic_loss(
    language: crate::i18n::Language,
    slot: DynamicSlot,
    index: u8,
    loss: DynamicLoss,
) -> String {
    let tr = |key, vars: &[(&str, &str)]| crate::i18n::tr_catalog_format(language, key, vars);
    let action = |action: KeyAction| format!("{action:?}");
    let detail = match loss {
        DynamicLoss::Key(key) => tr("rynk_read.key", &[("action", &action(key))]),
        DynamicLoss::ComboKeys(count) => {
            tr("rynk_read.combo_keys", &[("count", &count.to_string())])
        }
        DynamicLoss::MorsePattern(pattern) => tr(
            "rynk_read.morse_pattern",
            &[("pattern", &morse_steps(pattern))],
        ),
        DynamicLoss::NegativeOutput(output) => {
            tr("rynk_read.negative_output", &[("action", &action(output))])
        }
        DynamicLoss::StateMatch => tr("rynk_read.state_match", &[]),
        DynamicLoss::Bindable => tr("rynk_read.bindable", &[]),
        DynamicLoss::Layers(_) => tr("rynk_read.layers", &[]),
    };
    let index = index.to_string();
    let entry = |key| tr(key, &[("index", &index), ("detail", &detail)]);
    match slot {
        DynamicSlot::Combo => entry("rynk_read.combo"),
        DynamicSlot::TapDance => entry("rynk_read.tap_dance"),
        DynamicSlot::KeyOverride => entry("rynk_read.key_override"),
        DynamicSlot::Encoder { layer } => tr(
            "rynk_read.encoder",
            &[
                ("layer", &layer.to_string()),
                ("encoder", &index),
                ("detail", &detail),
            ],
        ),
    }
}

/// A morse pattern as Morse code: `·` a tap, `—` a hold, in press order.
fn morse_steps(pattern: MorsePattern) -> String {
    let bits = pattern.to_u16();
    // The highest set bit marks where the presses start.
    let presses = 15 - bits.leading_zeros().min(15);
    (0..presses)
        .rev()
        .map(|press| {
            if (bits >> press) & 1 == 1 {
                '—'
            } else {
                '·'
            }
        })
        .collect()
}

/// The combo a Vial-shaped entry writes: its non-empty keys in order, as the
/// firmware keeps them.
pub(crate) fn combo(entry: &ComboEntry, max_keys: usize) -> Result<Combo, DynamicError> {
    let mut combo = Combo::empty();
    for &key in entry.keys.iter().filter(|&&key| key != 0) {
        let action = binding_action(KeyBinding::Vial(key))?;
        if combo.actions.len() == max_keys || combo.actions.push(action).is_err() {
            return Err(DynamicError::TooManyComboKeys { max: max_keys });
        }
    }
    combo.output = binding_action(entry.output)?;
    combo.layer = entry.layer;
    Ok(combo)
}

pub(crate) fn combo_entry(combo: &Combo) -> (ComboEntry, Vec<DynamicLoss>) {
    let mut losses = Vec::new();
    let mut keys = [0u16; 4];
    for (slot, &action) in keys.iter_mut().zip(&combo.actions) {
        match exact_vial_keycode(action) {
            Some(keycode) => *slot = keycode,
            None => losses.push(DynamicLoss::Key(action)),
        }
    }
    if combo.actions.len() > keys.len() {
        losses.push(DynamicLoss::ComboKeys(combo.actions.len()));
    }
    let entry = ComboEntry {
        keys,
        output: key_binding(combo.output),
        layer: combo.layer,
    };
    (entry, losses)
}

/// The tap dance a morse key shows: its four Vial patterns and hold timeout.
pub(crate) fn tap_dance_entry(morse: &Morse) -> (TapDanceEntry, Vec<DynamicLoss>) {
    let field = |pattern| action_binding(morse.get(pattern).unwrap_or(Action::No));
    let entry = TapDanceEntry {
        on_tap: field(TAP),
        on_hold: field(HOLD),
        on_double_tap: field(DOUBLE_TAP),
        on_tap_hold: field(HOLD_AFTER_TAP),
        tapping_term: tapping_term(morse),
    };
    let losses = morse
        .actions
        .keys()
        .filter(|pattern| !TAP_DANCE_PATTERNS.contains(pattern))
        .map(|&pattern| DynamicLoss::MorsePattern(pattern))
        .collect();
    (entry, losses)
}

/// `morse` with a tap dance written over it, as the firmware's Vial path
/// updates it: the four patterns are replaced and the others kept. A changed
/// tapping term becomes both the hold and the gap timeout; an unchanged one
/// leaves the profile alone.
pub(crate) fn merge_tap_dance(
    morse: &Morse,
    entry: &TapDanceEntry,
    max_patterns: usize,
) -> Result<Morse, DynamicError> {
    let fields = [
        entry.on_tap,
        entry.on_hold,
        entry.on_double_tap,
        entry.on_tap_hold,
    ];
    let mut merged = morse.clone();
    for (pattern, binding) in TAP_DANCE_PATTERNS.into_iter().zip(fields) {
        let action = match binding_action(binding)? {
            KeyAction::No => Action::No,
            KeyAction::Single(action) => action,
            other => return Err(DynamicError::NotAnAction(other)),
        };
        merged
            .put(pattern, action)
            .map_err(|_| DynamicError::TooManyMorsePatterns { max: max_patterns })?;
    }
    if merged.actions.len() > max_patterns {
        return Err(DynamicError::TooManyMorsePatterns { max: max_patterns });
    }
    if entry.tapping_term != tapping_term(morse) {
        merged.profile.set_hold_timeout_ms(entry.tapping_term);
        merged.profile.set_gap_timeout_ms(entry.tapping_term);
    }
    Ok(merged)
}

fn tapping_term(morse: &Morse) -> u16 {
    morse
        .profile
        .hold_timeout_ms()
        .unwrap_or(DEFAULT_TAPPING_TERM)
}

fn action_binding(action: Action) -> KeyBinding {
    match action {
        Action::No => KeyBinding::Vial(0),
        action => key_binding(KeyAction::Single(action)),
    }
}

/// The key override a fork shows, as the firmware's
/// `fork_to_vial_key_override` reads it.
pub(crate) fn key_override_entry(fork: &Fork) -> (KeyOverrideEntry, Vec<DynamicLoss>) {
    let mut losses = Vec::new();
    if fork.trigger == KeyAction::No {
        return (KeyOverrideEntry::default(), losses);
    }
    let mut keycode = |action| {
        exact_vial_keycode(action).unwrap_or_else(|| {
            losses.push(DynamicLoss::Key(action));
            0
        })
    };
    let trigger = keycode(fork.trigger);
    let replacement = keycode(fork.positive_output);
    if !super::keycode::identical(fork.negative_output, fork.trigger) {
        losses.push(DynamicLoss::NegativeOutput(fork.negative_output));
    }
    let modifiers_only = |state: StateBits| StateBits {
        modifiers: state.modifiers,
        ..Default::default()
    };
    if modifiers_only(fork.match_any) != fork.match_any
        || modifiers_only(fork.match_none) != fork.match_none
    {
        losses.push(DynamicLoss::StateMatch);
    }
    if fork.bindable {
        losses.push(DynamicLoss::Bindable);
    }
    let layers = match fork.layers {
        u32::MAX => 0xFFFF,
        layers if layers > 0xFFFF => {
            losses.push(DynamicLoss::Layers(layers));
            layers as u16
        }
        layers => layers as u16,
    };
    let entry = KeyOverrideEntry {
        trigger,
        replacement,
        layers,
        trigger_mods: fork.match_any.modifiers.into_bits(),
        negative_mod_mask: fork.match_none.modifiers.into_bits(),
        suppressed_mods: fork.suppressed_modifiers.into_bits(),
        options: KeyOverrideOptionsState::from_bits(
            fork.options.into_bits() & VIAL_KEY_OVERRIDE_OPTION_BITS,
        ),
    };
    (entry, losses)
}

/// The fork a key override writes, as the firmware's
/// `vial_key_override_to_fork` builds it.
pub(crate) fn fork(entry: &KeyOverrideEntry) -> Result<Fork, DynamicError> {
    let trigger = binding_action(KeyBinding::Vial(entry.trigger))?;
    if trigger == KeyAction::No {
        return Ok(Fork::empty());
    }
    let state = |bits| StateBits {
        modifiers: ModifierCombination::from_bits(bits),
        ..Default::default()
    };
    Ok(Fork {
        trigger,
        // The trigger passes through when the condition is not met.
        negative_output: trigger,
        positive_output: binding_action(KeyBinding::Vial(entry.replacement))?,
        match_any: state(entry.trigger_mods),
        match_none: state(entry.negative_mod_mask),
        suppressed_modifiers: ModifierCombination::from_bits(entry.suppressed_mods),
        layers: match entry.layers {
            0xFFFF => u32::MAX,
            layers => layers as u32,
        },
        options: ForkOptions::from_bits(entry.options.bits() & VIAL_KEY_OVERRIDE_OPTION_BITS),
        // QMK key overrides don't cascade.
        bindable: false,
    })
}

/// An encoder's keycodes, counter-clockwise first as Vial reads them.
pub(crate) fn encoder_keycodes(action: EncoderAction) -> ([u16; 2], Vec<DynamicLoss>) {
    let mut losses = Vec::new();
    let mut keycode = |action| {
        exact_vial_keycode(action).unwrap_or_else(|| {
            losses.push(DynamicLoss::Key(action));
            0
        })
    };
    let keycodes = [keycode(action.counter_clockwise), keycode(action.clockwise)];
    (keycodes, losses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rynk::keycode::{catalog, identical, vial_keycode_action};
    use rmk_types_rynk::keycode::{HidKeyCode, KeyCode};

    const KC_A: u16 = 0x04;
    const KC_B: u16 = 0x05;
    const KC_ESC: u16 = 0x29;

    fn key(code: HidKeyCode) -> KeyAction {
        KeyAction::Single(Action::Key(KeyCode::Hid(code)))
    }

    /// The encoder Vial writes with `keycodes`, counter-clockwise first.
    fn encoder_action(keycodes: [u16; 2]) -> Result<EncoderAction, DynamicError> {
        let [counter_clockwise, clockwise] = keycodes;
        Ok(EncoderAction::new(
            binding_action(KeyBinding::Vial(clockwise))?,
            binding_action(KeyBinding::Vial(counter_clockwise))?,
        ))
    }

    #[test]
    fn combos_keep_their_keys_in_order_and_round_trip() {
        let entry = ComboEntry {
            keys: [KC_A, 0, KC_B, 0],
            output: KeyBinding::Vial(KC_ESC),
            layer: Some(2),
        };
        let combo = combo(&entry, 4).unwrap();
        assert_eq!(
            combo.actions.as_slice(),
            [key(HidKeyCode::A), key(HidKeyCode::B)]
        );
        assert_eq!(combo.output, key(HidKeyCode::Escape));
        assert_eq!(combo.layer, Some(2));
        // The firmware packs the keys; the editor gets them back without the gap.
        let (read, losses) = combo_entry(&combo);
        assert!(losses.is_empty());
        assert_eq!(read.keys, [KC_A, KC_B, 0, 0]);
        assert_eq!(read.output, KeyBinding::Vial(KC_ESC));
        assert_eq!(read.layer, Some(2));

        let user = KeyAction::Single(Action::User(90));
        let native = ComboEntry {
            output: KeyBinding::Rynk(user),
            layer: None,
            ..entry.clone()
        };
        assert_eq!(
            combo_entry(&super::combo(&native, 4).unwrap()).0.output,
            KeyBinding::Rynk(user)
        );

        assert_eq!(
            super::combo(
                &ComboEntry {
                    keys: [KC_A, KC_B, KC_ESC, 0],
                    ..entry.clone()
                },
                2
            ),
            Err(DynamicError::TooManyComboKeys { max: 2 })
        );
        assert_eq!(
            super::combo(
                &ComboEntry {
                    keys: [KC_A, 0x7000, 0, 0],
                    ..entry
                },
                4
            ),
            Err(DynamicError::Binding(KeyBindingError::UnsupportedKeycode(
                0x7000
            )))
        );
    }

    #[test]
    fn combo_keys_without_a_keycode_are_reported() {
        let user = KeyAction::Single(Action::User(90));
        let mut combo = Combo::empty();
        for action in [
            key(HidKeyCode::A),
            user,
            key(HidKeyCode::B),
            key(HidKeyCode::C),
            key(HidKeyCode::D),
        ] {
            combo.actions.push(action).unwrap();
        }
        let (entry, losses) = combo_entry(&combo);
        assert_eq!(entry.keys, [KC_A, 0, KC_B, 0x06]);
        assert_eq!(losses, [DynamicLoss::Key(user), DynamicLoss::ComboKeys(5)]);
    }

    #[test]
    fn every_tap_dance_field_round_trips_through_morse() {
        let mut fields = 0;
        for action in catalog::actions() {
            let binding = action_binding(action);
            let entry = TapDanceEntry {
                on_tap: binding,
                on_hold: KeyBinding::Vial(KC_A),
                on_double_tap: KeyBinding::Vial(0),
                on_tap_hold: binding,
                tapping_term: 180,
            };
            let morse = merge_tap_dance(&Morse::default(), &entry, 4).unwrap();
            // An HID no-key is KC_NO in the editor, so it leaves the pattern empty.
            let empty = matches!(
                action,
                Action::No | Action::Key(KeyCode::Hid(HidKeyCode::No))
            );
            assert_eq!(morse.get(TAP), (!empty).then_some(action));
            let (read, losses) = tap_dance_entry(&morse);
            assert!(losses.is_empty());
            assert_eq!(read, entry, "{action:?}");
            fields += 1;
        }
        assert!(fields > 70_000, "{fields}");
    }

    #[test]
    fn a_tap_dance_write_keeps_what_the_editor_does_not_show() {
        let triple_tap = MorsePattern::from_u16(0b1000);
        let mut morse = Morse::default();
        morse
            .put(TAP, Action::Key(KeyCode::Hid(HidKeyCode::A)))
            .unwrap();
        morse
            .put(triple_tap, Action::Key(KeyCode::Hid(HidKeyCode::C)))
            .unwrap();
        morse.profile = morse
            .profile
            .with_hold_timeout_ms(Some(200))
            .with_gap_timeout_ms(Some(120))
            .with_unilateral_tap(Some(true));

        let (mut entry, losses) = tap_dance_entry(&morse);
        assert_eq!(losses, [DynamicLoss::MorsePattern(triple_tap)]);
        assert_eq!(entry.tapping_term, 200);

        entry.on_hold = KeyBinding::Vial(KC_B);
        let merged = merge_tap_dance(&morse, &entry, 4).unwrap();
        assert_eq!(merged.get(triple_tap), morse.get(triple_tap));
        assert_eq!(
            merged.get(HOLD),
            Some(Action::Key(KeyCode::Hid(HidKeyCode::B)))
        );
        // The term did not change: the gap timeout and the rest stay.
        assert_eq!(merged.profile, morse.profile);

        entry.tapping_term = 300;
        let merged = merge_tap_dance(&morse, &entry, 4).unwrap();
        assert_eq!(merged.profile.hold_timeout_ms(), Some(300));
        assert_eq!(merged.profile.gap_timeout_ms(), Some(300));
        assert_eq!(merged.profile.unilateral_tap(), Some(true));

        entry.on_double_tap = KeyBinding::Vial(KC_ESC);
        assert_eq!(
            merge_tap_dance(&morse, &entry, 3),
            Err(DynamicError::TooManyMorsePatterns { max: 3 })
        );
        // A layer tap is a key action, not one a pattern runs.
        entry.on_double_tap = KeyBinding::Vial(0x4104);
        assert!(matches!(
            merge_tap_dance(&morse, &entry, 4),
            Err(DynamicError::NotAnAction(KeyAction::TapHold(..)))
        ));
    }

    /// The firmware's Shift + Backspace → Delete entry (`shift_backspace_wire_entry`).
    fn shift_backspace() -> KeyOverrideEntry {
        KeyOverrideEntry {
            trigger: 0x2A,
            replacement: 0x4C,
            layers: 0xFFFF,
            trigger_mods: 0x22,
            negative_mod_mask: 0x44,
            suppressed_mods: 0x02,
            options: KeyOverrideOptionsState::from_bits(0x80 | 0x08 | 0x07),
        }
    }

    #[test]
    fn key_overrides_follow_the_firmware_vectors() {
        let entry = shift_backspace();
        let fork = fork(&entry).unwrap();
        assert!(fork.options.enabled());
        assert!(!fork.bindable);
        assert_eq!(fork.trigger, key(HidKeyCode::Backspace));
        assert_eq!(fork.negative_output, fork.trigger);
        assert_eq!(fork.positive_output, key(HidKeyCode::Delete));
        assert_eq!(fork.match_any.modifiers.into_bits(), 0x22);
        assert_eq!(fork.match_none.modifiers.into_bits(), 0x44);
        assert_eq!(fork.suppressed_modifiers.into_bits(), 0x02);
        assert!(fork.options.one_mod());
        assert_eq!(fork.layers, u32::MAX);
        assert_eq!(key_override_entry(&fork), (entry.clone(), vec![]));

        let and_mode = KeyOverrideEntry {
            layers: 0x0005,
            suppressed_mods: 0x44,
            options: KeyOverrideOptionsState::from_bits(0x80 | 0x07),
            ..entry.clone()
        };
        let fork = super::fork(&and_mode).unwrap();
        assert!(!fork.options.one_mod());
        assert_eq!(fork.layers, 0x0005);
        assert_eq!(key_override_entry(&fork), (and_mode, vec![]));

        let options = KeyOverrideEntry {
            options: KeyOverrideOptionsState::from_bits(0x80 | 0b0011_0010),
            ..entry.clone()
        };
        let fork = super::fork(&options).unwrap();
        assert!(!fork.options.activate_on_trigger_down());
        assert!(fork.options.activate_on_required_mod_down());
        assert!(!fork.options.activate_on_negative_mod_up());
        assert!(fork.options.no_reregister_trigger());
        assert!(fork.options.no_unregister_on_other_key_down());
        assert_eq!(key_override_entry(&fork), (options, vec![]));

        assert_eq!(
            super::fork(&KeyOverrideEntry::default()).unwrap(),
            Fork::empty()
        );
        assert_eq!(
            key_override_entry(&Fork::empty()),
            (KeyOverrideEntry::default(), vec![])
        );
        let disabled = KeyOverrideEntry {
            options: KeyOverrideOptionsState::from_bits(0x08 | 0x07),
            ..entry
        };
        assert_eq!(
            key_override_entry(&super::fork(&disabled).unwrap()),
            (disabled, vec![])
        );
    }

    #[test]
    fn key_overrides_round_trip_every_option_layer_and_modifier() {
        let mut checked = 0;
        for slot in 0..32u16 {
            for options in 0..=u8::MAX {
                let options = options & VIAL_KEY_OVERRIDE_OPTION_BITS;
                let entry = KeyOverrideEntry {
                    trigger: 0x04 + slot,
                    replacement: 0x5220 + slot % 16,
                    layers: if slot == 31 {
                        0xFFFF
                    } else {
                        1 << (slot % 16) | slot
                    },
                    trigger_mods: options.rotate_left(slot as u32 % 8),
                    negative_mod_mask: !options,
                    suppressed_mods: options ^ slot as u8,
                    options: KeyOverrideOptionsState::from_bits(options),
                };
                let fork = fork(&entry).unwrap();
                assert_eq!(key_override_entry(&fork), (entry, vec![]));
                checked += 1;
            }
        }
        assert_eq!(checked, 32 * 256);
    }

    #[test]
    fn forks_richer_than_a_key_override_are_reported() {
        let mut fork = fork(&shift_backspace()).unwrap();
        let user = KeyAction::Single(Action::User(90));
        fork.negative_output = KeyAction::Transparent;
        fork.positive_output = user;
        fork.match_any.leds = fork.match_any.leds.with_caps_lock(true);
        fork.bindable = true;
        fork.layers = 1 << 20;
        let (entry, losses) = key_override_entry(&fork);
        assert_eq!(entry.replacement, 0);
        assert_eq!(
            losses,
            [
                DynamicLoss::Key(user),
                DynamicLoss::NegativeOutput(KeyAction::Transparent),
                DynamicLoss::StateMatch,
                DynamicLoss::Bindable,
                DynamicLoss::Layers(1 << 20),
            ]
        );
    }

    #[test]
    fn encoders_round_trip_every_keycode_with_an_action() {
        for keycode in (0..=u16::MAX).step_by(7) {
            let Some(action) = vial_keycode_action(keycode) else {
                assert!(encoder_action([keycode, KC_A]).is_err());
                continue;
            };
            let encoder = encoder_action([keycode, KC_A]).unwrap();
            assert!(identical(encoder.counter_clockwise, action));
            assert_eq!(encoder.clockwise, key(HidKeyCode::A));
            let ([ccw, cw], losses) = encoder_keycodes(encoder);
            assert!(losses.is_empty());
            assert_eq!(vial_keycode_action(ccw), Some(action));
            assert_eq!(cw, KC_A);
        }
        let user = KeyAction::Single(Action::User(90));
        assert_eq!(
            encoder_keycodes(EncoderAction::new(user, KeyAction::No)),
            ([0, 0], vec![DynamicLoss::Key(user)])
        );
        // An encoder nobody set, as the firmware keeps it, loses nothing.
        let unset = key(HidKeyCode::No);
        assert_eq!(
            encoder_keycodes(EncoderAction::new(unset, unset)),
            ([0, 0], vec![])
        );
    }

    #[test]
    fn every_read_loss_reads_in_both_languages() {
        assert_eq!(morse_steps(TAP), "·");
        assert_eq!(morse_steps(HOLD), "—");
        assert_eq!(morse_steps(DOUBLE_TAP), "··");
        assert_eq!(morse_steps(HOLD_AFTER_TAP), "·—");
        let user = KeyAction::Single(Action::User(90));
        let losses = [
            (DynamicSlot::Combo, 0, DynamicLoss::Key(user)),
            (DynamicSlot::Combo, 1, DynamicLoss::ComboKeys(6)),
            (
                DynamicSlot::TapDance,
                2,
                DynamicLoss::MorsePattern(MorsePattern::from_u16(0b1001)),
            ),
            (
                DynamicSlot::KeyOverride,
                3,
                DynamicLoss::NegativeOutput(user),
            ),
            (DynamicSlot::KeyOverride, 4, DynamicLoss::StateMatch),
            (DynamicSlot::KeyOverride, 5, DynamicLoss::Bindable),
            (DynamicSlot::KeyOverride, 6, DynamicLoss::Layers(1 << 20)),
            (DynamicSlot::Encoder { layer: 1 }, 0, DynamicLoss::Key(user)),
        ];
        let english = describe_dynamic_losses(crate::i18n::Language::English, &losses);
        let russian = describe_dynamic_losses(crate::i18n::Language::Russian, &losses);
        for message in [&english, &russian] {
            assert_eq!(message.lines().count(), losses.len() + 1, "{message}");
            assert!(
                !message.contains('{') && !message.contains("rynk_read."),
                "{message}"
            );
        }
        assert!(english.contains("pattern ··— "), "{english}");
        assert_ne!(english, russian);
    }
}
