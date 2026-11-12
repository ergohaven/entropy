//! Owned system keyboard mappings and freshness of native layout observations.
use crate::keycode::KeyLegendLayout;
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, sync::Arc};

/// A single key's output, including spacing labels for dead keys. Empty text
/// means this modifier level does not produce a printable character.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct KeyGlyph {
    pub text: String,
    pub dead: bool,
}

impl KeyGlyph {
    pub fn new(text: String, dead: bool) -> Self {
        Self {
            text: text.chars().filter(|c| !c.is_control()).collect(),
            dead,
        }
    }

    pub fn label(&self) -> String {
        if self.dead && !self.text.is_empty() {
            format!("◌{}", self.text)
        } else {
            self.text.clone()
        }
    }
}

/// Owned OS data, indexed by USB HID physical position. Levels are direct,
/// Shift, AltGr/Option, and Shift+AltGr/Option. A saved selection includes the
/// mapping so exports and manual selection also work after its source disappears.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SystemKeymap {
    pub id: String,
    pub name: String,
    pub keys: BTreeMap<u16, [KeyGlyph; 4]>,
}

impl SystemKeymap {
    #[cfg(test)]
    pub fn from_glyphs(id: String, name: String, glyphs: &Glyphs) -> Arc<Self> {
        Arc::new(Self {
            id,
            name,
            keys: glyphs
                .iter()
                .map(|(hid, base, shift)| {
                    (
                        *hid,
                        [
                            KeyGlyph::new(base.to_string(), false),
                            KeyGlyph::new(shift.to_string(), false),
                            KeyGlyph::default(),
                            KeyGlyph::default(),
                        ],
                    )
                })
                .collect(),
        })
    }

