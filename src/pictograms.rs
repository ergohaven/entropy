use anyhow::{bail, Context, Result};
use image::RgbaImage;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) const PICTOGRAM_WIDTH: usize = 35;
pub(crate) const PICTOGRAM_HEIGHT: usize = 35;
pub(crate) const PICTOGRAM_BYTES: usize = (PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT).div_ceil(8);
const HEADER_SIZE: usize = 256;
const SLOTS_PER_KIND: usize = 256;
const VALID_BYTES: usize = SLOTS_PER_KIND / 8;
const RECORD_METADATA_BYTES: usize = 4;
const RECORD_BYTES: usize = RECORD_METADATA_BYTES + PICTOGRAM_BYTES;
const PAYLOAD_SIZE: usize = SLOTS_PER_KIND * 2 * RECORD_BYTES;
const PACKAGE_SIZE: usize = HEADER_SIZE + PAYLOAD_SIZE;
const MAGIC: u32 = 0x4950_4845;
const FORMAT_VERSION: u8 = 4;
const CMD_QUERY: u8 = 0xC0;
const CMD_READ: u8 = 0xC1;
const CMD_BEGIN: u8 = 0xC2;
const CMD_DATA: u8 = 0xC3;
const CMD_COMMIT: u8 = 0xC4;
const CMD_DATA_STREAM: u8 = 0xC6;
const CMD_SLOT_BEGIN: u8 = 0xC7;
const CMD_SLOT_DATA: u8 = 0xC8;
const CMD_SLOT_COMMIT: u8 = 0xC9;
const CMD_VALID_READ: u8 = 0xCA;
const CMD_SLOT_READ: u8 = 0xCB;
const HID_PACKET_SIZE: usize = 32;
const STREAM_ACK_INTERVAL: usize = 32;

// v3 stores 32x32 masks, v4 stores all 1225 pixels of the 35x35 editor.
const LEGACY_PACKAGE_SIZE: usize = HEADER_SIZE + 512 * 132;
pub(crate) fn normalize_pictogram_bitmap(bitmap: &[u8]) -> Result<Vec<u8>> {
    if bitmap.len() == PICTOGRAM_BYTES {
        return Ok(bitmap.to_vec());
    }
    if bitmap.len() != 128 {
        bail!("invalid pictogram bitmap length");
    }
    let mut result = vec![0u8; PICTOGRAM_BYTES];
    for y in 0..35 {
        for x in 0..35 {
            let sx = (2 * x + 1) * 32 / 70;
            let sy = (2 * y + 1) * 32 / 70;
            let source = sy * 32 + sx;
            let dest = y * 35 + x;
            if bitmap[source / 8] & (0x80 >> (source % 8)) != 0 {
                result[dest / 8] |= 0x80 >> (dest % 8);
            }
        }
    }
    Ok(result)
}

