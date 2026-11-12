use super::*;
use super::sm62_shared_write_tests::{frame_events, drain};

fn position(out: &egui::FullOutput, label: &str) -> egui::Pos2 {
    out.shapes.iter().rev().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.job.text == label => {
            let p = t.pos + t.galley.size()/2.0;
            s.clip_rect.contains(p).then_some(p)
        }, _ => None
    }).unwrap_or_else(|| panic!("missing visible {label}"))
}
fn click(app: &mut EntropyApp, ctx: &egui::Context, p: egui::Pos2) {
    frame_events(app, ctx, vec![egui::Event::PointerMoved(p),
        egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},
        egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    frame_events(app, ctx, vec![]);
}
fn choose(app: &mut EntropyApp, ctx: &egui::Context, current: &str, option: &str) {
    let out = frame_events(app,ctx,vec![]);
    click(app,ctx,position(&out,current));
    frame_events(app,ctx,vec![]);
    let out = frame_events(app,ctx,vec![]);
    click(app,ctx,position(&out,option));
}

#[test]
fn half_dropdown_pointer_switching_is_read_only_and_retains_all_settings() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for ru in [false,true] {
        let (mut app,ctx,recorder)=setup();
        if ru { app.app_settings.language=crate::i18n::Language::Russian; }
        let saved=app.display_settings.clone();
        let (left,right)=if ru {("Левая","Правая")} else {("Left","Right")};
        for (from,to,mode) in [(left,right,if ru {"Плеер"} else {"Player"}), (right,left,if ru {"Часы"} else {"Clock"}), (left,right,if ru {"Плеер"} else {"Player"})] {
            choose(&mut app,&ctx,from,to);
            let out=frame_events(&mut app,&ctx,vec![]);
            position(&out,to); position(&out,mode);
            assert_eq!(app.display_settings,saved);
            assert!(!app.qmk_settings_write_busy());
            for qsid in (230..=249).chain([DISPLAY_TIMEOUT_QSID]) { assert!(app.settings_write_status(qsid).is_none()); }
            assert!(recorder.requests().is_empty(),"selector/repaint must issue zero requests");
        }
        app.hid_device=None;
        choose(&mut app,&ctx,right,left);
        assert_eq!(app.display_settings,saved,"offline inspection still retains all data");
        assert!(recorder.requests().is_empty());
    }
}

#[test]
fn selected_half_pointer_edits_use_original_queue_and_preserve_other_half() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,ctx,recorder)=setup();
    let initial=app.display_settings.clone();
    choose(&mut app,&ctx,"Left","Right");
    for (side,from,to) in [(1,"Right","Left"),(0,"Left","Right")] {
        let before=app.display_settings.sm62_values;
        let out=frame_events(&mut app,&ctx,vec![]);
        let label=position(&out,"Show layout on key press");
        let switch=out.shapes.iter().find_map(|s|match &s.shape {
            egui::Shape::Rect(r) if r.rect.width()==46.0 && r.rect.height()==24.0 && (r.rect.center().y-label.y).abs()<5.0=>Some(r.rect.center()), _=>None
        }).expect("selected half switch");
        click(&mut app,&ctx,switch);
        assert!(app.qmk_settings_write_busy(),"ordinary auto-apply queued");
        drain(&mut app,&ctx);
        let index=side*5+3;
        let mut expected=before;expected[index]=1-before[index];
        assert_eq!(app.display_settings.sm62_values,expected);
        assert_eq!(app.display_settings.confirmed_sm62_values[index],expected[index]);
        let requests=recorder.requests();
        let sets:Vec<_>=requests.iter().filter(|r|r[1]==0x0B).map(|r|u16::from_le_bytes([r[2],r[3]])).collect();
        assert_eq!(sets,if side==1 {vec![238]} else {vec![238,233]});
        let saved=app.display_settings.clone();let count=requests.len();
        choose(&mut app,&ctx,from,to);
        assert_eq!(app.display_settings,saved,"switching after an edit retains both halves");
        assert_eq!(recorder.requests().len(),count);
    }
    // Exercise the mode and idle dropdowns on the selected right half too.
    choose(&mut app,&ctx,"Player","Clock"); drain(&mut app,&ctx);
    choose(&mut app,&ctx,"No animation","10 seconds"); drain(&mut app,&ctx);
    assert_eq!(app.display_settings.sm62_values,[0,1,47,1,1,0,1,10,0,0]);
    let sets:Vec<_>=recorder.requests().iter().filter(|r|r[1]==0x0B).map(|r|u16::from_le_bytes([r[2],r[3]])).collect();
    assert_eq!(sets,vec![238,233,235,237,236]);
    assert_eq!(app.display_settings.sm62_date_values,initial.sm62_date_values);
    assert_eq!(app.display_settings.sm62_hold_ms,initial.sm62_hold_ms);
    assert_eq!(app.display_settings.sm62_language_values,initial.sm62_language_values);
    assert_eq!(app.display_settings.sm62_stars_values,initial.sm62_stars_values);
    assert_eq!(app.display_settings.display_timeout,initial.display_timeout);
    assert_eq!(app.display_settings.brightness,initial.brightness);
}


