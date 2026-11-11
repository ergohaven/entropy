use super::*;
use super::sm62_shared_write_tests::drain;

fn setup()->(EntropyApp,egui::Context,crate::hid::TestHidRecorder) {
    let ctx=egui::Context::default();let mut fonts=egui::FontDefinitions::default();
    fonts.families.insert(egui::FontFamily::Name("display_preview".into()),fonts.families[&egui::FontFamily::Proportional].clone());ctx.set_fonts(fonts);
    let mut app=EntropyApp::new_inert_for_test();app.app_settings.language=crate::i18n::Language::English;
    let (hid,recorder)=crate::hid::HidDevice::test_device();app.hid_device=Some(hid.into());
    let s=&mut app.display_settings;s.sm62_supported=true;s.sm62_hold_supported=true;s.display_timeout_supported=true;
    s.sm62_values=[0,0,20,0,1,1,0,20,1,1];s.confirmed_sm62_values=s.sm62_values;
    s.sm62_hold_ms=[112,333];s.confirmed_sm62_hold_ms=s.sm62_hold_ms;
    (app,ctx,recorder)
}
fn frame(app:&mut EntropyApp,ctx:&egui::Context,width:f32,events:Vec<egui::Event>)->egui::FullOutput {
    crate::app::discard_texture_deltas(ctx.run_pass(egui::RawInput {events,screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,2200.0))),..Default::default()},|ctx| {
        ctx.root_ui(|ui|egui::CentralPanel::default().show(ui,|ui|app.draw_sm62_display_settings_page(ui,ui.max_rect())));
    }))
}
fn position(out:&egui::FullOutput,label:&str)->egui::Pos2 {
    out.shapes.iter().rev().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text==label=>{
        let p=t.pos+t.galley.size()/2.0;s.clip_rect.contains(p).then_some(p)
    },_=>None}).unwrap_or_else(||panic!("missing visible {label}"))
}
fn field<'a>(out:&'a egui::FullOutput,label:&str)->&'a egui::epaint::RectShape {
    let y=position(out,label).y;
    out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Rect(r) if (r.rect.height()-34.0).abs()<0.1 && (r.rect.center().y-y).abs()<4.0=>Some(r),_=>None}).next().expect("actual control background")
}
fn click(app:&mut EntropyApp,ctx:&egui::Context,width:f32,p:egui::Pos2) {
    frame(app,ctx,width,vec![egui::Event::PointerMoved(p),egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()},egui::Event::PointerButton {pos:p,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
    frame(app,ctx,width,vec![]);
}

pub(super) fn select_hold(app:&mut EntropyApp,ctx:&egui::Context,current:&str,value:u16) {
    let out=frame(app,ctx,1100.0,vec![]);click(app,ctx,1100.0,position(&out,current));
    let target=format!("{value} ms");let mut out=frame(app,ctx,1100.0,vec![]);
    if !out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text==target && s.clip_rect.contains(t.pos+t.galley.size()/2.0))) {
        let p=position(&out,"400 ms");frame(app,ctx,1100.0,vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel {unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,delta:egui::vec2(0.0,-400.0),modifiers:Default::default(),source:egui::MouseWheelSource::Unknown}]);
        for _ in 0..30 {out=frame(app,ctx,1100.0,vec![]);}
    }
    click(app,ctx,1100.0,position(&out,&target));
}

