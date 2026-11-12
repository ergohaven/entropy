//! Resolves native keyboard evidence to the existing static legend catalog.
use crate::keycode::KeyLegendLayout;
use std::time::{Duration, Instant};

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
    Unsupported,
    Ambiguous,
    Stale,
    VariantUnavailable,
}

impl DetectionFailure {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Pending => "ui.legend_pending",
            Self::Unavailable => "ui.legend_unavailable",
            Self::Unsupported => "ui.legend_unsupported",
            Self::Ambiguous => "ui.legend_ambiguous",
            Self::Stale => "ui.legend_stale",
            Self::VariantUnavailable => "ui.legend_variant_unavailable",
        }
    }
}

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
    match id.as_str() {
        "00000409" | "00000419" | "00010419" => Ok(id),
        _ => Err(DetectionFailure::Unsupported),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeLayoutIdentity {
    Windows(String),
    Mac {
        source_id: String,
        keyboard_type: u32,
        glyphs: Glyphs,
    },
    Xkb {
        backend: &'static str,
        layout: String,
        variant: Option<String>,
        options: String,
        glyphs: Glyphs,
    },
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
            Self::Windows(id) => format!("Windows · {id}"),
            Self::Mac {
                source_id,
                keyboard_type,
                ..
            } => format!("macOS · {source_id} · {keyboard_type}"),
            Self::Xkb {
                backend,
                layout,
                variant,
                ..
            } => format!(
                "{backend} · {layout}({})",
                variant.as_deref().unwrap_or("?")
            ),
            Self::Unavailable(_) => String::new(),
            Self::Ibus {
                engine,
                layout,
                variant,
                ..
            } => format!(
                "IBus · {engine} · {layout}({})",
                variant.as_deref().unwrap_or("?")
            ),
        }
    }

    pub fn profile(&self) -> Result<KeyLegendLayout, DetectionFailure> {
        use KeyLegendLayout::{English, Russian, RussianTypewriter};
        let (family, glyphs) = match self {
            // Built-in identities only. See Microsoft's kbdus/kbdru/kbdru1
            // mapping references; native resolution must reject custom HKLs.
            Self::Windows(id) => {
                return match id.as_str() {
                    "00000409" => Ok(English),
                    "00000419" => Ok(Russian),
                    "00010419" => Ok(RussianTypewriter),
                    _ => Err(DetectionFailure::Unsupported),
                }
            }
            Self::Mac {
                source_id, glyphs, ..
            } => {
                let family = match source_id.as_str() {
                    "com.apple.keylayout.US" => English,
                    "com.apple.keylayout.RussianWin" => Russian,
                    _ => return Err(DetectionFailure::Unsupported),
                };
                (family, glyphs)
            }
            Self::Xkb {
                layout,
                variant,
                options,
                glyphs,
                ..
            } => {
                // Options may remap printable physical positions. Until an
                // option has independent evidence, use the manual fallback.
                if !xkb_options_supported(options) {
                    return Err(DetectionFailure::Unsupported);
                }
                let family = match (layout.as_str(), variant.as_deref()) {
                    ("us", Some("")) => English,
                    ("ru", Some("")) => Russian,
                    ("ru", Some("typewriter")) => RussianTypewriter,
                    _ => return Err(DetectionFailure::Unsupported),
                };
                (family, glyphs)
            }
            Self::Unavailable(reason) => return Err(*reason),
            // Global-engine metadata is retained, but GNOME pass-through
            // engines have not been verified to follow the selected source.
            Self::Ibus { .. } => return Err(DetectionFailure::VariantUnavailable),
        };
        if glyphs_match(glyphs, family) {
            Ok(family)
        } else {
            Err(DetectionFailure::Unsupported)
        }
    }
}

pub fn xkb_options_supported(options: &str) -> bool {
    options
        .split(',')
        .filter(|option| !option.is_empty())
        .all(|option| {
            matches!(
                option,
                "grp:alt_shift_toggle"
                    | "grp:ctrl_shift_toggle"
                    | "grp:win_space_toggle"
                    | "grp:menu_toggle"
                    | "grp:caps_toggle"
                    | "grp_led:scroll"
                    | "grp_led:caps"
            )
        })
}