#[test]
fn selected_half_layout_matrix_is_bounded_and_input_free() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for dark in [false,true] { for ru in [false,true] { for width in [540.0,1000.0] {
        let (mut app,ctx,recorder)=setup();
        app.app_settings.language=if ru {crate::i18n::Language::Russian} else {crate::i18n::Language::English};
        ctx.set_visuals(crate::app::app_lifecycle::audit_app_visuals(dark));
        let saved=app.display_settings.clone();
        let draw=|app:&mut EntropyApp,events| crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
            events,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,1800.0))),..Default::default()
        },|ctx| {ctx.root_ui(|ui|egui::CentralPanel::default().show(ui,|ui|app.draw_sm62_display_settings_page(ui,ui.max_rect())));}));
        draw(&mut app,vec![]);
        for (from,to) in if ru {[("Левая","Правая"),("Правая","Левая")]} else {[("Left","Right"),("Right","Left")]} {
            let out=draw(&mut app,vec![]);
            let p=position(&out,from);
            let events=|p|vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}];
            draw(&mut app,events(p));draw(&mut app,vec![]);let popup=draw(&mut app,vec![]);
            draw(&mut app,events(position(&popup,to)));draw(&mut app,vec![]);
            let out=draw(&mut app,vec![]);position(&out,to);
            let controls:Vec<_>=out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Rect(r) if r.rect.height()==34.0=>Some(r.rect),_=>None}).collect();
            assert!(controls.len()>=4,"half/mode/idle/timeout backgrounds");
            // Within each half's controls, retain common right-edge alignment.
            let mode=position(&out,if to=="Left"||to=="Левая" {if ru {"Часы"} else {"Clock"}} else {if ru {"Плеер"} else {"Player"}});
            let idle=position(&out,if to=="Left"||to=="Левая" {if ru {"47 с"} else {"47 s"}} else {if ru {"Без анимации"} else {"No animation"}});
            let field=|p|controls.iter().find(|r|r.contains(p)).unwrap();
            assert!((field(mode).right()-field(idle).right()).abs()<0.1);
            let half=field(position(&out,to));
            assert!((half.left()-field(mode).left()).abs()<0.1,"Half must use the settings-column control width");
            if width>=620.0 {
                let preview=out.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Rect(r) if r.rect.height()>180.0 && r.rect.width()<260.0=>Some(r.rect),_=>None}).expect("display preview");
                assert!(half.top() < preview.bottom() && half.bottom()>preview.top(),"Half belongs beside preview, not above it");
                assert!(position(&out,if ru {"Половинка"} else {"Half"}).x>preview.right(),"Half label belongs to the right-hand settings column");
            }
            assert!(controls.iter().all(|r|r.left()>=0.0&&r.right()<=width));
            let shared_y = position(&out,if ru {"Общие настройки экранов"} else {"Shared display settings"}).y;
            let block_lines = out.shapes.iter().filter(|s| matches!(&s.shape,
                egui::Shape::LineSegment { points, .. }
                    if (points[0].y-points[1].y).abs()<0.1
                    && points[0].y < shared_y
                    && (points[0].x-points[1].x).abs()>width*0.7)).count();
            // Narrow layout retains four ordinary control-row dividers; only
            // the two extra full-block bars are removed. Wide row dividers do
            // not span the preview, so none meet the full-block width here.
            assert_eq!(block_lines,if width<620.0 {4} else {0},
                "only ordinary row dividers, never extra top/bottom block bars");
            assert_eq!(app.display_settings,saved);assert!(recorder.requests().is_empty());assert!(!app.qmk_settings_write_busy());
        }
    }}}
}

