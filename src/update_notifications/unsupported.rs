//! Windows/Linux delivery is intentionally disabled until their native backends are implemented.
use super::{Event, Notification};

pub(super) const SUPPORTED: bool = false;

#[derive(Default)]
pub(super) struct Backend;

impl Backend {
    pub(super) fn initialize(&mut self, _ctx: &egui::Context) {}

    pub(super) fn available(&self) -> bool {
        false
    }

    pub(super) fn request_permission(&self) -> bool {
        false
    }

    pub(super) fn authorize(&self, _notification: Notification) {}

    pub(super) fn submit(&self, _notification: Notification) {}

    pub(super) fn drain(&self) -> Vec<Event> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_platform_never_prompts_or_delivers() {
        const { assert!(!SUPPORTED) };
        let mut backend = Backend;
        backend.initialize(&egui::Context::default());
        assert!(!backend.available());
        assert!(!backend.request_permission());
        let notification = Notification {
            key: "app".into(),
            version: "1.0.0".into(),
            title: "Update available".into(),
            body: "Entropy 1.0.0".into(),
        };
        backend.authorize(notification.clone());
        backend.submit(notification);
        assert!(backend.drain().is_empty());
    }
}