/// Components have explicit one-based group assignments; neutral components
/// do not consume a group. Reject additions we cannot interpret.
pub fn xkb_group(symbols: &str, group: usize) -> Option<(String, String)> {
    let mut result = None;
    for component in symbols.split('+') {
        if component == "pc" || component.starts_with("inet(") || component.starts_with("group(") {
            continue;
        }
        let (component, assigned) = match component.rsplit_once(':') {
            Some((name, number)) => (name, number.parse::<usize>().ok()?.checked_sub(1)?),
            None => (component, 0),
        };
        let (layout, variant) = if let Some((layout, variant)) = component.split_once('(') {
            (layout, variant.strip_suffix(')')?)
        } else {
            (component, "")
        };
        if !matches!(layout, "us" | "ru") {
            return None;
        }
        let variant = match (layout, variant) {
            ("us", "basic") | ("ru", "winkeys") => "",
            _ => variant,
        };
        if assigned == group {
            if result.is_some() {
                return None;
            }
            result = Some((layout.to_owned(), variant.to_owned()));
        }
    }
    result
}

/// Every printable main-block position must be present; available ISO
/// positions must agree too. Never match from a number-row spot check.
pub fn glyphs_match(glyphs: &[(u16, char, char)], profile: KeyLegendLayout) -> bool {
    (4..=0x38)
        .filter(|hid| !matches!(hid, 0x28..=0x2c | 0x32))
        .all(|hid| {
            glyphs
                .iter()
                .find(|(key, _, _)| *key == hid)
                .is_some_and(|(_, direct, shift)| {
                    crate::keycode::legend_printable_glyphs(hid, profile) == Some((*direct, *shift))
                })
        })
        && glyphs.iter().all(|(hid, direct, shift)| {
            crate::keycode::legend_printable_glyphs(*hid, profile) == Some((*direct, *shift))
        })
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
        russian_first: bool,
    ) -> Result<KeyLegendLayout, DetectionFailure> {
        if now >= self.valid_until {
            return Err(DetectionFailure::Stale);
        }
        let family = self.observation.identity.profile()?;
        Ok(if russian_first {
            family.toggled_order()
        } else {
            family
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xkb_groups_preserve_assignments_and_reject_ambiguous_components() {
        let names = "pc+us+inet(evdev)+ru:2+group(alt_shift_toggle)+ru(typewriter):3";
        assert_eq!(xkb_group(names, 0), Some(("us".into(), "".into())));
        assert_eq!(xkb_group(names, 1), Some(("ru".into(), "".into())));
        assert_eq!(
            xkb_group(names, 2),
            Some(("ru".into(), "typewriter".into()))
        );
        assert_eq!(
            xkb_group("pc+ru(typewriter)+us:2", 0),
            Some(("ru".into(), "typewriter".into()))
        );
        assert_eq!(xkb_group("pc+us+ru:2", 2), None);
        assert_eq!(xkb_group("pc+us+ru:1", 0), None);
        assert_eq!(xkb_group("pc+us+custom(mapping)", 0), None);
        assert!(!xkb_options_supported(
            "grp:alt_shift_toggle,shift:both_capslock"
        ));
        assert!(!xkb_options_supported("custom:printable"));
        assert!(xkb_options_supported("grp:alt_shift_toggle,grp_led:scroll"));
    }

    #[test]
    fn compiled_xkb_fixtures_match_ansi_profiles_but_reject_different_iso_positions() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("qmk_hid_host/xkb_layout_fixtures.json")).unwrap();
        for sample in fixture["samples"].as_array().unwrap() {
            let layout = sample["layout"].as_str().unwrap();
            let variant = sample["variant"].as_str().unwrap();
            let glyphs: Glyphs = sample["glyphs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    (
                        row[0].as_u64().unwrap() as u16,
                        char::from_u32(row[1].as_u64().unwrap() as u32).unwrap(),
                        char::from_u32(row[2].as_u64().unwrap() as u32).unwrap(),
                    )
                })
                .collect();
            let mut identity = NativeLayoutIdentity::Xkb {
                backend: "fixture",
                layout: layout.into(),
                variant: Some(variant.into()),
                options: String::new(),
                glyphs,
            };
            let family = match (layout, variant) {
                ("us", _) => KeyLegendLayout::English,
                (_, "typewriter") => KeyLegendLayout::RussianTypewriter,
                _ => KeyLegendLayout::Russian,
            };
            let expected = if sample["model"] == "pc104" {
                Ok(family)
            } else {
                Err(DetectionFailure::Unsupported)
            };
            assert_eq!(
                identity.profile(),
                expected,
                "{layout}({variant}) {}",
                sample["model"]
            );
            if let NativeLayoutIdentity::Xkb { glyphs, .. } = &mut identity {
                glyphs[0].1 = 'x';
            }
            assert_eq!(identity.profile(), Err(DetectionFailure::Unsupported));
        }
    }

    #[test]
    fn native_macos_fixtures_require_complete_equivalence_and_exact_source_ids() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("qmk_hid_host/macos_layout_fixtures.json")).unwrap();
        for sample in fixture["samples"].as_array().unwrap() {
            let id = sample["source_id"].as_str().unwrap();
            let keyboard_type = sample["keyboard_type"].as_u64().unwrap() as u32;
            let glyphs: Glyphs = sample["glyphs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    (
                        row[0].as_u64().unwrap() as u16,
                        char::from_u32(row[1].as_u64().unwrap() as u32).unwrap(),
                        char::from_u32(row[2].as_u64().unwrap() as u32).unwrap(),
                    )
                })
                .collect();
            let identity = NativeLayoutIdentity::Mac {
                source_id: id.into(),
                keyboard_type,
                glyphs: glyphs.clone(),
            };
            if id == "com.apple.keylayout.US" && keyboard_type == 40 {
                assert_eq!(identity.profile(), Ok(KeyLegendLayout::English));
            }
            if id != "com.apple.keylayout.US" || keyboard_type != 40 {
                assert_eq!(identity.profile(), Err(DetectionFailure::Unsupported));
            }
            let custom = NativeLayoutIdentity::Mac {
                source_id: "custom.Russian".into(),
                keyboard_type,
                glyphs,
            };
            assert_eq!(custom.profile(), Err(DetectionFailure::Unsupported));
        }
    }

    #[test]
    fn windows_hkl_resolution_rejects_unknown_and_ambiguous_registry_records() {
        // Synthetic values following WinForms' documented implementation
        // precedent, not captured evidence from a native Windows run.
        let records = vec![("00010419".to_owned(), 1)];
        assert_eq!(windows_klid(0x04190419, &[]), Ok("00000419".into()));
        assert_eq!(windows_klid(0xf0010419, &records), Ok("00010419".into()));
        assert_eq!(windows_klid(0x04090409, &[]), Ok("00000409".into()));
        assert_eq!(
            windows_klid(0xf0010419, &[]),
            Err(DetectionFailure::Unavailable)
        );
        let mut ambiguous = records;
        ambiguous.push(("a0010419".into(), 1));
        assert_eq!(
            windows_klid(0xf0010419, &ambiguous),
            Err(DetectionFailure::Ambiguous)
        );
        assert_eq!(
            windows_klid(0x12340419, &[]),
            Err(DetectionFailure::Unsupported)
        );
        assert_eq!(windows_klid(0, &[]), Err(DetectionFailure::Unavailable));
    }

    #[test]
    fn exact_windows_variants_and_automatic_order_are_distinct() {
        let now = Instant::now();
        let sample = LayoutSample::new(
            LayoutObservation {
                language: Some(1),
                identity: NativeLayoutIdentity::Windows("00010419".into()),
            },
            now,
            Duration::from_millis(100),
        );
        assert_eq!(
            sample.profile(now, true),
            Ok(KeyLegendLayout::RussianTypewriterPrimary)
        );
        assert_eq!(
            sample.profile(now + Duration::from_secs(6), false),
            Err(DetectionFailure::Stale)
        );
        assert_eq!(
            NativeLayoutIdentity::Windows("custom-ru".into()).profile(),
            Err(DetectionFailure::Unsupported)
        );
    }

    #[test]
    fn deadlines_expire_without_worker_progress_and_unchanged_queries_renew_them() {
        for interval in [Duration::from_millis(100), Duration::from_secs(10)] {
            let now = Instant::now();
            let observation = LayoutObservation {
                language: Some(0),
                identity: NativeLayoutIdentity::Windows("00000409".into()),
            };
            let first = LayoutSample::new(observation.clone(), now, interval);
            assert_eq!(
                first.profile(first.valid_until, false),
                Err(DetectionFailure::Stale)
            );
            let renewed = LayoutSample::new(observation, now + interval, interval);
            assert_eq!(
                renewed.profile(first.valid_until, true),
                Ok(KeyLegendLayout::English)
            );
        }
    }
}