fn timed_page(app: &mut EntropyApp, ctx: &egui::Context, time: f64) -> egui::FullOutput {
    crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {
        time: Some(time), screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0,2200.0))), ..Default::default()
    }, |ctx| {ctx.root_ui(|ui| egui::CentralPanel::default().show(ui, |ui| app.draw_sm62_display_settings_page(ui,ui.max_rect())));} ))
}
fn ship_uv(out: &egui::FullOutput) -> egui::Rect {
    out.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Rect(r)=>r.brush.as_ref().map(|b|b.uv),_=>None}).expect("actual orbit texture UV")
}
fn stars(out: &egui::FullOutput) -> Vec<egui::Pos2> {
    out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Circle(c) if c.radius<3.0=>Some(c.center),_=>None}).collect()
}

#[test]
fn pointer_selected_ship_advances_at_controlled_times_on_both_halves() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,ctx,recorder)=setup();
    for (side,from) in [(0,"Clock"),(1,"Player")] {
        if side==1 {choose(&mut app,&ctx,"Left","Right");}
        let before=app.display_settings.sm62_values;
        choose(&mut app,&ctx,from,"Spaceship");drain(&mut app,&ctx);
        assert_eq!(app.display_settings.sm62_values[side*5],2);
        assert_eq!(&app.display_settings.sm62_values[(1-side)*5..(1-side)*5+5],&before[(1-side)*5..(1-side)*5+5]);
        let writes=recorder.requests().len();
        // Source-backed C parity checks every pixel over arrival + two flight
        // cycles. This UI test checks the real pointer-selected route remains
        // a single composited texture and creates no animation HID traffic.
        let start=ctx.input(|i|i.time)+0.05;
        for offset in 0..180 {
            let out=timed_page(&mut app,&ctx,start+offset as f64*0.05);
            assert_eq!(ship_uv(&out),egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),
                "no whole-scene atlas animation may return through the page");
        }
        assert_eq!(recorder.requests().len(),writes,"animation sends no HID");
        assert!(!app.qmk_settings_write_busy());
    }
}

#[test]
fn star_positions_advance_without_changing_clock_or_hid() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    let (mut app,ctx,recorder)=setup();
    app.display_settings.sm62_values=[0,0,20,0,0,0,0,20,0,0];
    app.display_settings.confirmed_sm62_values=app.display_settings.sm62_values;
    app.display_settings.sm62_stars_values=[1,1];
    for side in 0..2 {
        if side==1 {choose(&mut app,&ctx,"Left","Right");}
        let start=ctx.input(|i|i.time)+1.0;
        let frames: Vec<_>=[0.0,0.1,0.2].into_iter().map(|t|timed_page(&mut app,&ctx,start+t)).collect();
        let positions:Vec<_>=frames.iter().map(stars).collect();
        assert!(positions.iter().all(|p|p.len()==26));
        assert!(positions.windows(2).all(|p|p[0]!=p[1]),"stars must move through actual preview paint");
        // Disconnected clock remains the same bitmap fallback while stars move.
        let meshes=|out:&egui::FullOutput|out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Mesh(m)=>Some(format!("{m:?}")),_=>None}).collect::<Vec<_>>();
        assert_eq!(meshes(&frames[0]),meshes(&frames[2]));
    }
    assert!(recorder.requests().is_empty());assert!(!app.qmk_settings_write_busy());
}

