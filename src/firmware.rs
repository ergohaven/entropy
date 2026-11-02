/// Firmware protocol type — determines how the app communicates with the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FirmwareProtocol {
    Vial,
    /// RMK's own host protocol over the board's vendor interface.
    Rynk,
}