fn backup_pictogram_package(directory: &Path, package: &[u8]) -> Result<()> {
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("device-{:08x}.ehp", crc32(package)));
    if path.exists() && std::fs::read(&path)? == package {
        return Ok(());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(package)?;
    file.sync_all()?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PictogramKind {
    #[default]
    Macro,
    TapDance,
}

impl PictogramKind {
    fn protocol_id(self) -> u8 {
        match self {
            Self::Macro => 0,
            Self::TapDance => 1,
        }
    }

    fn valid_offset(self) -> usize {
        match self {
            Self::Macro => 20,
            Self::TapDance => 20 + VALID_BYTES,
        }
    }

    fn payload_slot(self, slot: usize) -> usize {
        match self {
            Self::Macro => slot,
            Self::TapDance => SLOTS_PER_KIND + slot,
        }
    }
}

pub(crate) fn pictogram_keycode_slot(keycode: u16) -> Option<(PictogramKind, usize)> {
    if (0x7700..=0x77FF).contains(&keycode) {
        Some((PictogramKind::Macro, usize::from(keycode - 0x7700)))
    } else if (0x5700..=0x57FF).contains(&keycode) {
        Some((PictogramKind::TapDance, usize::from(keycode - 0x5700)))
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PictogramLibrary {
    package: Vec<u8>,
}

impl Default for PictogramLibrary {
    fn default() -> Self {
        Self::blank()
    }
}

impl PictogramLibrary {
    pub(crate) fn blank() -> Self {
        let mut package = vec![0x00; PACKAGE_SIZE];
        package[..HEADER_SIZE].fill(0);
        package[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        package[4] = FORMAT_VERSION;
        package[5] = PICTOGRAM_WIDTH as u8;
        package[6] = PICTOGRAM_HEIGHT as u8;
        package[7] = PICTOGRAM_BYTES as u8;
        package[8..12].copy_from_slice(&(PACKAGE_SIZE as u32).to_le_bytes());
        let mut library = Self { package };
        library.refresh_checksums();
        library
    }

    pub(crate) fn from_package(package: Vec<u8>) -> Result<Self> {
        if package.len() == LEGACY_PACKAGE_SIZE
            && package[4..8] == [3, 32, 32, 128]
            && u32::from_le_bytes(package[0..4].try_into().unwrap()) == MAGIC
            && u32::from_le_bytes(package[8..12].try_into().unwrap()) as usize
                == LEGACY_PACKAGE_SIZE
        {
            if crc32(&package[HEADER_SIZE..])
                != u32::from_le_bytes(package[12..16].try_into().unwrap())
                || pictogram_header_crc(&package[..HEADER_SIZE])
                    != u32::from_le_bytes(package[16..20].try_into().unwrap())
            {
                bail!("legacy pictogram checksum mismatch");
            }
            let mut result = Self::blank();
            for kind in [PictogramKind::Macro, PictogramKind::TapDance] {
                for slot in 0..256 {
                    if package[kind.valid_offset() + slot / 8] & (1 << (slot % 8)) == 0 {
                        continue;
                    }
                    let start = HEADER_SIZE + kind.payload_slot(slot) * 132;
                    let bitmap = normalize_pictogram_bitmap(&package[start + 4..start + 132])?;
                    result.set_colored(
                        kind,
                        slot,
                        &bitmap,
                        package[start..start + 3].try_into().unwrap(),
                    );
                }
            }
            return Ok(result);
        }
        if package.len() != PACKAGE_SIZE
            || u32::from_le_bytes(package[0..4].try_into().unwrap()) != MAGIC
            || package[4] != FORMAT_VERSION
            || package[5] as usize != PICTOGRAM_WIDTH
            || package[6] as usize != PICTOGRAM_HEIGHT
            || package[7] as usize != PICTOGRAM_BYTES
            || u32::from_le_bytes(package[8..12].try_into().unwrap()) as usize != PACKAGE_SIZE
        {
            bail!("invalid pictogram package header");
        }
        let expected_data = u32::from_le_bytes(package[12..16].try_into().unwrap());
        if crc32(&package[HEADER_SIZE..]) != expected_data {
            bail!("pictogram payload checksum mismatch");
        }
        let expected_header = u32::from_le_bytes(package[16..20].try_into().unwrap());
        if pictogram_header_crc(&package[..HEADER_SIZE]) != expected_header {
            bail!("pictogram header checksum mismatch");
        }
        Ok(Self { package })
    }

    pub(crate) fn has(&self, kind: PictogramKind, slot: usize) -> bool {
        slot < SLOTS_PER_KIND
            && self.package[kind.valid_offset() + slot / 8] & (1 << (slot % 8)) != 0
    }

    pub(crate) fn bitmap(&self, kind: PictogramKind, slot: usize) -> Option<&[u8]> {
        if !self.has(kind, slot) {
            return None;
        }
        let start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES + RECORD_METADATA_BYTES;
        Some(&self.package[start..start + PICTOGRAM_BYTES])
    }

    pub(crate) fn color(&self, kind: PictogramKind, slot: usize) -> Option<[u8; 3]> {
        if !self.has(kind, slot) {
            return None;
        }
        let start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES;
        Some(
            self.package[start..start + 3]
                .try_into()
                .expect("three-byte color"),
        )
    }

    pub(crate) fn set(&mut self, kind: PictogramKind, slot: usize, bitmap: &[u8]) {
        self.set_colored(kind, slot, bitmap, [84, 189, 191]);
    }

    pub(crate) fn set_colored(
        &mut self,
        kind: PictogramKind,
        slot: usize,
        bitmap: &[u8],
        color: [u8; 3],
    ) {
        if slot >= SLOTS_PER_KIND || bitmap.len() != PICTOGRAM_BYTES {
            return;
        }
        self.package[kind.valid_offset() + slot / 8] |= 1 << (slot % 8);
        let start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES;
        self.package[start..start + 3].copy_from_slice(&color);
        self.package[start + 3] = 1;
        self.package[start + RECORD_METADATA_BYTES..start + RECORD_BYTES].copy_from_slice(bitmap);
        self.refresh_checksums();
    }

    pub(crate) fn clear(&mut self, kind: PictogramKind, slot: usize) {
        if slot >= SLOTS_PER_KIND {
            return;
        }
        self.package[kind.valid_offset() + slot / 8] &= !(1 << (slot % 8));
        let start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES;
        self.package[start..start + RECORD_BYTES].fill(0x00);
        self.refresh_checksums();
    }

    fn refresh_checksums(&mut self) {
        let data_crc = crc32(&self.package[HEADER_SIZE..]);
        self.package[12..16].copy_from_slice(&data_crc.to_le_bytes());
        self.package[16..20].fill(0);
        let header_crc = pictogram_header_crc(&self.package[..HEADER_SIZE]);
        self.package[16..20].copy_from_slice(&header_crc.to_le_bytes());
    }
}

pub(crate) const BUILTIN_PICTOGRAM_KEYS: &[&str] = &[
    "display_settings.pictogram_builtin_play",
    "display_settings.pictogram_builtin_pause",
    "display_settings.pictogram_builtin_stop",
    "display_settings.pictogram_builtin_copy",
    "display_settings.pictogram_builtin_paste",
    "display_settings.pictogram_builtin_undo",
    "display_settings.pictogram_builtin_redo",
    "display_settings.pictogram_builtin_save",
    "display_settings.pictogram_builtin_terminal",
    "display_settings.pictogram_builtin_code",
    "display_settings.pictogram_builtin_volume",
    "display_settings.pictogram_builtin_mute",
    "display_settings.pictogram_builtin_microphone",
    "display_settings.pictogram_builtin_camera",
    "display_settings.pictogram_builtin_mail",
    "display_settings.pictogram_builtin_lock",
    "display_settings.pictogram_builtin_search",
    "display_settings.pictogram_builtin_home",
    "display_settings.pictogram_builtin_settings",
    "display_settings.pictogram_builtin_single_tap",
    "display_settings.pictogram_builtin_double_tap",
    "display_settings.pictogram_builtin_hold",
    "display_settings.pictogram_builtin_cut",
    "display_settings.pictogram_builtin_screenshot",
    "display_settings.pictogram_builtin_mouse_button_left",
    "display_settings.pictogram_builtin_mouse_button_right",
    "display_settings.pictogram_builtin_mouse_button_middle",
    "display_settings.pictogram_builtin_mouse_up",
    "display_settings.pictogram_builtin_mouse_down",
    "display_settings.pictogram_builtin_mouse_left",
    "display_settings.pictogram_builtin_mouse_right",
    "display_settings.pictogram_builtin_brightness_down",
    "display_settings.pictogram_builtin_brightness_up",
    "display_settings.pictogram_builtin_computer",
    "display_settings.pictogram_builtin_web_search",
    "display_settings.pictogram_builtin_previous_track",
    "display_settings.pictogram_builtin_next_track",
    "display_settings.pictogram_builtin_calculator",
];

struct PictogramCanvas {
    pixels: [bool; 32 * 32],
}

#[derive(Clone, Copy)]
enum MouseButtonHighlight {
    None,
    Left,
    Right,
    Middle,
}

impl PictogramCanvas {
    fn new() -> Self {
        Self {
            pixels: [false; 32 * 32],
        }
    }

    fn pixel(&mut self, x: i32, y: i32) {
        if (0..32 as i32).contains(&x) && (0..32 as i32).contains(&y) {
            self.pixels[y as usize * 32 + x as usize] = true;
        }
    }

    fn pixel_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.pixel(x, y);
            }
        }
    }

    fn clear_pixel(&mut self, x: i32, y: i32) {
        if (0..32 as i32).contains(&x) && (0..32 as i32).contains(&y) {
            self.pixels[y as usize * 32 + x as usize] = false;
        }
    }

    fn clear_pixel_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.clear_pixel(x, y);
            }
        }
    }

    fn circle_pixels(&mut self, cx: i32, cy: i32, radius: i32, filled: bool) {
        let outer = radius * radius;
        let inner = (radius - 2).max(0).pow(2);
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let distance = (x - cx).pow(2) + (y - cy).pow(2);
                if distance <= outer && (filled || distance >= inner) {
                    self.pixel(x, y);
                }
            }
        }
    }

    fn clear_circle_pixels(&mut self, cx: i32, cy: i32, radius: i32) {
        let outer = radius * radius;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                if (x - cx).pow(2) + (y - cy).pow(2) <= outer {
                    self.clear_pixel(x, y);
                }
            }
        }
    }

    fn design_coordinate(value: i32) -> i32 {
        (value * (32 as i32 - 1) + 11) / 23
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32) {
        let x0 = Self::design_coordinate(x0);
        let y0 = Self::design_coordinate(y0);
        let x1 = Self::design_coordinate(x1);
        let y1 = Self::design_coordinate(y1);
        let thickness = if thickness <= 1 {
            1
        } else {
            Self::design_coordinate(thickness) - Self::design_coordinate(0)
        };
        self.line_pixels(x0, y0, x1, y1, thickness);
    }

    fn line_pixels(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, thickness: i32) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            let radius = thickness.saturating_sub(1) / 2;
            for oy in -radius..=radius {
                for ox in -radius..=radius {
                    self.pixel(x0 + ox, y0 + oy);
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let twice = error * 2;
            if twice >= dy {
                error += dy;
                x0 += sx;
            }
            if twice <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, filled: bool) {
        let x0 = Self::design_coordinate(x0);
        let y0 = Self::design_coordinate(y0);
        let x1 = Self::design_coordinate(x1);
        let y1 = Self::design_coordinate(y1);
        if filled {
            for y in y0..=y1 {
                self.line_pixels(x0, y, x1, y, 1);
            }
        } else {
            self.line_pixels(x0, y0, x1, y0, 1);
            self.line_pixels(x1, y0, x1, y1, 1);
            self.line_pixels(x1, y1, x0, y1, 1);
            self.line_pixels(x0, y1, x0, y0, 1);
        }
    }

    fn circle(&mut self, cx: i32, cy: i32, radius: i32, filled: bool) {
        let cx = Self::design_coordinate(cx);
        let cy = Self::design_coordinate(cy);
        let radius = Self::design_coordinate(radius) - Self::design_coordinate(0);
        let outer = radius * radius;
        let inner = (radius - 3).max(0).pow(2);
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let distance = (x - cx).pow(2) + (y - cy).pow(2);
                if distance <= outer && (filled || distance >= inner) {
                    self.pixel(x, y);
                }
            }
        }
    }

    /// Draw the same upright mouse silhouette used by the firmware, directly
    /// on the 32x32 target grid. Keeping this in pixel coordinates avoids the
    /// asymmetric rounding that the older 24x24 design-coordinate path caused.
    fn mouse_body_pixels(&mut self, x: i32, y: i32, highlight: MouseButtonHighlight) {
        // Filled, vertically symmetric outer shell (16x24).
        for (row, left, right) in [
            (0, 6, 9),
            (1, 4, 11),
            (2, 2, 13),
            (3, 1, 14),
            (4, 1, 14),
            (5, 1, 14),
            (6, 1, 14),
            (7, 1, 14),
            (8, 1, 14),
            (9, 1, 14),
            (10, 1, 14),
            (11, 1, 14),
            (12, 1, 14),
            (13, 1, 14),
            (14, 1, 14),
            (15, 1, 14),
            (16, 1, 14),
            (17, 1, 14),
            (18, 1, 14),
            (19, 1, 14),
            (20, 2, 13),
            (21, 3, 12),
            (22, 4, 11),
            (23, 6, 9),
        ] {
            self.pixel_rect(x + left, y + row, x + right, y + row);
        }

        // Hollow the shell while preserving a crisp two-pixel outline.
        self.clear_pixel_rect(x + 4, y + 3, x + 11, y + 3);
        self.clear_pixel_rect(x + 3, y + 4, x + 12, y + 8);
        self.clear_pixel_rect(x + 3, y + 11, x + 12, y + 18);
        self.clear_pixel_rect(x + 4, y + 19, x + 11, y + 19);
        self.clear_pixel_rect(x + 5, y + 20, x + 10, y + 20);

        // Upper button separator and lower shell separator.
        self.pixel_rect(x + 7, y, x + 8, y + 9);
        self.pixel_rect(x + 1, y + 9, x + 14, y + 10);

        match highlight {
            MouseButtonHighlight::None => {}
            MouseButtonHighlight::Left => self.pixel_rect(x + 3, y + 4, x + 6, y + 8),
            MouseButtonHighlight::Right => self.pixel_rect(x + 9, y + 4, x + 12, y + 8),
            MouseButtonHighlight::Middle => self.pixel_rect(x + 7, y + 3, x + 8, y + 7),
        }
    }

    fn arrow_up_pixels(&mut self) {
        self.pixel_rect(15, 0, 16, 7);
        for (y, x0, x1) in [
            (0, 15, 16),
            (1, 14, 17),
            (2, 13, 18),
            (3, 12, 19),
            (4, 11, 20),
        ] {
            self.pixel_rect(x0, y, x1, y);
        }
    }

    fn arrow_down_pixels(&mut self) {
        self.pixel_rect(15, 24, 16, 31);
        for (y, x0, x1) in [
            (27, 11, 20),
            (28, 12, 19),
            (29, 13, 18),
            (30, 14, 17),
            (31, 15, 16),
        ] {
            self.pixel_rect(x0, y, x1, y);
        }
    }

    fn arrow_left_pixels(&mut self) {
        self.pixel_rect(0, 15, 8, 16);
        for (x, y0, y1) in [
            (0, 15, 16),
            (1, 14, 17),
            (2, 13, 18),
            (3, 12, 19),
            (4, 11, 20),
        ] {
            self.pixel_rect(x, y0, x, y1);
        }
    }

    fn arrow_right_pixels(&mut self) {
        self.pixel_rect(23, 15, 31, 16);
        for (x, y0, y1) in [
            (27, 11, 20),
            (28, 12, 19),
            (29, 13, 18),
            (30, 14, 17),
            (31, 15, 16),
        ] {
            self.pixel_rect(x, y0, x, y1);
        }
    }

    fn horizontal_triangle(&mut self, points_right: bool) {
        let left = Self::design_coordinate(6);
        let right = Self::design_coordinate(18);
        let center_y = (Self::design_coordinate(11) + Self::design_coordinate(12)) / 2;
        let base_half_height = Self::design_coordinate(18) - center_y;
        for x in left..=right {
            let distance_from_tip = if points_right { right - x } else { x - left };
            let half_height =
                (base_half_height * distance_from_tip + (right - left) / 2) / (right - left);
            self.line_pixels(x, center_y - half_height, x, center_y + half_height, 1);
        }
    }

    fn play_triangle(&mut self) {
        self.horizontal_triangle(true);
    }

    fn bitmap(self) -> [u8; PICTOGRAM_BYTES] {
        let mut bitmap = [0x00; 128];
        for (index, active) in self.pixels.into_iter().enumerate() {
            if active {
                bitmap[index / 8] |= 1 << (7 - index % 8);
            }
        }
        normalize_pictogram_bitmap(&bitmap)
            .expect("built-in 32px design")
            .try_into()
            .unwrap()
    }
}

