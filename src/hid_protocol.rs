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
pub(super) const CMD_VIA_CUSTOM_GET_VALUE: u8 = 0x08;
pub(super) const CMD_VIA_LIGHTING_SAVE: u8 = 0x09;
pub(super) const CMD_VIA_GET_LAYER_COUNT: u8 = 0x11;
pub(super) const CMD_VIA_KEYMAP_GET_BUFFER: u8 = 0x12;
pub(super) const CMD_VIA_MACRO_GET_COUNT: u8 = 0x0C;
pub(super) const CMD_VIA_MACRO_GET_BUFFER_SIZE: u8 = 0x0D;
pub(super) const CMD_VIA_MACRO_GET_BUFFER: u8 = 0x0E;
pub(super) const CMD_VIA_MACRO_SET_BUFFER: u8 = 0x0F;
pub(super) const CMD_VIA_VIAL_PREFIX: u8 = 0xFE;

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
pub(super) const CMD_VIAL_GET_ENCODER: u8 = 0x03;
pub(super) const CMD_VIAL_SET_ENCODER: u8 = 0x04;
pub(super) const CMD_VIAL_GET_UNLOCK_STATUS: u8 = 0x05;
pub(super) const CMD_VIAL_UNLOCK_START: u8 = 0x06;
pub(super) const CMD_VIAL_UNLOCK_POLL: u8 = 0x07;
pub(super) const CMD_VIAL_LOCK: u8 = 0x08;
pub(super) const CMD_VIAL_QMK_SETTINGS_QUERY: u8 = 0x09;
pub(super) const CMD_VIAL_QMK_SETTINGS_GET: u8 = 0x0A;
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
