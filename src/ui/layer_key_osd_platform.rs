use super::layout_indicator_window::{
    layer_key_osd_metrics, LayerKeyOsdMetrics, LAYER_KEY_OSD_EDGE_MARGIN_X,
    LAYER_KEY_OSD_EDGE_MARGIN_Y,
};
use super::*;

#[derive(Clone)]
pub(super) struct LayerKeyOsdRequest {
    pub(super) title: String,
    pub(super) detail: String,
    pub(super) size: NotificationSize,
    pub(super) position: NotificationPosition,
    pub(super) theme: NotificationTheme,
    pub(super) opacity: f32,
    pub(super) timeout_ms: u32,
}

pub(super) fn show_layer_key_osd(request: LayerKeyOsdRequest) -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() && wayland_layer::show(request.clone()) {
            return true;
        }
    }

    let _ = request;
    false
}

#[cfg(target_os = "linux")]
mod wayland_layer {
    use super::*;
    use ab_glyph::{point, Font, FontArc, ScaleFont};
    use smithay_client_toolkit::{
        compositor::{CompositorHandler, CompositorState},
        delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
        shell::{
            wlr_layer::{
                Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
                LayerSurfaceConfigure,
            },
            WaylandSurface,
        },
        shm::{slot::SlotPool, Shm, ShmHandler},
    };
    use std::num::NonZeroU32;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use wayland_client::{
        globals::registry_queue_init,
        protocol::{wl_output, wl_region, wl_shm, wl_surface},
        Connection, Dispatch, QueueHandle,
    };

    static OSD_GENERATION: AtomicU64 = AtomicU64::new(0);

