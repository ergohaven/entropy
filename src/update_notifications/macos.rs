//! macOS delivery only. Release eligibility, history, and routing live in the parent module.
use super::{Event, Notification};
use block2::{DynBlock, RcBlock};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{Bool, ProtocolObject},
    AnyThread, DefinedClass,
};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSProcessInfo, NSString};
use objc2_user_notifications::*;
use std::sync::mpsc;

#[allow(deprecated)]
fn presentation_options_for_major_version(major: isize) -> UNNotificationPresentationOptions {
    if major >= 11 {
        UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List
    } else {
        UNNotificationPresentationOptions::Alert
    }
}

struct DelegateState {
    sender: mpsc::Sender<Event>,
    ctx: std::sync::OnceLock<egui::Context>,
}

impl DelegateState {
    fn emit(&self, event: Event) {
        let _ = self.sender.send(event);
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "EntropyUpdateNotificationDelegate"]
    #[ivars = DelegateState]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((presentation_options_for_major_version(
                NSProcessInfo::processInfo()
                    .operatingSystemVersion()
                    .majorVersion,
            ),));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn respond(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &DynBlock<dyn Fn()>,
        ) {
            // A dismiss action must never reopen the window.
            if response
                .actionIdentifier()
                .isEqualToString(unsafe { UNNotificationDefaultActionIdentifier })
            {
                let id = response.notification().request().identifier().to_string();
                if let Some((key, version)) = id
                    .strip_prefix("entropy:")
                    .and_then(|id| id.rsplit_once(':'))
                {
                    self.ivars()
                        .emit(Event::Clicked(key.to_owned(), version.to_owned()));
                }
            }
            completion.call(());
        }
    }
);

struct NativeCenter {
    center: Retained<UNUserNotificationCenter>,
    // UNUserNotificationCenter holds its delegate weakly.
    _delegate: Retained<NotificationDelegate>,
    receiver: mpsc::Receiver<Event>,
    sender: mpsc::Sender<Event>,
    ctx: egui::Context,
}

thread_local! {
    static PREPARED_CENTER: std::cell::RefCell<Option<NativeCenter>> = const { std::cell::RefCell::new(None) };
}

// Apple requires a delegate before application:didFinishLaunching: returns.
// Queue click events until eframe creates the app and supplies its repaint context.
pub(crate) fn prepare() {
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }
    PREPARED_CENTER.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let delegate = NotificationDelegate::alloc().set_ivars(DelegateState {
            sender: sender.clone(),
            ctx: std::sync::OnceLock::new(),
        });
        // SAFETY: NSObject init initializes our freshly allocated subclass; ivars are already set.
        let delegate: Retained<NotificationDelegate> = unsafe { msg_send![super(delegate), init] };
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        slot.replace(Some(NativeCenter {
            center,
            _delegate: delegate,
            receiver,
            sender,
            ctx: egui::Context::default(),
        }));
    });
}

pub(super) const SUPPORTED: bool = true;

#[derive(Default)]
pub(super) struct Backend {
    native: Option<NativeCenter>,
}

impl Backend {
    pub(super) fn initialize(&mut self, ctx: &egui::Context) {
        // currentNotificationCenter raises an ObjC exception in unbundled Cargo/test hosts.
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            return;
        }
        prepare();
        self.native = PREPARED_CENTER.with(|slot| slot.borrow_mut().take());
        if let Some(native) = &mut self.native {
            native.ctx = ctx.clone();
            let _ = native._delegate.ivars().ctx.set(ctx.clone());
        }
    }

    pub(super) fn available(&self) -> bool {
        self.native.is_some()
    }

    pub(super) fn request_permission(&self) -> bool {
        let Some(native) = &self.native else {
            return false;
        };
        let sender = native.sender.clone();
        let ctx = native.ctx.clone();
        let callback = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            if !granted.as_bool() {
                let _ = sender.send(Event::PermissionDenied);
                ctx.request_repaint();
            }
        });
        native
            .center
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert,
                &callback,
            );
        true
    }

    pub(super) fn authorize(&self, notification: Notification) {
        let Some(native) = &self.native else {
            return;
        };
        let sender = native.sender.clone();
        let ctx = native.ctx.clone();
        let authorization = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            let event = if granted.as_bool() {
                Event::Authorized(notification.clone())
            } else {
                Event::PermissionDenied
            };
            let _ = sender.send(event);
            ctx.request_repaint();
        });
        native
            .center
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert,
                &authorization,
            );
    }

    pub(super) fn submit(&self, notification: Notification) {
        let Some(native) = &self.native else {
            return;
        };
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&format!(
                "entropy:{}:{}",
                notification.key, notification.version
            )),
            &content,
            None,
        );
        let sender = native.sender.clone();
        let ctx = native.ctx.clone();
        let completion = RcBlock::new(move |error: *mut NSError| {
            let event = if error.is_null() {
                Event::Delivered(notification.key.clone(), notification.version.clone())
            }
            // SAFETY: NSError is valid for the duration of Apple's completion callback.
            else {
                Event::Failed {
                    key: notification.key.clone(),
                    version: notification.version.clone(),
                    error: unsafe { &*error }.localizedDescription().to_string(),
                }
            };
            let _ = sender.send(event);
            ctx.request_repaint();
        });
        native
            .center
            .addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    }

    pub(super) fn drain(&self) -> Vec<Event> {
        self.native
            .as_ref()
            .map(|native| native.receiver.try_iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn foreground_presentation_supports_the_macos_10_15_deployment_target() {
        assert_eq!(
            presentation_options_for_major_version(10),
            UNNotificationPresentationOptions::Alert
        );
        assert_eq!(
            presentation_options_for_major_version(11),
            UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List
        );
    }

    #[test]
    fn unbundled_test_host_does_not_initialize_native_notification_center() {
        if NSBundle::mainBundle().bundleIdentifier().is_some() {
            return;
        }
        let mut backend = Backend::default();
        backend.initialize(&egui::Context::default());
        assert!(!backend.available());
        assert!(!backend.request_permission());
    }
}
