use std::collections::{BTreeMap, BTreeSet};

use crate::keyboard::KeyboardLayout;
use crate::keycode::KeyOutputLayout;

pub(crate) const TYPING_TRAINER_SYMBOL_COUNTS: [usize; 3] = [25, 50, 100];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TypingTrainerCharacterStats {
    pub(crate) attempts: u64,
    pub(crate) errors: u64,
}

pub(crate) type TypingTrainerCharacterStatsMap = BTreeMap<char, TypingTrainerCharacterStats>;

pub(crate) fn record_symbol_attempt(
    stats: &mut TypingTrainerCharacterStatsMap,
    expected: char,
    was_error: bool,
) {
    let entry = stats.entry(expected).or_default();
    entry.attempts += 1;
    if was_error {
        entry.errors += 1;
    }
}

pub(crate) fn weighted_symbol_text(
    symbols: &[char],
    count: usize,
    stats: &TypingTrainerCharacterStatsMap,
    seed: usize,
) -> String {
    if symbols.is_empty() || count == 0 {
        return String::new();
    }

    let weights: Vec<u64> = symbols
        .iter()
        .map(|symbol| {
            let character_stats = stats.get(symbol).copied().unwrap_or_default();
            100 + 100 * character_stats.errors / character_stats.attempts.max(1)
        })
        .collect();
    let total_weight: u64 = weights.iter().sum();
    let mut random = SymbolRandom::new(seed);
    let mut text = String::with_capacity(count);

    for _ in 0..count {
        let mut target = random.next_u64() % total_weight;
        let index = weights
            .iter()
            .position(|weight| {
                if target < *weight {
                    true
                } else {
                    target -= *weight;
                    false
                }
            })
            .expect("symbol weights are nonzero");
        text.push(symbols[index]);
    }

    if symbols.len() > 1
        && text
            .chars()
            .all(|symbol| symbol == text.chars().next().unwrap())
    {
        text.pop();
        text.push(symbols[1]);
    }

    text
}

struct SymbolRandom(u64);

impl SymbolRandom {
    fn new(seed: usize) -> Self {
        Self((seed as u64).wrapping_add(0x9e37_79b9_7f4a_7c15))
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Collects every character the loaded layout can type in the given input
/// layout, across all layers and both direct and Shift outputs.
pub(crate) fn printable_symbols_from_layout(
    layout: &KeyboardLayout,
    key_output_layout: KeyOutputLayout,
) -> Vec<char> {
    let mut symbols = BTreeSet::new();

    for binding in layout.layers.iter().flatten() {
        let Some((direct, shifted)) =
            super::key_binding_printable_output(*binding, key_output_layout)
        else {
            continue;
        };
        symbols.insert(direct);
        if let Some(shifted) = shifted {
            symbols.insert(shifted);
        }
    }

    symbols.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::FirmwareProtocol;
    use crate::keyboard::{KeyBinding, KeyboardLayout, PhysicalKey};
    use std::collections::{BTreeMap, BTreeSet};

    fn test_layout(layers: Vec<Vec<KeyBinding>>) -> KeyboardLayout {
        let cols = layers.first().map(Vec::len).unwrap_or(0);
        KeyboardLayout {
            name: "test".to_owned(),
            rows: 1,
            cols,
            keys: (0..cols)
                .map(|col| PhysicalKey {
                    x: col as f32,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    row: 0,
                    col: col as u8,
                    label: format!("0,{col}"),
                    rotation: 0.0,
                    rotation_x: 0.0,
                    rotation_y: 0.0,
                    layout_condition: None,
                })
                .collect(),
            encoders: vec![],
            layers,
            encoder_layers: vec![],
            layer_names: vec![],
            custom_keycodes: vec![],
            layout_options: vec![],
            live_features: Default::default(),
            supports_rgb: false,
            lighting_mode: None,
            firmware: FirmwareProtocol::Vial,
        }
    }

    fn vial_layers(layers: Vec<Vec<u16>>) -> Vec<Vec<KeyBinding>> {
        layers
            .into_iter()
            .map(|layer| layer.into_iter().map(Into::into).collect())
            .collect()
    }

    #[test]
    fn printable_symbols_collect_direct_and_shifted_characters_from_all_layers() {
        let layout = test_layout(vial_layers(vec![
            vec![0x001e, 0x0004, 0x0038, 0x0000],
            vec![0x0001, 0x0028, 0x0004, 0x001e],
        ]));

        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::English),
            vec!['!', '/', '1', '?', 'A', 'a']
        );
    }