#[test]
fn preview_repaint_is_gated_by_visible_animation_and_effective_mode() {
    use std::time::Duration;
    for (settings,scene,stars_enabled,off,live,visible,animated) in [
        ([2,0,20,1,0],0,false,false,false,true,true), // explicit ship ignores idle-off
        ([0,1,20,1,0],1,false,false,false,true,true), // idle scene ship
        ([0,0,20,1,0],1,false,false,false,true,false), // No animation
        ([0,0,20,1,0],0,true,false,false,true,true), // stars independent of idle
        ([1,0,20,1,0],0,true,false,false,true,true), // player stars
        ([1,0,20,1,0],0,false,false,false,true,false),
        ([2,1,20,1,0],2,true,false,false,true,false), // keypress layout overrides ship
        ([3,0,20,1,0],0,true,false,false,true,false),
        ([2,1,20,1,0],0,true,true,false,true,false), // screens off
        ([2,1,20,1,0],0,true,false,false,false,false), // scrolled out
        ([0,0,20,1,0],0,false,false,true,true,false), // live clock keeps 30s only
    ] { for side in 0..2 {
        let ctx=egui::Context::default();
        let mut fonts=egui::FontDefinitions::default();
        fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());ctx.set_fonts(fonts);
        let mut delay=Duration::ZERO;
        let mut last=None;
        for n in 0..5 {
            let out=crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {time:Some(10.0+n as f64*0.1),..Default::default()},|ctx| {
                ctx.root_ui(|ui|egui::CentralPanel::default().show(ui,|ui| {
                    if !visible {ui.set_clip_rect(egui::Rect::NOTHING);}
                    super::super::display_preview::draw_sm62_preview(ui,&settings,scene,false,245.0,side,None,stars_enabled,off,live);
                }));
            }));
            delay=out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            last=Some(out);
        }
        assert_eq!(delay<=Duration::from_millis(50),animated,"settings={settings:?} scene={scene} stars={stars_enabled} off={off} live={live} visible={visible}: {delay:?}");
        if live {assert_eq!(delay,Duration::from_secs(30));}
        if off || settings[0]==3 || scene==2 {assert!(stars(&last.unwrap()).is_empty());}
    }}
}

fn setup() -> (EntropyApp, egui::Context, crate::hid::TestHidRecorder) {
    let ctx = egui::Context::default();
    let mut fonts = egui::FontDefinitions::default();
    fonts.families.insert(egui::FontFamily::Name("display_preview".into()), fonts.families[&egui::FontFamily::Proportional].clone());
    ctx.set_fonts(fonts);
    let mut app = EntropyApp::new_inert_for_test();
    app.app_settings.language = crate::i18n::Language::English;
    let (hid, recorder) = crate::hid::HidDevice::test_device();
    app.hid_device = Some(hid.into());
    let s = &mut app.display_settings;
    s.sm62_supported = true;
    s.display_timeout_supported = true;
    s.sm62_date_supported = true;
    s.sm62_hold_supported = true;
    s.sm62_language_supported = true;
    s.sm62_stars_supported = true;
    s.sm62_screens_off_supported = true;
    s.sm62_values = [0, 1, 47, 0, 1, 1, 0, 83, 1, 0];
    s.confirmed_sm62_values = s.sm62_values;
    s.sm62_date_values = [1,0]; s.confirmed_sm62_date_values = [1,0];
    s.sm62_hold_ms = [112,333]; s.confirmed_sm62_hold_ms = [112,333];
    (app, ctx, recorder)
}

#[test]
fn single_display_block_paints_only_selected_half() {
    let _storage = crate::app::app_storage::ScopedTempConfig::new();
    for language in [crate::i18n::Language::English, crate::i18n::Language::Russian] {
        let (mut app, ctx, recorder) = setup();
        app.app_settings.language = language;
        let before = app.display_settings.clone();
        frame_events(&mut app, &ctx, vec![]);
        let out = frame_events(&mut app, &ctx, vec![]);
        let labels: Vec<_> = out.shapes.iter().filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.job.text.as_str()), _ => None
        }).collect();
        let ru = language == crate::i18n::Language::Russian;
        assert_eq!(labels.iter().filter(|t| **t == if ru {"Дисплей"} else {"Display"}).count(), 1, "one primary heading: {labels:?}");
        assert!(labels.contains(&if ru {"Половинка"} else {"Half"}), "selector inside settings block");
        assert!(labels.contains(&if ru {"Левая"} else {"Left"}));
        for old in ["Левый экран", "Правый экран", "Left display", "Right display"] { assert!(!labels.contains(&old)); }
        assert_eq!(labels.iter().filter(|t| **t == if ru {"Режим по умолчанию"} else {"Default mode"}).count(), 1);
        assert!(!labels.contains(&if ru {"Плеер"} else {"Player"}), "unselected mode must not paint");
        assert_eq!(app.display_settings, before);
        assert!(recorder.requests().is_empty());
        assert!(!app.qmk_settings_write_busy());
    }
}
