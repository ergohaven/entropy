/// Raw HID packet size (without report ID byte)
pub(super) const MSG_LEN: usize = 32;

// VIA commands
pub(super) const CMD_VIA_GET_PROTOCOL_VERSION: u8 = 0x01;
pub(super) const CMD_VIA_GET_KEYBOARD_VALUE: u8 = 0x02;
pub(super) const CMD_VIA_SET_KEYBOARD_VALUE: u8 = 0x03;
pub(super) const CMD_VIA_GET_KEYCODE: u8 = 0x04;
pub(super) const CMD_VIA_SET_KEYCODE: u8 = 0x05;
pub(super) const CMD_VIA_LIGHTING_SET_VALUE: u8 = 0x07;
pub(super) const CMD_VIA_LIGHTING_GET_VALUE: u8 = 0x08;
pub(super) const CMD_VIA_CUSTOM_SET_VALUE: u8 = 0x07;
pub(super) const CMD_VIA_CUSTOM_GET_VALUE: u8 = 0x08;
pub(super) const CMD_VIA_LIGHTING_SAVE: u8 = 0x09;
pub(super) const CMD_VIA_GET_LAYER_COUNT: u8 = 0x11;
pub(super) const CMD_VIA_KEYMAP_GET_BUFFER: u8 = 0x12;
pub(super) const CMD_VIA_MACRO_GET_COUNT: u8 = 0x0C;
pub(super) const CMD_VIA_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
pub(super) const CMD_VIA_MACRO_GET_BUFFER: u8 = 0x0E;
pub(super) const CMD_VIA_MACRO_SET_BUFFER: u8 = 0x0F;
pub(crate) const CMD_VIA_VIAL_PREFIX: u8 = 0xFE;

pub(super) const VIA_LAYOUT_OPTIONS: u8 = 0x02;
pub(super) const VIA_SWITCH_MATRIX_STATE: u8 = 0x03;
pub(super) const VIA_FIRMWARE_VERSION: u8 = 0x04;
pub(super) const QMK_BACKLIGHT_BRIGHTNESS: u8 = 0x09;
pub(super) const QMK_BACKLIGHT_EFFECT: u8 = 0x0A;
pub(super) const QMK_RGBLIGHT_BRIGHTNESS: u8 = 0x80;
pub(super) const QMK_RGBLIGHT_EFFECT: u8 = 0x81;
pub(super) const QMK_RGBLIGHT_EFFECT_SPEED: u8 = 0x82;
pub(super) const QMK_RGBLIGHT_COLOR: u8 = 0x83;
pub(super) const VIALRGB_GET_INFO: u8 = 0x40;
pub(super) const VIALRGB_GET_MODE: u8 = 0x41;
pub(super) const VIALRGB_GET_SUPPORTED: u8 = 0x42;
pub(super) const VIALRGB_SET_MODE: u8 = 0x41;

pub(super) const ERGOHAVEN_CUSTOM_NAMESPACE: u8 = 0xE8;
pub(super) const ERGOHAVEN_CUSTOM_BATTERY_HALVES: u8 = 0x01;
pub(super) const ERGOHAVEN_BATTERY_HALVES_VERSION: u8 = 0x01;

// Vial sub-commands (used after CMD_VIA_VIAL_PREFIX)
pub(super) const CMD_VIAL_GET_KEYBOARD_ID: u8 = 0x00;
pub(super) const CMD_VIAL_GET_SIZE: u8 = 0x01;
pub(super) const CMD_VIAL_GET_DEFINITION: u8 = 0x02;
pub(crate) const CMD_VIAL_GET_ENCODER: u8 = 0x03;
pub(super) const CMD_VIAL_SET_ENCODER: u8 = 0x04;
pub(super) const CMD_VIAL_GET_UNLOCK_STATUS: u8 = 0x05;
pub(super) const CMD_VIAL_UNLOCK_START: u8 = 0x06;
pub(super) const CMD_VIAL_UNLOCK_POLL: u8 = 0x07;
pub(super) const CMD_VIAL_LOCK: u8 = 0x08;
pub(super) const CMD_VIAL_QMK_SETTINGS_QUERY: u8 = 0x09;
pub(crate) const CMD_VIAL_QMK_SETTINGS_GET: u8 = 0x0A;
pub(super) const CMD_VIAL_QMK_SETTINGS_SET: u8 = 0x0B;
pub(super) const CMD_VIAL_DYNAMIC_ENTRY_OP: u8 = 0x0D;
pub(super) const DYNAMIC_VIAL_GET_NUM_ENTRIES: u8 = 0x00;
pub(super) const DYNAMIC_VIAL_TAP_DANCE_GET: u8 = 0x01;
pub(super) const DYNAMIC_VIAL_TAP_DANCE_SET: u8 = 0x02;
pub(super) const DYNAMIC_VIAL_COMBO_GET: u8 = 0x03;
pub(super) const DYNAMIC_VIAL_COMBO_SET: u8 = 0x04;
pub(super) const DYNAMIC_VIAL_KEY_OVERRIDE_GET: u8 = 0x05;
pub(super) const DYNAMIC_VIAL_KEY_OVERRIDE_SET: u8 = 0x06;
pub(super) const DYNAMIC_VIAL_ALT_REPEAT_KEY_GET: u8 = 0x07;
pub(super) const DYNAMIC_VIAL_ALT_REPEAT_KEY_SET: u8 = 0x08;

