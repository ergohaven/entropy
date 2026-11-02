//! Golden HID request streams. They pin the exact bytes Entropy sends over
//! Vial, so a refactor above the transport cannot change the wire unnoticed.
//! Record or refresh them with `ENTROPY_UPDATE_GOLDEN=1 cargo test`.

use std::path::PathBuf;

const UPDATE_ENV: &str = "ENTROPY_UPDATE_GOLDEN";

/// Scripts an RMK keyboard that remembers what it is told: VIA keycodes,
/// native key and dynamic actions, combo layers and QMK settings read back
/// exactly as written. Everything else falls back to the built-in replies.
pub(crate) fn fake_rmk_keyboard(recorder: &crate::hid::TestHidRecorder) {
    use std::collections::HashMap;
    let mut keycodes = HashMap::<[u8; 3], [u8; 2]>::new();
    let mut native = HashMap::<[u8; 4], Vec<u8>>::new();
    let mut combo_layers = HashMap::<u8, [u8; 2]>::new();
    let mut settings = HashMap::<[u8; 2], Vec<u8>>::new();
    let mut dynamic_entries = HashMap::<[u8; 2], Vec<u8>>::new();
    recorder.respond_by(move |request| {
        let mut reply = [0u8; 32];
        match request[..3] {
            // Tap dance, combo, key override and alt repeat SET (even op) and
            // GET (odd op): the GET reply carries the SET payload after status.
            [0xfe, 0x0d, op @ (0x02 | 0x04 | 0x06 | 0x08)] => {
                dynamic_entries.insert([op, request[3]], request[4..].to_vec());
            }
            [0xfe, 0x0d, op @ (0x01 | 0x03 | 0x05 | 0x07)] => {
                let entry = dynamic_entries
                    .get(&[op + 1, request[3]])
                    .cloned()
                    .unwrap_or_default();
                reply[1..1 + entry.len().min(31)].copy_from_slice(&entry[..entry.len().min(31)]);
            }
            [0x05, ..] => {
                keycodes.insert(
                    [request[1], request[2], request[3]],
                    [request[4], request[5]],
                );
                reply = *request;
            }
            [0x04, ..] => {
                reply = *request;
                let code = keycodes.get(&[request[1], request[2], request[3]]);
                reply[4..6].copy_from_slice(code.unwrap_or(&[0, 0]));
            }
            // Native key action (0x03) and dynamic action (0x05): keyed by the
            // three address bytes after the version.
            [0x07, 0xe8, sub @ (0x03 | 0x05)] => {
                let len = usize::from(request[7]);
                native.insert(
                    [sub, request[4], request[5], request[6]],
                    request[8..8 + len].to_vec(),
                );
                reply[..4].copy_from_slice(&request[..4]);
            }
            [0x08, 0xe8, sub @ (0x03 | 0x05)] => {
                reply[..4].copy_from_slice(&request[..4]);
                let payload = native
                    .get(&[sub, request[4], request[5], request[6]])
                    .cloned()
                    .unwrap_or_else(|| vec![0]);
                reply[5] = payload.len() as u8;
                reply[6..6 + payload.len()].copy_from_slice(&payload);
            }
            [0x07, 0xe8, 0x07] => {
                combo_layers.insert(request[4], [request[5], request[6]]);
                reply[..4].copy_from_slice(&request[..4]);
                reply[7] = request[4];
            }
            [0x08, 0xe8, 0x07] => {
                reply[..4].copy_from_slice(&request[..4]);
                let [present, layer] = combo_layers.get(&request[4]).copied().unwrap_or_default();
                reply[5] = present;
                reply[6] = layer;
                reply[7] = request[4];
            }
            [0xfe, 0x0b, _] => {
                settings.insert([request[2], request[3]], request[4..].to_vec());
            }
            [0xfe, 0x0a, _] => {
                let value = settings
                    .get(&[request[2], request[3]])
                    .cloned()
                    .unwrap_or_default();
                reply[1..1 + value.len().min(31)].copy_from_slice(&value[..value.len().min(31)]);
            }
            _ => return None,
        }
        Some(Ok(reply))
    });
}

/// Compares `frames` with `tests/golden/vial/<name>.frames`: one request per
/// line in hex, trailing zero padding dropped.
pub(crate) fn assert_vial_frames(name: &str, frames: &[[u8; 32]]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/vial")
        .join(format!("{name}.frames"));
    let actual = frames
        .iter()
        .map(|frame| {
            let len = frame
                .iter()
                .rposition(|byte| *byte != 0)
                .map_or(1, |last| last + 1);
            frame[..len]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
                + "\n"
        })
        .collect::<String>();
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| {
            panic!("{}: {error}; record it with {UPDATE_ENV}=1", path.display())
        })
        .replace('\r', "");
    if expected == actual {
        return;
    }
    let (expected_lines, actual_lines): (Vec<_>, Vec<_>) =
        (expected.lines().collect(), actual.lines().collect());
    let first = expected_lines
        .iter()
        .zip(&actual_lines)
        .position(|(expected, actual)| expected != actual)
        .unwrap_or(expected_lines.len().min(actual_lines.len()));
    panic!(
        "Vial request stream `{name}` changed at request {}: expected {:?}, sent {:?} ({} expected, {} sent)",
        first + 1,
        expected_lines.get(first),
        actual_lines.get(first),
        expected_lines.len(),
        actual_lines.len(),
    );
}