// Native 35px designs: symmetric shapes share integer axis 17, no intermediate scaling.
struct Icon35([bool; 35 * 35]);
impl Icon35 {
    fn new() -> Self {
        Self([false; 35 * 35])
    }
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, fill: bool) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                if (0..35).contains(&x)
                    && (0..35).contains(&y)
                    && (fill || x < x0 + 2 || x > x1 - 2 || y < y0 + 2 || y > y1 - 2)
                {
                    self.0[(y * 35 + x) as usize] = true;
                }
            }
        }
    }
    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, fill: bool, on: bool) {
        for y in 0..35 {
            for x in 0..35 {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let outer = dx * dx / (rx * rx) + dy * dy / (ry * ry) <= 1.0;
                let inner = rx > 2.0
                    && ry > 2.0
                    && dx * dx / ((rx - 2.0) * (rx - 2.0)) + dy * dy / ((ry - 2.0) * (ry - 2.0))
                        < 1.0;
                if outer && (fill || !inner) {
                    self.0[y * 35 + x] = on;
                }
            }
        }
    }
    fn circle(&mut self, x: i32, y: i32, r: i32, fill: bool, on: bool) {
        self.ellipse(x as f32, y as f32, r as f32, r as f32, fill, on);
    }
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = dx * dx + dy * dy;
        for y in 0..35 {
            for x in 0..35 {
                let t = if len == 0.0 {
                    0.0
                } else {
                    (((x as f32 - x0) * dx + (y as f32 - y0) * dy) / len).clamp(0.0, 1.0)
                };
                if (x as f32 - x0 - t * dx).powi(2) + (y as f32 - y0 - t * dy).powi(2)
                    <= 1.15 * 1.15
                {
                    self.0[y * 35 + x] = true;
                }
            }
        }
    }
    fn path(&mut self, p: &[(i32, i32)]) {
        for q in p.windows(2) {
            self.line(q[0].0 as f32, q[0].1 as f32, q[1].0 as f32, q[1].1 as f32);
        }
    }
    fn polygon(&mut self, p: &[(i32, i32)]) {
        for y in 0..35 {
            for x in 0..35 {
                let mut inside = false;
                let mut j = p.len() - 1;
                for i in 0..p.len() {
                    let (a, b) = (p[i], p[j]);
                    if (a.1 > y) != (b.1 > y)
                        && (x as f32)
                            < (b.0 - a.0) as f32 * (y - a.1) as f32 / (b.1 - a.1) as f32
                                + a.0 as f32
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    self.0[(y * 35 + x) as usize] = true;
                }
            }
        }
        let mut edge = p.to_vec();
        edge.push(p[0]);
        self.path(&edge);
    }
    fn arc(&mut self, cx: i32, cy: i32, r: i32) {
        for y in 0..35 {
            for x in 0..35 {
                let dx = x - cx;
                let dy = y - cy;
                let d = dx * dx + dy * dy;
                if dx > 0 && dy.abs() <= r * 3 / 4 && d <= r * r && d >= (r - 2) * (r - 2) {
                    self.0[(y * 35 + x) as usize] = true;
                }
            }
        }
    }
    fn mirror_x(&mut self) {
        for y in 0..35 {
            self.0[y * 35..(y + 1) * 35].reverse();
        }
    }
    fn mouse(&mut self, cx: i32, cy: i32, button: u8) {
        self.ellipse(cx as f32, cy as f32, 8.0, 12.0, false, true);
        self.path(&[(cx, cy - 11), (cx, cy - 1)]);
        self.path(&[(cx - 7, cy), (cx + 7, cy)]);
        match button {
            1 => self.rect(cx - 5, cy - 7, cx - 2, cy - 3, true),
            2 => self.rect(cx + 2, cy - 7, cx + 5, cy - 3, true),
            3 => self.rect(cx - 1, cy - 8, cx + 1, cy - 4, true),
            _ => {}
        }
    }
    fn bitmap(&self) -> [u8; PICTOGRAM_BYTES] {
        let mut b = [0u8; PICTOGRAM_BYTES];
        for (i, on) in self.0.iter().enumerate() {
            if *on {
                b[i / 8] |= 0x80 >> (i % 8);
            }
        }
        b
    }
}

pub(crate) fn builtin_pictogram_bitmap(index: usize) -> [u8; PICTOGRAM_BYTES] {
    static ICONS: std::sync::OnceLock<[[u8; PICTOGRAM_BYTES]; BUILTIN_PICTOGRAM_KEYS.len()]> =
        std::sync::OnceLock::new();
    ICONS.get_or_init(|| std::array::from_fn(render_builtin_pictogram_bitmap))
        [index.min(BUILTIN_PICTOGRAM_KEYS.len() - 1)]
}

fn render_builtin_pictogram_bitmap(index: usize) -> [u8; PICTOGRAM_BYTES] {
    let mut i = Icon35::new();
    match index {
        0 => i.polygon(&[(9, 6), (27, 17), (9, 28)]),
        1 => {
            i.rect(9, 7, 13, 27, true);
            i.rect(21, 7, 25, 27, true);
        }
        2 => i.rect(7, 7, 27, 27, true),
        3 => {
            i.rect(6, 5, 22, 24, false);
            for y in 10..30 {
                for x in 12..29 {
                    i.0[y * 35 + x] = false;
                }
            }
            i.rect(12, 10, 28, 29, false);
        }
        4 => {
            i.rect(7, 8, 27, 30, false);
            i.rect(12, 4, 22, 10, false);
            i.path(&[(12, 16), (22, 16)]);
            i.path(&[(12, 23), (22, 23)]);
        }
        5 | 6 => {
            i.path(&[(12, 7), (5, 14), (12, 21)]);
            i.path(&[(6, 14), (20, 14), (25, 17), (27, 22), (27, 27)]);
            if index == 6 {
                i.mirror_x();
            }
        }
        7 => {
            i.rect(6, 5, 28, 29, false);
            i.rect(10, 5, 23, 13, false);
            i.rect(11, 20, 23, 29, false);
            i.rect(19, 7, 20, 11, true);
        }
        8 => {
            i.path(&[(7, 9), (15, 17), (7, 25)]);
            i.path(&[(19, 25), (28, 25)]);
        }
        9 => {
            i.path(&[(10, 9), (3, 17), (10, 25)]);
            i.path(&[(24, 9), (31, 17), (24, 25)]);
            i.path(&[(20, 6), (14, 28)]);
        }
        10 | 11 => {
            i.polygon(&[(5, 13), (10, 13), (16, 7), (16, 27), (10, 21), (5, 21)]);
            if index == 10 {
                i.arc(15, 17, 8);
                i.arc(15, 17, 14);
            } else {
                i.path(&[(22, 12), (29, 22)]);
                i.path(&[(29, 12), (22, 22)]);
            }
        }
        12 => {
            i.circle(17, 9, 4, true, true);
            i.rect(13, 9, 21, 19, true);
            i.circle(17, 19, 4, true, true);
            for y in 17..28 {
                for x in 7..28 {
                    let d = (x - 17) * (x - 17) + (y - 17) * (y - 17);
                    if (64..=100).contains(&d) {
                        i.0[(y * 35 + x) as usize] = true;
                    }
                }
            }
            i.path(&[(7, 13), (7, 17)]);
            i.path(&[(27, 13), (27, 17)]);
            i.path(&[(17, 27), (17, 31)]);
            i.path(&[(11, 31), (23, 31)]);
        }
        13 => {
            i.rect(4, 11, 30, 28, false);
            i.rect(10, 7, 18, 11, true);
            i.circle(17, 19, 6, false, true);
            i.rect(25, 14, 27, 15, true);
        }
        14 => {
            i.rect(4, 8, 30, 27, false);
            i.path(&[(5, 10), (17, 20), (29, 10)]);
        }
        15 => {
            i.circle(17, 12, 7, false, true);
            i.rect(10, 12, 11, 18, true);
            i.rect(23, 12, 24, 18, true);
            i.rect(7, 16, 27, 30, true);
            i.circle(17, 22, 2, true, false);
            for y in 22..27 {
                for x in 16..19 {
                    i.0[y * 35 + x] = false;
                }
            }
        }
        16 => {
            i.circle(14, 14, 9, false, true);
            i.path(&[(21, 21), (30, 30)]);
        }
        17 => {
            i.path(&[(4, 16), (17, 5), (30, 16)]);
            i.rect(8, 16, 26, 29, false);
            i.rect(14, 21, 20, 29, false);
        }
        18 => {
            let mut teeth = Vec::new();
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::FRAC_PI_4;
                for (offset, radius) in [(-0.32, 9.0), (-0.07, 14.0), (0.07, 14.0), (0.32, 9.0)] {
                    teeth.push((
                        (17.0 + radius * (a + offset).cos()).round() as i32,
                        (17.0 + radius * (a + offset).sin()).round() as i32,
                    ));
                }
            }
            i.polygon(&teeth);
            i.circle(17, 17, 6, true, false);
        }
        19 | 20 => {
            i.circle(17, 17, 14, false, true);
            if index == 19 {
                i.path(&[(13, 14), (17, 10), (17, 25)]);
                i.path(&[(12, 25), (22, 25)]);
            } else {
                i.path(&[(12, 13), (14, 10), (20, 10), (22, 13), (12, 24), (22, 24)]);
            }
        }
        21 => {
            i.path(&[(17, 5), (17, 23)]);
            i.path(&[(11, 18), (17, 24), (23, 18)]);
            i.path(&[(8, 29), (26, 29)]);
        }
        22 => {
            i.path(&[(12, 23), (27, 5)]);
            i.path(&[(22, 23), (7, 5)]);
            i.circle(10, 26, 4, false, true);
            i.circle(24, 26, 4, false, true);
            i.circle(17, 17, 2, true, true);
            i.circle(17, 17, 1, true, false);
        }
        23 => {
            i.rect(4, 7, 30, 28, false);
            i.circle(24, 13, 3, true, true);
            i.path(&[(6, 25), (13, 16), (20, 24), (24, 20), (28, 25)]);
        }
        24..=26 => i.mouse(17, 17, (index - 23) as u8),
        27 | 28 => {
            // Keep the mouse upright for both directions; only the arrow flips.
            let cy = if index == 27 { 22 } else { 12 };
            i.ellipse(17.0, cy as f32, 7.0, 10.0, false, true);
            i.path(&[(17, cy - 9), (17, cy - 1)]);
            i.path(&[(11, cy), (23, cy)]);
            if index == 27 {
                i.path(&[(17, 1), (17, 8)]);
                i.path(&[(13, 5), (17, 1), (21, 5)]);
            } else {
                i.path(&[(17, 26), (17, 33)]);
                i.path(&[(13, 29), (17, 33), (21, 29)]);
            }
        }
        29 | 30 => {
            i.mouse(25, 17, 0);
            i.path(&[(1, 17), (12, 17)]);
            i.path(&[(5, 13), (1, 17), (5, 21)]);
            if index == 30 {
                i.mirror_x();
            }
        }
        31 => {
            i.circle(16, 17, 13, true, true);
            i.circle(23, 10, 12, true, false);
        }
        32 => {
            i.circle(17, 17, 6, true, true);
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::FRAC_PI_4;
                i.line(
                    17.0 + 10.0 * a.cos(),
                    17.0 + 10.0 * a.sin(),
                    17.0 + 14.0 * a.cos(),
                    17.0 + 14.0 * a.sin(),
                );
            }
        }
        33 => {
            i.rect(4, 6, 30, 25, false);
            i.path(&[(17, 25), (17, 30)]);
            i.path(&[(10, 30), (24, 30)]);
        }
        34 => {
            i.circle(14, 14, 10, false, true);
            i.ellipse(14.0, 14.0, 5.0, 10.0, false, true);
            i.path(&[(4, 14), (24, 14)]);
            i.circle(24, 24, 6, false, true);
            i.path(&[(28, 28), (32, 32)]);
        }
        35 | 36 => {
            i.polygon(&[(25, 7), (10, 17), (25, 27)]);
            i.rect(6, 7, 8, 27, true);
            if index == 36 {
                i.mirror_x();
            }
        }
        _ => {
            i.rect(8, 3, 26, 31, false);
            i.rect(11, 7, 23, 14, false);
            for y in [18, 23, 28] {
                for x in [11, 16, 21] {
                    i.rect(x, y, x + 2, y + 1, true);
                }
            }
        }
    }
    i.bitmap()
}