pub(super) const BUFFER_FETCH_CHUNK: usize = 28;

/// These successful Vial replies carry only the requested value, not the
/// encoder index or QSID. A transport therefore cannot distinguish a late
/// reply to the previous request from the reply to the current one.
pub(crate) fn vial_reply_is_uncorrelated(command: &[u8]) -> bool {
    command.first() == Some(&CMD_VIA_VIAL_PREFIX)
        && matches!(
            command.get(1),
            Some(&CMD_VIAL_GET_ENCODER | &CMD_VIAL_QMK_SETTINGS_GET)
        )
}

/// Requests that only read keyboard state. A read-only HID session refuses
/// everything else, so a new or unknown command fails closed.
pub(crate) fn is_read_request(command: &[u8]) -> bool {
    match command {
        [CMD_VIA_GET_PROTOCOL_VERSION
        | CMD_VIA_GET_KEYBOARD_VALUE
        | CMD_VIA_GET_KEYCODE
        | CMD_VIA_CUSTOM_GET_VALUE
        | CMD_VIA_MACRO_GET_COUNT
        | CMD_VIA_MACRO_GET_BUFFER_SIZE
        | CMD_VIA_MACRO_GET_BUFFER
        | CMD_VIA_GET_LAYER_COUNT
        | CMD_VIA_KEYMAP_GET_BUFFER, ..] => true,
        [CMD_VIA_VIAL_PREFIX, CMD_VIAL_DYNAMIC_ENTRY_OP, operation, ..] => matches!(
            *operation,
            DYNAMIC_VIAL_GET_NUM_ENTRIES
                | DYNAMIC_VIAL_TAP_DANCE_GET
                | DYNAMIC_VIAL_COMBO_GET
                | DYNAMIC_VIAL_KEY_OVERRIDE_GET
                | DYNAMIC_VIAL_ALT_REPEAT_KEY_GET
        ),
        [CMD_VIA_VIAL_PREFIX, subcommand, ..] => matches!(
            *subcommand,
            CMD_VIAL_GET_KEYBOARD_ID
                | CMD_VIAL_GET_SIZE
                | CMD_VIAL_GET_DEFINITION
                | CMD_VIAL_GET_ENCODER
                | CMD_VIAL_GET_UNLOCK_STATUS
                | CMD_VIAL_QMK_SETTINGS_QUERY
                | CMD_VIAL_QMK_SETTINGS_GET
        ),
        _ => false,
    }
}

/// The `.entlayout` section left incomplete when this read fails. `None` for
/// reads outside the bundle, or whose failure the connect already turns into
/// an error or a complete fallback (identity, definition, optional probes,
/// the keymap buffer with its per-key fallback).
pub(crate) fn entlayout_section_of_read(command: &[u8]) -> Option<&'static str> {
    match command {
        [CMD_VIA_GET_KEYCODE, ..] => Some("Keymap"),
        [CMD_VIA_GET_KEYBOARD_VALUE, VIA_LAYOUT_OPTIONS, ..] => Some("LayoutOptions"),
        [CMD_VIA_MACRO_GET_COUNT | CMD_VIA_MACRO_GET_BUFFER_SIZE | CMD_VIA_MACRO_GET_BUFFER, ..] => {
            Some("Macros")
        }
        // Native key actions, dynamic actions and combo layers (see rmk_native).
        [CMD_VIA_CUSTOM_GET_VALUE, ERGOHAVEN_CUSTOM_NAMESPACE, 0x03..=0x07, ..] => {
            Some("RmkNativeActions")
        }
        [CMD_VIA_VIAL_PREFIX, CMD_VIAL_GET_ENCODER, ..] => Some("Encoders"),
        [CMD_VIA_VIAL_PREFIX, CMD_VIAL_DYNAMIC_ENTRY_OP, operation, ..] => match *operation {
            DYNAMIC_VIAL_GET_NUM_ENTRIES => Some("DynamicEntries"),
            DYNAMIC_VIAL_TAP_DANCE_GET => Some("TapDance"),
            DYNAMIC_VIAL_COMBO_GET => Some("Combos"),
            DYNAMIC_VIAL_KEY_OVERRIDE_GET => Some("KeyOverrides"),
            DYNAMIC_VIAL_ALT_REPEAT_KEY_GET => Some("AltRepeat"),
            _ => None,
        },
        [CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_GET, low, high, ..]
            if (200..232).contains(&u16::from_le_bytes([*low, *high])) =>
        {
            Some("LayerNames")
        }
        [CMD_VIA_VIAL_PREFIX, CMD_VIAL_QMK_SETTINGS_QUERY | CMD_VIAL_QMK_SETTINGS_GET, ..] => {
            Some("QmkSettings")
        }
        _ => None,
    }
}
