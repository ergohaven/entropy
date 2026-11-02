//! Macros: the editor keeps each macro in Vial's bytes, a Rynk board takes
//! `MacroOp`s.
//!
//! The two codecs are the firmware's Vial ones (`rmk/src/keyboard/macros.rs`),
//! so a macro means on a Rynk board what it meant on a Vial RMK board. Where the
//! firmware drops a byte or an op without a word, these report it.

use rmk_types_rynk::action::{Action, KeyAction};
use rmk_types_rynk::keyboard_macros::MacroOp;

use super::keycode::{exact_vial_keycode, vial_keycode_action};

/// The longest delay one Vial delay op holds: two 1-based bytes.
const VIAL_DELAY_MAX: u16 = 254 * 255 + 254;

/// What a macro lost between the two encodings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MacroLoss {
    /// A byte the firmware types nothing for: a control or non-ASCII character.
    Byte(u8),
    /// An `01 xx` pair of a kind the firmware does not know.
    UnknownOp(u8),
    /// Bytes cut off in the middle of an op.
    Truncated(Vec<u8>),
    /// A key op whose keycode has no action, or whose action is not one a
    /// macro step can take.
    Keycode(u16),
    /// An op read from the board that Vial bytes cannot spell.
    Op(MacroOp),
}

/// The ops Vial `bytes` run on the firmware, and what did not make it.
pub(crate) fn vial_macro_ops(bytes: &[u8]) -> (Vec<MacroOp>, Vec<MacroLoss>) {
    let (mut ops, mut losses) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != 0x01 {
            // The bytes `from_ascii` types: printable ASCII and a few controls.
            let c = bytes[i];
            if matches!(c, 0x20..=0x7E | b'\t' | b'\n' | 0x08 | 0x1B | 0x7F) {
                ops.push(MacroOp::Char(c));
            } else {
                losses.push(MacroLoss::Byte(c));
            }
            i += 1;
            continue;
        }
        let op_len = match bytes.get(i + 1) {
            Some(1..=3) => 3,
            Some(4..=7) => 4,
            Some(_) => 2,
            None => 2,
        };
        let Some(op) = bytes.get(i..i + op_len) else {
            // The firmware stops decoding at a truncated op.
            losses.push(MacroLoss::Truncated(bytes[i..].to_vec()));
            break;
        };
        match op[1] {
            kind @ 1..=3 => push_key_op(&mut ops, &mut losses, kind, op[2] as u16),
            4 => {
                let ms = (op[2].max(1) - 1) as u16 + (op[3].max(1) - 1) as u16 * 255;
                ops.push(MacroOp::Delay(ms));
            }
            kind @ 5..=7 => {
                let raw = u16::from_le_bytes([op[2], op[3]]);
                let keycode = if raw & 0xFF00 == 0xFF00 {
                    (raw & 0xFF) << 8
                } else {
                    raw
                };
                push_key_op(&mut ops, &mut losses, kind - 4, keycode);
            }
            kind => losses.push(MacroLoss::UnknownOp(kind)),
        }
        i += op_len;
    }
    (ops, losses)
}

fn push_key_op(ops: &mut Vec<MacroOp>, losses: &mut Vec<MacroLoss>, kind: u8, keycode: u16) {
    let action = match vial_keycode_action(keycode) {
        Some(KeyAction::Single(action) | KeyAction::Tap(action)) if action != Action::No => action,
        _ => return losses.push(MacroLoss::Keycode(keycode)),
    };
    ops.push(match kind {
        1 => MacroOp::Tap(action),
        2 => MacroOp::Press(action),
        _ => MacroOp::Release(action),
    });
}

/// Vial's bytes for `ops` read from a Rynk board, and the ops they cannot
/// spell. Those bytes run the same ops when written back, less the losses.
pub(crate) fn macro_vial_bytes(ops: &[MacroOp]) -> (Vec<u8>, Vec<MacroLoss>) {
    let (mut bytes, mut losses) = (Vec::new(), Vec::new());
    for &op in ops {
        match op {
            MacroOp::Tap(action) => render_key(&mut bytes, &mut losses, op, 1, action),
            MacroOp::Press(action) => render_key(&mut bytes, &mut losses, op, 2, action),
            MacroOp::Release(action) => render_key(&mut bytes, &mut losses, op, 3, action),
            MacroOp::Delay(mut ms) => loop {
                let chunk = ms.min(VIAL_DELAY_MAX);
                bytes.extend_from_slice(&[
                    0x01,
                    0x04,
                    (chunk % 255) as u8 + 1,
                    (chunk / 255) as u8 + 1,
                ]);
                ms -= chunk;
                if ms == 0 {
                    break;
                }
            },
            MacroOp::Char(c) if matches!(c, 0x20..=0x7E | b'\t' | b'\n' | 0x08 | 0x1B | 0x7F) => {
                bytes.push(c)
            }
            MacroOp::Char(_) | MacroOp::PauseForRelease => losses.push(MacroLoss::Op(op)),
        }
    }
    (bytes, losses)
}