// Recognize the stock gear exported by v072–v074 without importing it as a user copy.
fn legacy_35px_gear_bitmap() -> &'static [u8; PICTOGRAM_BYTES] {
    static GEAR: std::sync::OnceLock<[u8; PICTOGRAM_BYTES]> = std::sync::OnceLock::new();
    GEAR.get_or_init(|| {
        let mut i = Icon35::new();
        let mut teeth = Vec::new();
        for n in 0..8 {
            let a = n as f32 * std::f32::consts::FRAC_PI_4;
            for (offset, radius) in [(-0.36, 10.0), (-0.18, 14.0), (0.18, 14.0), (0.36, 10.0)] {
                teeth.push((
                    (17.0 + radius * (a + offset).cos()).round() as i32,
                    (17.0 + radius * (a + offset).sin()).round() as i32,
                ));
            }
        }
        i.polygon(&teeth);
        i.circle(17, 17, 5, true, false);
        i.bitmap()
    })
}

pub(crate) fn legacy_builtin_pictogram_index(bitmap: &[u8]) -> Option<usize> {
    static ICONS: std::sync::OnceLock<[[u8; PICTOGRAM_BYTES]; BUILTIN_PICTOGRAM_KEYS.len()]> =
        std::sync::OnceLock::new();
    ICONS
        .get_or_init(|| std::array::from_fn(legacy_builtin_pictogram_bitmap))
        .iter()
        .position(|icon| icon.as_slice() == bitmap)
        .or_else(|| (legacy_35px_gear_bitmap().as_slice() == bitmap).then_some(18))
}

pub(crate) fn legacy_builtin_pictogram_bitmap(index: usize) -> [u8; PICTOGRAM_BYTES] {
    let mut icon = PictogramCanvas::new();
    match index.min(BUILTIN_PICTOGRAM_KEYS.len() - 1) {
        0 => {
            icon.play_triangle();
        }
        1 => {
            icon.rect(6, 5, 9, 18, true);
            icon.rect(14, 5, 17, 18, true);
        }
        2 => icon.rect(6, 6, 17, 17, true),
        3 => {
            icon.rect(5, 4, 15, 14, false);
            icon.rect(9, 8, 19, 18, false);
        }
        4 => {
            icon.rect(6, 6, 17, 19, false);
            icon.rect(9, 4, 14, 7, false);
            icon.line(9, 11, 15, 11, 1);
            icon.line(9, 15, 15, 15, 1);
        }
        5 | 6 => {
            let mirror = |x: i32| if index == 5 { x } else { 23 - x };
            icon.line(mirror(5), 11, mirror(10), 6, 2);
            icon.line(mirror(5), 11, mirror(10), 16, 2);
            icon.line(mirror(6), 11, mirror(15), 11, 2);
            icon.line(mirror(15), 11, mirror(18), 14, 2);
        }
        7 => {
            icon.rect(5, 4, 18, 19, false);
            icon.rect(8, 4, 15, 9, false);
            icon.rect(8, 13, 15, 19, false);
        }
        8 => {
            icon.line(5, 6, 10, 11, 2);
            icon.line(10, 11, 5, 16, 2);
            icon.line(12, 17, 19, 17, 2);
        }
        9 => {
            icon.line(9, 6, 4, 11, 2);
            icon.line(4, 11, 9, 16, 2);
            icon.line(15, 6, 20, 11, 2);
            icon.line(20, 11, 15, 16, 2);
            icon.line(13, 4, 10, 19, 1);
        }
        10 | 11 => {
            icon.rect(4, 9, 7, 14, true);
            icon.line(7, 9, 12, 5, 2);
            icon.line(12, 5, 12, 18, 2);
            icon.line(12, 18, 7, 14, 2);
            if index == 10 {
                icon.circle(12, 12, 7, false);
                icon.circle(12, 12, 9, false);
            } else {
                icon.line(15, 8, 20, 16, 2);
                icon.line(20, 8, 15, 16, 2);
            }
        }
        12 => {
            icon.rect(9, 4, 14, 14, false);
            icon.circle(11, 9, 6, false);
            icon.line(11, 15, 11, 19, 2);
            icon.line(7, 19, 15, 19, 2);
        }
        13 => {
            icon.rect(4, 7, 19, 18, false);
            icon.rect(7, 5, 11, 7, true);
            icon.circle(12, 12, 4, false);
        }
        14 => {
            icon.rect(4, 6, 19, 18, false);
            icon.line(4, 7, 11, 13, 1);
            icon.line(19, 7, 12, 13, 1);
        }
        15 => {
            icon.rect(6, 10, 17, 19, true);
            icon.circle(11, 10, 5, false);
            icon.circle(11, 14, 1, true);
        }
        16 => {
            icon.circle(10, 9, 6, false);
            icon.line(14, 14, 20, 20, 3);
        }
        17 => {
            icon.line(4, 11, 11, 4, 2);
            icon.line(11, 4, 19, 11, 2);
            icon.rect(6, 11, 17, 19, false);
            icon.rect(10, 15, 13, 19, false);
        }
        18 => {
            icon.circle(11, 11, 4, false);
            icon.circle(11, 11, 8, false);
            for (x, y) in [
                (11, 2),
                (11, 20),
                (2, 11),
                (20, 11),
                (5, 5),
                (17, 5),
                (5, 17),
                (17, 17),
            ] {
                icon.circle(x, y, 1, true);
            }
        }
        19 | 20 => {
            icon.circle(11, 11, 9, false);
            if index == 19 {
                icon.line(11, 7, 11, 16, 2);
                icon.line(9, 8, 11, 6, 2);
                icon.line(8, 17, 14, 17, 2);
            } else {
                icon.line(8, 8, 11, 6, 2);
                icon.line(11, 6, 14, 8, 2);
                icon.line(14, 8, 8, 16, 2);
                icon.line(8, 17, 15, 17, 2);
            }
        }
        22 => {
            icon.circle(7, 17, 3, false);
            icon.circle(16, 17, 3, false);
            icon.line(8, 15, 19, 4, 2);
            icon.line(15, 15, 5, 5, 2);
        }
        23 => {
            icon.rect(3, 5, 20, 18, false);
            icon.circle(16, 9, 2, true);
            icon.line(4, 17, 9, 11, 2);
            icon.line(9, 11, 13, 15, 2);
            icon.line(13, 15, 16, 12, 2);
            icon.line(16, 12, 20, 17, 2);
        }
        24 | 25 | 26 => {
            let highlight = match index {
                24 => MouseButtonHighlight::Left,
                25 => MouseButtonHighlight::Right,
                _ => MouseButtonHighlight::Middle,
            };
            icon.mouse_body_pixels(8, 4, highlight);
        }
        27 | 28 | 29 | 30 => match index {
            27 => {
                icon.mouse_body_pixels(8, 7, MouseButtonHighlight::None);
                icon.arrow_up_pixels();
            }
            28 => {
                icon.mouse_body_pixels(8, 1, MouseButtonHighlight::None);
                icon.arrow_down_pixels();
            }
            29 => {
                icon.mouse_body_pixels(13, 4, MouseButtonHighlight::None);
                icon.arrow_left_pixels();
            }
            _ => {
                icon.mouse_body_pixels(3, 4, MouseButtonHighlight::None);
                icon.arrow_right_pixels();
            }
        },
        31 => {
            icon.circle_pixels(15, 16, 11, true);
            icon.clear_circle_pixels(21, 10, 10);
        }
        32 => {
            icon.circle_pixels(16, 16, 5, true);
            for (x0, y0, x1, y1) in [
                (16, 1, 16, 6),
                (16, 25, 16, 30),
                (1, 16, 6, 16),
                (25, 16, 30, 16),
                (5, 5, 9, 9),
                (23, 23, 27, 27),
                (27, 5, 23, 9),
                (5, 27, 9, 23),
            ] {
                icon.line_pixels(x0, y0, x1, y1, 3);
            }
        }
        33 => {
            icon.pixel_rect(3, 5, 28, 23);
            icon.clear_pixel_rect(6, 8, 25, 20);
            icon.pixel_rect(14, 23, 17, 27);
            icon.pixel_rect(9, 28, 22, 30);
        }
        34 => {
            icon.circle_pixels(11, 11, 9, false);
            icon.line_pixels(2, 11, 20, 11, 1);
            icon.line_pixels(11, 2, 11, 20, 1);
            icon.circle_pixels(21, 21, 6, false);
            icon.line_pixels(25, 25, 30, 30, 3);
        }
        35 => {
            icon.horizontal_triangle(false);
            icon.rect(3, 5, 5, 18, true);
        }
        36 => {
            icon.horizontal_triangle(true);
            icon.rect(19, 5, 21, 18, true);
        }
        37 => {
            icon.pixel_rect(6, 2, 25, 29);
            icon.clear_pixel_rect(8, 4, 23, 27);
            icon.pixel_rect(9, 6, 22, 12);
            icon.clear_pixel_rect(11, 8, 20, 10);
            for y in [16, 21, 26] {
                for x in [10, 15, 20] {
                    icon.pixel_rect(x, y, x + 2, y + 2);
                }
            }
        }
        _ => {
            icon.line(11, 3, 11, 15, 3);
            icon.line(7, 11, 11, 15, 3);
            icon.line(15, 11, 11, 15, 3);
            icon.rect(5, 18, 17, 20, true);
        }
    }
    if index == 30 {
        let left = legacy_builtin_pictogram_bitmap(29);
        let mut right = [0u8; PICTOGRAM_BYTES];
        for y in 0..35 {
            for x in 0..35 {
                let a = y * 35 + x;
                let b = y * 35 + 34 - x;
                if left[a / 8] & (0x80 >> (a % 8)) != 0 {
                    right[b / 8] |= 0x80 >> (b % 8);
                }
            }
        }
        right
    } else {
        icon.bitmap()
    }
}

