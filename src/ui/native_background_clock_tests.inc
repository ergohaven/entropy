mod native_background_clock_tests {
    use super::*;

    fn raw_input(time: f64) -> egui::RawInput {
        egui::RawInput { time: Some(time), focused: false, ..Default::default() }
    }

    fn hidden_input(time: f64) -> egui::RawInput {
        let mut raw = raw_input(time);
        raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap().minimized = Some(true);
        raw
    }

    #[test]
    fn logic_only_advances_discovery_deadline_without_advancing_ui_time() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        let mut app = EntropyApp::new_inert_for_test();
        let ctx = egui::Context::default();
        let mut frame = eframe::Frame::_new_kittest();
        // Seed a real visible pass in the native integration's existing epoch.
        ctx.run_ui(raw_input(100.0), |_| {}).drop_without_applying_deltas();
        app.last_device_scan_at = 100.0;
        app.last_full_device_scan_at = 100.0;
        let (tx, scans) = mpsc::channel();
        app.test_device_scan_requests = Some(tx);
        let (tx, connects) = mpsc::channel();
        app.test_connect_requests = Some(tx);
        let passes = ctx.cumulative_pass_nr();

        // Match prepare_raw_input -> raw_input_hook -> real run_logic -> App::logic.
        // Only the integration time advances: no UI pass, sleep or hardware scan.
        for time in [101.0, 101.999] {
            let mut raw = hidden_input(time);
            eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
            let _output = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
            assert!(scans.try_recv().is_err(), "discovery ran before its 2s deadline");
        }
        let mut raw = hidden_input(102.0);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
        let _output = ctx.run_logic(&raw, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
        assert_eq!(ctx.input(|i| i.time), 100.0, "run_logic must leave UI time frozen");
        assert_eq!(ctx.cumulative_pass_nr(), passes, "background logic must not render");
        let (usb_only, complete) = scans.try_recv().expect("monotonic native time must release discovery deadline");
        assert!(!usb_only, "empty inventory reaches its ordinary full-discovery deadline");
        assert_eq!(app.last_device_scan_at, 102.0, "keep the visible frame clock epoch");
        complete.send(Ok(vec![])).unwrap();

        // Restoration uses the same native epoch, not a new relative clock.
        let mut raw = raw_input(102.5);
        eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
        ctx.run_ui(raw, |ui| eframe::App::logic(&mut app, ui.ctx(), &mut frame))
            .drop_without_applying_deltas();
        assert_eq!(ctx.input(|i| i.time), 102.5);
        assert_eq!(app.last_device_scan_at, 102.0);
        assert!(scans.try_recv().is_err());
        assert!(connects.try_recv().is_err());
    }

    #[test]
    fn native_clock_is_context_local_and_child_input_cannot_replace_root_time() {
        let _storage = crate::app::app_storage::ScopedTempConfig::new();
        for epoch in [10.0, 1000.0] {
            let mut app = EntropyApp::new_inert_for_test();
            let ctx = egui::Context::default();
            let mut frame = eframe::Frame::_new_kittest();
            ctx.run_ui(raw_input(epoch), |_| {}).drop_without_applying_deltas();
            app.last_device_scan_at = epoch;
            app.last_full_device_scan_at = epoch;
            let (tx, scans) = mpsc::channel();
            app.test_device_scan_requests = Some(tx);
            let mut root = raw_input(epoch + 2.0);
            eframe::App::raw_input_hook(&mut app, &ctx, &mut root);
            let mut child = raw_input(epoch + 999.0);
            child.viewport_id = egui::ViewportId::from_hash_of("clock-test-child");
            eframe::App::raw_input_hook(&mut app, &ctx, &mut child);
            let _output = ctx.run_logic(&root, |ctx| eframe::App::logic(&mut app, ctx, &mut frame));
            let (usb_only, _complete) = scans.try_recv().expect("each context must advance in its own epoch");
            assert!(!usb_only, "empty inventory reaches its ordinary full-discovery deadline");
            assert_eq!(app.last_device_scan_at, epoch + 2.0);
        }
    }
}
