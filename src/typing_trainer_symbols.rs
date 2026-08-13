use std::collections::{BTreeMap, BTreeSet};

use crate::keyboard::{KeyBinding, KeyboardLayout};

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

pub(crate) fn printable_symbols_from_layout(layout: &KeyboardLayout) -> Vec<char> {
    let mut symbols = BTreeSet::new();

    for binding in layout.layers.iter().flatten() {
        let KeyBinding::Vial(keycode) = binding else {
            continue;
        };
        let Some((direct, shifted)) = direct_and_shifted_symbols(*keycode) else {
            continue;
        };
        symbols.insert(direct);
        if let Some(shifted) = shifted {
            symbols.insert(shifted);
        }
    }

    symbols.into_iter().collect()
}

fn direct_and_shifted_symbols(keycode: u16) -> Option<(char, Option<char>)> {
    let letter = (0x0004..=0x001d).contains(&keycode).then(|| {
        let direct = char::from_u32(u32::from(b'a') + u32::from(keycode - 0x0004))
            .expect("Vial letter keycode maps to ASCII");
        (direct, Some(direct.to_ascii_uppercase()))
    });
    if letter.is_some() {
        return letter;
    }

    match keycode {
        0x001e => Some(('1', Some('!'))),
        0x001f => Some(('2', Some('@'))),
        0x0020 => Some(('3', Some('#'))),
        0x0021 => Some(('4', Some('$'))),
        0x0022 => Some(('5', Some('%'))),
        0x0023 => Some(('6', Some('^'))),
        0x0024 => Some(('7', Some('&'))),
        0x0025 => Some(('8', Some('*'))),
        0x0026 => Some(('9', Some('('))),
        0x0027 => Some(('0', Some(')'))),
        0x002d => Some(('-', Some('_'))),
        0x002e => Some(('=', Some('+'))),
        0x002f => Some(('[', Some('{'))),
        0x0030 => Some((']', Some('}'))),
        0x0031 => Some(('\\', Some('|'))),
        0x0033 => Some((';', Some(':'))),
        0x0034 => Some(('\'', Some('"'))),
        0x0035 => Some(('`', Some('~'))),
        0x0036 => Some((',', Some('<'))),
        0x0037 => Some(('.', Some('>'))),
        0x0038 => Some(('/', Some('?'))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firmware::FirmwareProtocol;
    use crate::keyboard::{KeyboardLayout, PhysicalKey};
    use std::collections::{BTreeMap, BTreeSet};

    fn test_layout(layers: Vec<Vec<u16>>) -> KeyboardLayout {
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
            layers: layers
                .into_iter()
                .map(|layer| layer.into_iter().map(Into::into).collect())
                .collect(),
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

    #[test]
    fn printable_symbols_collect_direct_and_shifted_characters_from_all_layers() {
        let layout = test_layout(vec![
            vec![0x001e, 0x0004, 0x0038, 0x0000],
            vec![0x0001, 0x0028, 0x0004, 0x001e],
        ]);

        assert_eq!(
            printable_symbols_from_layout(&layout),
            vec!['!', '/', '1', '?', 'A', 'a']
        );
    }

    #[test]
    fn printable_symbols_deduplicates_keycodes_across_layers() {
        let layout = test_layout(vec![vec![0x0004], vec![0x0004], vec![0x0004]]);

        assert_eq!(printable_symbols_from_layout(&layout), vec!['A', 'a']);
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