pub(crate) fn pictogram_bitmap_levels(bitmap: &[u8]) -> Vec<u8> {
    (0..PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT)
        .map(|index| {
            if bitmap[index / 8] & (1 << (7 - index % 8)) != 0 {
                255
            } else {
                0
            }
        })
        .collect()
}

pub(crate) fn builtin_pictogram_index(bitmap: &[u8]) -> Option<usize> {
    (0..BUILTIN_PICTOGRAM_KEYS.len())
        .find(|index| builtin_pictogram_bitmap(*index).as_slice() == bitmap)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PictogramSettingsState {
    pub(crate) supported: Option<bool>,
    pub(crate) loaded: bool,
    pub(crate) loading: bool,
    // Terminal read outcome for this connection. Keep the error separate from
    // firmware support: a failed recovery read must not become an automatic
    // per-render retry, nor make a supported device appear unsupported.
    pub(crate) load_failure: Option<(u64, String)>,
    // A failed transfer leaves an independent draft that recovery reads must not replace.
    pub(crate) preserve_editor_on_load: bool,
    pub(crate) library: PictogramLibrary,
    pub(crate) selected_kind: PictogramKind,
    pub(crate) selected_slot: usize,
    pub(crate) source_levels: Vec<u8>,
    pub(crate) source_file_name: String,
    pub(crate) threshold: u8,
    pub(crate) inverted: bool,
    pub(crate) selected_builtin: Option<usize>,
    pub(crate) editor_color: [u8; 3],
    pub(crate) editor_name: String,
    pub(crate) search_query: String,
    pub(crate) selected_saved_pictogram: Option<usize>,
    pub(crate) editor_draw_foreground: bool,
    pub(crate) undo: Vec<Vec<u8>>,
    pub(crate) editor_last_cell: Option<(u8, u8)>,
    pub(crate) upload_due: Option<std::time::Instant>,
}

impl Default for PictogramSettingsState {
    fn default() -> Self {
        let bitmap = builtin_pictogram_bitmap(0);
        Self {
            supported: None,
            loaded: false,
            loading: false,
            load_failure: None,
            preserve_editor_on_load: false,
            library: PictogramLibrary::default(),
            selected_kind: PictogramKind::default(),
            selected_slot: 0,
            source_levels: pictogram_bitmap_levels(&bitmap),
            source_file_name: String::new(),
            threshold: 128,
            inverted: false,
            selected_builtin: Some(0),
            editor_color: [84, 189, 191],
            editor_name: String::new(),
            search_query: String::new(),
            selected_saved_pictogram: None,
            editor_draw_foreground: true,
            undo: Vec::new(),
            editor_last_cell: None,
            upload_due: None,
        }
    }
}

impl PictogramSettingsState {
    pub(crate) fn needs_automatic_load(&self, generation: u64) -> bool {
        !self.loaded
            && !self.loading
            && self.supported != Some(false)
            && self
                .load_failure
                .as_ref()
                .is_none_or(|(failed_generation, _)| *failed_generation != generation)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedPictogram {
    pub(crate) levels: Vec<u8>,
    pub(crate) threshold: u8,
    pub(crate) inverted: bool,
    pub(crate) file_name: String,
}

pub(crate) fn prepare_pictogram(path: &Path) -> Result<PreparedPictogram> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    if metadata.len() > 16 * 1024 * 1024 {
        bail!("source image is larger than 16 MB");
    }
    let image = image::open(path)
        .with_context(|| format!("cannot decode {}", path.display()))?
        .to_rgba8();
    let (cropped, has_transparency) = crop_transparent_bounds(&image);
    let resized = fit_icon(&cropped);
    let mut levels = Vec::with_capacity(PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT);
    if has_transparency {
        levels.extend(resized.pixels().map(|pixel| pixel[3]));
    } else {
        levels.extend(resized.pixels().map(|pixel| {
            let luma =
                (u32::from(pixel[0]) * 54 + u32::from(pixel[1]) * 183 + u32::from(pixel[2]) * 19)
                    / 256;
            255 - luma as u8
        }));
    }
    let threshold = otsu_threshold(&levels).clamp(24, 232);
    let high_count = levels.iter().filter(|value| **value >= threshold).count();
    let inverted = !has_transparency && high_count > levels.len() / 2;
    Ok(PreparedPictogram {
        levels,
        threshold,
        inverted,
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("icon")
            .to_owned(),
    })
}

fn crop_transparent_bounds(image: &RgbaImage) -> (RgbaImage, bool) {
    let has_transparency = image.pixels().any(|pixel| pixel[3] < 250);
    if !has_transparency {
        return (image.clone(), false);
    }
    let mut min_x = image.width();
    let mut min_y = image.height();
    let mut max_x = 0;
    let mut max_y = 0;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return (RgbaImage::new(1, 1), true);
    }
    (
        image::imageops::crop_imm(image, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image(),
        true,
    )
}

fn fit_icon(image: &RgbaImage) -> RgbaImage {
    let scale = (PICTOGRAM_WIDTH as f32 / image.width().max(1) as f32)
        .min(PICTOGRAM_HEIGHT as f32 / image.height().max(1) as f32);
    let width = ((image.width() as f32 * scale).round() as u32).clamp(1, PICTOGRAM_WIDTH as u32);
    let height = ((image.height() as f32 * scale).round() as u32).clamp(1, PICTOGRAM_HEIGHT as u32);
    let resized =
        image::imageops::resize(image, width, height, image::imageops::FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(PICTOGRAM_WIDTH as u32, PICTOGRAM_HEIGHT as u32);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((PICTOGRAM_WIDTH as u32 - width) / 2),
        i64::from((PICTOGRAM_HEIGHT as u32 - height) / 2),
    );
    canvas
}

pub(crate) fn quantize_pictogram(
    levels: &[u8],
    threshold: u8,
    inverted: bool,
) -> [u8; PICTOGRAM_BYTES] {
    let mut bitmap = [0x00; PICTOGRAM_BYTES];
    for (index, level) in levels
        .iter()
        .take(PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT)
        .enumerate()
    {
        let level = if inverted { 255 - *level } else { *level };
        if level >= threshold {
            bitmap[index / 8] |= 1 << (7 - index % 8);
        }
    }
    bitmap
}

fn otsu_threshold(values: &[u8]) -> u8 {
    let mut histogram = [0u32; 256];
    for value in values {
        histogram[*value as usize] += 1;
    }
    let total = values.len() as f64;
    let sum = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * f64::from(*count))
        .sum::<f64>();
    let mut background_weight = 0.0;
    let mut background_sum = 0.0;
    let mut best_variance = -1.0;
    let mut best = 128;
    for (threshold, count) in histogram.iter().enumerate() {
        background_weight += f64::from(*count);
        if background_weight == 0.0 {
            continue;
        }
        let foreground_weight = total - background_weight;
        if foreground_weight == 0.0 {
            break;
        }
        background_sum += threshold as f64 * f64::from(*count);
        let background_mean = background_sum / background_weight;
        let foreground_mean = (sum - background_sum) / foreground_weight;
        let variance =
            background_weight * foreground_weight * (background_mean - foreground_mean).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best = threshold as u8;
        }
    }
    best
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc ^ 0xFFFF_FFFF
}

fn pictogram_header_crc(header: &[u8]) -> u32 {
    let mut copy = header.to_vec();
    copy[16..20].fill(0);
    crc32(&copy)
}

fn checked_response(command: u8, response: [u8; HID_PACKET_SIZE]) -> Result<[u8; HID_PACKET_SIZE]> {
    if response[0] != command {
        bail!("pictogram command 0x{command:02X} is unsupported");
    }
    if response[1] != 0 {
        bail!(
            "pictogram command 0x{command:02X} failed with status {}",
            response[1]
        );
    }
    Ok(response)
}