    #[test]
    fn printable_symbols_follow_the_russian_input_mapping() {
        let layout = test_layout(vial_layers(vec![
            vec![0x0004, 0x0038],
            vec![0x002f, 0x0220],
        ]));

        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::Russian),
            vec![',', '.', 'Ф', 'Х', 'ф', 'х', '№']
        );
    }

    #[test]
    fn printable_symbols_include_native_universal_symbols() {
        let layout = test_layout(vec![vec![
            crate::universal_symbols::binding(crate::universal_symbols::USER_SYMBOL_START),
            crate::universal_symbols::binding(
                crate::universal_symbols::USER_RUSSIAN_LETTER_START + 1,
            ),
            crate::universal_symbols::binding(crate::universal_symbols::USER_TOGGLE),
        ]]);

        // Punctuation is typed in both layouts; the Russian letter only while
        // the Russian layout is active.
        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::English),
            vec!['.']
        );
        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::Russian),
            vec!['.', 'Б', 'б']
        );
    }

    #[test]
    fn printable_symbols_include_the_tap_side_of_tap_hold_keys() {
        use rmk_types::action::{Action, KeyAction};
        use rmk_types::keycode::{HidKeyCode, KeyCode};
        use rmk_types::modifier::ModifierCombination;

        let layout = test_layout(vec![vec![
            // LCTL_T(KC_A) in Vial form.
            KeyBinding::Vial(0x2104),
            // Native tap-hold whose tap types a Universal Symbol.
            KeyBinding::Rmk(KeyAction::TapHold(
                Action::User(crate::universal_symbols::USER_SYMBOL_START + 4),
                Action::Modifier(ModifierCombination::LSHIFT),
                Default::default(),
            )),
            KeyBinding::Rmk(KeyAction::Tap(Action::Key(KeyCode::Hid(HidKeyCode::B)))),
        ]]);

        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::English),
            vec!['!', 'A', 'B', 'a', 'b']
        );
    }

    #[test]
    fn printable_symbols_deduplicates_keycodes_across_layers() {
        let layout = test_layout(vial_layers(vec![vec![0x0004], vec![0x0004], vec![0x0004]]));

        assert_eq!(
            printable_symbols_from_layout(&layout, KeyOutputLayout::English),
            vec!['A', 'a']
        );
    }

    #[test]
    fn weighted_symbol_text_handles_empty_and_single_symbol_pools() {
        let stats = BTreeMap::new();

        assert_eq!(weighted_symbol_text(&['a', 'b', 'c'], 0, &stats, 9), "");
        assert_eq!(weighted_symbol_text(&['x'], 4, &stats, 9), "xxxx");
    }

    #[test]
    fn weighted_symbol_text_keeps_multiple_symbols_in_a_multi_symbol_pool() {
        let text = weighted_symbol_text(&['a', 'b', 'c'], 24, &BTreeMap::new(), 9);

        assert!(text.chars().collect::<BTreeSet<_>>().len() > 1);
    }

    #[test]
    fn every_symbol_a_layout_can_produce_is_accepted_by_the_trainer() {
        let mut bindings: Vec<KeyBinding> = (0x0000_u16..=0x00ff).map(Into::into).collect();
        bindings.extend((0..crate::universal_symbols::SYMBOLS.len()).map(|offset| {
            crate::universal_symbols::binding(
                crate::universal_symbols::USER_SYMBOL_START + offset as u8,
            )
        }));
        bindings.extend(
            (0..crate::universal_symbols::RUSSIAN_LETTERS.len()).map(|offset| {
                crate::universal_symbols::binding(
                    crate::universal_symbols::USER_RUSSIAN_LETTER_START + offset as u8,
                )
            }),
        );
        let layout = test_layout(vec![bindings]);

        for key_output_layout in [KeyOutputLayout::English, KeyOutputLayout::Russian] {
            for symbol in printable_symbols_from_layout(&layout, key_output_layout) {
                assert!(
                    crate::app::app_state::typing_trainer_accepts_char(symbol),
                    "{symbol:?} ({key_output_layout:?}) can be drawn into the pool but the \
                     trainer refuses it as input"
                );
            }
        }
    }

    #[test]
    fn weighted_symbol_text_prioritizes_characters_with_more_errors() {
        let mut stats = BTreeMap::new();
        for _ in 0..10 {
            record_symbol_attempt(&mut stats, 'b', true);
        }

        let text = weighted_symbol_text(&['a', 'b', 'c'], 3_000, &stats, 9);
        let b_count = text.chars().filter(|ch| *ch == 'b').count();
        let a_count = text.chars().filter(|ch| *ch == 'a').count();
        let c_count = text.chars().filter(|ch| *ch == 'c').count();

        assert!(b_count > a_count);
        assert!(b_count > c_count);
    }

    #[test]
    fn record_symbol_attempt_counts_attempts_and_only_error_outcomes() {
        let mut stats = BTreeMap::new();

        record_symbol_attempt(&mut stats, '!', false);
        record_symbol_attempt(&mut stats, '!', true);

        assert_eq!(
            stats.get(&'!'),
            Some(&TypingTrainerCharacterStats {
                attempts: 2,
                errors: 1,
            })
        );
    }
}