#[test]
fn duration_dropdown_pointer_options_and_confirmed_pair_write() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for ru in [false,true] {
        let (mut app,ctx,recorder)=setup();if ru {app.app_settings.language=crate::i18n::Language::Russian;}
        frame(&mut app,&ctx,1100.0,vec![]);let out=frame(&mut app,&ctx,1100.0,vec![]);
        let label=if ru {"Показывать раскладку в течение"} else {"Show layout for"};
        let before=app.display_settings.clone();click(&mut app,&ctx,1100.0,field(&out,label).rect.center());
        let out=frame(&mut app,&ctx,1100.0,vec![]);
        let labels:Vec<_>=out.shapes.iter().filter_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.job.text.ends_with(if ru {" мс"} else {" ms"})=>Some(t.galley.job.text.clone()),_=>None}).collect();
        let expected:Vec<_>=(300..=1000).step_by(100).map(|n|format!("{n} {}",if ru {"мс"} else {"ms"})).collect();
        assert_eq!(labels,expected,"pointer opens exactly eight modern duration options, never a numeric editor");
        assert_eq!(app.display_settings,before);assert!(recorder.requests().is_empty());
        let p=position(&out,&expected[1]);frame(&mut app,&ctx,1100.0,vec![egui::Event::PointerMoved(p),egui::Event::MouseWheel {unit:egui::MouseWheelUnit::Point,phase:egui::TouchPhase::Move,delta:egui::vec2(0.0,-400.0),modifiers:Default::default(),source:egui::MouseWheelSource::Unknown}]);
        let mut out=frame(&mut app,&ctx,1100.0,vec![]);for _ in 0..30 {out=frame(&mut app,&ctx,1100.0,vec![]);}
        click(&mut app,&ctx,1100.0,position(&out,&expected[7]));drain(&mut app,&ctx);
        assert_eq!(app.display_settings.sm62_hold_ms,[1000;2]);assert_eq!(app.display_settings.confirmed_sm62_hold_ms,[1000;2]);
        let requests=recorder.requests();let sets:Vec<_>=requests.iter().filter(|r|r[1]==0x0b).collect();
        assert_eq!(sets.len(),2);
        for qsid in [242,243] {
            assert!(sets.iter().any(|r|u16::from_le_bytes([r[2],r[3]])==qsid && u16::from_le_bytes([r[4],r[5]])==1000));
            assert!(requests.iter().any(|r|r[1]==0x0a && u16::from_le_bytes([r[2],r[3]])==qsid));
        }
        let mut after=app.display_settings.clone();after.sm62_hold_ms=before.sm62_hold_ms;after.confirmed_sm62_hold_ms=before.confirmed_sm62_hold_ms;assert_eq!(after,before);
    }
}

#[test]
fn all_sm62_list_trigger_geometry_matches_standard_without_repaint_writes() {
    let _storage=crate::app::app_storage::ScopedTempConfig::new();
    for width in [540.0,1000.0] {for dark in [false,true] {for ru in [false,true] {for pair in [[350,350],[112,333],[300,300]] {
        let (mut app,ctx,recorder)=setup();ctx.set_visuals(crate::app::app_lifecycle::audit_app_visuals(dark));
        if ru {app.app_settings.language=crate::i18n::Language::Russian;}
        app.display_settings.sm62_hold_ms=pair;app.display_settings.confirmed_sm62_hold_ms=pair;
        let before=app.display_settings.clone();frame(&mut app,&ctx,width,vec![]);let out=frame(&mut app,&ctx,width,vec![]);
        let timeout_label=crate::i18n::tr_catalog(app.app_settings.language,"display_settings.display_timeout");
        let fields:Vec<_>=(if ru {["Половинка","Режим по умолчанию","Анимация ожидания через","Показывать раскладку в течение",timeout_label]}
            else {["Half","Default mode","Enable standby mode after","Show layout for",timeout_label]}).iter().map(|label|field(&out,label)).collect();
        for r in &fields {
            assert_eq!(r.rect.size(),egui::vec2(196.0,34.0),"all five trigger widths use standard settings dropdown size width={width}");
            assert_eq!(r.corner_radius.nw,9,"modern shared dropdown radius");
            assert_eq!(r.fill,crate::ui_style::surface_fill(dark),"standard modern dropdown surface");
            assert_eq!(r.stroke,crate::ui_style::modal_outline_stroke(dark),"standard modern dropdown outline");
            assert!(r.rect.left()>=0.0 && r.rect.right()<=width);
        }
        assert_eq!(fields[0].rect.right(),fields[1].rect.right());assert_eq!(fields[1].rect.right(),fields[2].rect.right());
        assert_eq!(fields[3].rect.right(),fields[4].rect.right());
        let text=if pair[0]!=pair[1] {if ru {"Различаются".to_owned()} else {"Mixed".to_owned()}}
            else {format!("{} {}",pair[0],if ru {"мс"} else {"ms"})};position(&out,&text);
        assert_eq!(app.display_settings,before);assert!(recorder.requests().is_empty());assert!(!app.qmk_settings_write_busy());
    }}}}
}