/// `01 kind kc` for a one-byte keycode, `01 kind+4 lo hi` for the rest, with
/// Vial's escape of a zero low byte so the payload never holds a terminator.
fn render_key(
    bytes: &mut Vec<u8>,
    losses: &mut Vec<MacroLoss>,
    op: MacroOp,
    kind: u8,
    action: Action,
) {
    let Some(keycode) = exact_vial_keycode(KeyAction::Single(action)).filter(|&kc| kc != 0) else {
        return losses.push(MacroLoss::Op(op));
    };
    if keycode < 0x100 {
        bytes.extend_from_slice(&[0x01, kind, keycode as u8]);
    } else {
        let word = if keycode & 0xFF == 0 {
            0xFF00 | (keycode >> 8)
        } else {
            keycode
        };
        let [lo, hi] = word.to_le_bytes();
        bytes.extend_from_slice(&[0x01, kind + 4, lo, hi]);
    }
}

/// Bytes `ops` take in the firmware's macro buffer, behind their length.
pub(crate) fn macro_slot_size(ops: &[MacroOp]) -> usize {
    let len: usize = ops
        .iter()
        .map(|op| postcard::to_slice(op, &mut [0u8; 16]).map_or(0, |bytes| bytes.len()))
        .sum();
    varint_len(len) + len
}

/// Bytes `macros` take of a buffer with `slots` macro slots: every slot costs
/// its length prefix, an empty one a single byte. A board refuses a `SetMacro`
/// that would pass its `macro_space_size`.
pub(crate) fn macro_space_used<'a>(
    macros: impl IntoIterator<Item = &'a [MacroOp]>,
    slots: usize,
) -> usize {
    let mut used = 0;
    let mut count = 0;
    for ops in macros.into_iter().take(slots) {
        used += macro_slot_size(ops);
        count += 1;
    }
    used + slots.saturating_sub(count)
}

/// Bytes the editor's Vial `macros` take of a Rynk board's pool once they are
/// ops: a typed character costs two there, one in Vial's buffer.
pub(crate) fn vial_macros_space_used(macros: &[Vec<u8>], slots: usize) -> usize {
    let ops: Vec<Vec<MacroOp>> = macros.iter().map(|bytes| vial_macro_ops(bytes).0).collect();
    macro_space_used(ops.iter().map(Vec::as_slice), slots)
}

