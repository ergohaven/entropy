//! Framed host-text transport over keycodes available to ordinary Vial/QMK macros.
//!
//! GUI+F20 starts a frame. F13..F20 then carry octal digits: two digits for
//! Unicode scalar count, followed by seven digits per scalar. Host Smart Input
//! backends consume the frame and emit reconstructed text.

use std::time::{Duration, Instant};

pub const KC_F13: u16 = 0x0068;
pub const KC_F20: u16 = KC_F13 + 7;
pub const MOD_GUI: u16 = 0x0800;
pub const START_TRIGGER_KEYCODE: u16 = MOD_GUI | KC_F20;

const COUNT_DIGITS: usize = 2;
const CODEPOINT_DIGITS: usize = 7;
const MAX_CODEPOINTS: usize = 0o77;
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportOutcome {
    PassThrough,
    Started,
    Consumed,
    Complete(String),
}

#[derive(Clone, Debug)]
enum DecodeState {
    Idle,
    Count {
        digits: usize,
        value: u32,
    },
    Codepoints {
        remaining: usize,
        digits: usize,
        value: u32,
        output: String,
    },
}

#[derive(Clone, Debug)]
pub struct HostTextTransportDecoder {
    state: DecodeState,
    last_event_at: Option<Instant>,
    consume_keyup_for: Option<u16>,
}

impl Default for HostTextTransportDecoder {
    fn default() -> Self {
        Self {
            state: DecodeState::Idle,
            last_event_at: None,
            consume_keyup_for: None,
        }
    }
}

impl HostTextTransportDecoder {
    pub fn handle(
        &mut self,
        trigger_keycode: u16,
        pressed: bool,
        now: Instant,
    ) -> TransportOutcome {
        if self.timed_out(now) {
            self.reset();
        }

        let base_keycode = trigger_keycode & 0x00ff;
        if !pressed && self.consume_keyup_for == Some(base_keycode) {
            self.consume_keyup_for = None;
            return TransportOutcome::Consumed;
        }

        if matches!(self.state, DecodeState::Idle) {
            if base_keycode != KC_F20 || trigger_keycode & MOD_GUI == 0 {
                return TransportOutcome::PassThrough;
            }
            if !pressed {
                return TransportOutcome::Consumed;
            }
            self.state = DecodeState::Count {
                digits: 0,
                value: 0,
            };
            self.last_event_at = Some(now);
            return TransportOutcome::Started;
        }

        if !(KC_F13..=KC_F20).contains(&base_keycode) {
            self.reset();
            return TransportOutcome::PassThrough;
        }

        self.last_event_at = Some(now);
        if !pressed {
            return TransportOutcome::Consumed;
        }

        let digit = u32::from(base_keycode - KC_F13);
        let outcome = match &mut self.state {
            DecodeState::Idle => TransportOutcome::PassThrough,
            DecodeState::Count { digits, value } => {
                *value = (*value << 3) | digit;
                *digits += 1;
                if *digits != COUNT_DIGITS {
                    TransportOutcome::Consumed
                } else if *value == 0 || *value as usize > MAX_CODEPOINTS {
                    self.reset();
                    TransportOutcome::Consumed
                } else {
                    self.state = DecodeState::Codepoints {
                        remaining: *value as usize,
                        digits: 0,
                        value: 0,
                        output: String::new(),
                    };
                    TransportOutcome::Consumed
                }
            }
            DecodeState::Codepoints {
                remaining,
                digits,
                value,
                output,
            } => {
                *value = (*value << 3) | digit;
                *digits += 1;
                if *digits != CODEPOINT_DIGITS {
                    TransportOutcome::Consumed
                } else if let Some(ch) = char::from_u32(*value) {
                    output.push(ch);
                    *remaining -= 1;
                    if *remaining == 0 {
                        let completed = std::mem::take(output);
                        self.state = DecodeState::Idle;
                        self.last_event_at = None;
                        TransportOutcome::Complete(completed)
                    } else {
                        *digits = 0;
                        *value = 0;
                        TransportOutcome::Consumed
                    }
                } else {
                    self.reset();
                    TransportOutcome::Consumed
                }
            }
        };

        self.consume_keyup_for = Some(base_keycode);
        outcome
    }

    fn timed_out(&self, now: Instant) -> bool {
        !matches!(self.state, DecodeState::Idle)
            && self
                .last_event_at
                .is_some_and(|last| now.saturating_duration_since(last) > TRANSPORT_TIMEOUT)
    }

    fn reset(&mut self) {
        self.state = DecodeState::Idle;
        self.last_event_at = None;
        self.consume_keyup_for = None;
    }
}

