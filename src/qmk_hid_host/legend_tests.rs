use super::*;
use crate::key_legend_detection::{LayoutObservation, NativeLayoutIdentity};

fn native_identity(id: &str) -> NativeLayoutIdentity {
    NativeLayoutIdentity::Mapped(crate::key_legend_detection::SystemKeymap::from_glyphs(
        id.into(),
        id.into(),
        &vec![(4, 'a', 'A')],
    ))
}

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
            if sample.observation.identity == native_identity(klid) {
                return sample;
            }
        }
        assert!(Instant::now() < until, "legend sample never arrived");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn legends_start_without_a_bridge_and_publish_same_language_changes() {
    let identity = Arc::new(Mutex::new(native_identity("00000419")));
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
    *identity.lock().unwrap() = native_identity("00010419");
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
    let service =
        HostDataService::start(|| LegendDesktop(Arc::new(Mutex::new(native_identity("00000419")))));
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

#[test]
fn repaint_callbacks_can_read_the_published_snapshot() {
    let service =
        HostDataService::start(|| LegendDesktop(Arc::new(Mutex::new(native_identity("00000419")))));
    let ctx = egui::Context::default();
    let (sent, received) = std::sync::mpsc::channel();
    let shared = Arc::downgrade(&service.owner.shared);
    ctx.set_request_repaint_callback(move |_| {
        if let Some(shared) = shared.upgrade() {
            let state = shared.state.lock().unwrap();
            let _ = sent.send(state.snapshot.layout_sample.is_some());
        }
    });
    let _subscription = service.subscribe_legends(ctx);
    assert!(received.recv_timeout(Duration::from_secs(5)).unwrap());
}

#[test]
fn an_unchanged_identity_repaints_when_fresh_evidence_recovers_from_expiry() {
    let service =
        HostDataService::start(|| LegendDesktop(Arc::new(Mutex::new(native_identity("00000419")))));
    let ctx = egui::Context::default();
    let repaints = Arc::new(AtomicU64::new(0));
    ctx.set_request_repaint_callback({
        let repaints = repaints.clone();
        move |_| {
            repaints.fetch_add(1, Ordering::Relaxed);
        }
    });
    let subscription = service.subscribe_legends(ctx.clone());
    await_legend(&subscription, "00000419");
    for _ in 0..3 {
        let _ = crate::app::discard_texture_deltas(ctx.run_ui(Default::default(), |_| {}));
    }
    let before = repaints.load(Ordering::Relaxed);
    {
        let mut state = service.owner.shared.state.lock().unwrap();
        // A fake expired deadline avoids waiting for the native query budget.
        state.snapshot.layout_sample.as_mut().unwrap().valid_until = Instant::now();
    }
    let until = Instant::now() + Duration::from_secs(5);
    while repaints.load(Ordering::Relaxed) == before {
        assert!(
            Instant::now() < until,
            "fresh unchanged evidence did not repaint"
        );
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        subscription
            .snapshot()
            .unwrap()
            .profile(Instant::now(), false),
        native_identity("00000419")
            .profile()
            .map(|layout| layout.toggled_order())
    );
}
