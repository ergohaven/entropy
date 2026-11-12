use super::*;
use crate::key_legend_detection::{LayoutObservation, NativeLayoutIdentity};

#[derive(Clone)]
struct LegendDesktop(Arc<Mutex<NativeLayoutIdentity>>);

impl DesktopSource for LegendDesktop {
    fn volume(&mut self) -> Option<u8> {
        None
    }
    fn layout(&mut self) -> LayoutObservation {
        LayoutObservation {
            language: Some(1),
            identity: self.0.lock().unwrap().clone(),
        }
    }
    fn media(&mut self) -> Option<MediaInfo> {
        None
    }
}

fn await_legend(subscription: &LegendLayoutSubscription, klid: &str) -> LayoutSample {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(sample) = subscription.snapshot() {
            if sample.observation.identity == NativeLayoutIdentity::Windows(klid.to_owned()) {
                return sample;
            }
        }
        assert!(Instant::now() < until, "legend sample never arrived");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn legends_start_without_a_bridge_and_publish_same_language_changes() {
    let identity = Arc::new(Mutex::new(NativeLayoutIdentity::Windows("00000419".into())));
    let service = HostDataService::start({
        let identity = identity.clone();
        move || LegendDesktop(identity.clone())
    });
    let repaints = Arc::new(AtomicU64::new(0));
    let ctx = egui::Context::default();
    ctx.set_request_repaint_callback({
        let repaints = repaints.clone();
        move |_| {
            repaints.fetch_add(1, Ordering::Relaxed);
        }
    });
    let subscription = service.subscribe_legends(ctx);
    let first = await_legend(&subscription, "00000419");
    assert_eq!(service.snapshot().layout, Some(1));
    *identity.lock().unwrap() = NativeLayoutIdentity::Windows("00010419".into());
    let second = await_legend(&subscription, "00010419");
    assert!(second.sampled_at > first.sampled_at);
    assert_eq!(service.snapshot().layout, Some(1));
    assert!(repaints.load(Ordering::Relaxed) > 0);
    drop(subscription);
    let state = service.owner.shared.state.lock().unwrap();
    assert_eq!(state.demand, [0, 0, 0]);
    assert!(state.legend_listeners.is_empty());
    assert!(state.snapshot.layout_sample.is_none());
}

#[test]
fn dropping_legends_preserves_bridge_demand_and_removes_only_its_listener() {
    let service = HostDataService::start(|| {
        LegendDesktop(Arc::new(Mutex::new(NativeLayoutIdentity::Windows(
            "00000419".into(),
        ))))
    });
    let bridge = Arc::new(BridgeWake::default());
    let _bridge_subscription = service.subscribe(
        HostDataMode {
            layout: true,
            ..Default::default()
        },
        &bridge,
    );
    let a = service.subscribe_legends(egui::Context::default());
    let b = service.subscribe_legends(egui::Context::default());
    await_legend(&a, "00000419");
    drop(a);
    assert_eq!(service.owner.shared.state.lock().unwrap().demand[1], 2);
    drop(b);
    let state = service.owner.shared.state.lock().unwrap();
    assert_eq!(state.demand[1], 1);
    assert!(state.legend_listeners.is_empty());
    assert_eq!(state.listeners.len(), 1);
    assert!(state.snapshot.layout_sample.is_some());
}