impl crate::hid::HidDevice {
    pub(crate) fn load_pictograms(&self, progress: &AtomicU32) -> Result<PictogramLibrary> {
        let query = checked_response(CMD_QUERY, self.usb_send(&[CMD_QUERY])?)?;
        let legacy = query[2] == 3
            && query[4] == 32
            && query[5] == 32
            && u16::from_le_bytes([query[6], query[7]]) == 128
            && u32::from_le_bytes(query[8..12].try_into().unwrap()) as usize == LEGACY_PACKAGE_SIZE;
        let current = query[2] == FORMAT_VERSION
            && query[4] == 35
            && query[5] == 35
            && u16::from_le_bytes([query[6], query[7]]) as usize == PICTOGRAM_BYTES
            && u32::from_le_bytes(query[8..12].try_into().unwrap()) as usize == PACKAGE_SIZE;
        if !legacy && !current {
            bail!("unsupported pictogram format");
        }
        let record_bytes = if legacy { 132 } else { RECORD_BYTES };
        let package_bytes = if legacy {
            LEGACY_PACKAGE_SIZE
        } else {
            PACKAGE_SIZE
        };
        if query[3] == 0 {
            progress.store(1000, Ordering::Relaxed);
            return Ok(PictogramLibrary::blank());
        }
        if query[16] >= 2 {
            let kinds = [PictogramKind::Macro, PictogramKind::TapDance];
            let mut validity = [[0u8; VALID_BYTES]; 2];
            for (kind_index, kind) in kinds.iter().copied().enumerate() {
                for offset in (0..VALID_BYTES).step_by(30) {
                    let request = [CMD_VALID_READ, kind.protocol_id(), offset as u8];
                    let response = checked_response(CMD_VALID_READ, self.usb_send(&request)?)?;
                    let amount = (VALID_BYTES - offset).min(30);
                    validity[kind_index][offset..offset + amount]
                        .copy_from_slice(&response[2..2 + amount]);
                }
            }

            let occupied = kinds
                .iter()
                .enumerate()
                .flat_map(|(kind_index, kind)| {
                    (0..SLOTS_PER_KIND)
                        .filter(move |slot| validity[kind_index][slot / 8] & (1 << (slot % 8)) != 0)
                        .map(move |slot| (*kind, slot))
                })
                .collect::<Vec<_>>();
            let requests_per_record = record_bytes.div_ceil(30);
            let total_requests = occupied.len() * requests_per_record;
            let mut completed_requests = 0usize;
            let mut library = PictogramLibrary::blank();
            let mut original = vec![0u8; if legacy { LEGACY_PACKAGE_SIZE } else { 0 }];
            if legacy {
                original[..4].copy_from_slice(&MAGIC.to_le_bytes());
                original[4..8].copy_from_slice(&[3, 32, 32, 128]);
                original[8..12].copy_from_slice(&(LEGACY_PACKAGE_SIZE as u32).to_le_bytes());
            }
            for (kind_index, kind) in kinds.iter().copied().enumerate() {
                let valid_start = kind.valid_offset();
                library.package[valid_start..valid_start + VALID_BYTES]
                    .copy_from_slice(&validity[kind_index]);
                if legacy {
                    original[valid_start..valid_start + VALID_BYTES]
                        .copy_from_slice(&validity[kind_index]);
                }
            }
            for (kind, slot) in occupied {
                let mut record = vec![0u8; record_bytes];
                for offset in (0..record_bytes).step_by(30) {
                    let mut request = [0u8; 6];
                    request[0] = CMD_SLOT_READ;
                    request[1] = kind.protocol_id();
                    request[2..4].copy_from_slice(&(slot as u16).to_le_bytes());
                    request[4..6].copy_from_slice(&(offset as u16).to_le_bytes());
                    let response = checked_response(CMD_SLOT_READ, self.usb_send(&request)?)?;
                    let amount = (record_bytes - offset).min(30);
                    record[offset..offset + amount].copy_from_slice(&response[2..2 + amount]);
                    completed_requests += 1;
                    progress.store(
                        if total_requests == 0 {
                            1000
                        } else {
                            (completed_requests as u32 * 1000 / total_requests as u32).min(1000)
                        },
                        Ordering::Relaxed,
                    );
                }
                if legacy {
                    let start = HEADER_SIZE + kind.payload_slot(slot) * 132;
                    original[start..start + 132].copy_from_slice(&record);
                }
                let bitmap = normalize_pictogram_bitmap(&record[4..])?;
                library.set_colored(kind, slot, &bitmap, record[..3].try_into().unwrap());
            }
            library.refresh_checksums();
            progress.store(1000, Ordering::Relaxed);
            if legacy {
                let crc = crc32(&original[HEADER_SIZE..]);
                original[12..16].copy_from_slice(&crc.to_le_bytes());
                let crc = pictogram_header_crc(&original[..HEADER_SIZE]);
                original[16..20].copy_from_slice(&crc.to_le_bytes());
                self.backup_pictogram_package(&original)?;
            }
            return Ok(library);
        }
        let mut package = Vec::with_capacity(package_bytes);
        for offset in (0..package_bytes).step_by(30) {
            let mut request = [0u8; 5];
            request[0] = CMD_READ;
            request[1..5].copy_from_slice(&(offset as u32).to_le_bytes());
            let response = checked_response(CMD_READ, self.usb_send(&request)?)?;
            let amount = (package_bytes - offset).min(30);
            package.extend_from_slice(&response[2..2 + amount]);
            progress.store(
                ((offset + amount) as u32 * 1000 / package_bytes as u32).min(1000),
                Ordering::Relaxed,
            );
        }
        PictogramLibrary::from_package(package)
    }

    fn backup_pictogram_package(&self, package: &[u8]) -> Result<()> {
        #[cfg(test)]
        if let Some(directory) = self.test_pictogram_backup_directory()? {
            return backup_pictogram_package(&directory, package);
        }
        let directory = super::pictogram_library_path().with_file_name("pictogram-backups");
        backup_pictogram_package(&directory, package)
    }

    pub(crate) fn upload_pictograms(
        &self,
        mut library: PictogramLibrary,
        progress: &AtomicU32,
    ) -> Result<PictogramLibrary> {
        let query = checked_response(CMD_QUERY, self.usb_send(&[CMD_QUERY])?)?;
        if query[17] < FORMAT_VERSION {
            bail!("Update macropad firmware to use 35x35 pictograms");
        }
        library.refresh_checksums();
        self.backup_pictogram_package(&library.package)?;
        let crc = crc32(&library.package);
        let mut begin = [0u8; 9];
        begin[0] = CMD_BEGIN;
        begin[1..5].copy_from_slice(&(PACKAGE_SIZE as u32).to_le_bytes());
        begin[5..9].copy_from_slice(&crc.to_le_bytes());
        checked_response(CMD_BEGIN, self.usb_send(&begin)?)?;

        let packets = library.package.len().div_ceil(29);
        for (sequence, chunk) in library.package.chunks(29).enumerate() {
            let acknowledge = (sequence + 1) % STREAM_ACK_INTERVAL == 0 || sequence + 1 == packets;
            let mut packet = [0u8; HID_PACKET_SIZE];
            packet[0] = if acknowledge {
                CMD_DATA
            } else {
                CMD_DATA_STREAM
            };
            packet[1..3].copy_from_slice(&(sequence as u16).to_le_bytes());
            packet[3..3 + chunk.len()].copy_from_slice(chunk);
            if acknowledge {
                let response = checked_response(CMD_DATA, self.usb_send(&packet)?)?;
                let expected = (sequence as u16).wrapping_add(1);
                let actual = u16::from_le_bytes([response[2], response[3]]);
                if actual != expected {
                    bail!("pictogram packet {sequence} was acknowledged as {actual}");
                }
            } else {
                self.write_output_report(&packet)?;
            }
            progress.store(
                ((sequence + 1) as u32 * 1000 / packets as u32).min(1000),
                Ordering::Relaxed,
            );
        }
        checked_response(CMD_COMMIT, self.usb_send(&[CMD_COMMIT])?)?;
        Ok(library)
    }

