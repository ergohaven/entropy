use super::*;
use crate::key_legend_detection::{DetectionFailure, LayoutSample, LegendMode};
use std::time::Instant;

impl EntropyApp {
    pub(crate) fn key_legend_layout(&self) -> KeyLegendLayout {
        match self.app_settings.key_legend_mode {
            LegendMode::Manual => self.app_settings.key_legend_layout.clone(),
            LegendMode::FollowSystem => self
                .legend_detected
                .clone()
                .unwrap_or_else(|_| self.app_settings.key_legend_layout.clone()),
        }
    }

    pub(crate) fn resolve_key_legends(&mut self, now: Instant) {
        self.legend_detected = self
            .legend_sample
            .as_ref()
            .ok_or(DetectionFailure::Pending)
            .and_then(|sample| {
                sample.profile(now, self.app_settings.automatic_legend_system_first)
            });
    }

    pub(crate) fn update_key_legends(&mut self, ctx: &egui::Context) {
        if self.app_settings.key_legend_mode == LegendMode::Manual {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.legend_subscription = None;
            }
            self.legend_sample = None;
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            if !self.headless {
                let subscription = self.legend_subscription.get_or_insert_with(|| {
                    crate::qmk_hid_host::subscribe_legend_layout(ctx.clone())
                });
                self.legend_sample = subscription.snapshot();
            }
        }
        let now = Instant::now();
        self.resolve_key_legends(now);
        if self.app_settings.key_legend_mode == LegendMode::FollowSystem {
            if let Some(sample) = &self.legend_sample {
                if sample.valid_until > now {
                    ctx.request_repaint_after(sample.valid_until - now);
                }
            }
        }
    }

    pub(crate) fn key_legend_order_profile(&self) -> Option<KeyLegendLayout> {
        let profile = match self.app_settings.key_legend_mode {
            LegendMode::Manual => self.app_settings.key_legend_layout.clone(),
            LegendMode::FollowSystem => self.legend_detected.clone().ok()?,
        };
        profile.is_multilingual().then_some(profile)
    }

    pub(crate) fn toggle_key_legend_order(&mut self) -> bool {
        let Some(profile) = self.key_legend_order_profile() else {
            return false;
        };
        match self.app_settings.key_legend_mode {
            LegendMode::Manual => self.app_settings.key_legend_layout = profile.toggled_order(),
            LegendMode::FollowSystem => {
                self.app_settings.automatic_legend_system_first =
                    !self.app_settings.automatic_legend_system_first;
                self.resolve_key_legends(Instant::now());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_legend_detection::{
        test_typewriter_layout, LayoutObservation, NativeLayoutIdentity,
    };
    use std::time::Duration;
    fn sample(profile: KeyLegendLayout, now: Instant) -> LayoutSample {
        let KeyLegendLayout::System { keymap, .. } = profile else {
            panic!()
        };
        LayoutSample::new(
            LayoutObservation {
                language: Some(1),
                identity: NativeLayoutIdentity::Mapped(keymap),
            },
            now,
            Duration::from_millis(100),
        )
    }
    #[test]
    fn automatic_legends_restore_manual_mapping_and_keep_order_separate() {
        let mut app = EntropyApp::new_inert_for_test();
        let manual = test_typewriter_layout(false);
        app.app_settings.key_legend_layout = manual.clone();
        app.app_settings.key_legend_mode = LegendMode::FollowSystem;
        let now = Instant::now();
        app.legend_sample = Some(sample(manual.clone(), now));
        app.resolve_key_legends(now);
        assert_eq!(app.key_legend_layout(), manual.toggled_order());
        assert!(app.toggle_key_legend_order());
        assert_eq!(app.key_legend_layout(), manual);
        assert_eq!(app.app_settings.key_legend_layout, manual);
        app.resolve_key_legends(now + Duration::from_secs(6));
        assert_eq!(app.key_legend_layout(), manual);
        assert_eq!(app.key_legend_order_profile(), None);
        assert!(!app.toggle_key_legend_order());
        app.app_settings.key_legend_mode = LegendMode::Manual;
        assert_eq!(app.key_legend_layout(), manual);
    }
    #[test]
    fn automatic_samples_do_not_replace_saved_manual_or_export_mappings() {
        let mut app = EntropyApp::new_inert_for_test();
        app.app_settings.key_legend_mode = LegendMode::FollowSystem;
        let before = serde_json::to_string(&app.app_settings).unwrap();
        let now = Instant::now();
        app.resolve_key_legends(now);
        assert_eq!(app.legend_detected, Err(DetectionFailure::Pending));
        app.legend_sample = Some(sample(test_typewriter_layout(false), now));
        app.resolve_key_legends(now);
        assert!(matches!(
            app.key_legend_layout(),
            KeyLegendLayout::System { .. }
        ));
        assert_eq!(serde_json::to_string(&app.app_settings).unwrap(), before);
        app.resolve_key_legends(now + Duration::from_secs(6));
        assert_eq!(app.key_legend_layout(), KeyLegendLayout::English);
        assert_eq!(
            app.app_settings.layout_image_export.key_legend_layout,
            KeyLegendLayout::English
        );
    }
}
