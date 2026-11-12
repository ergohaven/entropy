// Source: Ergohaven screen_orbit.c (SM62 prepared-scene renderer).
// RLE palettes and runs are retained verbatim; stars are composed before ship
// and white flame in RGB565, using shipping LVGL's SWAP=1 channel arithmetic.
const ORBIT_FLIGHT: [i8;76] = [
    9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,
    9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,9,15,
    10,11,12,13,14,13,14,13,14,13,14,13,14,13,14,13,14,13,14,13,14,
    13,14,13,14,13,14,13,14,13,14,13,14,12,11,10,
];
const ORBIT_STAR_RGB: [u32;8] = [0xffffff,0xff9223,0xfff1eb,0xe9ecff,0xbfd3ff,0xfef9ff,0xff7800,0xbacfff];

fn orbit_rgb565(rgb:u32)->u16 {
    (((rgb>>19)&31)<<11 | ((rgb>>10)&63)<<5 | ((rgb>>3)&31)) as u16
}
fn orbit_flame_mix(background:u16,alpha:u8)->u16 {
    // lv_color_mix with LV_COLOR_DEPTH=16, LV_COLOR_16_SWAP=1,
    // LV_COLOR_MIX_ROUND_OFS=128 and LV_UDIV255. Not an 8-bit alpha blend.
    let mix=|value:u16,max:u32| {
        let sum=max*u32::from(alpha)+u32::from(value)*(255-u32::from(alpha))+128;
        ((sum*0x8081)>>23) as u16
    };
    mix(background>>11,31)<<11 | mix((background>>5)&63,63)<<5 | mix(background&31,31)
}
fn orbit_asset(frame:usize,flame:bool)->&'static [u8] {
    let mut bytes=include_bytes!("../../assets/sm62/orbit-rle.bin").as_slice();
    for index in 0..32 {
        let len=u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        if index==frame*2+usize::from(flame) {return &bytes[4..4+len];}
        bytes=&bytes[4+len..];
    }
    unreachable!("validated firmware frame")
}
fn orbit_rle(pixels:&mut [u16],asset:&[u8],flame:bool) {
    let (x0,y0,width)=(asset[48] as usize,asset[49] as usize,asset[50] as usize);
    let mut position=0;
    for packed in &asset[52..] {
        let index=usize::from(packed&15);
        for _ in 0..packed>>4 {
            if index!=0 {
                let dest=(y0+position/width)*240+x0+position%width;
                pixels[dest]=if flame {orbit_flame_mix(pixels[dest],asset[index*3+1])}
                    else {orbit_rgb565(u32::from(asset[index*3])<<16|u32::from(asset[index*3+1])<<8|u32::from(asset[index*3+2]))};
            }
            position+=1;
        }
    }
}
#[derive(Clone)]
struct OrbitScene {
    random:u32,
    stars:[[i32;3];20],
    index:u64,
    prepared:Vec<egui::Color32>,
    visible:Vec<egui::Color32>,
    timer_ms:u64,
}
impl OrbitScene {
    fn random(&mut self)->u32 {
        // Only the random boundary is host-local. Tests inject the identical
        // stream into production C; no baked star coordinates or scene frames.
        self.random=self.random.wrapping_mul(1664525).wrapping_add(1013904223);
        self.random>>1
    }
    fn new(seed:u32,now:u64)->Self {
        let mut s=Self {random:seed,stars:[[0;3];20],index:0,prepared:Vec::new(),visible:Vec::new(),timer_ms:now};
        for i in 0..20 {s.stars[i]=[(s.random()%240) as i32,(s.random()%224) as i32,(s.random()%8+3) as i32];}
        s.prepare(-1);s.visible=s.prepared.clone();s
    }
    fn frame(index:u64)->i8 {
        if index>=14 {ORBIT_FLIGHT[((index-14)%76) as usize]}
        else if index>5 {(index-5) as i8} else {-1}
    }
    fn prepare(&mut self,frame:i8) {
        let mut pixels=vec![orbit_rgb565(0x070705);240*224];
        let put=|pixels:&mut [u16],x:i32,y:i32,c:u16| {pixels[y as usize*240+x as usize]=c;};
        for i in 0..20 {
            let [x,y,speed]=self.stars[i];let c=orbit_rgb565(ORBIT_STAR_RGB[i%8]);
            put(&mut pixels,x,y,c);
            if speed>=7 && x>0 && x<238 && y>0 && y<222 {
                for (dx,dy) in [(0,-1),(-1,0),(1,0),(0,1)] {put(&mut pixels,x+dx,y+dy,c);}
            } else if speed>=5 && x<239 && y<223 {
                for (dx,dy) in [(1,0),(0,1),(1,1)] {put(&mut pixels,x+dx,y+dy,c);}
            }
        }
        if frame>=0 {
            orbit_rle(&mut pixels,orbit_asset(frame as usize,false),false);
            orbit_rle(&mut pixels,orbit_asset(frame as usize,true),true);
        }
        let to_color=|c:u16| egui::Color32::from_rgb(
            (((c>>11) as u32*263+7)>>5) as u8,
            ((((c>>5)&63) as u32*259+3)>>6) as u8,
            (((c&31) as u32*263+7)>>5) as u8);
        // Firmware animation_image is 240x224 at (0,28), never stretched to 280.
        self.prepared=vec![to_color(orbit_rgb565(0x070705));240*280];
        for (dest,c) in self.prepared[28*240..252*240].iter_mut().zip(pixels) {*dest=to_color(c);}
        for i in 0..20 {
            self.stars[i][0]-=self.stars[i][2];
            if self.stars[i][0]<0 {self.stars[i]=[239,(self.random()%224) as i32,(self.random()%8+3) as i32];}
        }
    }
    fn advance(&mut self,now:u64) {
        // Same prepare/flush order as C: invalidate the existing prepared scene,
        // then prepare the successor only after that frame completes. The first
        // 50ms boundary therefore repeats the entry starfield, not frame 9.
        if now.saturating_sub(self.timer_ms)>=50 {
            // Firmware advances once per completed cadence, never catches up
            // missed wall-clock frames after a slow/blocked owner iteration.
            self.timer_ms=now;
            self.visible=self.prepared.clone();
            self.index+=1;
            self.prepare(Self::frame(self.index));
        }
    }
    fn resume(&mut self,now:u64) {self.timer_ms=now;self.visible=self.prepared.clone();}
}
#[derive(Clone)]
struct OrbitPreview {
    scene:OrbitScene,
    texture:egui::TextureHandle,
    active:bool,
    restart_on_entry:bool,
    last_pass:u64,
}
fn sm62_orbit_update(ui:&egui::Ui,side:usize,active:bool,permanent:bool,suspended:bool)->Option<egui::TextureHandle> {
    let id=egui::Id::unique(("sm62_orbit_compositor",side));
    let now=(ui.input(|i|i.time).max(0.0)*1000.0).round() as u64;
    let pass=ui.ctx().cumulative_pass_nr();
    let previous=ui.ctx().data(|data|data.get_temp::<OrbitPreview>(id));
        if !active {
            if let Some(mut state)=previous {
                state.active=false;
                if !suspended {state.restart_on_entry=true;}
                ui.ctx().data_mut(|data|data.insert_temp(id,state));
            }
            return None;
        }
        let mut state=if let Some(mut state)=previous {
            if !state.active || pass>state.last_pass+1 {
                if permanent || !state.restart_on_entry {state.scene.resume(now);} else {
                    // srand on idle re-entry does not reinitialize persistent
                    // stars in firmware; restart only index/timer and random.
                    state.scene.random=now as u32;state.scene.index=0;state.scene.timer_ms=now;
                    state.scene.prepare(-1);state.scene.visible=state.scene.prepared.clone();
                }
            }
            state
        } else {
            let scene=OrbitScene::new(now as u32,now);
            let texture=ui.ctx().load_texture("sm62-orbit-composited",egui::ColorImage::new([240,280],scene.visible.clone()),egui::TextureOptions::LINEAR);
            OrbitPreview {scene,texture,active:true,restart_on_entry:false,last_pass:pass}
        };
        state.scene.advance(now);
        state.texture.set(egui::ColorImage::new([240,280],state.scene.visible.clone()),egui::TextureOptions::LINEAR);
        state.active=true;state.restart_on_entry=false;state.last_pass=pass;
        let texture=state.texture.clone();ui.ctx().data_mut(|data|data.insert_temp(id,state));Some(texture)
}
