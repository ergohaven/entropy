//! Rynk boards: RMK's native host protocol, which ergohaven RMK firmware speaks
//! once the Vial path is frozen on native v1.
//!
//! Vial boards keep `rmk_types` (native v1); Rynk boards use the firmware's own
//! revision as `rmk_types_rynk`. The two never meet at runtime: only .entlayout
//! migration converts between them, plus key labels, which reuse the v1 code.

pub(crate) mod client;
pub(crate) mod dynamic;
pub(crate) mod entlayout;
pub(crate) mod keycode;
pub(crate) mod link;
pub(crate) mod macros;
pub(crate) mod native_v1;
pub(crate) mod read_only;
pub(crate) mod session;
pub(crate) mod transport;