    pub fn legend(&self, value: u16, english_first: bool, shifted: bool) -> Option<String> {
        let (hid, level) = if value < 0x100 {
            (value, None)
        } else if (0x100..0x2000).contains(&value) {
            let mods = value >> 8;
            let altgr = mods & 0x1d == 0x14 || mods & 0x0d == 0x05;
            let shift = mods & 2 != 0;
            if !altgr && mods & 0x0f & !2 != 0 {
                return None;
            }
            (
                value & 0xff,
                Some(usize::from(shift) + if altgr { 2 } else { 0 }),
            )
        } else {
            return None;
        };
        if hid == 44 {
            return None;
        }
        let keys = self.keys.get(&hid)?;
        let english =
            crate::keycode::printable_output_for_hid(hid, crate::keycode::KeyOutputLayout::English);
        let pair = |selected: String, us: String| {
            if selected.is_empty() {
                us
            } else if us.is_empty() || selected == us {
                selected
            } else if english_first {
                format!("{us}  {selected}")
            } else {
                format!("{selected}  {us}")
            }
        };
        if let Some(level) = level {
            let selected = keys[level].label();
            if selected.is_empty() {
                return None;
            }
            if level == 1 {
                let us = english
                    .and_then(|(_, c)| c)
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                if !us.is_empty() && !selected.is_empty() && us != selected {
                    return Some(if english_first {
                        format!("{us}\n{selected}")
                    } else {
                        format!("{selected}\n{us}")
                    });
                }
            }
            return Some(selected);
        }
        let base = pair(
            keys[0].label(),
            english.map(|(c, _)| c.to_string()).unwrap_or_default(),
        );
        let upper = pair(
            keys[1].label(),
            english
                .and_then(|(_, c)| c)
                .map(|c| c.to_string())
                .unwrap_or_default(),
        );
        let mut bottom = base;
        let mut top = upper;
        if !keys[2].text.is_empty() && keys[2] != keys[0] {
            bottom.push_str(&format!(" · {}", keys[2].label()));
        }
        if !keys[3].text.is_empty() && keys[3] != keys[1] {
            top.push_str(&format!(" · {}", keys[3].label()));
        }
        if shifted && top != bottom && !top.is_empty() {
            Some(format!("{top}\n{bottom}"))
        } else {
            Some(bottom)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemLayoutDescriptor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegendMode {
    #[default]
    Manual,
    FollowSystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionFailure {
    Pending,
    Unavailable,
    Ambiguous,
    Stale,
    VariantUnavailable,
}

impl DetectionFailure {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Pending => "ui.legend_pending",
            Self::Unavailable => "ui.legend_unavailable",
            Self::Ambiguous => "ui.legend_ambiguous",
            Self::Stale => "ui.legend_stale",
            Self::VariantUnavailable => "ui.legend_variant_unavailable",
        }
    }
}

#[cfg(test)]
pub type Glyphs = Vec<(u16, char, char)>;

/// Follows WinForms' HKL convention without activating a source. Special
/// device identifiers must have exactly one registry match.
pub fn windows_klid(hkl: usize, records: &[(String, u16)]) -> Result<String, DetectionFailure> {
    if hkl == 0 {
        return Err(DetectionFailure::Unavailable);
    }
    let device = ((hkl >> 16) & 0xffff) as u16;
    let id = if device & 0xf000 == 0xf000 {
        let mut matches = records.iter().filter(|(_, id)| *id == device & 0x0fff);
        let first = matches.next().ok_or(DetectionFailure::Unavailable)?;
        if matches.next().is_some() {
            return Err(DetectionFailure::Ambiguous);
        }
        first.0.to_ascii_uppercase()
    } else {
        format!(
            "{:08X}",
            if device == 0 {
                (hkl & 0xffff) as u16
            } else {
                device
            }
        )
    };
    Ok(id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeLayoutIdentity {
    Mapped(Arc<SystemKeymap>),
    Ibus {
        engine: String,
        layout: String,
        variant: Option<String>,
        options: Option<String>,
    },
    Unavailable(DetectionFailure),
}
impl NativeLayoutIdentity {
    pub fn description(&self) -> String {
        match self {
            Self::Mapped(map) => map.name.clone(),
            Self::Ibus { engine, .. } => format!("IBus · {engine}"),
            Self::Unavailable(_) => String::new(),
        }
    }
    pub fn profile(&self) -> Result<KeyLegendLayout, DetectionFailure> {
        match self {
            Self::Mapped(keymap) => Ok(KeyLegendLayout::System {
                keymap: keymap.clone(),
                english_first: false,
            }),
            Self::Unavailable(reason) => Err(*reason),
            Self::Ibus { .. } => Err(DetectionFailure::Unavailable),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutObservation {
    pub language: Option<u8>,
    pub identity: NativeLayoutIdentity,
}

impl From<Option<u8>> for LayoutObservation {
    fn from(language: Option<u8>) -> Self {
        Self {
            language,
            identity: NativeLayoutIdentity::Unavailable(DetectionFailure::Unavailable),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayoutSample {
    pub observation: LayoutObservation,
    pub sampled_at: Instant,
    pub valid_until: Instant,
}

impl LayoutSample {
    pub fn new(observation: LayoutObservation, now: Instant, interval: Duration) -> Self {
        Self {
            observation,
            sampled_at: now,
            valid_until: now + interval + Duration::from_secs(5),
        }
    }

    pub fn profile(
        &self,
        now: Instant,
        system_first: bool,
    ) -> Result<KeyLegendLayout, DetectionFailure> {
        if now >= self.valid_until {
            return Err(DetectionFailure::Stale);
        }
        let family = self.observation.identity.profile()?;
        Ok(if !system_first {
            family.toggled_order()
        } else {
            family
        })
    }
}

#[cfg(test)]
pub(crate) fn test_typewriter_layout(english_first: bool) -> KeyLegendLayout {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("qmk_hid_host/xkb_layout_fixtures.json")).unwrap();
    let sample = fixture["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["layout"] == "ru" && s["variant"] == "typewriter" && s["model"] == "pc105")
        .unwrap();
    let glyphs = sample["glyphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            (
                g[0].as_u64().unwrap() as u16,
                char::from_u32(g[1].as_u64().unwrap() as u32).unwrap(),
                char::from_u32(g[2].as_u64().unwrap() as u32).unwrap(),
            )
        })
        .collect();
    KeyLegendLayout::System {
        keymap: SystemKeymap::from_glyphs(
            "xkb:ru:typewriter".into(),
            "Russian Typewriter".into(),
            &glyphs,
        ),
        english_first,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_mapping_keeps_space_and_nonproducing_modifier_actions_visible() {
        let map = SystemKeymap::from_glyphs("xkb:us:".into(), "US".into(), &vec![(4, 'a', 'A'), (44, ' ', ' ')]);
        let layout = KeyLegendLayout::System { keymap: map, english_first: false };
        let label = |value, layout| crate::keycode::keycode_label_with_names_and_layout(value, &[], &[], layout);
        assert_eq!(label(44, layout.clone()), label(44, KeyLegendLayout::English));
        let modifier = label(0x1404, layout);
        assert!(!modifier.trim().is_empty());
        assert!(modifier.contains('a'));
        assert!(modifier.contains("Alt"));
    }
    #[test]
    fn system_keymaps_are_not_restricted_to_a_language_catalog() {
        let identity = NativeLayoutIdentity::Mapped(SystemKeymap::from_glyphs(
            "com.apple.keylayout.French".into(),
            "French".into(),
            &vec![(4, 'q', 'Q'), (20, 'a', 'A'), (30, '&', '1')],
        ));
        let profile = identity.profile().unwrap();
        let text = crate::keycode::keycode_label_with_names_and_layout(4, &[], &[], profile);
        assert!(text.contains('q'));
        assert!(text.contains('Q'));
    }
    #[test]
    fn translated_glyphs_and_dead_keys_survive_serialization_and_modifier_labels() {
        let mut map = (*SystemKeymap::from_glyphs(
            "xkb:de:".into(),
            "German".into(),
            &vec![(20, 'q', 'Q'), (46, '´', '`')],
        ))
        .clone();
        map.keys.get_mut(&20).unwrap()[2] = KeyGlyph::new("@".into(), false);
        map.keys.get_mut(&46).unwrap()[0].dead = true;
        let profile = KeyLegendLayout::System {
            keymap: Arc::new(map),
            english_first: false,
        };
        let restored: KeyLegendLayout =
            serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(profile, restored);
        assert_eq!(
            crate::keycode::keycode_label_with_names_and_layout(0x1414, &[], &[], restored.clone()),
            "@"
        );
        assert!(
            crate::keycode::keycode_label_with_names_and_layout(46, &[], &[], restored)
                .contains("◌´")
        );
    }
    #[test]
    fn samples_expire_and_layout_order_does_not_change_the_keymap() {
        let layout = test_typewriter_layout(false);
        let KeyLegendLayout::System { keymap, .. } = layout.clone() else {
            panic!()
        };
        let now = Instant::now();
        let sample = LayoutSample::new(
            LayoutObservation {
                language: Some(1),
                identity: NativeLayoutIdentity::Mapped(keymap),
            },
            now,
            Duration::from_millis(100),
        );
        assert_eq!(sample.profile(now, true), Ok(layout.clone()));
        assert_eq!(sample.profile(now, false), Ok(layout.toggled_order()));
        assert_eq!(
            sample.profile(now + Duration::from_secs(6), false),
            Err(DetectionFailure::Stale)
        );
    }
    #[test]
    fn windows_klids_accept_other_languages_and_still_reject_ambiguous_ids() {
        assert_eq!(windows_klid(0x04070407, &[]), Ok("00000407".into()));
        assert_eq!(
            windows_klid(0xf0010419, &[("00010419".into(), 1)]),
            Ok("00010419".into())
        );
        assert_eq!(
            windows_klid(
                0xf0010419,
                &[("00010419".into(), 1), ("00020419".into(), 1)]
            ),
            Err(DetectionFailure::Ambiguous)
        );
    }
}