    pub(super) fn show(request: LayerKeyOsdRequest) -> bool {
        let generation = OSD_GENERATION
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let (ready_tx, ready_rx) = mpsc::channel();
        if std::thread::Builder::new()
            .name("entropy-layer-key-osd-wayland".to_owned())
            .spawn(move || {
                if let Err(err) = run(request, generation, ready_tx) {
                    log::debug!("Wayland layer notification failed: {err:?}");
                }
            })
            .is_err()
        {
            return false;
        }

        match ready_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(ready) => ready,
            Err(mpsc::RecvTimeoutError::Timeout) => true,
            Err(mpsc::RecvTimeoutError::Disconnected) => false,
        }
    }

    fn run(
        request: LayerKeyOsdRequest,
        generation: u64,
        ready_tx: mpsc::Sender<bool>,
    ) -> anyhow::Result<()> {
        let mut ready_tx = Some(ready_tx);
        let conn = Connection::connect_to_env()?;
        let (globals, mut event_queue) = registry_queue_init(&conn)?;
        let qh = event_queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;
        let shm = Shm::bind(&globals, &qh)?;

        let metrics = layer_key_osd_metrics(request.size);
        let width = metrics.size.x.round().max(1.0) as u32;
        let height = metrics.size.y.round().max(1.0) as u32;
        let surface = compositor.create_surface(&qh);
        let input_region = compositor.wl_compositor().create_region(&qh, ());
        surface.set_input_region(Some(&input_region));
        input_region.destroy();
        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Overlay,
            Some("entropy-osd"),
            None,
        );
        let (anchor, margins) = layer_shell_placement(request.position);
        layer.set_anchor(anchor);
        layer.set_margin(margins.0, margins.1, margins.2, margins.3);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        layer.set_size(width, height);
        layer.commit();

        let pool = SlotPool::new((width * height * 4) as usize, &shm)?;
        let font = FontArc::try_from_slice(include_bytes!("../../assets/Roboto-Regular.ttf"))?;
        let mut osd = WaylandOsd {
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            shm,
            layer,
            pool,
            font,
            request,
            metrics,
            width,
            height,
            first_configure: true,
            drawn: false,
            closed: false,
        };

        while !osd.drawn && !osd.closed && OSD_GENERATION.load(Ordering::Relaxed) == generation {
            if let Err(err) = event_queue.blocking_dispatch(&mut osd) {
                if let Some(ready_tx) = ready_tx.take() {
                    let _ = ready_tx.send(false);
                }
                return Err(err.into());
            }
        }
        if let Some(ready_tx) = ready_tx.take() {
            let _ = ready_tx.send(osd.drawn && !osd.closed);
        }
        conn.flush()?;

        let deadline = Instant::now()
            + Duration::from_millis(clamp_notification_timeout_ms(osd.request.timeout_ms) as u64);
        while Instant::now() < deadline
            && !osd.closed
            && OSD_GENERATION.load(Ordering::Relaxed) == generation
        {
            event_queue.dispatch_pending(&mut osd)?;
            conn.flush()?;
            std::thread::sleep(Duration::from_millis(16));
        }

        osd.layer.wl_surface().attach(None, 0, 0);
        osd.layer.commit();
        conn.flush()?;
        Ok(())
    }

    struct WaylandOsd {
        registry_state: RegistryState,
        output_state: OutputState,
        shm: Shm,
        layer: LayerSurface,
        pool: SlotPool,
        font: FontArc,
        request: LayerKeyOsdRequest,
        metrics: LayerKeyOsdMetrics,
        width: u32,
        height: u32,
        first_configure: bool,
        drawn: bool,
        closed: bool,
    }

    impl WaylandOsd {
        fn draw(&mut self) -> anyhow::Result<()> {
            let width = self.width;
            let height = self.height;
            let stride = width as i32 * 4;
            let (buffer, canvas) = self.pool.create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )?;
            canvas.fill(0);

            paint_osd(
                canvas,
                width,
                height,
                &self.font,
                &self.request,
                self.metrics,
            );
            self.layer
                .wl_surface()
                .damage_buffer(0, 0, width as i32, height as i32);
            buffer.attach_to(self.layer.wl_surface())?;
            self.layer.commit();
            self.drawn = true;
            Ok(())
        }
    }

    impl CompositorHandler for WaylandOsd {
        fn scale_factor_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_factor: i32,
        ) {
        }

        fn transform_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_transform: wl_output::Transform,
        ) {
        }

        fn frame(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _time: u32,
        ) {
        }

        fn surface_enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }

        fn surface_leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
        }
    }

    impl OutputHandler for WaylandOsd {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.output_state
        }

        fn new_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn update_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn output_destroyed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
    }

    impl LayerShellHandler for WaylandOsd {
        fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
            self.closed = true;
        }

        fn configure(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _layer: &LayerSurface,
            configure: LayerSurfaceConfigure,
            _serial: u32,
        ) {
            self.width = NonZeroU32::new(configure.new_size.0).map_or(self.width, NonZeroU32::get);
            self.height =
                NonZeroU32::new(configure.new_size.1).map_or(self.height, NonZeroU32::get);
            if self.first_configure {
                self.first_configure = false;
                if let Err(err) = self.draw() {
                    log::debug!("Wayland layer notification draw failed: {err:?}");
                    self.closed = true;
                }
            }
        }
    }

    impl ShmHandler for WaylandOsd {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    delegate_compositor!(WaylandOsd);
    delegate_output!(WaylandOsd);
    delegate_shm!(WaylandOsd);
    delegate_layer!(WaylandOsd);
    delegate_registry!(WaylandOsd);

    impl ProvidesRegistryState for WaylandOsd {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry_state
        }

        registry_handlers![OutputState];
    }

    impl Dispatch<wl_region::WlRegion, ()> for WaylandOsd {
        fn event(
            _: &mut Self,
            _: &wl_region::WlRegion,
            _: wl_region::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    fn layer_shell_placement(position: NotificationPosition) -> (Anchor, (i32, i32, i32, i32)) {
        const X: i32 = LAYER_KEY_OSD_EDGE_MARGIN_X as i32;
        const Y: i32 = LAYER_KEY_OSD_EDGE_MARGIN_Y as i32;
        match position {
            NotificationPosition::TopLeft => (Anchor::TOP | Anchor::LEFT, (Y, 0, 0, X)),
            NotificationPosition::TopCenter => (Anchor::TOP, (Y, 0, 0, 0)),
            NotificationPosition::TopRight => (Anchor::TOP | Anchor::RIGHT, (Y, X, 0, 0)),
            NotificationPosition::CenterLeft => (Anchor::LEFT, (0, 0, 0, X)),
            NotificationPosition::Center => (Anchor::empty(), (0, 0, 0, 0)),
            NotificationPosition::CenterRight => (Anchor::RIGHT, (0, X, 0, 0)),
            NotificationPosition::BottomLeft => (Anchor::BOTTOM | Anchor::LEFT, (0, 0, Y, X)),
            NotificationPosition::BottomCenter => (Anchor::BOTTOM, (0, 0, Y, 0)),
            NotificationPosition::BottomRight => (Anchor::BOTTOM | Anchor::RIGHT, (0, X, Y, 0)),
        }
    }

    fn paint_osd(
        canvas: &mut [u8],
        width: u32,
        height: u32,
        font: &FontArc,
        request: &LayerKeyOsdRequest,
        metrics: LayerKeyOsdMetrics,
    ) {
        let opacity = clamp_notification_opacity(request.opacity);
        let alpha = |value: u8| ((value as f32) * opacity).round().clamp(0.0, 255.0) as u8;
        let (fill, stroke, title_color, detail_color) = match request.theme {
            NotificationTheme::Dark => (
                [30, 30, 34, alpha(226)],
                [255, 255, 255, alpha(28)],
                [248, 248, 248, alpha(244)],
                [210, 210, 214, alpha(210)],
            ),
            NotificationTheme::Light => (
                [246, 246, 248, alpha(232)],
                [28, 28, 32, alpha(32)],
                [24, 24, 28, alpha(244)],
                [92, 92, 98, alpha(216)],
            ),
        };

        let rect = RectPx {
            x: 4.0,
            y: 4.0,
            w: width as f32 - 8.0,
            h: height as f32 - 8.0,
            r: metrics.corner_radius,
        };
        draw_rounded_rect(canvas, width, height, rect, fill);
        draw_rounded_stroke(canvas, width, height, rect, 1.0, stroke);

        let cx = width as f32 * 0.5;
        let cy = height as f32 * 0.5;
        draw_text_centered(
            canvas,
            width,
            height,
            font,
            &request.title,
            cx,
            cy + metrics.title_offset_y,
            metrics.title_font,
            title_color,
        );
        if !request.detail.is_empty() {
            draw_text_centered(
                canvas,
                width,
                height,
                font,
                &request.detail,
                cx,
                cy + metrics.detail_offset_y,
                metrics.detail_font,
                detail_color,
            );
        }
    }

    #[derive(Clone, Copy)]
    struct RectPx {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
    }

    fn draw_rounded_rect(canvas: &mut [u8], width: u32, height: u32, rect: RectPx, color: [u8; 4]) {
        let min_x = rect.x.floor().max(0.0) as i32;
        let min_y = rect.y.floor().max(0.0) as i32;
        let max_x = (rect.x + rect.w).ceil().min(width as f32) as i32;
        let max_y = (rect.y + rect.h).ceil().min(height as f32) as i32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                if rounded_rect_contains(rect, x as f32 + 0.5, y as f32 + 0.5) {
                    blend_pixel(canvas, width, height, x, y, color, 1.0);
                }
            }
        }
    }

    fn draw_rounded_stroke(
        canvas: &mut [u8],
        width: u32,
        height: u32,
        rect: RectPx,
        thickness: f32,
        color: [u8; 4],
    ) {
        let inner = RectPx {
            x: rect.x + thickness,
            y: rect.y + thickness,
            w: rect.w - thickness * 2.0,
            h: rect.h - thickness * 2.0,
            r: (rect.r - thickness).max(0.0),
        };
        let min_x = rect.x.floor().max(0.0) as i32;
        let min_y = rect.y.floor().max(0.0) as i32;
        let max_x = (rect.x + rect.w).ceil().min(width as f32) as i32;
        let max_y = (rect.y + rect.h).ceil().min(height as f32) as i32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                if rounded_rect_contains(rect, px, py) && !rounded_rect_contains(inner, px, py) {
                    blend_pixel(canvas, width, height, x, y, color, 1.0);
                }
            }
        }
    }

    fn rounded_rect_contains(rect: RectPx, x: f32, y: f32) -> bool {
        let left = rect.x;
        let right = rect.x + rect.w;
        let top = rect.y;
        let bottom = rect.y + rect.h;
        if x < left || x >= right || y < top || y >= bottom {
            return false;
        }
        let r = rect.r.min(rect.w * 0.5).min(rect.h * 0.5);
        let cx = x.clamp(left + r, right - r);
        let cy = y.clamp(top + r, bottom - r);
        let dx = x - cx;
        let dy = y - cy;
        dx * dx + dy * dy <= r * r
    }

    fn draw_text_centered(
        canvas: &mut [u8],
        width: u32,
        height: u32,
        font: &FontArc,
        text: &str,
        center_x: f32,
        center_y: f32,
        size: f32,
        color: [u8; 4],
    ) {
        if text.trim().is_empty() {
            return;
        }
        let mut font_size = size;
        let max_width = width as f32 - 32.0;
        while measure_text(font, text, font_size) > max_width && font_size > 8.0 {
            font_size -= 0.5;
        }

        let scaled = font.as_scaled(font_size);
        let text_width = measure_text(font, text, font_size);
        let mut cursor_x = center_x - text_width * 0.5;
        let baseline_y = center_y + font_size * 0.36;
        for ch in text.chars() {
            let glyph_id = font.glyph_id(ch);
            let advance = scaled.h_advance(glyph_id);
            let glyph = glyph_id.with_scale_and_position(font_size, point(cursor_x, baseline_y));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|x, y, coverage| {
                    blend_pixel(
                        canvas,
                        width,
                        height,
                        bounds.min.x as i32 + x as i32,
                        bounds.min.y as i32 + y as i32,
                        color,
                        coverage,
                    );
                });
            }
            cursor_x += advance;
        }
    }

    fn measure_text(font: &FontArc, text: &str, size: f32) -> f32 {
        let scaled = font.as_scaled(size);
        text.chars()
            .map(|ch| scaled.h_advance(font.glyph_id(ch)))
            .sum()
    }

    fn blend_pixel(
        canvas: &mut [u8],
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        color: [u8; 4],
        coverage: f32,
    ) {
        if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
            return;
        }
        let idx = ((y as u32 * width + x as u32) * 4) as usize;
        let src_a = (color[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
        if src_a <= 0.0 {
            return;
        }
        let dst_b = canvas[idx] as f32 / 255.0;
        let dst_g = canvas[idx + 1] as f32 / 255.0;
        let dst_r = canvas[idx + 2] as f32 / 255.0;
        let dst_a = canvas[idx + 3] as f32 / 255.0;
        let out_a = src_a + dst_a * (1.0 - src_a);
        if out_a <= 0.0 {
            return;
        }
        let blend = |src: u8, dst: f32| {
            let src = src as f32 / 255.0;
            ((src * src_a + dst * dst_a * (1.0 - src_a)) / out_a * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        canvas[idx] = blend(color[2], dst_b);
        canvas[idx + 1] = blend(color[1], dst_g);
        canvas[idx + 2] = blend(color[0], dst_r);
        canvas[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}