pub fn encode_text_payload(text: &str) -> Option<Vec<u16>> {
    let codepoints = text.chars().map(u32::from).collect::<Vec<_>>();
    if codepoints.is_empty() || codepoints.len() > MAX_CODEPOINTS {
        return None;
    }

    let mut payload = Vec::with_capacity(COUNT_DIGITS + codepoints.len() * CODEPOINT_DIGITS);
    push_octal_digits(&mut payload, codepoints.len() as u32, COUNT_DIGITS);
    for codepoint in codepoints {
        push_octal_digits(&mut payload, codepoint, CODEPOINT_DIGITS);
    }
    Some(payload)
}

fn push_octal_digits(payload: &mut Vec<u16>, value: u32, digit_count: usize) {
    for shift in (0..digit_count).rev() {
        let digit = ((value >> (shift * 3)) & 0x7) as u16;
        payload.push(KC_F13 + digit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(text: &str) -> String {
        let start = Instant::now();
        let mut decoder = HostTextTransportDecoder::default();
        assert_eq!(
            decoder.handle(START_TRIGGER_KEYCODE, true, start),
            TransportOutcome::Started
        );
        assert_eq!(
            decoder.handle(START_TRIGGER_KEYCODE, false, start),
            TransportOutcome::Consumed
        );

        let mut completed = None;
        for keycode in encode_text_payload(text).unwrap() {
            let outcome = decoder.handle(keycode, true, start);
            if let TransportOutcome::Complete(value) = outcome {
                completed = Some(value);
            } else {
                assert_eq!(outcome, TransportOutcome::Consumed);
            }
            assert_eq!(
                decoder.handle(keycode, false, start),
                TransportOutcome::Consumed
            );
        }
        completed.expect("transport should complete")
    }

    #[test]
    fn round_trips_single_and_multi_codepoint_emoji() {
        for emoji in ["😀", "👍🏽", "👨‍👩‍👧‍👦", "🏳️‍🌈"] {
            assert_eq!(decode(emoji), emoji);
        }
    }

    #[test]
    fn ignores_held_modifiers_during_framed_text() {
        let start = Instant::now();
        let mut decoder = HostTextTransportDecoder::default();
        assert_eq!(
            decoder.handle(START_TRIGGER_KEYCODE | 0x0300, true, start),
            TransportOutcome::Started
        );

        let mut completed = None;
        for keycode in encode_text_payload("👍🏽").unwrap() {
            let outcome = decoder.handle(keycode | 0x0300, true, start);
            if let TransportOutcome::Complete(value) = outcome {
                completed = Some(value);
            }
        }
        assert_eq!(completed.as_deref(), Some("👍🏽"));
    }

    #[test]
    fn ignores_unrelated_transport_chords_while_idle() {
        let mut decoder = HostTextTransportDecoder::default();
        assert_eq!(
            decoder.handle(KC_F13, true, Instant::now()),
            TransportOutcome::PassThrough
        );
    }

    #[test]
    fn timed_out_payload_returns_to_regular_symbol_handling() {
        let start = Instant::now();
        let mut decoder = HostTextTransportDecoder::default();
        assert_eq!(
            decoder.handle(START_TRIGGER_KEYCODE, true, start),
            TransportOutcome::Started
        );
        assert_eq!(
            decoder.handle(
                KC_F13,
                true,
                start + TRANSPORT_TIMEOUT + Duration::from_millis(1)
            ),
            TransportOutcome::PassThrough
        );
    }

    #[test]
    fn rejects_empty_or_oversized_payloads() {
        assert_eq!(encode_text_payload(""), None);
        assert_eq!(encode_text_payload(&"x".repeat(MAX_CODEPOINTS + 1)), None);
    }

    #[test]
    fn linux_backends_keep_transport_shape_in_sync() {
        let ibus = include_str!("../linux/ibus/entropy-ibus-engine");
        let fcitx = include_str!("../linux/fcitx5/src/entropyuniversalsymbols.cpp");

        for source in [ibus, fcitx] {
            assert!(source.contains("HOST_TEXT_START_TRIGGER"));
            assert!(source.contains("HOST_TEXT_COUNT_DIGITS"));
            assert!(source.contains("HOST_TEXT_CODEPOINT_DIGITS"));
        }
        assert!(ibus.contains("HOST_TEXT_COUNT_DIGITS = 2"));
        assert!(ibus.contains("HOST_TEXT_CODEPOINT_DIGITS = 7"));
        assert!(fcitx.contains("HOST_TEXT_COUNT_DIGITS = 2"));
        assert!(fcitx.contains("HOST_TEXT_CODEPOINT_DIGITS = 7"));
    }
}