    pub(crate) fn upload_pictogram_slot(
        &self,
        mut library: PictogramLibrary,
        kind: PictogramKind,
        slot: usize,
        progress: &AtomicU32,
    ) -> Result<PictogramLibrary> {
        if slot >= SLOTS_PER_KIND {
            bail!("pictogram slot is out of range");
        }
        library.refresh_checksums();
        let query = checked_response(CMD_QUERY, self.usb_send(&[CMD_QUERY])?)?;
        if query[17] < FORMAT_VERSION {
            bail!("Update macropad firmware to use 35x35 pictograms");
        }
        if query[2] == 3 || query[16] < 1 {
            return self.upload_pictograms(library, progress);
        }

        let present = library.has(kind, slot);
        let record_start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES;
        let record = &library.package[record_start..record_start + RECORD_BYTES];
        let mut begin = [0u8; 9];
        begin[0] = CMD_SLOT_BEGIN;
        begin[1] = kind.protocol_id();
        begin[2..4].copy_from_slice(&(slot as u16).to_le_bytes());
        begin[4] = u8::from(present);
        begin[5..9].copy_from_slice(&crc32(record).to_le_bytes());
        checked_response(CMD_SLOT_BEGIN, self.usb_send(&begin)?)?;

        let packets = record.len().div_ceil(29);
        for (sequence, chunk) in record.chunks(29).enumerate() {
            let mut packet = [0u8; HID_PACKET_SIZE];
            packet[0] = CMD_SLOT_DATA;
            packet[1..3].copy_from_slice(&(sequence as u16).to_le_bytes());
            packet[3..3 + chunk.len()].copy_from_slice(chunk);
            checked_response(CMD_SLOT_DATA, self.usb_send(&packet)?)?;
            progress.store(
                ((sequence + 1) as u32 * 900 / packets as u32).min(900),
                Ordering::Relaxed,
            );
        }
        checked_response(CMD_SLOT_COMMIT, self.usb_send(&[CMD_SLOT_COMMIT])?)?;
        progress.store(1000, Ordering::Relaxed);
        Ok(library)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backup_directory() -> tempfile::TempDir {
        tempfile::tempdir_in(
            std::env::var_os("ENTROPY_TEST_ARTIFACT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir),
        )
        .unwrap()
    }

    #[test]
    fn repeat_read_failure_is_terminal_only_for_its_connection_generation() {
        let mut state = PictogramSettingsState::default();
        state.supported = Some(true);
        state.load_failure = Some((7, "firmware read error".into()));
        assert!(!state.needs_automatic_load(7));
        assert!(state.needs_automatic_load(8));
        state.loading = true;
        assert!(!state.needs_automatic_load(8));
        state.loading = false;
        state.loaded = true;
        assert!(!state.needs_automatic_load(8));
        state.loaded = false;
        state.supported = Some(false);
        assert!(!state.needs_automatic_load(8));
    }

    #[test]
    fn repeat_slot_replacement_on_same_connection_preserves_other_slots() {
        let (hid, recorder) = crate::hid::HidDevice::test_device();
        let mut library = PictogramLibrary::blank();
        library.set(PictogramKind::TapDance, 2, &[0xA5; PICTOGRAM_BYTES]);
        for bitmap in [[0xAA; PICTOGRAM_BYTES], [0x55; PICTOGRAM_BYTES]] {
            library.set(PictogramKind::Macro, 0, &bitmap);
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            let progress = AtomicU32::new(0);
            library = hid
                .upload_pictogram_slot(library, PictogramKind::Macro, 0, &progress)
                .unwrap();
            assert_eq!(progress.load(Ordering::Relaxed), 1000);
            assert_eq!(
                library.bitmap(PictogramKind::Macro, 0),
                Some(bitmap.as_slice())
            );
            assert_eq!(
                library.bitmap(PictogramKind::TapDance, 2),
                Some([0xA5; PICTOGRAM_BYTES].as_slice())
            );
            recorder.respond_with(test_pictogram_read_responses(&library));
            assert_eq!(hid.load_pictograms(&progress).unwrap(), library);
        }
        let requests = recorder.requests();
        assert_eq!(requests.iter().filter(|r| r[0] == CMD_SLOT_BEGIN).count(), 2);
        assert_eq!(requests.iter().filter(|r| r[0] == CMD_SLOT_COMMIT).count(), 2);
        // Both replacements start at packet zero without opening another owner.
        assert_eq!(
            requests
                .iter()
                .filter(|r| r[0] == CMD_SLOT_DATA && r[1..3] == [0, 0])
                .count(),
            2
        );
    }

    #[test]
    fn repeat_slot_retry_after_firmware_rejection_uses_same_connection() {
        for rejected_command in [CMD_SLOT_BEGIN, CMD_SLOT_DATA, CMD_SLOT_COMMIT] {
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            let mut confirmed = PictogramLibrary::blank();
            confirmed.set(PictogramKind::Macro, 0, &[0xAA; PICTOGRAM_BYTES]);
            let mut desired = confirmed.clone();
            desired.set(PictogramKind::Macro, 0, &[0x55; PICTOGRAM_BYTES]);
            let mut responses = test_pictogram_upload_responses(true, 0);
            let rejected = responses
                .iter()
                .position(|r| r[0] == rejected_command)
                .unwrap();
            responses.truncate(rejected + 1);
            responses[rejected][1] = 4; // Firmware FLASH_ERROR, not a disconnect.
            recorder.respond_with(responses);
            let progress = AtomicU32::new(0);
            let error = hid
                .upload_pictogram_slot(desired.clone(), PictogramKind::Macro, 0, &progress)
                .unwrap_err();
            assert!(error.to_string().contains("status 4"));
            assert!(!crate::hid::is_disconnect_error(&error));
            // Re-read actual storage before retrying an uncertain operation.
            recorder.respond_with(test_pictogram_read_responses(&confirmed));
            assert_eq!(hid.load_pictograms(&progress).unwrap(), confirmed);
            recorder.respond_with(test_pictogram_upload_responses(true, 0));
            let uploaded = hid
                .upload_pictogram_slot(desired.clone(), PictogramKind::Macro, 0, &progress)
                .unwrap();
            assert_eq!(uploaded, desired);
            assert_eq!(progress.load(Ordering::Relaxed), 1000);
        }
    }

    fn bitmap_pixel(bitmap: &[u8], x: usize, y: usize) -> bool {
        let index = y * PICTOGRAM_WIDTH + x;
        bitmap[index / 8] & (1 << (7 - index % 8)) != 0
    }

    fn mirrored_horizontally(left: &[u8], right: &[u8]) -> bool {
        (0..PICTOGRAM_HEIGHT).all(|y| {
            (0..PICTOGRAM_WIDTH).all(|x| {
                bitmap_pixel(left, x, y) == bitmap_pixel(right, PICTOGRAM_WIDTH - 1 - x, y)
            })
        })
    }

    #[test]
    fn legacy_package_migration_preserves_all_slot_assignments_and_colors() {
        let mut old = vec![0u8; LEGACY_PACKAGE_SIZE];
        old[..4].copy_from_slice(&MAGIC.to_le_bytes());
        old[4..8].copy_from_slice(&[3, 32, 32, 128]);
        old[8..12].copy_from_slice(&(LEGACY_PACKAGE_SIZE as u32).to_le_bytes());
        for kind in [PictogramKind::Macro, PictogramKind::TapDance] {
            for slot in 0..256 {
                old[kind.valid_offset() + slot / 8] |= 1 << (slot % 8);
                let start = HEADER_SIZE + kind.payload_slot(slot) * 132;
                old[start..start + 4].copy_from_slice(&[kind.protocol_id(), slot as u8, 77, 1]);
                for (i, b) in old[start + 4..start + 132].iter_mut().enumerate() {
                    *b = (slot + i) as u8;
                }
            }
        }
        let crc = crc32(&old[HEADER_SIZE..]);
        old[12..16].copy_from_slice(&crc.to_le_bytes());
        let crc = pictogram_header_crc(&old[..HEADER_SIZE]);
        old[16..20].copy_from_slice(&crc.to_le_bytes());
        let converted = PictogramLibrary::from_package(old.clone()).unwrap();
        for kind in [PictogramKind::Macro, PictogramKind::TapDance] {
            for slot in 0..256 {
                let start = HEADER_SIZE + kind.payload_slot(slot) * 132;
                assert_eq!(
                    converted.color(kind, slot),
                    Some([kind.protocol_id(), slot as u8, 77])
                );
                assert_eq!(
                    converted.bitmap(kind, slot).unwrap(),
                    normalize_pictogram_bitmap(&old[start + 4..start + 132]).unwrap()
                );
            }
        }
        assert_eq!(
            PictogramLibrary::from_package(converted.package.clone()).unwrap(),
            converted
        );
        old[300] ^= 1;
        assert!(PictogramLibrary::from_package(old).is_err());
    }

    #[test]
    fn full_upload_writes_real_backup_in_each_test_owner_directory() {
        let mut library = PictogramLibrary::blank();
        library.set(PictogramKind::Macro, 7, &[0xA5; PICTOGRAM_BYTES]);
        library.refresh_checksums();
        let filename = format!("device-{:08x}.ehp", crc32(&library.package));
        // Independent owners can back up identical packages concurrently without
        // sharing HOME or colliding in one global backup namespace.
        let first = backup_directory();
        let second = backup_directory();
        for root in [&first, &second] {
            let directory = root.path().join("backups");
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            recorder.set_pictogram_backup_directory(directory.clone());
            for _ in 0..2 {
                recorder.respond_with(test_pictogram_upload_responses(false, 0));
                let uploaded = hid
                    .upload_pictograms(library.clone(), &AtomicU32::new(0))
                    .unwrap();
                assert_eq!(uploaded, library);
                assert_eq!(
                    std::fs::read(directory.join(&filename)).unwrap(),
                    library.package
                );
                assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
            }
            assert_eq!(
                recorder
                    .requests()
                    .iter()
                    .filter(|r| r[0] == CMD_BEGIN)
                    .count(),
                2
            );
            assert_eq!(
                recorder
                    .requests()
                    .iter()
                    .filter(|r| r[0] == CMD_COMMIT)
                    .count(),
                2
            );
        }
    }

    #[test]
    fn mandatory_backup_failure_prevents_destructive_begin_and_all_upload_packets() {
        let mut library = PictogramLibrary::blank();
        library.refresh_checksums();
        for failure in 0..3 {
            let root = backup_directory();
            let directory = root.path().join("backups");
            let (hid, recorder) = crate::hid::HidDevice::test_device();
            if failure == 0 {
                // A file where a directory is required fails even as root and
                // on Windows; no permission bits or environment mutation needed.
                std::fs::write(&directory, b"not a directory").unwrap();
                recorder.set_pictogram_backup_directory(directory.clone());
            } else if failure == 1 {
                std::fs::create_dir(&directory).unwrap();
                let path = directory.join(format!("device-{:08x}.ehp", crc32(&library.package)));
                std::fs::write(path, b"existing incomplete backup").unwrap();
                recorder.set_pictogram_backup_directory(directory.clone());
            } // failure 2: missing explicit test destination must not reach HOME.
            recorder.respond_with(test_pictogram_upload_responses(false, 0));
            let progress = AtomicU32::new(0);
            assert!(hid.upload_pictograms(library.clone(), &progress).is_err());
            let requests = recorder.requests();
            assert_eq!(
                requests.len(),
                1,
                "backup failure sent destructive upload traffic"
            );
            assert_eq!(requests[0][0], CMD_QUERY);
            assert_eq!(progress.load(Ordering::Relaxed), 0);
            if failure == 0 {
                assert_eq!(std::fs::read(directory).unwrap(), b"not a directory");
            } else if failure == 1 {
                let path = directory.join(format!("device-{:08x}.ehp", crc32(&library.package)));
                assert_eq!(std::fs::read(path).unwrap(), b"existing incomplete backup");
            }
        }
    }

    #[test]
    fn blank_library_round_trips_and_updates_slots() {
        let mut library = PictogramLibrary::blank();
        assert!(!library.has(PictogramKind::Macro, 7));
        let bitmap = [0xA5; PICTOGRAM_BYTES];
        library.set_colored(PictogramKind::Macro, 7, &bitmap, [12, 34, 56]);
        assert_eq!(
            library.bitmap(PictogramKind::Macro, 7),
            Some(bitmap.as_slice())
        );
        assert_eq!(library.color(PictogramKind::Macro, 7), Some([12, 34, 56]));
        let parsed = PictogramLibrary::from_package(library.package.clone()).unwrap();
        assert_eq!(
            parsed.bitmap(PictogramKind::Macro, 7),
            Some(bitmap.as_slice())
        );
        assert_eq!(parsed.color(PictogramKind::Macro, 7), Some([12, 34, 56]));
        library.clear(PictogramKind::Macro, 7);
        assert!(!library.has(PictogramKind::Macro, 7));
    }

    #[test]
    fn every_validity_map_boundary_round_trips_for_macros_and_tap_dance() {
        let slots = [0, 1, 7, 8, 31, 32, 127, 128, 254, 255];
        let mut library = PictogramLibrary::blank();
        for (kind_index, kind) in [PictogramKind::Macro, PictogramKind::TapDance]
            .into_iter()
            .enumerate()
        {
            for slot in slots {
                let mut bitmap = [0u8; PICTOGRAM_BYTES];
                bitmap[slot % PICTOGRAM_BYTES] = 0x80 >> (slot % 8);
                library.set_colored(kind, slot, &bitmap, [kind_index as u8, slot as u8, 0xA5]);
            }
        }

        let parsed = PictogramLibrary::from_package(library.package.clone()).unwrap();
        for (kind_index, kind) in [PictogramKind::Macro, PictogramKind::TapDance]
            .into_iter()
            .enumerate()
        {
            for slot in slots {
                let bitmap = parsed.bitmap(kind, slot).expect("occupied slot");
                assert_eq!(bitmap[slot % PICTOGRAM_BYTES], 0x80 >> (slot % 8));
                assert_eq!(
                    parsed.color(kind, slot),
                    Some([kind_index as u8, slot as u8, 0xA5])
                );
            }
        }
    }

    #[test]
    fn quantization_outputs_exactly_one_bit_per_pixel() {
        let mut levels = vec![0; PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT];
        levels[0] = 255;
        levels[PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT - 1] = 255;
        let bitmap = quantize_pictogram(&levels, 128, false);
        assert_eq!(bitmap.len(), 154);
        assert_eq!(bitmap[0] & 0x80, 0x80);
        assert_eq!(bitmap[153] & 0x80, 0x80);
        assert_eq!(bitmap[153] & 0x7f, 0);
    }

    #[test]
    fn transparent_bounds_are_trimmed_before_fitting() {
        let mut image = RgbaImage::new(20, 20);
        image.put_pixel(8, 9, image::Rgba([255, 255, 255, 255]));
        image.put_pixel(9, 9, image::Rgba([255, 255, 255, 255]));
        let (cropped, transparent) = crop_transparent_bounds(&image);
        assert!(transparent);
        assert_eq!(cropped.dimensions(), (2, 1));
    }

    #[test]
    fn builtin_pictograms_are_nonempty_and_distinct() {
        let mut bitmaps = std::collections::BTreeSet::new();
        for index in 0..BUILTIN_PICTOGRAM_KEYS.len() {
            let bitmap = builtin_pictogram_bitmap(index);
            assert!(bitmap.iter().any(|byte| *byte != 0x00));
            assert!(bitmap.iter().any(|byte| *byte != 0xFF));
            assert!(
                bitmaps.insert(bitmap),
                "duplicate built-in pictogram {index}"
            );
        }
    }

    #[test]
    fn builtin_pictograms_round_trip_exactly_through_the_35_by_35_editor_grid() {
        for index in 0..BUILTIN_PICTOGRAM_KEYS.len() {
            let bitmap = builtin_pictogram_bitmap(index);
            let levels = pictogram_bitmap_levels(&bitmap);
            assert_eq!(levels.len(), PICTOGRAM_WIDTH * PICTOGRAM_HEIGHT);
            assert_eq!(quantize_pictogram(&levels, 128, false), bitmap);
        }
    }

    #[test]
    fn mouse_and_media_layer_pictograms_are_available_as_builtins() {
        let expected = [
            "display_settings.pictogram_builtin_cut",
            "display_settings.pictogram_builtin_screenshot",
            "display_settings.pictogram_builtin_mouse_button_left",
            "display_settings.pictogram_builtin_mouse_button_right",
            "display_settings.pictogram_builtin_mouse_button_middle",
            "display_settings.pictogram_builtin_mouse_up",
            "display_settings.pictogram_builtin_mouse_down",
            "display_settings.pictogram_builtin_mouse_left",
            "display_settings.pictogram_builtin_mouse_right",
            "display_settings.pictogram_builtin_brightness_down",
            "display_settings.pictogram_builtin_brightness_up",
            "display_settings.pictogram_builtin_computer",
            "display_settings.pictogram_builtin_web_search",
            "display_settings.pictogram_builtin_previous_track",
            "display_settings.pictogram_builtin_next_track",
            "display_settings.pictogram_builtin_calculator",
        ];
        assert!(expected
            .iter()
            .all(|key| BUILTIN_PICTOGRAM_KEYS.contains(key)));
    }

    #[test]
    fn lock_and_scissors_are_symmetric_and_volume_has_open_sound_arcs() {
        for index in [15, 22] {
            let bitmap = builtin_pictogram_bitmap(index);
            assert!(mirrored_horizontally(&bitmap, &bitmap));
        }
        let volume = builtin_pictogram_bitmap(10);
        assert!(bitmap_pixel(&volume, 23, 17));
        assert!(bitmap_pixel(&volume, 29, 17));
        assert!(!bitmap_pixel(&volume, 25, 17));
        assert!(!bitmap_pixel(&volume, 15, 3));
        assert!(!bitmap_pixel(&volume, 15, 31));
    }

    #[test]
    fn previous_stock_library_is_recognized_without_becoming_user_copies() {
        for index in 0..BUILTIN_PICTOGRAM_KEYS.len() {
            assert!(
                legacy_builtin_pictogram_index(&legacy_builtin_pictogram_bitmap(index)).is_some()
            );
        }
    }

    #[test]
    fn mouse_pictograms_are_pixel_aligned_and_symmetric() {
        let left_button = builtin_pictogram_bitmap(24);
        let right_button = builtin_pictogram_bitmap(25);
        let middle_button = builtin_pictogram_bitmap(26);
        let move_left = builtin_pictogram_bitmap(29);
        let move_right = builtin_pictogram_bitmap(30);

        assert!(mirrored_horizontally(&left_button, &right_button));
        assert!(mirrored_horizontally(&middle_button, &middle_button));
        assert!(mirrored_horizontally(&move_left, &move_right));

        // Direction arrows reach the corresponding edge and stay distinct
        // from the centered mouse body.
        assert!(bitmap_pixel(&builtin_pictogram_bitmap(27), 17, 0));
        assert!(bitmap_pixel(&builtin_pictogram_bitmap(28), 17, 34));
        assert!(bitmap_pixel(&move_left, 0, 17));
        assert!(bitmap_pixel(&move_right, 34, 17));
    }
}

#[cfg(test)]
pub(crate) fn test_pictogram_upload_responses(
    slot_upload: bool,
    commit_status: u8,
) -> Vec<[u8; 32]> {
    let mut query = [0u8; 32];
    query[0] = CMD_QUERY;
    query[2] = FORMAT_VERSION;
    query[16] = 2;
    query[17] = FORMAT_VERSION;
    let mut responses = vec![query];
    let mut begin = [0; 32];
    begin[0] = if slot_upload {
        CMD_SLOT_BEGIN
    } else {
        CMD_BEGIN
    };
    responses.push(begin);
    let bytes = if slot_upload {
        RECORD_BYTES
    } else {
        PACKAGE_SIZE
    };
    let packets = bytes.div_ceil(29);
    for sequence in 0..packets {
        if slot_upload || (sequence + 1) % STREAM_ACK_INTERVAL == 0 || sequence + 1 == packets {
            let mut reply = [0; 32];
            reply[0] = if slot_upload { CMD_SLOT_DATA } else { CMD_DATA };
            reply[2..4].copy_from_slice(&((sequence + 1) as u16).to_le_bytes());
            responses.push(reply);
        }
    }
    let mut commit = [0; 32];
    commit[0] = if slot_upload {
        CMD_SLOT_COMMIT
    } else {
        CMD_COMMIT
    };
    commit[1] = commit_status;
    responses.push(commit);
    responses
}

#[cfg(test)]
pub(crate) fn test_pictogram_read_responses(library: &PictogramLibrary) -> Vec<[u8; 32]> {
    let mut query = [0; 32];
    query[0] = CMD_QUERY;
    query[2] = FORMAT_VERSION;
    query[3] = 1;
    query[4] = PICTOGRAM_WIDTH as u8;
    query[5] = PICTOGRAM_HEIGHT as u8;
    query[6..8].copy_from_slice(&(PICTOGRAM_BYTES as u16).to_le_bytes());
    query[8..12].copy_from_slice(&(PACKAGE_SIZE as u32).to_le_bytes());
    query[16] = 2;
    let mut responses = vec![query];
    for kind in [PictogramKind::Macro, PictogramKind::TapDance] {
        for offset in (0..VALID_BYTES).step_by(30) {
            let mut response = [0; 32];
            response[0] = CMD_VALID_READ;
            let amount = (VALID_BYTES - offset).min(30);
            let start = kind.valid_offset() + offset;
            response[2..2 + amount].copy_from_slice(&library.package[start..start + amount]);
            responses.push(response);
        }
    }
    for kind in [PictogramKind::Macro, PictogramKind::TapDance] {
        for slot in 0..SLOTS_PER_KIND {
            if !library.has(kind, slot) {
                continue;
            }
            let start = HEADER_SIZE + kind.payload_slot(slot) * RECORD_BYTES;
            for offset in (0..RECORD_BYTES).step_by(30) {
                let mut response = [0; 32];
                response[0] = CMD_SLOT_READ;
                let amount = (RECORD_BYTES - offset).min(30);
                response[2..2 + amount]
                    .copy_from_slice(&library.package[start + offset..start + offset + amount]);
                responses.push(response);
            }
        }
    }
    responses
}
