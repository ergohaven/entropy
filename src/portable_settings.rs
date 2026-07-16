use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableCategory {
    TapHold,
    Touchpad,
    GraveEscape,
    Combo,
    AutoShift,
    OneShot,
    MouseKeys,
    Magic,
    Module,
    LayerLeds,
    Bluetooth,
    Rgb,
    LayerNames,
    LayoutOptions,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct PortableSettingId {
    pub(crate) namespace: String,
    pub(crate) category: PortableCategory,
    pub(crate) semantic: String,
    pub(crate) primary_qsid: Option<u16>,
    #[serde(default)]
    pub(crate) linked_qsids: Vec<u16>,
}

impl PortableSettingId {
    pub(crate) fn qmk(
        category: PortableCategory,
        semantic: impl Into<String>,
        primary_qsid: u16,
        linked_qsids: impl IntoIterator<Item = u16>,
    ) -> Self {
        let mut linked_qsids: Vec<_> = linked_qsids.into_iter().collect();
        linked_qsids.sort_unstable();
        linked_qsids.dedup();
        Self {
            namespace: "qmk".into(),
            category,
            semantic: semantic.into(),
            primary_qsid: Some(primary_qsid),
            linked_qsids,
        }
    }

    pub(crate) fn named(
        namespace: impl Into<String>,
        category: PortableCategory,
        semantic: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            category,
            semantic: semantic.into(),
            primary_qsid: None,
            linked_qsids: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableValueKind {
    Boolean,
    Unsigned,
    Text,
    Select,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireWidth {
    Bit,
    Bits8,
    Bits16,
    Utf8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ValueRange {
    pub(crate) min: u64,
    pub(crate) max: u64,
}

impl ValueRange {
    pub(crate) const fn new(min: u64, max: u64) -> Self {
        Self { min, max }
    }

    fn contains(self, value: u64) -> bool {
        self.min <= value && value <= self.max
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PortableSettingSpec {
    pub(crate) id: PortableSettingId,
    pub(crate) category: PortableCategory,
    pub(crate) kind: PortableValueKind,
    pub(crate) wire_width: WireWidth,
    pub(crate) range: Option<ValueRange>,
    #[serde(default)]
    pub(crate) bit_meanings: Vec<String>,
    #[serde(default)]
    pub(crate) ordered_variants: Vec<String>,
}

impl PortableSettingSpec {
    pub(crate) fn qmk_boolean(
        category: PortableCategory,
        semantic: impl Into<String>,
        qsid: u16,
    ) -> Self {
        let semantic = semantic.into();
        Self {
            id: PortableSettingId::qmk(category, semantic.clone(), qsid, []),
            category,
            kind: PortableValueKind::Boolean,
            wire_width: WireWidth::Bits8,
            range: None,
            bit_meanings: Vec::new(),
            ordered_variants: Vec::new(),
        }
    }

    pub(crate) fn qmk_text(
        category: PortableCategory,
        semantic: impl Into<String>,
        qsid: u16,
    ) -> Self {
        let semantic = semantic.into();
        Self {
            id: PortableSettingId::qmk(category, semantic.clone(), qsid, []),
            category,
            kind: PortableValueKind::Text,
            wire_width: WireWidth::Utf8,
            range: None,
            bit_meanings: Vec::new(),
            ordered_variants: Vec::new(),
        }
    }

    pub(crate) fn qmk_numeric(
        category: PortableCategory,
        semantic: impl Into<String>,
        qsid: u16,
        wire_width: WireWidth,
        range: ValueRange,
    ) -> Self {
        let semantic = semantic.into();
        Self {
            id: PortableSettingId::qmk(category, semantic.clone(), qsid, []),
            category,
            kind: PortableValueKind::Unsigned,
            wire_width,
            range: Some(range),
            bit_meanings: Vec::new(),
            ordered_variants: Vec::new(),
        }
    }

    pub(crate) fn qmk_bits(
        category: PortableCategory,
        semantic: impl Into<String>,
        qsid: u16,
        wire_width: WireWidth,
        bit_meanings: Vec<String>,
    ) -> Self {
        let semantic = semantic.into();
        Self {
            id: PortableSettingId::qmk(category, semantic.clone(), qsid, []),
            category,
            kind: PortableValueKind::Unsigned,
            wire_width,
            range: None,
            bit_meanings,
            ordered_variants: Vec::new(),
        }
    }

    pub(crate) fn qmk_select(
        category: PortableCategory,
        semantic: impl Into<String>,
        qsid: u16,
        wire_width: WireWidth,
        ordered_variants: Vec<String>,
    ) -> Self {
        let semantic = semantic.into();
        Self {
            id: PortableSettingId::qmk(category, semantic.clone(), qsid, []),
            category,
            kind: PortableValueKind::Select,
            wire_width,
            range: None,
            bit_meanings: Vec::new(),
            ordered_variants,
        }
    }

    pub(crate) fn compatibility_with(&self, current: &Self) -> Result<(), Incompatibility> {
        if self.id != current.id {
            return Err(Incompatibility::Identity);
        }
        if self.category != current.category {
            return Err(Incompatibility::Category);
        }
        if self.kind != current.kind {
            return Err(Incompatibility::ValueKind);
        }
        if self.wire_width != current.wire_width {
            return Err(Incompatibility::WireWidth);
        }
        if self.range != current.range {
            return Err(Incompatibility::Range);
        }
        if self.bit_meanings != current.bit_meanings {
            return Err(Incompatibility::BitContract);
        }
        if self.ordered_variants != current.ordered_variants {
            return Err(Incompatibility::OrderedVariants);
        }
        Ok(())
    }

    fn accepts(&self, value: &PortableValue) -> bool {
        match (self.kind, value) {
            (PortableValueKind::Boolean, PortableValue::Boolean(_)) => true,
            (PortableValueKind::Unsigned, PortableValue::Unsigned(value)) => {
                self.range.map_or(true, |range| range.contains(*value))
            }
            (PortableValueKind::Text, PortableValue::Text(_)) => true,
            (PortableValueKind::Select, PortableValue::Select(index)) => {
                usize::from(*index) < self.ordered_variants.len()
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Incompatibility {
    Identity,
    Category,
    ValueKind,
    WireWidth,
    Range,
    BitContract,
    OrderedVariants,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(crate) enum PortableValue {
    Boolean(bool),
    Unsigned(u64),
    Text(String),
    Select(u16),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PortableSetting {
    pub(crate) spec: PortableSettingSpec,
    pub(crate) value: PortableValue,
}

impl PortableSetting {
    pub(crate) fn new(
        spec: PortableSettingSpec,
        value: PortableValue,
    ) -> Result<Self, InvalidPortableValue> {
        if spec.accepts(&value) {
            Ok(Self { spec, value })
        } else {
            Err(InvalidPortableValue)
        }
    }

    pub(crate) fn id(&self) -> &PortableSettingId {
        &self.spec.id
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.spec.category == self.spec.id.category && self.spec.accepts(&self.value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InvalidPortableValue;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StrictCaptureState {
    Captured(PortableSetting),
    Unavailable(String),
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DiffKind {
    Changed {
        trusted: PortableValue,
        current: PortableValue,
    },
    Incompatible(Incompatibility),
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingDiff {
    pub(crate) id: PortableSettingId,
    pub(crate) kind: DiffKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiffGroup {
    pub(crate) category: PortableCategory,
    pub(crate) settings: Vec<SettingDiff>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GroupedDiff {
    pub(crate) groups: Vec<DiffGroup>,
}

impl GroupedDiff {
    pub(crate) fn changed_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|group| &group.settings)
            .filter(|item| matches!(item.kind, DiffKind::Changed { .. }))
            .count()
    }

    pub(crate) fn incompatible_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|group| &group.settings)
            .filter(|item| matches!(item.kind, DiffKind::Incompatible(_)))
            .count()
    }
}

pub(crate) fn grouped_diff<'a>(
    trusted: impl IntoIterator<Item = &'a PortableSetting>,
    current: impl IntoIterator<Item = &'a PortableSetting>,
) -> GroupedDiff {
    let current: BTreeMap<_, _> = current.into_iter().map(|item| (item.id(), item)).collect();
    let mut groups: BTreeMap<PortableCategory, Vec<SettingDiff>> = BTreeMap::new();
    for trusted in trusted {
        let kind = match current.get(trusted.id()) {
            Some(current) => match trusted.spec.compatibility_with(&current.spec) {
                Ok(()) if trusted.value == current.value => continue,
                Ok(()) => DiffKind::Changed {
                    trusted: trusted.value.clone(),
                    current: current.value.clone(),
                },
                Err(reason) => DiffKind::Incompatible(reason),
            },
            None => DiffKind::Unavailable,
        };
        groups
            .entry(trusted.spec.category)
            .or_default()
            .push(SettingDiff {
                id: trusted.id().clone(),
                kind,
            });
    }
    GroupedDiff {
        groups: groups
            .into_iter()
            .map(|(category, settings)| DiffGroup { category, settings })
            .collect(),
    }
}

pub(crate) fn known_qmk_setting(qsid: u16) -> Option<PortableSettingSpec> {
    let numeric = |category, semantic: String, width, max| {
        PortableSettingSpec::qmk_numeric(category, semantic, qsid, width, ValueRange::new(0, max))
    };
    Some(match qsid {
        1 => PortableSettingSpec::qmk_bits(
            PortableCategory::GraveEscape,
            "grave_escape",
            qsid,
            WireWidth::Bits8,
            (0..4)
                .map(|bit| format!("grave_escape_bit_{bit}"))
                .collect(),
        ),
        2 => numeric(
            PortableCategory::Combo,
            "combo_term".into(),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        3 => PortableSettingSpec::qmk_bits(
            PortableCategory::AutoShift,
            "auto_shift_flags",
            qsid,
            WireWidth::Bits8,
            (0..8).map(|bit| format!("auto_shift_bit_{bit}")).collect(),
        ),
        4 => numeric(
            PortableCategory::AutoShift,
            "auto_shift_timeout".into(),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        5 => numeric(
            PortableCategory::OneShot,
            "one_shot_tap_toggle".into(),
            WireWidth::Bits8,
            u8::MAX.into(),
        ),
        6 => numeric(
            PortableCategory::OneShot,
            "one_shot_timeout".into(),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        7 => numeric(
            PortableCategory::TapHold,
            "tapping_term".into(),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        9..=17 => numeric(
            PortableCategory::MouseKeys,
            format!("mouse_keys_{qsid}"),
            WireWidth::Bits8,
            u8::MAX.into(),
        ),
        18..=19 | 27 => numeric(
            PortableCategory::TapHold,
            format!("tap_hold_{qsid}"),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        20 => numeric(
            PortableCategory::TapHold,
            format!("tap_hold_{qsid}"),
            WireWidth::Bits8,
            u8::MAX.into(),
        ),
        22..=24 | 26 => PortableSettingSpec::qmk_boolean(
            PortableCategory::TapHold,
            format!("tap_hold_{qsid}"),
            qsid,
        ),
        21 => PortableSettingSpec::qmk_bits(
            PortableCategory::Magic,
            "magic",
            qsid,
            WireWidth::Bits16,
            (0..10).map(|bit| format!("magic_bit_{bit}")).collect(),
        ),
        25 => numeric(
            PortableCategory::TapHold,
            "quick_tap_term".into(),
            WireWidth::Bits16,
            1000,
        ),
        120 => numeric(
            PortableCategory::Touchpad,
            "touchpad_120".into(),
            WireWidth::Bits16,
            u16::MAX.into(),
        ),
        121..=124 => numeric(
            PortableCategory::Touchpad,
            format!("touchpad_{qsid}"),
            WireWidth::Bits8,
            u8::MAX.into(),
        ),
        142 => PortableSettingSpec::qmk_boolean(
            PortableCategory::Touchpad,
            "touchpad_auto_layer_enable",
            qsid,
        ),
        143 => numeric(
            PortableCategory::Touchpad,
            "touchpad_auto_layer".into(),
            WireWidth::Bits8,
            u8::MAX.into(),
        ),
        200..=231 => PortableSettingSpec::qmk_text(
            PortableCategory::LayerNames,
            format!("layer_name_{}", qsid - 200),
            qsid,
        ),
        _ => return None,
    })
}

pub(crate) fn portable_qmk_ids() -> BTreeSet<u16> {
    (0..=255)
        .filter(|qsid| known_qmk_setting(*qsid).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_tap_spec() -> PortableSettingSpec {
        PortableSettingSpec::qmk_numeric(
            PortableCategory::TapHold,
            "quick_tap_term",
            25,
            WireWidth::Bits16,
            ValueRange::new(0, 1000),
        )
    }

    #[test]
    fn registry_primitives_cover_reported_settings_and_exclude_unknowns() {
        let quick_tap = known_qmk_setting(25).expect("Quick Tap must be portable");
        assert_eq!(quick_tap, quick_tap_spec());
        assert_eq!(known_qmk_setting(18).unwrap().wire_width, WireWidth::Bits16);
        assert_eq!(
            known_qmk_setting(22).unwrap().kind,
            PortableValueKind::Boolean
        );
        assert_eq!(
            known_qmk_setting(142).unwrap().category,
            PortableCategory::Touchpad
        );
        assert_eq!(
            known_qmk_setting(142).unwrap().kind,
            PortableValueKind::Boolean
        );
        assert_eq!(
            known_qmk_setting(143).unwrap().category,
            PortableCategory::Touchpad
        );
        assert_eq!(
            known_qmk_setting(200).unwrap().kind,
            PortableValueKind::Text
        );
        assert_eq!(known_qmk_setting(200).unwrap().wire_width, WireWidth::Utf8);
        assert!(known_qmk_setting(999).is_none());
    }

    #[test]
    fn compatibility_rejects_contract_mismatches() {
        let trusted = quick_tap_spec();

        let mut kind = trusted.clone();
        kind.kind = PortableValueKind::Boolean;
        assert_eq!(
            trusted.compatibility_with(&kind),
            Err(Incompatibility::ValueKind)
        );

        let mut width = trusted.clone();
        width.wire_width = WireWidth::Bits8;
        assert_eq!(
            trusted.compatibility_with(&width),
            Err(Incompatibility::WireWidth)
        );

        let mut range = trusted.clone();
        range.range = Some(ValueRange::new(0, 500));
        assert_eq!(
            trusted.compatibility_with(&range),
            Err(Incompatibility::Range)
        );

        let mut category = trusted.clone();
        category.category = PortableCategory::MouseKeys;
        assert_eq!(
            trusted.compatibility_with(&category),
            Err(Incompatibility::Category)
        );

        let mut bits = PortableSettingSpec::qmk_bits(
            PortableCategory::Magic,
            "magic",
            21,
            WireWidth::Bits16,
            vec!["swap_control_caps_lock".into()],
        );
        let trusted_bits = bits.clone();
        bits.bit_meanings.push("caps_lock_as_control".into());
        assert_eq!(
            trusted_bits.compatibility_with(&bits),
            Err(Incompatibility::BitContract)
        );

        let mut variants = PortableSettingSpec::qmk_select(
            PortableCategory::Module,
            "mode",
            130,
            WireWidth::Bits8,
            vec!["Off".into(), "On".into()],
        );
        let trusted_variants = variants.clone();
        variants.ordered_variants.swap(0, 1);
        assert_eq!(
            trusted_variants.compatibility_with(&variants),
            Err(Incompatibility::OrderedVariants)
        );
    }

    #[test]
    fn linked_qsids_are_part_of_stable_identity() {
        let primary =
            PortableSettingId::qmk(PortableCategory::LayerLeds, "layer_color", 150, [151, 152]);
        let reordered =
            PortableSettingId::qmk(PortableCategory::LayerLeds, "layer_color", 150, [152, 151]);
        let different =
            PortableSettingId::qmk(PortableCategory::LayerLeds, "layer_color", 150, [151]);

        assert_eq!(primary, reordered);
        assert_ne!(primary, different);
    }

    #[test]
    fn grouped_diff_separates_changed_and_incompatible_fields() {
        let trusted = PortableSetting::new(quick_tap_spec(), PortableValue::Unsigned(200)).unwrap();
        let current = PortableSetting::new(quick_tap_spec(), PortableValue::Unsigned(0)).unwrap();
        let mut incompatible_spec = known_qmk_setting(143).unwrap();
        incompatible_spec.range = Some(ValueRange::new(0, 3));
        let old_auto =
            PortableSetting::new(known_qmk_setting(143).unwrap(), PortableValue::Unsigned(2))
                .unwrap();
        let new_auto = PortableSetting::new(incompatible_spec, PortableValue::Unsigned(1)).unwrap();

        let diff = grouped_diff([&trusted, &old_auto], [&current, &new_auto]);
        assert_eq!(diff.changed_count(), 1);
        assert_eq!(diff.incompatible_count(), 1);
        assert_eq!(diff.groups[0].category, PortableCategory::TapHold);
    }
}