fn varint_len(value: usize) -> usize {
    match value {
        0..=0x7F => 1,
        0x80..=0x3FFF => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmk_types_rynk::keyboard_macros::Macro;
    use rmk_types_rynk::keycode::{HidKeyCode, KeyCode};
    use rmk_types_rynk::modifier::ModifierCombination;

    fn key(code: HidKeyCode) -> Action {
        Action::Key(KeyCode::Hid(code))
    }

    #[test]
    fn every_op_shape_round_trips_as_the_firmware_spells_it() {
        let ops = [
            MacroOp::Tap(key(HidKeyCode::A)),
            MacroOp::Press(key(HidKeyCode::LShift)),
            MacroOp::Release(key(HidKeyCode::LShift)),
            MacroOp::Delay(100),
            MacroOp::Char(b'x'),
            MacroOp::Tap(Action::KeyWithModifier(
                HidKeyCode::A,
                ModifierCombination::LCTRL,
            )),
            // 0x7E00: the zero low byte is escaped.
            MacroOp::Tap(Action::User(0)),
            MacroOp::Press(Action::PersistentDefaultLayer(1)),
        ];
        let bytes = [
            0x01, 0x01, 0x04, // tap A
            0x01, 0x02, 0xE1, // press LShift
            0x01, 0x03, 0xE1, // release LShift
            0x01, 0x04, 0x65, 0x01, // 100 ms
            b'x', // a character is its own byte
            0x01, 0x05, 0x04, 0x01, // WM(A, LCtrl) = 0x0104, little-endian
            0x01, 0x05, 0x7E, 0xFF, // User(0) = 0x7E00 escaped
            0x01, 0x06, 0xE1, 0x52, // PDF(1) = 0x52E1
        ];
        assert_eq!(macro_vial_bytes(&ops), (bytes.to_vec(), vec![]));
        assert_eq!(vial_macro_ops(&bytes), (ops.to_vec(), vec![]));
    }

    /// The editor's extended ops 5/6/7 carry 16-bit keycodes: every keycode a
    /// macro step can hold crosses both ways unchanged.
    #[test]
    fn extended_ops_carry_every_sixteen_bit_keycode() {
        let mut crossed = 0;
        for keycode in 0x0100..=u16::MAX {
            let Some(KeyAction::Single(action)) = vial_keycode_action(keycode) else {
                continue;
            };
            if exact_vial_keycode(KeyAction::Single(action)) != Some(keycode) {
                continue;
            }
            for (kind, op) in [
                (5, MacroOp::Tap(action)),
                (6, MacroOp::Press(action)),
                (7, MacroOp::Release(action)),
            ] {
                let word = if keycode & 0xFF == 0 {
                    0xFF00 | (keycode >> 8)
                } else {
                    keycode
                };
                let [lo, hi] = word.to_le_bytes();
                let bytes = vec![0x01, kind, lo, hi];
                assert_eq!(vial_macro_ops(&bytes), (vec![op], vec![]), "{keycode:#06x}");
                assert_eq!(macro_vial_bytes(&[op]), (bytes, vec![]), "{keycode:#06x}");
            }
            crossed += 1;
        }
        assert!(crossed > 5_000, "{crossed}");
    }

    #[test]
    fn delays_split_at_the_encoding_limit_and_round_trip() {
        for ms in [0, 1, 254, 255, 65024, 65025, u16::MAX] {
            let (bytes, losses) = macro_vial_bytes(&[MacroOp::Delay(ms)]);
            assert!(losses.is_empty());
            let (ops, losses) = vial_macro_ops(&bytes);
            assert!(losses.is_empty());
            let total: u32 = ops
                .iter()
                .map(|op| match op {
                    MacroOp::Delay(ms) => *ms as u32,
                    _ => panic!("not a delay"),
                })
                .sum();
            assert_eq!(total, ms as u32, "{ms}ms");
            assert!(ops.len() <= 2);
        }
    }

    #[test]
    fn what_the_firmware_drops_is_reported() {
        assert_eq!(
            vial_macro_ops(&[
                0x01, 0x05, 0x02, 0x77, 0x01, 0x01, 0x00, 0x07, b'a', 0xD0, 0x01, 0x09, b'b'
            ]),
            (
                vec![
                    MacroOp::Tap(Action::TriggerMacro(2)),
                    MacroOp::Char(b'a'),
                    MacroOp::Char(b'b')
                ],
                vec![
                    MacroLoss::Keycode(0x0000),
                    MacroLoss::Byte(0x07),
                    MacroLoss::Byte(0xD0),
                    MacroLoss::UnknownOp(0x09),
                ]
            )
        );
        // A layer tap is a key, not a step.
        assert_eq!(
            vial_macro_ops(&[0x01, 0x05, 0x04, 0x41]),
            (vec![], vec![MacroLoss::Keycode(0x4104)])
        );
        assert_eq!(
            vial_macro_ops(&[b'a', 0x01, 0x04, 0x01]),
            (
                vec![MacroOp::Char(b'a')],
                vec![MacroLoss::Truncated(vec![0x01, 0x04, 0x01])]
            )
        );
        assert_eq!(
            vial_macro_ops(&[0x01]),
            (vec![], vec![MacroLoss::Truncated(vec![0x01])])
        );

        let pause = MacroOp::PauseForRelease;
        let user = MacroOp::Tap(Action::User(100));
        let unicode = MacroOp::Char(0xD0);
        assert_eq!(
            macro_vial_bytes(&[pause, user, unicode, MacroOp::Tap(key(HidKeyCode::B))]),
            (
                vec![0x01, 0x01, 0x05],
                vec![
                    MacroLoss::Op(pause),
                    MacroLoss::Op(user),
                    MacroLoss::Op(unicode)
                ]
            )
        );
    }

    #[test]
    fn slot_sizes_are_the_postcard_bytes_the_board_stores() {
        let text: Vec<MacroOp> = (0..2016).map(|_| MacroOp::Char(b'a')).collect();
        let step = [MacroOp::Tap(Action::KeyWithModifier(
            HidKeyCode::A,
            ModifierCombination::LCTRL,
        ))];
        for ops in [&[][..], &step, &text[..63], &text[..64], &text] {
            let mut buf = [0u8; 8200];
            let encoded = postcard::to_slice(&Macro::from_slice(ops).unwrap(), &mut buf).unwrap();
            // `Macro` serializes as its op count; the buffer keeps the byte length.
            let op_bytes = encoded.len() - varint_len(ops.len());
            assert_eq!(macro_slot_size(ops), varint_len(op_bytes) + op_bytes);
        }
        // Spec §2.7: the worst Vial pool, 32 slots of text filling 2048 bytes,
        // takes 2 × 2016 + 32 × 2 = 4096 bytes on Rynk.
        let mut slots: Vec<&[MacroOp]> = vec![&text[..63]; 32];
        slots[0] = &text[..2016 - 31 * 63];
        assert_eq!(slots.iter().map(|ops| ops.len() + 1).sum::<usize>(), 2048);
        assert!(macro_space_used(slots.iter().copied(), 32) <= 4096);
        assert!(macro_space_used([&text[..]], 32) <= 4096);
        assert_eq!(
            macro_space_used([&step[..]], 32),
            macro_slot_size(&step) + 31
        );
    }
}
