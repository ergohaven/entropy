//! Firmware profiles bind a release asset to a keyboard and its flash layout.

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
    Rp2040Sm62,
    Nrf52840Softdevice,
    Nrf52840Dongle,
}

impl FlashLayout {
    /// UF2 family and application-only flash interval (end excluded).
    pub(super) fn limits(self) -> (u32, u32, u32) {
        match self {
            Self::Rp2040 => (0xe48bff56, 0x10000000, 0x10100000),
            // SM62's linker caps code here, before its resident layout store.
            Self::Rp2040Sm62 => (0xe48bff56, 0x10000000, 0x101f2000),
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
    unlock: Option<UnlockChallenge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UnlockChallenge {
    positions: [(u8, u8); 2],
    keys: &'static str,
}

const D_F_ROW_2: UnlockChallenge = UnlockChallenge {
    positions: [(2, 1), (2, 2)],
    keys: "D + F",
};
const J_K_ROW_2: UnlockChallenge = UnlockChallenge {
    keys: "J + K",
    ..D_F_ROW_2
};
const D_F_ROW_1: UnlockChallenge = UnlockChallenge {
    positions: [(1, 1), (1, 2)],
    ..D_F_ROW_2
};
const J_K_ROW_1: UnlockChallenge = UnlockChallenge {
    keys: "J + K",
    ..D_F_ROW_1
};
const VELVET_D_F: UnlockChallenge = UnlockChallenge {
    positions: [(1, 3), (1, 4)],
    ..D_F_ROW_2
};

impl Part {
    pub(crate) fn peripheral(self) -> bool {
        self.role == Role::Peripheral
    }

    pub(crate) fn rp2040(self) -> bool {
        matches!(self.layout, FlashLayout::Rp2040 | FlashLayout::Rp2040Sm62)
    }

    pub(crate) fn unlock_keys(self, positions: &[(u8, u8)]) -> Option<&'static str> {
        let challenge = self.unlock?;
        (positions.len() == 2
            && challenge
                .positions
                .iter()
                .all(|key| positions.contains(key)))
        .then_some(challenge.keys)
    }
}

pub(crate) const PHENOM_CENTRAL: Part = Part {
    role: Role::Central,
    target: FirmwareTarget::Central,
    layout: FlashLayout::Rp2040,
    label: ("Left half", "Левая половинка"),
    suffix: "left",
    unlock: Some(D_F_ROW_2),
};
pub(crate) const PHENOM_PERIPHERAL: Part = Part {
    role: Role::Peripheral,
    target: FirmwareTarget::Peripheral(0),
    layout: FlashLayout::Rp2040,
    label: ("Right half", "Правая половинка"),
    suffix: "right",
    unlock: Some(D_F_ROW_2),
};
pub(crate) const K04_LEFT_PERIPHERAL: Part = Part {
    role: Role::Peripheral,
    target: FirmwareTarget::Peripheral(0),
    layout: FlashLayout::Nrf52840Softdevice,
    label: ("Left half", "Левая половинка"),
    suffix: "left",
    unlock: Some(D_F_ROW_2),
};
pub(crate) const K04_RIGHT_PERIPHERAL: Part = Part {
    suffix: "right",
    target: FirmwareTarget::Peripheral(1),
    label: ("Right half", "Правая половинка"),
    unlock: Some(J_K_ROW_2),
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
    unlock: None,
    ..K04_LEFT_PERIPHERAL
};
pub(crate) const K04_DONGLE: Part = Part {
    role: Role::Dongle,
    target: FirmwareTarget::Central,
    layout: FlashLayout::Nrf52840Dongle,
    label: ("Qube USB dongle", "USB-донгл Qube"),
    suffix: "dongle",
    unlock: None,
};

const LEFT_CENTRAL: Part = Part {
    label: K04_LEFT_PERIPHERAL.label,
    ..K04_LEFT_CENTRAL
};
const RIGHT_PERIPHERAL: Part = Part {
    label: K04_RIGHT_PERIPHERAL.label,
    ..K04_SECONDARY
};
const CLASSIC_LEFT_PERIPHERAL: Part = Part {
    unlock: Some(D_F_ROW_1),
    ..K04_LEFT_PERIPHERAL
};
const CLASSIC_RIGHT_PERIPHERAL: Part = Part {
    unlock: Some(J_K_ROW_1),
    ..K04_RIGHT_PERIPHERAL
};
const VELVET_QUBE_LEFT: Part = Part {
    unlock: Some(VELVET_D_F),
    ..K04_LEFT_PERIPHERAL
};
const VELVET_LEFT_PERIPHERAL: Part = Part {
    suffix: "left_peripheral",
    ..VELVET_QUBE_LEFT
};
const VELVET_RIGHT_CENTRAL: Part = Part {
    suffix: "right_main",
    label: K04_RIGHT_PERIPHERAL.label,
    ..K04_LEFT_CENTRAL
};

#[derive(Clone, Copy)]
pub(crate) struct Profile {
    pub product_id: u16,
    pub archive: &'static str,
    pub output: &'static str,
    pub qmk: bool,
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

    pub(super) fn shares_product_id(self) -> bool {
        PROFILES
            .iter()
            .any(|profile| profile.product_id == self.product_id && profile.archive != self.archive)
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
    classic(0x36, "op36", "op36", CLASSIC_STANDALONE_PARTS),
    classic(0x36, "op36-qube", "op36_qube", CLASSIC_QUBE_PARTS),
    classic(0x44, "imperial44", "imperial44", CLASSIC_STANDALONE_PARTS),
    classic(
        0x44,
        "imperial44-qube",
        "imperial44_qube",
        CLASSIC_QUBE_PARTS,
    ),
    classic(0x70, "k03", "k03", &[RIGHT_PERIPHERAL, LEFT_CENTRAL]),
    classic(0x70, "k03-qube", "k03_qube", QUBE_PARTS),
    classic(
        0xbe,
        "velvet",
        "velvet_standalone",
        &[VELVET_LEFT_PERIPHERAL, VELVET_RIGHT_CENTRAL],
    ),
    classic(
        0xbe,
        "velvet-qube",
        "velvet_qube",
        &[VELVET_QUBE_LEFT, CLASSIC_RIGHT_PERIPHERAL, K04_DONGLE],
    ),
];

const QUBE_PARTS: &[Part] = &[K04_LEFT_PERIPHERAL, K04_RIGHT_PERIPHERAL, K04_DONGLE];
const CLASSIC_QUBE_PARTS: &[Part] = &[
    CLASSIC_LEFT_PERIPHERAL,
    CLASSIC_RIGHT_PERIPHERAL,
    K04_DONGLE,
];
const CLASSIC_STANDALONE_PARTS: &[Part] = &[
    Part {
        target: FirmwareTarget::Peripheral(0),
        ..CLASSIC_RIGHT_PERIPHERAL
    },
    LEFT_CENTRAL,
];

const fn classic(
    product_id: u16,
    archive: &'static str,
    output: &'static str,
    parts: &'static [Part],
) -> Profile {
    Profile {
        product_id,
        archive,
        output,
        qmk: false,
        parts,
        either_half_primary: false,
        connection_note: None,
    }
}

const fn phenom(product_id: u16, archive: &'static str, output: &'static str) -> Profile {
    Profile {
        product_id,
        archive,
        output,
        qmk: false,
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
        qmk: false,
        parts: if qube {
            QUBE_PARTS
        } else {
            &[K04_SECONDARY, K04_LEFT_CENTRAL]
        },
        either_half_primary: false,
        connection_note: None,
    }
}

const QMK_KEYBOARD: Part = Part {
    role: Role::Central,
    target: FirmwareTarget::Standalone,
    layout: FlashLayout::Rp2040,
    label: ("Keyboard", "Клавиатура"),
    suffix: "keyboard",
    unlock: None,
};
const QMK_FIRST_HALF: Part = Part {
    label: ("USB-connected half", "Подключённая по USB половинка"),
    suffix: "first",
    ..QMK_KEYBOARD
};
const QMK_OTHER_HALF: Part = Part {
    role: Role::Peripheral,
    label: ("Other half", "Вторая половинка"),
    suffix: "other",
    ..QMK_FIRST_HALF
};

const QMK_SM62_PARTS: &[Part] = &[
    Part { layout: FlashLayout::Rp2040Sm62, ..QMK_FIRST_HALF },
    Part { layout: FlashLayout::Rp2040Sm62, ..QMK_OTHER_HALF },
];

pub(crate) fn qmk_profile(pid: u16, asset: &str) -> Option<Profile> {
    let &(_, archive, split) = crate::app::QMK_RELEASE_PROFILES
        .iter()
        .find(|(product, prefix, _)| *product == pid && *prefix == asset)?;
    Some(Profile {
        product_id: pid,
        archive,
        output: archive,
        qmk: true,
        parts: match (archive, split) {
            ("sm62_v1", _) => QMK_SM62_PARTS,
            (_, true) => &[QMK_FIRST_HALF, QMK_OTHER_HALF],
            (_, false) => &[QMK_KEYBOARD],
        },
        either_half_primary: split,
        connection_note: None,
    })
}

pub(crate) fn profile_for_archive(pid: u16, archive: &str) -> Option<Profile> {
    PROFILES
        .iter()
        .find(|profile| profile.product_id == pid && profile.archive == archive)
        .copied()
}

#[cfg(test)]
pub(crate) fn profile(pid: u16) -> Option<Profile> {
    let mut matches = PROFILES.iter().filter(|profile| profile.product_id == pid);
    let profile = matches.next()?;
    matches.next().is_none().then_some(*profile)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    // Synthetic models stay outside PROFILES: no real keyboard is enabled.
    pub(crate) const SINGLE_RP2040: Profile = Profile {
        product_id: 0xffff,
        archive: "future-board",
        output: "future_board",
        qmk: false,
        parts: &[Part {
            role: Role::Central,
            target: FirmwareTarget::Standalone,
            layout: FlashLayout::Rp2040,
            label: ("Keyboard", "Клавиатура"),
            suffix: "application",
            unlock: None,
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
    fn firmware_flash_registry_covers_the_native_rmk_release_series() {
        let mut keys = std::collections::BTreeSet::new();
        for profile in PROFILES {
            assert!(keys.insert((profile.product_id, profile.archive)));
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
        assert_eq!(
            keys,
            [
                (0x36, "op36"),
                (0x36, "op36-qube"),
                (0x44, "imperial44"),
                (0x44, "imperial44-qube"),
                (0x52, "phenom"),
                (0x53, "phenom-mini"),
                (0x54, "phenom-micro"),
                (0x70, "k03"),
                (0x70, "k03-qube"),
                (0x71, "k04-qube"),
                (0x72, "k04-mini-qube"),
                (0x73, "k04-micro-qube"),
                (0x74, "k04"),
                (0x75, "k04-mini"),
                (0x76, "k04-micro"),
                (0xbe, "velvet"),
                (0xbe, "velvet-qube"),
            ]
            .into_iter()
            .collect()
        );
        assert!(profile(0x00b2).is_none());
    }

    #[test]
    fn firmware_flash_velvet_uses_right_primary_and_qube_keeps_left_right_order() {
        let standalone = profile_for_archive(0xbe, "velvet").unwrap();
        let qube = profile_for_archive(0xbe, "velvet-qube").unwrap();
        assert!(profile(0xbe).is_none());
        assert_eq!(
            standalone.image_name(standalone.parts[0]),
            "velvet_standalone_left_peripheral.uf2"
        );
        assert_eq!(
            standalone.image_name(standalone.parts[1]),
            "velvet_standalone_right_main.uf2"
        );
        assert_eq!(standalone.parts[0].label.0, "Left half");
        assert_eq!(
            standalone.parts[0].unlock_keys(&[(1, 4), (1, 3)]),
            Some("D + F")
        );
        assert_eq!(standalone.parts[0].unlock_keys(&[(1, 1), (1, 2)]), None);
        assert_eq!(standalone.parts[1].label.0, "Right half");
        assert_eq!(
            qube.parts
                .iter()
                .map(|part| part.suffix)
                .collect::<Vec<_>>(),
            ["left", "right", "dongle"]
        );
        assert_eq!(qube.parts[1].unlock_keys(&[(1, 1), (1, 2)]), Some("J + K"));
    }
}
