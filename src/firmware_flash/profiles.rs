//! Explicitly reviewed release profiles. Rynk compatibility alone never opts a
//! keyboard into flashing: its package, component order and memory map must agree.

use crate::rynk::client::{FirmwareTarget, RynkBoard};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Central,
    Peripheral,
    Dongle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlashLayout {
    Rp2040,
    Nrf52840Softdevice,
    Nrf52840Dongle,
}

impl FlashLayout {
    /// UF2 family and application-only flash interval (end excluded).
    pub(super) fn limits(self) -> (u32, u32, u32) {
        match self {
            Self::Rp2040 => (0xe48bff56, 0x10000000, 0x10100000),
            Self::Nrf52840Softdevice => (0xada52840, 0x26000, 0xcc000),
            Self::Nrf52840Dongle => (0xada52840, 0x1000, 0xcc000),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A complete component entry within a profile. Equality includes its filename
/// and presentation; engine decisions use role and layout explicitly.
pub(crate) struct Part {
    pub role: Role,
    pub target: FirmwareTarget,
    pub layout: FlashLayout,
    pub label: (&'static str, &'static str),
    pub suffix: &'static str,
}

impl Part {
    pub(crate) fn peripheral(self) -> bool {
        self.role == Role::Peripheral
    }

    pub(crate) fn rp2040(self) -> bool {
        self.layout == FlashLayout::Rp2040
    }
}

pub(crate) const PHENOM_CENTRAL: Part = Part {
    role: Role::Central,
    target: FirmwareTarget::Central,
    layout: FlashLayout::Rp2040,
    label: ("Left half", "Левая половинка"),
    suffix: "left",
};
pub(crate) const PHENOM_PERIPHERAL: Part = Part {
    role: Role::Peripheral,
    target: FirmwareTarget::Peripheral(0),
    layout: FlashLayout::Rp2040,
    label: ("Right half", "Правая половинка"),
    suffix: "right",
};
pub(crate) const K04_LEFT_PERIPHERAL: Part = Part {
    role: Role::Peripheral,
    target: FirmwareTarget::Peripheral(0),
    layout: FlashLayout::Nrf52840Softdevice,
    label: ("Left half", "Левая половинка"),
    suffix: "left",
};
pub(crate) const K04_RIGHT_PERIPHERAL: Part = Part {
    suffix: "right",
    target: FirmwareTarget::Peripheral(1),
    label: ("Right half", "Правая половинка"),
    ..K04_LEFT_PERIPHERAL
};
pub(crate) const K04_SECONDARY: Part = Part {
    label: ("Secondary half", "Вторая половинка"),
    target: FirmwareTarget::Peripheral(0),
    ..K04_RIGHT_PERIPHERAL
};
pub(crate) const K04_LEFT_CENTRAL: Part = Part {
    role: Role::Central,
    target: FirmwareTarget::Central,
    label: ("Primary half", "Основная половинка"),
    ..K04_LEFT_PERIPHERAL
};
pub(crate) const K04_DONGLE: Part = Part {
    role: Role::Dongle,
    target: FirmwareTarget::Central,
    layout: FlashLayout::Nrf52840Dongle,
    label: ("Qube USB dongle", "USB-донгл Qube"),
    suffix: "dongle",
};

#[derive(Clone, Copy)]
pub(crate) struct Profile {
    pub product_id: u16,
    pub archive: &'static str,
    pub output: &'static str,
    /// Transfer peripherals first, then the runtime USB component. A whole
    /// keyboard can use a single Central image; no split-specific engine needed.
    pub parts: &'static [Part],
    pub either_half_primary: bool,
    pub connection_note: Option<(&'static str, &'static str)>,
}

impl Profile {
    /// Revision 2 release firmware exposes only the fixed primary/dongle.
    pub(crate) fn runtime_part(self, board: &RynkBoard) -> Option<Part> {
        let legacy_primary = self.parts.last().copied()?;
        if board.maintenance_only() {
            return None;
        }
        if board.info.revision == 2 {
            return Some(legacy_primary);
        }
        let target = board.firmware_target?;
        self.parts.iter().copied().find(|part| {
            part.target == target && (*part == legacy_primary || self.either_half_primary)
        })
    }

    pub(crate) fn has_peripherals(self) -> bool {
        self.parts.iter().any(|part| part.peripheral())
    }

    pub(crate) fn has_dongle(self) -> bool {
        self.parts.iter().any(|part| part.role == Role::Dongle)
    }

    pub(super) fn image_name(self, part: Part) -> String {
        format!("{}_{}.uf2", self.output, part.suffix)
    }
}

// This registry is also the runtime USB allowlist, including topology checks.
pub(crate) const PROFILES: &[Profile] = &[
    phenom(0x52, "phenom", "phenom"),
    phenom(0x53, "phenom-mini", "phenom_mini"),
    phenom(0x54, "phenom-micro", "phenom_micro"),
    k04(0x71, "k04-qube", "k04_qube_k04", true),
    k04(0x72, "k04-mini-qube", "k04_qube_mini", true),
    k04(0x73, "k04-micro-qube", "k04_qube_micro", true),
    k04(0x74, "k04", "k04_series_k04", false),
    k04(0x75, "k04-mini", "k04_series_mini", false),
    k04(0x76, "k04-micro", "k04_series_micro", false),
];

const fn phenom(product_id: u16, archive: &'static str, output: &'static str) -> Profile {
    Profile {
        product_id,
        archive,
        output,
        parts: &[PHENOM_PERIPHERAL, PHENOM_CENTRAL],
        either_half_primary: true,
        connection_note: Some((
            "Keep the Phenom halves linked. Move only the computer's USB cable when the guide asks.",
            "Оставьте половинки Phenom соединёнными. По инструкции переставляйте только USB-кабель компьютера.",
        )),
    }
}

const fn k04(product_id: u16, archive: &'static str, output: &'static str, qube: bool) -> Profile {
    Profile {
        product_id,
        archive,
        output,
        parts: if qube {
            &[K04_LEFT_PERIPHERAL, K04_RIGHT_PERIPHERAL, K04_DONGLE]
        } else {
            &[K04_SECONDARY, K04_LEFT_CENTRAL]
        },
        either_half_primary: false,
        connection_note: None,
    }
}

pub(crate) fn profile(pid: u16) -> Option<Profile> {
    PROFILES
        .iter()
        .find(|profile| profile.product_id == pid)
        .copied()
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    // Synthetic models stay outside PROFILES: no real keyboard is enabled.
    pub(crate) const SINGLE_RP2040: Profile = Profile {
        product_id: 0xffff,
        archive: "future-board",
        output: "future_board",
        parts: &[Part {
            role: Role::Central,
            target: FirmwareTarget::Standalone,
            layout: FlashLayout::Rp2040,
            label: ("Keyboard", "Клавиатура"),
            suffix: "application",
        }],
        either_half_primary: false,
        connection_note: None,
    };
    pub(crate) const SINGLE_NRF52840: Profile = Profile {
        parts: &[Part {
            layout: FlashLayout::Nrf52840Softdevice,
            ..SINGLE_RP2040.parts[0]
        }],
        ..SINGLE_RP2040
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_flash_revision_three_admits_either_detected_phenom_half_only() {
        use crate::rynk::client::testing::{capabilities, info};
        let phenom = profile(0x52).unwrap();
        let k04 = profile(0x74).unwrap();
        let mut board = RynkBoard {
            info: info(2),
            capabilities: capabilities(512),
            firmware_target: None,
        };
        assert_eq!(phenom.runtime_part(&board), Some(PHENOM_CENTRAL));
        board.info.revision = 3;
        assert_eq!(phenom.runtime_part(&board), None);
        board.firmware_target = Some(FirmwareTarget::Peripheral(0));
        assert_eq!(phenom.runtime_part(&board), Some(PHENOM_PERIPHERAL));
        assert_eq!(k04.runtime_part(&board), None);
        board.firmware_target = Some(FirmwareTarget::Central);
        assert_eq!(phenom.runtime_part(&board), Some(PHENOM_CENTRAL));
        assert_eq!(k04.runtime_part(&board), Some(K04_LEFT_CENTRAL));
    }

    #[test]
    fn firmware_flash_registry_has_unique_models_and_peripherals_before_one_runtime_target() {
        let mut products = std::collections::BTreeSet::new();
        for profile in PROFILES {
            assert!(products.insert(profile.product_id));
            let (last, peripherals) = profile.parts.split_last().unwrap();
            assert!(!last.peripheral());
            assert!(peripherals.iter().all(|part| part.peripheral()));
            let names: std::collections::BTreeSet<_> = profile
                .parts
                .iter()
                .map(|part| profile.image_name(*part))
                .collect();
            assert_eq!(names.len(), profile.parts.len());
        }
        assert_eq!(products, (0x52..=0x54).chain(0x71..=0x76).collect());
        assert!(profile(0x00b2).is_none());
    }
}
