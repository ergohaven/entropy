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
                if !options.is_empty() {
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
        };
        if glyphs_match(glyphs, family) {
            Ok(family)
        } else {
            Err(DetectionFailure::Unsupported)
        }
    }
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
