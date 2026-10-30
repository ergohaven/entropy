mod preview_edge_tests {
    use super::*;
    fn fonts(ctx: &egui::Context) {
        let mut fonts=egui::FontDefinitions::default();
        fonts.families.insert(egui::FontFamily::Name("display_preview".into()), fonts.families[&egui::FontFamily::Proportional].clone());
        ctx.set_fonts(fonts);
    }
    #[test]
    fn sm62_preview_edge_fills_outer_frame_without_moving_content() {
        let _storage=crate::app::app_storage::ScopedTempConfig::new();
        let (hid,recorder)=crate::hid::HidDevice::test_device();
        for scale in [0.5,0.825,1.0] {
            for mode in [0,1,2,3] { for side in [0,1] { for off in [false,true] {
                let ctx=egui::Context::default();fonts(&ctx);
                let mut settings=[0u8;8];settings[0]=mode;
                let mut geometry=None;
                let output=crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(), |ctx| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui| {
                        ui.set_width(240.0*scale+14.0);
                        let start=ui.cursor().min;
                        let size=fitted_display_size((ui.available_width()-14.0).min(240.0),280.0*scale);
                        let outer=egui::Rect::from_min_size(start,size+egui::vec2(14.0,14.0));
                        let screen=egui::Rect::from_center_size(outer.center(),size);
                        draw_sm62_preview(ui,&settings,0,true,280.0*scale+14.0,side,Some("RU"),true,off,false);
                        geometry=Some((outer,screen));
                    }));
                }));
                let (outer,screen)=geometry.unwrap();
                let frame=output.shapes.iter().find_map(|s| match &s.shape {egui::Shape::Rect(r) if r.rect==outer=>Some(r),_=>None}).expect("real outer frame");
                assert_eq!(frame.fill,if off {Color32::BLACK} else {Color32::from_rgb(7,7,5)},"dark fill must reach inside outer outline: mode={mode} side={side} off={off} scale={scale}");
                assert_eq!(frame.stroke,Stroke::new(1.0,Color32::from_gray(75)));
                assert_eq!(frame.stroke_kind,egui::StrokeKind::Inside);
                assert_eq!(frame.corner_radius,egui::CornerRadius::from(display_corner_radius(size_from(screen))+7.0));
                assert!(!output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Rect(r) if r.rect==screen && r.stroke.width>0.0)),"no inset seam");
                let reference=crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput::default(),|ctx| {
                    ctx.root_ui(|ui| egui::CentralPanel::default().show(ui,|ui|paint_sm62_screen(ui,screen,&settings,0,true,side,Some("RU"),true,off,false)));
                }));
                // Exact production content shapes (including meshes/UV/positions) stay unchanged.
                let actual:Vec<_>=output.shapes.iter().filter(|s| !matches!(&s.shape,egui::Shape::Rect(r) if r.rect==outer)).collect();
                assert_eq!(format!("{:?}",actual),format!("{:?}",reference.shapes.iter().collect::<Vec<_>>()));
                let primitives=ctx.tessellate(output.shapes,1.0);
                assert!(!primitives.is_empty(),"actual egui tessellation");
            }}}
        }
        assert!(recorder.requests().is_empty(),"preview emits no HID requests");drop(hid);
    }
    fn size_from(screen:egui::Rect)->f32 {screen.width()}

    #[cfg(target_os="linux")]
    #[test]
    #[ignore="requires X11 and ENTROPY_EDGE_RENDER_OUTPUT"]
    fn sm62_preview_edge_native_screenshot() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        struct Fixture { frames:usize,path:std::path::PathBuf }
        impl eframe::App for Fixture {
            fn persist_egui_memory(&self)->bool {false}
            fn ui(&mut self,ui:&mut egui::Ui,_:&mut eframe::Frame) {
                let ctx=ui.ctx().clone();ctx.set_pixels_per_point(1.0);
                egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_gray(40))).show(ui,|ui| {
                    ui.horizontal(|ui| {for mode in [0,1,2,3] {ui.vertical(|ui| {ui.set_width(254.0);for off in [false,true] {
                        let mut settings=[0u8;8];settings[0]=mode;
                        draw_sm62_preview(ui,&settings,0,true,294.0,0,Some("RU"),false,off,false);
                    }});}});
                });
                self.frames+=1;if self.frames==5 {
                    let (path,close)=(self.path.clone(),ctx.clone());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                        let rgba:Vec<u8>=image.pixels.iter().flat_map(|p|p.to_array()).collect();
                        image::save_buffer(&path,&rgba,image.size[0] as u32,image.size[1] as u32,image::ColorType::Rgba8).unwrap();
                        close.send_viewport_cmd(egui::ViewportCommand::Close);
                    })));
                }
                assert!(self.frames<120);ctx.request_repaint();
            }
        }
        let path=std::path::PathBuf::from(std::env::var("ENTROPY_EDGE_RENDER_OUTPUT").unwrap());let output=path.clone();
        eframe::run_native("SM62 edge fixture (no HID)",eframe::NativeOptions {
            viewport:egui::ViewportBuilder::default().with_inner_size([1040.0,600.0]).with_resizable(false),
            renderer:eframe::Renderer::Glow,persist_window:false,
            event_loop_builder:Some(Box::new(|b|{b.with_any_thread(true);})),..Default::default()
        },Box::new(move |cc| {fonts(&cc.egui_ctx);Ok(Box::new(Fixture{frames:0,path}))})).unwrap();
        assert!(output.is_file());
    }
}
