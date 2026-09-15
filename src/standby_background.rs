use anyhow::{bail, Context, Result};
use image::{AnimationDecoder, DynamicImage, GenericImageView, RgbaImage};
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const CMD_BIND: u8 = 0xB8;
const FAST_PACKET_SIZE: usize = 64;

const CMD_QUERY: u8 = 0xB0;
const CMD_BEGIN: u8 = 0xB1;
const CMD_DATA: u8 = 0xB2;
const CMD_COMMIT: u8 = 0xB3;
const CMD_CLEAR: u8 = 0xB4;
const CMD_DATA_STREAM: u8 = 0xB5;
const CMD_SESSION: u8 = 0xB6;
const CMD_SPEED: u8 = 0xB7;
const STARTUP_CMD_QUERY: u8 = 0xD0;
const STARTUP_CMD_BEGIN: u8 = 0xD1;
const STARTUP_CMD_DATA: u8 = 0xD2;
const STARTUP_CMD_COMMIT: u8 = 0xD3;
const STARTUP_CMD_CLEAR: u8 = 0xD4;
const STARTUP_CMD_DATA_STREAM: u8 = 0xD5;
const STARTUP_CMD_PREVIEW: u8 = 0xD6;
const STARTUP_PREVIEW_WIDTH: usize = 60;
const STARTUP_PREVIEW_HEIGHT: usize = 70;
const STARTUP_PREVIEW_SIZE: usize = STARTUP_PREVIEW_WIDTH * STARTUP_PREVIEW_HEIGHT;
const LEGACY_FORMAT_VERSION: u8 = 3;
const FORMAT_VERSION: u8 = 4;
const WIDTH: u32 = 240;
const HEIGHT: u32 = 280;
const HEADER_SIZE: usize = 256;
const IMAGE_FRAME_SIZE: usize = 1024 + WIDTH as usize * HEIGHT as usize;
const LEGACY_ANIMATION_PALETTE_SIZE: usize = 16 * 4;
const LEGACY_ANIMATION_FRAME_SIZE: usize =
    LEGACY_ANIMATION_PALETTE_SIZE + WIDTH as usize * HEIGHT as usize / 2;
const ANIMATION_PALETTE_SIZE: usize = 64 * 4;
const ANIMATION_PIXEL_SIZE: usize = WIDTH as usize * HEIGHT as usize * 3 / 4;
const ANIMATION_FRAME_PADDING: usize = 32;
const ANIMATION_FRAME_SIZE: usize =
    ANIMATION_PALETTE_SIZE + ANIMATION_PIXEL_SIZE + ANIMATION_FRAME_PADDING;
const MAX_FRAMES: usize = 56;
const MIN_SPEED_PERCENT: u16 = 25;
const MAX_SPEED_PERCENT: u16 = 400;
const DEFAULT_SPEED_PERCENT: u16 = 100;
const MAX_SOURCE_FRAMES: usize = 300;
const MAGIC: u32 = 0x4742_4845;
const STARTUP_MAGIC: u32 = 0x4953_4845;
const HID_PACKET_SIZE: usize = 32;
const STREAM_ACK_INTERVAL: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BackgroundInfo {
    pub(crate) format_version: u8,
    pub(crate) kind: u8,
    pub(crate) frame_count: u8,
    pub(crate) total_size: u32,
    pub(crate) max_size: u32,
    pub(crate) max_frames: u8,
    pub(crate) speed_percent: u16,
    pub(crate) header_crc: u32,
    pub(crate) fast_packet_size: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct BackgroundUploadResult {
    pub(crate) source_path: String,
    pub(crate) kind: u8,
    pub(crate) frame_count: u8,
    pub(crate) total_size: u32,
    pub(crate) file_name: String,
    pub(crate) preview_rgba: Vec<u8>,
    pub(crate) preview_frames_rgba: Vec<Vec<u8>>,
    pub(crate) preview_delays_ms: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupImageInfo {
    pub(crate) format_version: u8,
    pub(crate) present: bool,
    pub(crate) total_size: u32,
    pub(crate) max_size: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct StartupImageUploadResult {
    pub(crate) total_size: u32,
    pub(crate) file_name: String,
    pub(crate) preview_rgba: Vec<u8>,
}

fn response(command: u8, data: [u8; HID_PACKET_SIZE]) -> Result<[u8; HID_PACKET_SIZE]> {
    if data[0] != command {
        bail!("background command 0x{command:02X} was not recognized by the firmware");
    }
    if data[1] != 0 {
        bail!(
            "background command 0x{command:02X} failed with status {}",
            data[1]
        );
    }
    Ok(data)
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().expect("four-byte slice"))
}

fn next_sequence(sequence: usize) -> u16 {
    (sequence as u16).wrapping_add(1)
}

pub(crate) struct StandbyAnimationLoadGuard<'a> {
    hid: &'a crate::hid::HidDevice,
    active: bool,
}

impl Drop for StandbyAnimationLoadGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = self.hid.set_standby_animation_session(false) {
                log::debug!("standby animation resume command failed: {error:#}");
            }
        }
    }
}

impl crate::hid::HidDevice {
    fn set_standby_animation_session(&self, paused: bool) -> Result<()> {
        response(
            CMD_SESSION,
            self.usb_send(&[CMD_SESSION, u8::from(paused)])?,
        )?;
        Ok(())
    }

    pub(crate) fn pause_standby_animation_for_load(&self) -> StandbyAnimationLoadGuard<'_> {
        let active = match self.set_standby_animation_session(true) {
            Ok(()) => true,
            Err(error) => {
                // v031 and older do not implement the optional session command.
                log::debug!("standby animation pause is unavailable: {error:#}");
                false
            }
        };
        StandbyAnimationLoadGuard { hid: self, active }
    }

    pub(crate) fn get_standby_background_info(&self) -> Result<BackgroundInfo> {
        let data = response(CMD_QUERY, self.usb_send(&[CMD_QUERY])?)?;
        if !matches!(data[2], LEGACY_FORMAT_VERSION | FORMAT_VERSION) {
            bail!("unsupported background format version {}", data[2]);
        }
        Ok(BackgroundInfo {
            format_version: data[2],
            kind: data[3],
            frame_count: data[4],
            max_size: read_u32(&data[5..9]),
            total_size: read_u32(&data[9..13]),
            max_frames: data[13],
            header_crc: read_u32(&data[16..20]),
            fast_packet_size: data[20],
            speed_percent: if data[2] == FORMAT_VERSION {
                u16::from_le_bytes([data[14], data[15]]).clamp(MIN_SPEED_PERCENT, MAX_SPEED_PERCENT)
            } else {
                DEFAULT_SPEED_PERCENT
            },
        })
    }

    pub(crate) fn set_standby_background_speed(&self, speed_percent: u16) -> Result<()> {
        let speed_percent = speed_percent.clamp(MIN_SPEED_PERCENT, MAX_SPEED_PERCENT);
        let mut request = [0u8; 3];
        request[0] = CMD_SPEED;
        request[1..3].copy_from_slice(&speed_percent.to_le_bytes());
        response(CMD_SPEED, self.usb_send(&request)?)?;
        let confirmed = self.get_standby_background_info()?.speed_percent;
        if confirmed != speed_percent {
            bail!("firmware kept animation speed at {confirmed}% instead of {speed_percent}%");
        }
        Ok(())
    }

    pub(crate) fn clear_standby_background(&self) -> Result<()> {
        response(CMD_CLEAR, self.usb_send(&[CMD_CLEAR])?)?;
        Ok(())
    }

    pub(crate) fn upload_standby_background(
        &self,
        path: &Path,
        fallback: [u8; 3],
        scale_mode: super::StandbyBackgroundScale,
        progress: &AtomicU32,
        cancel: &AtomicBool,
    ) -> Result<Option<BackgroundUploadResult>> {
        self.upload_standby_background_with_cache(
            path,
            fallback,
            scale_mode,
            progress,
            cancel,
            cache_background_package,
        )
    }

    // Keep preparation, capability selection and transfer identical when a
    // constructor-free native test injects its own cache destination.
    pub(crate) fn upload_standby_background_with_cache(
        &self,
        path: &Path,
        fallback: [u8; 3],
        scale_mode: super::StandbyBackgroundScale,
        progress: &AtomicU32,
        cancel: &AtomicBool,
        cache: impl FnOnce(&[u8]) -> Result<()>,
    ) -> Result<Option<BackgroundUploadResult>> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        progress.store(5, Ordering::Relaxed);
        let info = self.get_standby_background_info()?;
        let max_frames = usize::from(info.max_frames).min(MAX_FRAMES);
        if max_frames == 0 {
            bail!("firmware reported a zero background frame limit");
        }
        let prepared = prepare_background(
            path,
            fallback,
            scale_mode,
            max_frames,
            info.format_version,
            progress,
        )?;
        if prepared.package.len() > info.max_size as usize {
            bail!(
                "prepared background is {} bytes, firmware limit is {} bytes",
                prepared.package.len(),
                info.max_size
            );
        }

        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        // Select the fast interface before BEGIN. Never switch transports
        // mid-upload: queued writes and sequence numbers must stay ordered.
        let fast = if info.fast_packet_size == FAST_PACKET_SIZE as u8 {
            FastBackgroundChannel::open(self).unwrap_or_else(|error| {
                log::warn!(
                    "64-byte background channel unavailable; using legacy transport: {error:#}"
                );
                None
            })
        } else {
            None
        };
        log::info!(
            "background upload: {} bytes using {}-byte reports",
            prepared.package.len(),
            if fast.is_some() { 64 } else { 32 }
        );
        if !transmit_background_sized(
            &prepared.package,
            progress,
            cancel,
            if fast.is_some() {
                FAST_PACKET_SIZE
            } else {
                HID_PACKET_SIZE
            },
            |packet| {
                if let Some(channel) = &fast {
                    channel.send(packet)
                } else {
                    self.usb_send(packet)
                }
            },
            |packet| {
                if let Some(channel) = &fast {
                    channel.write(packet)
                } else {
                    self.write_output_report(packet)
                }
            },
        )? {
            return Ok(None);
        }
        if let Err(error) = cache(&prepared.package) {
            log::warn!("background preview cache: {error:#}");
        }
        progress.store(1000, Ordering::Relaxed);
        Ok(Some(BackgroundUploadResult {
            source_path: path.to_string_lossy().into_owned(),
            kind: prepared.kind,
            frame_count: prepared.frame_count,
            total_size: prepared.package.len() as u32,
            file_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("background")
                .to_owned(),
            preview_rgba: prepared.preview_rgba,
            preview_frames_rgba: prepared.preview_frames_rgba,
            preview_delays_ms: prepared.preview_delays_ms,
        }))
    }

    pub(crate) fn get_startup_image_info(&self) -> Result<StartupImageInfo> {
        let data = response(STARTUP_CMD_QUERY, self.usb_send(&[STARTUP_CMD_QUERY])?)?;
        if data[2] != 1 {
            bail!("unsupported startup image format version {}", data[2]);
        }
        Ok(StartupImageInfo {
            format_version: data[2],
            present: data[3] != 0,
            max_size: read_u32(&data[4..8]),
            total_size: read_u32(&data[8..12]),
        })
    }

    pub(crate) fn get_startup_image_preview(&self, fallback: [u8; 3]) -> Result<Vec<u8>> {
        let mut indices = Vec::with_capacity(STARTUP_PREVIEW_SIZE);
        while indices.len() < STARTUP_PREVIEW_SIZE {
            let mut request = [0u8; HID_PACKET_SIZE];
            request[0] = STARTUP_CMD_PREVIEW;
            request[1..5].copy_from_slice(&(indices.len() as u32).to_le_bytes());
            let data = response(STARTUP_CMD_PREVIEW, self.usb_send(&request)?)?;
            let count = (STARTUP_PREVIEW_SIZE - indices.len()).min(HID_PACKET_SIZE - 2);
            indices.extend_from_slice(&data[2..2 + count]);
        }
        Ok(expand_startup_device_preview(&indices, fallback))
    }

    pub(crate) fn clear_startup_image(&self) -> Result<()> {
        response(STARTUP_CMD_CLEAR, self.usb_send(&[STARTUP_CMD_CLEAR])?)?;
        Ok(())
    }

    pub(crate) fn upload_startup_image(
        &self,
        path: &Path,
        fallback: [u8; 3],
        progress: &AtomicU32,
    ) -> Result<StartupImageUploadResult> {
        progress.store(5, Ordering::Relaxed);
        let info = self.get_startup_image_info()?;
        let prepared = prepare_startup_image(path, fallback)?;
        if prepared.package.len() > info.max_size as usize {
            bail!(
                "prepared startup image is {} bytes, firmware limit is {} bytes",
                prepared.package.len(),
                info.max_size
            );
        }

        let package_crc = crc32(&prepared.package);
        let mut begin = [0u8; HID_PACKET_SIZE];
        begin[0] = STARTUP_CMD_BEGIN;
        begin[1..5].copy_from_slice(&(prepared.package.len() as u32).to_le_bytes());
        begin[5..9].copy_from_slice(&package_crc.to_le_bytes());
        response(STARTUP_CMD_BEGIN, self.usb_send(&begin)?)?;

        let packets = prepared.package.len().div_ceil(29);
        for (sequence, chunk) in prepared.package.chunks(29).enumerate() {
            let acknowledge = (sequence + 1) % STREAM_ACK_INTERVAL == 0 || sequence + 1 == packets;
            let mut packet = [0u8; HID_PACKET_SIZE];
            packet[0] = if acknowledge {
                STARTUP_CMD_DATA
            } else {
                STARTUP_CMD_DATA_STREAM
            };
            packet[1..3].copy_from_slice(&(sequence as u16).to_le_bytes());
            packet[3..3 + chunk.len()].copy_from_slice(chunk);
            if acknowledge {
                let reply = response(STARTUP_CMD_DATA, self.usb_send(&packet)?)?;
                let expected = next_sequence(sequence);
                let acknowledged = u16::from_le_bytes([reply[2], reply[3]]);
                if acknowledged != expected {
                    bail!("startup image packet {sequence} was acknowledged as {acknowledged}");
                }
            } else {
                self.write_output_report(&packet)?;
            }
            progress.store(
                100 + ((sequence + 1) as u32 * 890 / packets as u32),
                Ordering::Relaxed,
            );
        }
        response(STARTUP_CMD_COMMIT, self.usb_send(&[STARTUP_CMD_COMMIT])?)?;
        progress.store(1000, Ordering::Relaxed);
        Ok(StartupImageUploadResult {
            total_size: prepared.package.len() as u32,
            file_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("startup-image")
                .to_owned(),
            preview_rgba: prepared.preview_rgba,
        })
    }
}

struct PreparedStartupImage {
    package: Vec<u8>,
    preview_rgba: Vec<u8>,
}

fn prepare_startup_image(path: &Path, fallback: [u8; 3]) -> Result<PreparedStartupImage> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    if metadata.len() > 32 * 1024 * 1024 {
        bail!("source file is larger than 32 MB");
    }
    let image = image::open(path).with_context(|| format!("cannot decode {}", path.display()))?;
    let image = prepare_frame(image, fallback, super::StandbyBackgroundScale::Fill)?;
    let mut payload = Vec::with_capacity(IMAGE_FRAME_SIZE);
    encode_rgb332_transparent(&image, &mut payload);
    // Build the preview from the exact LVGL palette and indices sent to flash,
    // not from a parallel approximation of the source image.
    let preview_rgba = decode_indexed_8bit_frame(&payload, fallback)?.into_raw();
    let data_crc = crc32(&payload);
    let total_size = HEADER_SIZE + payload.len();
    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&STARTUP_MAGIC.to_le_bytes());
    header[4] = 1;
    header[8..10].copy_from_slice(&(WIDTH as u16).to_le_bytes());
    header[10..12].copy_from_slice(&(HEIGHT as u16).to_le_bytes());
    header[12..16].copy_from_slice(&(IMAGE_FRAME_SIZE as u32).to_le_bytes());
    header[16..20].copy_from_slice(&(total_size as u32).to_le_bytes());
    header[20..24].copy_from_slice(&data_crc.to_le_bytes());
    let checksum = header_crc(&header);
    header[24..28].copy_from_slice(&checksum.to_le_bytes());
    let mut package = Vec::with_capacity(total_size);
    package.extend_from_slice(&header);
    package.extend_from_slice(&payload);
    Ok(PreparedStartupImage {
        package,
        preview_rgba,
    })
}

struct PreparedBackground {
    kind: u8,
    frame_count: u8,
    package: Vec<u8>,
    preview_rgba: Vec<u8>,
    preview_frames_rgba: Vec<Vec<u8>>,
    preview_delays_ms: Vec<u16>,
}

fn background_cache_path(checksum: u32) -> std::path::PathBuf {
    super::app_settings_path()
        .parent()
        .unwrap()
        .join("background-cache")
        .join(format!("{checksum:08x}.ehbg"))
}

fn cache_background_package(package: &[u8]) -> Result<()> {
    let checksum = read_u32(&package[24..28]);
    let path = background_cache_path(checksum);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, package)?;
    Ok(())
}

fn decode_cached_background(
    package: &[u8],
    info: BackgroundInfo,
) -> Option<BackgroundUploadResult> {
    if info.kind == 0
        || info.header_crc == 0
        || package.len() != info.total_size as usize
        || package.len() < HEADER_SIZE
    {
        return None;
    }
    let header: &[u8; HEADER_SIZE] = package[..HEADER_SIZE].try_into().ok()?;
    if read_u32(&header[0..4]) != MAGIC
        || !matches!(header[4], LEGACY_FORMAT_VERSION | FORMAT_VERSION)
        || header[4] != info.format_version
        || header[5] != info.kind
        || header[6] != info.frame_count
        || header_crc(header) != info.header_crc
        || read_u32(&header[24..28]) != info.header_crc
        || crc32(&package[HEADER_SIZE..]) != read_u32(&header[20..24])
    {
        return None;
    }
    let frame_size = if info.kind == 1 {
        IMAGE_FRAME_SIZE
    } else if info.kind == 2 && header[4] == LEGACY_FORMAT_VERSION {
        LEGACY_ANIMATION_FRAME_SIZE
    } else if info.kind == 2 {
        ANIMATION_FRAME_SIZE
    } else {
        return None;
    };
    if info.frame_count == 0
        || info.frame_count as usize > MAX_FRAMES
        || package.len() != HEADER_SIZE + frame_size * info.frame_count as usize
    {
        return None;
    }
    let mut frames = Vec::new();
    let mut delays = Vec::new();
    for (number, frame) in package[HEADER_SIZE..].chunks_exact(frame_size).enumerate() {
        let mut rgba = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
        for pixel in 0..WIDTH as usize * HEIGHT as usize {
            let index = if info.kind == 1 {
                frame[1024 + pixel] as usize
            } else if header[4] == LEGACY_FORMAT_VERSION {
                let packed = frame[LEGACY_ANIMATION_PALETTE_SIZE + pixel / 2];
                (if pixel % 2 == 0 {
                    packed >> 4
                } else {
                    packed & 15
                }) as usize
            } else {
                let packed = &frame[256 + (pixel / 4) * 3..];
                (match pixel % 4 {
                    0 => packed[0] >> 2,
                    1 => ((packed[0] & 3) << 4) | (packed[1] >> 4),
                    2 => ((packed[1] & 15) << 2) | (packed[2] >> 6),
                    _ => packed[2] & 63,
                }) as usize
            };
            let color = &frame[index * 4..index * 4 + 4];
            rgba.extend_from_slice(&[color[2], color[1], color[0], color[3]]);
        }
        frames.push(rgba);
        delays.push(u16::from_le_bytes([
            header[28 + number * 2],
            header[29 + number * 2],
        ]));
    }
    Some(BackgroundUploadResult {
        source_path: String::new(),
        kind: info.kind,
        frame_count: info.frame_count,
        total_size: info.total_size,
        file_name: String::new(),
        preview_rgba: frames[0].clone(),
        preview_frames_rgba: frames,
        preview_delays_ms: delays,
    })
}

pub(crate) fn restore_background_preview(info: BackgroundInfo) -> Option<BackgroundUploadResult> {
    if info.kind == 0 || info.header_crc == 0 {
        return None;
    }
    if let Ok(package) = std::fs::read(background_cache_path(info.header_crc)) {
        if let Some(preview) = decode_cached_background(&package, info) {
            return Some(preview);
        }
    }
    // Recover a pre-cache upload only if its original source still produces
    // exactly the package stored on this device. Never upload or modify it.
    let settings = super::load_app_settings();
    let path = settings.standby_background_source_path?;
    let prepared = prepare_background(
        Path::new(&path),
        [0, 0, 0],
        settings.standby_background_scale,
        info.max_frames as usize,
        info.format_version,
        &AtomicU32::new(0),
    )
    .ok()?;
    let mut preview = decode_cached_background(&prepared.package, info)?;
    let _ = cache_background_package(&prepared.package);
    preview.file_name = Path::new(&path).file_name()?.to_string_lossy().into_owned();
    Some(preview)
}

// Separate usage/interface: ordinary Vial remains a 32-byte report device.
// Bind with a nonce through the already selected Vial device, then probe
// candidate fast interfaces. VID/PID/serial alone are not unique on QMK boards.
struct FastBackgroundChannel {
    device: hidapi::HidDevice,
}
impl FastBackgroundChannel {
    fn open(legacy: &crate::hid::HidDevice) -> Result<Option<Self>> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nonce = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos() as u64)
            ^ NEXT.fetch_add(1, Ordering::Relaxed)
            ^ ((std::process::id() as u64) << 32);
        let nonce = nonce.to_le_bytes();
        let mut bind = [0u8; HID_PACKET_SIZE];
        bind[0] = CMD_BIND;
        bind[2..10].copy_from_slice(&nonce);
        let reply = response(CMD_BIND, legacy.usb_send(&bind)?)?;
        if reply[2..10] != nonce {
            return Ok(None);
        }
        let api = hidapi::HidApi::new()?;
        for info in api
            .device_list()
            .filter(|info| info.usage_page() == 0xFF60 && info.usage() == 0x62)
        {
            let Ok(device) = info.open_device(&api) else {
                continue;
            };
            let channel = Self { device };
            let Ok(reply) = channel.send(&[CMD_BIND]) else {
                continue;
            };
            if reply[0] == CMD_BIND && reply[1] == 0 && reply[2..10] == nonce {
                return Ok(Some(channel));
            }
        }
        Ok(None)
    }
    fn write(&self, packet: &[u8]) -> Result<()> {
        if packet.len() > FAST_PACKET_SIZE {
            bail!("fast report exceeds64 bytes");
        }
        // hidapi includes the zero report-ID prefix on writes, but not reads.
        let mut report = [0u8; FAST_PACKET_SIZE + 1];
        report[1..1 + packet.len()].copy_from_slice(packet);
        let written = self.device.write(&report)?;
        if written != report.len() {
            bail!("short fast HID write: {written}");
        }
        Ok(())
    }
    fn send(&self, packet: &[u8]) -> Result<[u8; HID_PACKET_SIZE]> {
        self.write(packet)?;
        let mut reply = [0u8; FAST_PACKET_SIZE];
        let read = self.device.read_timeout(&mut reply, 1500)?;
        if read != FAST_PACKET_SIZE {
            bail!("incomplete fast HID acknowledgement: {read}");
        }
        if reply[0] != packet[0] {
            bail!("unexpected fast HID acknowledgement");
        }
        Ok(reply[..HID_PACKET_SIZE].try_into().unwrap())
    }
}

#[cfg(test)]
fn transmit_background(
    package: &[u8],
    progress: &AtomicU32,
    cancel: &AtomicBool,
    send: impl FnMut(&[u8]) -> Result<[u8; HID_PACKET_SIZE]>,
    write: impl FnMut(&[u8]) -> Result<()>,
) -> Result<bool> {
    transmit_background_sized(package, progress, cancel, HID_PACKET_SIZE, send, write)
}

fn transmit_background_sized(
    package: &[u8],
    progress: &AtomicU32,
    cancel: &AtomicBool,
    packet_size: usize,
    mut send: impl FnMut(&[u8]) -> Result<[u8; HID_PACKET_SIZE]>,
    mut write: impl FnMut(&[u8]) -> Result<()>,
) -> Result<bool> {
    assert!(matches!(packet_size, 32 | 64));
    if cancel.load(Ordering::Relaxed) {
        return Ok(false);
    }
    let crc = crc32(&package);
    let mut begin = vec![0u8; packet_size];
    begin[0] = CMD_BEGIN;
    begin[1..5].copy_from_slice(&(package.len() as u32).to_le_bytes());
    begin[5..9].copy_from_slice(&crc.to_le_bytes());
    response(CMD_BEGIN, send(&begin)?)?;

    progress.store(150, Ordering::Relaxed);
    let packets = package.len().div_ceil(packet_size - 3);
    for (sequence, chunk) in package.chunks(packet_size - 3).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            response(CMD_CLEAR, send(&[CMD_CLEAR])?)?;
            return Ok(false);
        }
        let acknowledge = (sequence + 1) % STREAM_ACK_INTERVAL == 0 || sequence + 1 == packets;
        let mut packet = vec![0u8; packet_size];
        packet[0] = if acknowledge {
            CMD_DATA
        } else {
            CMD_DATA_STREAM
        };
        packet[1..3].copy_from_slice(&(sequence as u16).to_le_bytes());
        packet[3..3 + chunk.len()].copy_from_slice(chunk);
        if acknowledge {
            let reply = response(CMD_DATA, send(&packet)?)?;
            let expected = next_sequence(sequence);
            let acknowledged = u16::from_le_bytes([reply[2], reply[3]]);
            if acknowledged != expected {
                bail!("background packet {sequence} was acknowledged as {acknowledged}");
            }
        } else {
            write(&packet)?;
        }
        progress.store(
            150 + ((sequence + 1) as u32 * 840 / packets as u32),
            Ordering::Relaxed,
        );
    }
    if cancel.load(Ordering::Relaxed) {
        response(CMD_CLEAR, send(&[CMD_CLEAR])?)?;
        return Ok(false);
    }
    response(CMD_COMMIT, send(&[CMD_COMMIT])?)?;
    Ok(true)
}

fn prepare_background(
    path: &Path,
    fallback: [u8; 3],
    scale_mode: super::StandbyBackgroundScale,
    max_frames: usize,
    format_version: u8,
    progress: &AtomicU32,
) -> Result<PreparedBackground> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
    if metadata.len() > 32 * 1024 * 1024 {
        bail!("source file is larger than 32 MB");
    }
    let is_gif = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("gif"));
    let (images, delays) = if is_gif {
        decode_gif(path, fallback, scale_mode, max_frames, progress)?
    } else {
        let image =
            image::open(path).with_context(|| format!("cannot decode {}", path.display()))?;
        (vec![prepare_frame(image, fallback, scale_mode)?], vec![0])
    };
    progress.store(100, Ordering::Relaxed);

    let frame_count = images.len();
    if frame_count == 0 || frame_count > max_frames {
        bail!("invalid prepared frame count {frame_count}");
    }
    let kind = if frame_count == 1 && !is_gif { 1 } else { 2 };
    let frame_size = if kind == 1 {
        IMAGE_FRAME_SIZE
    } else if format_version == FORMAT_VERSION {
        ANIMATION_FRAME_SIZE
    } else {
        LEGACY_ANIMATION_FRAME_SIZE
    };
    let mut payload = Vec::with_capacity(frame_count * frame_size);
    let mut preview_frames_rgba = Vec::with_capacity(frame_count);
    for (index, image) in images.iter().enumerate() {
        let quantized = if kind == 1 {
            encode_rgb332_transparent(image, &mut payload)
        } else if format_version == FORMAT_VERSION {
            encode_adaptive_6bit(image, &mut payload)
        } else {
            encode_adaptive_4bit_legacy(image, &mut payload)
        };
        preview_frames_rgba.push(quantized.into_raw());
        progress.store(
            100 + ((index + 1) as u32 * 45 / frame_count as u32),
            Ordering::Relaxed,
        );
    }
    let preview_rgba = preview_frames_rgba[0].clone();
    let data_crc = crc32(&payload);
    let total_size = HEADER_SIZE + payload.len();
    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    header[4] = format_version;
    header[5] = kind;
    header[6] = frame_count as u8;
    header[8..10].copy_from_slice(&(WIDTH as u16).to_le_bytes());
    header[10..12].copy_from_slice(&(HEIGHT as u16).to_le_bytes());
    header[12..16].copy_from_slice(&(frame_size as u32).to_le_bytes());
    header[16..20].copy_from_slice(&(total_size as u32).to_le_bytes());
    header[20..24].copy_from_slice(&data_crc.to_le_bytes());
    for (index, delay) in delays.iter().enumerate() {
        let offset = 28 + index * 2;
        header[offset..offset + 2].copy_from_slice(&delay.to_le_bytes());
    }
    let checksum = header_crc(&header);
    header[24..28].copy_from_slice(&checksum.to_le_bytes());

    let mut package = Vec::with_capacity(total_size);
    package.extend_from_slice(&header);
    package.extend_from_slice(&payload);
    Ok(PreparedBackground {
        kind,
        frame_count: frame_count as u8,
        package,
        preview_rgba,
        preview_frames_rgba,
        preview_delays_ms: delays,
    })
}

fn decode_gif(
    path: &Path,
    fallback: [u8; 3],
    scale_mode: super::StandbyBackgroundScale,
    max_frames: usize,
    progress: &AtomicU32,
) -> Result<(Vec<RgbaImage>, Vec<u16>)> {
    let file = std::fs::File::open(path)?;
    let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))?;
    let frames = decoder
        .into_frames()
        .take(MAX_SOURCE_FRAMES + 1)
        .collect::<image::ImageResult<Vec<_>>>()?;
    if frames.len() > MAX_SOURCE_FRAMES {
        bail!("GIF contains more than {MAX_SOURCE_FRAMES} frames");
    }
    if frames.is_empty() {
        bail!("GIF contains no frames");
    }

    let source_delays: Vec<u32> = frames
        .iter()
        .map(|frame| {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            if denominator == 0 {
                100
            } else {
                (numerator / denominator).max(20)
            }
        })
        .collect();
    let output_count = frames.len().min(max_frames);
    let mut images = Vec::with_capacity(output_count);
    let mut delays = Vec::with_capacity(output_count);
    for output in 0..output_count {
        let start = output * frames.len() / output_count;
        let end = ((output + 1) * frames.len() / output_count).max(start + 1);
        images.push(prepare_frame(
            DynamicImage::ImageRgba8(frames[start].buffer().clone()),
            fallback,
            scale_mode,
        )?);
        let delay = source_delays[start..end]
            .iter()
            .copied()
            .sum::<u32>()
            .max(20)
            .min(u16::MAX as u32) as u16;
        delays.push(delay);
        progress.store(
            10 + ((output + 1) as u32 * 80 / output_count as u32),
            Ordering::Relaxed,
        );
    }
    Ok((images, delays))
}

fn prepare_frame(
    image: DynamicImage,
    _fallback: [u8; 3],
    scale_mode: super::StandbyBackgroundScale,
) -> Result<RgbaImage> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        bail!("image has zero width or height");
    }
    let width_scale = WIDTH as f64 / width as f64;
    let height_scale = HEIGHT as f64 / height as f64;
    if scale_mode == super::StandbyBackgroundScale::Stretch {
        return Ok(image
            .resize_exact(WIDTH, HEIGHT, image::imageops::FilterType::Lanczos3)
            .into_rgba8());
    }
    let scale = match scale_mode {
        super::StandbyBackgroundScale::Fit => width_scale.min(height_scale),
        super::StandbyBackgroundScale::Fill => width_scale.max(height_scale),
        super::StandbyBackgroundScale::Stretch => unreachable!(),
    };
    let resized_width = match scale_mode {
        super::StandbyBackgroundScale::Fit => {
            (width as f64 * scale).round().clamp(1.0, WIDTH as f64) as u32
        }
        super::StandbyBackgroundScale::Fill => (width as f64 * scale).ceil() as u32,
        super::StandbyBackgroundScale::Stretch => unreachable!(),
    };
    let resized_height = match scale_mode {
        super::StandbyBackgroundScale::Fit => {
            (height as f64 * scale).round().clamp(1.0, HEIGHT as f64) as u32
        }
        super::StandbyBackgroundScale::Fill => (height as f64 * scale).ceil() as u32,
        super::StandbyBackgroundScale::Stretch => unreachable!(),
    };
    let resized = image
        .resize_exact(
            resized_width,
            resized_height,
            image::imageops::FilterType::Lanczos3,
        )
        .into_rgba8();
    let frame = match scale_mode {
        super::StandbyBackgroundScale::Fit => {
            let mut canvas = RgbaImage::from_pixel(WIDTH, HEIGHT, image::Rgba([0, 0, 0, 0]));
            let x = (WIDTH - resized_width) / 2;
            let y = (HEIGHT - resized_height) / 2;
            image::imageops::overlay(&mut canvas, &resized, i64::from(x), i64::from(y));
            canvas
        }
        super::StandbyBackgroundScale::Fill => {
            let x = (resized_width - WIDTH) / 2;
            let y = (resized_height - HEIGHT) / 2;
            image::imageops::crop_imm(&resized, x, y, WIDTH, HEIGHT).to_image()
        }
        super::StandbyBackgroundScale::Stretch => unreachable!(),
    };
    Ok(frame)
}

fn preview_over_background(image: &RgbaImage, background: [u8; 3]) -> RgbaImage {
    let mut preview = RgbaImage::from_pixel(
        WIDTH,
        HEIGHT,
        image::Rgba([background[0], background[1], background[2], 255]),
    );
    image::imageops::overlay(&mut preview, image, 0, 0);
    preview
}

fn decode_indexed_8bit_frame(payload: &[u8], background: [u8; 3]) -> Result<RgbaImage> {
    if payload.len() != IMAGE_FRAME_SIZE {
        bail!("invalid indexed startup frame size {}", payload.len());
    }
    let palette = &payload[..1024];
    let indices = &payload[1024..];
    let mut rgba = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for &index in indices {
        let offset = usize::from(index) * 4;
        let alpha = palette[offset + 3];
        if alpha == 0 {
            rgba.extend_from_slice(&[background[0], background[1], background[2], 255]);
        } else {
            rgba.extend_from_slice(&[
                palette[offset + 2],
                palette[offset + 1],
                palette[offset],
                255,
            ]);
        }
    }
    RgbaImage::from_raw(WIDTH, HEIGHT, rgba).context("invalid indexed startup preview")
}

fn rgb332_color(index: u8, fallback: [u8; 3]) -> [u8; 4] {
    if index == 0 {
        return [fallback[0], fallback[1], fallback[2], 255];
    }
    if index == 1 {
        return [0, 0, 0, 255];
    }
    [
        ((u16::from(index) >> 5) * 255 / 7) as u8,
        (((u16::from(index) >> 2) & 0x07) * 255 / 7) as u8,
        ((u16::from(index) & 0x03) * 255 / 3) as u8,
        255,
    ]
}

fn expand_startup_device_preview(indices: &[u8], fallback: [u8; 3]) -> Vec<u8> {
    debug_assert_eq!(indices.len(), STARTUP_PREVIEW_SIZE);
    let mut rgba = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let index = indices[(y / 4) * STARTUP_PREVIEW_WIDTH + x / 4];
            rgba.extend_from_slice(&rgb332_color(index, fallback));
        }
    }
    rgba
}

const BAYER: [[i16; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

fn alpha_is_visible(alpha: u8, x: usize, y: usize) -> bool {
    alpha != 0 && i16::from(alpha) > BAYER[y & 3][x & 3] * 16
}

fn encode_rgb332_transparent(image: &RgbaImage, output: &mut Vec<u8>) -> RgbaImage {
    output.extend_from_slice(&[0, 0, 0, 0]);
    output.extend_from_slice(&[0, 0, 0, 255]);
    for index in 2..256u16 {
        let red = (((index >> 5) & 0x07) * 255 / 7) as u8;
        let green = (((index >> 2) & 0x07) * 255 / 7) as u8;
        let blue = ((index & 0x03) * 255 / 3) as u8;
        output.extend_from_slice(&[blue, green, red, 255]);
    }
    let mut preview = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for (index, pixel) in image.pixels().enumerate() {
        let x = index % WIDTH as usize;
        let y = index / WIDTH as usize;
        if !alpha_is_visible(pixel[3], x, y) {
            output.push(0);
            preview.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let threshold = BAYER[y & 3][x & 3] - 8;
        let red = (pixel[0] as i16 + threshold * 3).clamp(0, 255) as u8;
        let green = (pixel[1] as i16 + threshold * 3).clamp(0, 255) as u8;
        let blue = (pixel[2] as i16 + threshold * 7).clamp(0, 255) as u8;
        let quantized = ((red & 0xE0) | ((green & 0xE0) >> 3) | (blue >> 6)).max(1);
        output.push(quantized);
        let color = if quantized == 1 {
            [0, 0, 0]
        } else {
            [
                ((u16::from(quantized) >> 5) * 255 / 7) as u8,
                (((u16::from(quantized) >> 2) & 0x07) * 255 / 7) as u8,
                ((u16::from(quantized) & 0x03) * 255 / 3) as u8,
            ]
        };
        preview.extend_from_slice(&[color[0], color[1], color[2], 255]);
    }
    RgbaImage::from_raw(WIDTH, HEIGHT, preview).expect("RGB332 preview dimensions")
}

fn adaptive_palette(image: &RgbaImage, visible_colors: usize) -> Vec<[u8; 3]> {
    let opaque_count = image.pixels().filter(|pixel| pixel[3] >= 16).count();
    if opaque_count == 0 {
        return vec![[0, 0, 0]];
    }
    let stride = opaque_count.div_ceil(4096);
    let mut seen = 0usize;
    let samples = image
        .pixels()
        .filter_map(|pixel| {
            if pixel[3] < 16 {
                return None;
            }
            let take = seen % stride == 0;
            seen += 1;
            take.then_some([pixel[0], pixel[1], pixel[2]])
        })
        .collect::<Vec<_>>();
    let mut boxes = vec![samples];
    while boxes.len() < visible_colors {
        let Some((box_index, channel, range)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, colors)| colors.len() > 1)
            .map(|(index, colors)| {
                let mut minimum = [u8::MAX; 3];
                let mut maximum = [u8::MIN; 3];
                for color in colors {
                    for channel in 0..3 {
                        minimum[channel] = minimum[channel].min(color[channel]);
                        maximum[channel] = maximum[channel].max(color[channel]);
                    }
                }
                let (channel, range) = (0..3)
                    .map(|channel| (channel, maximum[channel] - minimum[channel]))
                    .max_by_key(|(_, range)| *range)
                    .unwrap();
                (index, channel, range)
            })
            .max_by_key(|(index, _, range)| usize::from(*range) * boxes[*index].len())
        else {
            break;
        };
        if range == 0 {
            break;
        }
        boxes[box_index].sort_unstable_by_key(|color| color[channel]);
        let split_at = boxes[box_index].len() / 2;
        let other = boxes[box_index].split_off(split_at);
        boxes.push(other);
    }
    boxes
        .iter()
        .map(|colors| {
            let count = colors.len() as u32;
            let sum = colors.iter().fold([0u32; 3], |mut sum, color| {
                for channel in 0..3 {
                    sum[channel] += u32::from(color[channel]);
                }
                sum
            });
            let mean = [
                (sum[0] / count) as u8,
                (sum[1] / count) as u8,
                (sum[2] / count) as u8,
            ];
            colors
                .iter()
                .copied()
                .min_by_key(|color| {
                    (0..3)
                        .map(|channel| {
                            let delta = i32::from(mean[channel]) - i32::from(color[channel]);
                            delta * delta
                        })
                        .sum::<i32>()
                })
                .unwrap_or(mean)
        })
        .collect()
}

fn encode_adaptive_4bit_legacy(image: &RgbaImage, output: &mut Vec<u8>) -> RgbaImage {
    let palette = adaptive_palette(image, 15);
    output.extend_from_slice(&[0, 0, 0, 0]);
    for index in 0..15 {
        let color = palette[index.min(palette.len() - 1)];
        output.extend_from_slice(&[color[2], color[1], color[0], 255]);
    }
    let mut high_nibble = None;
    let mut preview = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for (index, pixel) in image.pixels().enumerate() {
        let x = index % WIDTH as usize;
        let y = index / WIDTH as usize;
        let palette_index = if alpha_is_visible(pixel[3], x, y) {
            let dither = (BAYER[y & 3][x & 3] - 8) * 2;
            let adjusted = [
                (i16::from(pixel[0]) + dither).clamp(0, 255) as u8,
                (i16::from(pixel[1]) + dither).clamp(0, 255) as u8,
                (i16::from(pixel[2]) + dither).clamp(0, 255) as u8,
            ];
            palette
                .iter()
                .enumerate()
                .min_by_key(|(_, color)| {
                    (0..3)
                        .map(|channel| {
                            let delta = i32::from(adjusted[channel]) - i32::from(color[channel]);
                            delta * delta
                        })
                        .sum::<i32>()
                })
                .map_or(1, |(index, _)| index as u8 + 1)
        } else {
            0
        };
        if palette_index == 0 {
            preview.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let color = palette[usize::from(palette_index - 1)];
            preview.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
        if let Some(high) = high_nibble.take() {
            output.push((high << 4) | palette_index);
        } else {
            high_nibble = Some(palette_index);
        }
    }
    debug_assert!(high_nibble.is_none());
    RgbaImage::from_raw(WIDTH, HEIGHT, preview).expect("legacy adaptive preview dimensions")
}

fn pack_6bit_group(indices: [u8; 4]) -> [u8; 3] {
    [
        (indices[0] << 2) | (indices[1] >> 4),
        (indices[1] << 4) | (indices[2] >> 2),
        (indices[2] << 6) | indices[3],
    ]
}

fn encode_adaptive_6bit(image: &RgbaImage, output: &mut Vec<u8>) -> RgbaImage {
    let frame_start = output.len();
    let palette = adaptive_palette(image, 63);
    output.extend_from_slice(&[0, 0, 0, 0]);
    for index in 0..63 {
        let color = palette[index.min(palette.len() - 1)];
        output.extend_from_slice(&[color[2], color[1], color[0], 255]);
    }
    let mut group = [0u8; 4];
    let mut group_len = 0usize;
    let mut preview = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
    for (index, pixel) in image.pixels().enumerate() {
        let x = index % WIDTH as usize;
        let y = index / WIDTH as usize;
        let palette_index = if alpha_is_visible(pixel[3], x, y) {
            let dither = (BAYER[y & 3][x & 3] - 8) * 2;
            let adjusted = [
                (i16::from(pixel[0]) + dither).clamp(0, 255) as u8,
                (i16::from(pixel[1]) + dither).clamp(0, 255) as u8,
                (i16::from(pixel[2]) + dither).clamp(0, 255) as u8,
            ];
            palette
                .iter()
                .enumerate()
                .min_by_key(|(_, color)| {
                    (0..3)
                        .map(|channel| {
                            let delta = i32::from(adjusted[channel]) - i32::from(color[channel]);
                            delta * delta
                        })
                        .sum::<i32>()
                })
                .map_or(1, |(index, _)| index as u8 + 1)
        } else {
            0
        };
        if palette_index == 0 {
            preview.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let color = palette[usize::from(palette_index - 1)];
            preview.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
        group[group_len] = palette_index;
        group_len += 1;
        if group_len == 4 {
            output.extend_from_slice(&pack_6bit_group(group));
            group_len = 0;
        }
    }
    debug_assert_eq!(group_len, 0);
    output.resize(frame_start + ANIMATION_FRAME_SIZE, 0xFF);
    RgbaImage::from_raw(WIDTH, HEIGHT, preview).expect("adaptive preview dimensions")
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn header_crc(header: &[u8; HEADER_SIZE]) -> u32 {
    let mut copy = *header;
    copy[24..28].fill(0);
    crc32(&copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::StandbyBackgroundScale;

    #[test]
    fn both_report_sizes_transfer_identical_full_animation_and_sequence_wrap() {
        use std::cell::{Cell, RefCell};
        let package: Vec<u8> = (0..HEADER_SIZE + MAX_FRAMES * ANIMATION_FRAME_SIZE)
            .map(|i| (i.wrapping_mul(17) >> 3) as u8)
            .collect();
        let mut counts = Vec::new();
        for packet_size in [32, 64] {
            let received = RefCell::new(Vec::new());
            let sequence = Cell::new(0usize);
            let handle = |packet: &[u8]| -> Result<[u8; HID_PACKET_SIZE]> {
                let mut reply = [0; HID_PACKET_SIZE];
                reply[0] = packet[0];
                if matches!(packet[0], CMD_DATA | CMD_DATA_STREAM) {
                    assert_eq!(packet.len(), packet_size);
                    assert_eq!(
                        u16::from_le_bytes([packet[1], packet[2]]),
                        sequence.get() as u16
                    );
                    let remaining = package.len() - received.borrow().len();
                    received
                        .borrow_mut()
                        .extend_from_slice(&packet[3..3 + remaining.min(packet_size - 3)]);
                    sequence.set(sequence.get() + 1);
                    reply[2..4].copy_from_slice(&(sequence.get() as u16).to_le_bytes());
                }
                if packet[0] == CMD_COMMIT {
                    assert_eq!(*received.borrow(), package);
                }
                Ok(reply)
            };
            assert!(transmit_background_sized(
                &package,
                &AtomicU32::new(0),
                &AtomicBool::new(false),
                packet_size,
                |p| handle(p),
                |p| handle(p).map(|_| ())
            )
            .unwrap());
            assert_eq!(*received.borrow(), package);
            counts.push(sequence.get());
        }
        assert_eq!(counts[0], package.len().div_ceil(29));
        assert_eq!(counts[1], package.len().div_ceil(61));
        assert!(counts[1] * 2 < counts[0]);
    }

    #[test]
    fn cancelled_background_transfer_never_commits_partial_data() {
        use std::cell::RefCell;
        for packet_size in [32, 64] {
            for cancel_at in [0, 1, 2, 3] {
                let cancel = AtomicBool::new(cancel_at == 0);
                let progress = AtomicU32::new(0);
                let commands = RefCell::new(Vec::new());
                let completed = transmit_background_sized(
                    &[42; 100],
                    &progress,
                    &cancel,
                    packet_size,
                    |packet| {
                        commands.borrow_mut().push(packet[0]);
                        let mut reply = [0; HID_PACKET_SIZE];
                        reply[0] = packet[0];
                        if packet[0] == CMD_DATA {
                            reply[2..4].copy_from_slice(
                                &u16::from_le_bytes([packet[1], packet[2]])
                                    .wrapping_add(1)
                                    .to_le_bytes(),
                            );
                            if cancel_at == 2 {
                                cancel.store(true, Ordering::Relaxed);
                            }
                        }
                        Ok(reply)
                    },
                    |packet| {
                        commands.borrow_mut().push(packet[0]);
                        if cancel_at == 1 {
                            cancel.store(true, Ordering::Relaxed);
                        }
                        Ok(())
                    },
                )
                .unwrap();
                let commands = commands.into_inner();
                assert_eq!(completed, cancel_at == 3);
                if cancel_at == 0 {
                    assert!(commands.is_empty());
                } else if cancel_at < 3 {
                    assert_eq!(commands.last(), Some(&CMD_CLEAR));
                    assert!(!commands.contains(&CMD_COMMIT));
                } else {
                    assert_eq!(commands.last(), Some(&CMD_COMMIT));
                    assert!(!commands.contains(&CMD_CLEAR));
                }
            }
        }
    }

    #[test]
    fn adaptive_palette_keeps_real_source_colors() {
        let colors = [
            image::Rgba([255, 0, 0, 255]),
            image::Rgba([0, 255, 0, 255]),
            image::Rgba([0, 0, 255, 255]),
        ];
        let mut image = RgbaImage::new(3, 1);
        for (x, color) in colors.iter().copied().enumerate() {
            image.put_pixel(x as u32, 0, color);
        }
        let palette = adaptive_palette(&image, 2);
        assert!(palette
            .iter()
            .all(|color| colors.iter().any(|source| source.0[..3] == color[..])));
    }

    #[test]
    fn six_bit_indices_pack_four_pixels_into_three_bytes() {
        assert_eq!(pack_6bit_group([0, 1, 2, 63]), [0x00, 0x10, 0xBF]);
    }

    #[test]
    fn upload_sequence_wraps_after_u16_max() {
        assert_eq!(next_sequence(65_534), 65_535);
        assert_eq!(next_sequence(65_535), 0);
        assert_eq!(next_sequence(65_536), 1);
    }

    #[test]
    fn gif_respects_two_mib_frame_limit_and_keeps_duration() {
        let path = std::env::temp_dir().join(format!("entropy-2mib-{}.gif", std::process::id()));
        {
            let mut encoder =
                image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
            for i in 0..56 {
                let frame = image::Frame::from_parts(
                    RgbaImage::from_pixel(2, 2, image::Rgba([i * 4, 80, 100, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                );
                encoder.encode_frame(frame).unwrap();
            }
        }
        let prepared = prepare_background(
            &path,
            [0, 0, 0],
            StandbyBackgroundScale::Fit,
            20,
            FORMAT_VERSION,
            &AtomicU32::new(0),
        )
        .unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(prepared.frame_count, 20);
        let info = BackgroundInfo {
            format_version: FORMAT_VERSION,
            kind: prepared.kind,
            frame_count: prepared.frame_count,
            total_size: prepared.package.len() as u32,
            header_crc: read_u32(&prepared.package[24..28]),
            ..Default::default()
        };
        let restored = decode_cached_background(&prepared.package, info).unwrap();
        assert_eq!(restored.preview_frames_rgba, prepared.preview_frames_rgba);
        assert_eq!(restored.preview_delays_ms, prepared.preview_delays_ms);
        let mut damaged = prepared.package.clone();
        damaged[HEADER_SIZE + 110] ^= 1;
        assert!(decode_cached_background(&damaged, info).is_none());
        assert!(decode_cached_background(
            &prepared.package,
            BackgroundInfo {
                header_crc: info.header_crc ^ 1,
                ..info
            }
        )
        .is_none());

        assert_eq!(prepared.package.len(), 1_014_016);
        assert_eq!(
            prepared
                .preview_delays_ms
                .iter()
                .map(|&v| u32::from(v))
                .sum::<u32>(),
            5_600
        );
        assert_eq!(
            header_crc(prepared.package[..HEADER_SIZE].try_into().unwrap()),
            read_u32(&prepared.package[24..28])
        );
    }

    #[test]
    fn package_sizes_fit_reserved_firmware_limit() {
        assert_eq!(IMAGE_FRAME_SIZE, 68_224);
        assert_eq!(ANIMATION_FRAME_SIZE, 50_688);
        let maximum_package = HEADER_SIZE + MAX_FRAMES * ANIMATION_FRAME_SIZE;
        assert_eq!(maximum_package, 2_838_784);
        assert!(maximum_package.div_ceil(29) > u16::MAX as usize);
    }

    #[test]
    fn crc_matches_standard_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn png_is_resized_and_packaged_for_lvgl() {
        let path = std::env::temp_dir().join(format!(
            "entropy-standby-background-{}.png",
            std::process::id()
        ));
        let source = RgbaImage::from_pixel(12, 8, image::Rgba([240, 80, 32, 255]));
        source.save(&path).unwrap();
        let progress = AtomicU32::new(0);
        let fallback = [9, 17, 25];
        let prepared = prepare_background(
            &path,
            fallback,
            StandbyBackgroundScale::Fit,
            MAX_FRAMES,
            FORMAT_VERSION,
            &progress,
        )
        .unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(prepared.kind, 1);
        assert_eq!(prepared.frame_count, 1);
        let info = BackgroundInfo {
            format_version: FORMAT_VERSION,
            kind: 1,
            frame_count: 1,
            total_size: prepared.package.len() as u32,
            header_crc: read_u32(&prepared.package[24..28]),
            ..Default::default()
        };
        assert_eq!(
            decode_cached_background(&prepared.package, info)
                .unwrap()
                .preview_rgba,
            prepared.preview_rgba
        );

        assert_eq!(prepared.package.len(), HEADER_SIZE + IMAGE_FRAME_SIZE);
        assert_eq!(read_u32(&prepared.package[0..4]), MAGIC);
        assert_eq!(
            read_u32(&prepared.package[16..20]) as usize,
            prepared.package.len()
        );
        assert_eq!(
            read_u32(&prepared.package[24..28]),
            header_crc(prepared.package[..HEADER_SIZE].try_into().unwrap())
        );
        assert_eq!(
            prepared.preview_rgba.len(),
            WIDTH as usize * HEIGHT as usize * 4
        );
        assert_eq!(&prepared.preview_rgba[..4], &[0, 0, 0, 0]);
        let center = ((HEIGHT as usize / 2 * WIDTH as usize) + WIDTH as usize / 2) * 4;
        assert_eq!(
            &prepared.preview_rgba[center..center + 4],
            &[218, 36, 0, 255]
        );
    }

    #[test]
    fn startup_png_is_packaged_as_indexed_full_screen_image() {
        let path =
            std::env::temp_dir().join(format!("entropy-startup-image-{}.png", std::process::id()));
        let source = RgbaImage::from_pixel(12, 8, image::Rgba([240, 80, 32, 255]));
        source.save(&path).unwrap();
        let prepared = prepare_startup_image(&path, [9, 17, 25]).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(prepared.package.len(), HEADER_SIZE + IMAGE_FRAME_SIZE);
        assert_eq!(read_u32(&prepared.package[0..4]), STARTUP_MAGIC);
        assert_eq!(
            prepared.preview_rgba.len(),
            WIDTH as usize * HEIGHT as usize * 4
        );
        let decoded = decode_indexed_8bit_frame(&prepared.package[HEADER_SIZE..], [9, 17, 25])
            .unwrap()
            .into_raw();
        assert_eq!(prepared.preview_rgba, decoded);
    }

    #[test]
    fn compact_device_preview_expands_to_the_display_without_changing_colors() {
        let mut indices = vec![0; STARTUP_PREVIEW_SIZE];
        indices[STARTUP_PREVIEW_WIDTH + 1] = 0xE3;
        let preview = expand_startup_device_preview(&indices, [9, 17, 25]);

        assert_eq!(preview.len(), WIDTH as usize * HEIGHT as usize * 4);
        assert_eq!(&preview[..4], &[9, 17, 25, 255]);
        let sampled = ((5usize * WIDTH as usize) + 5) * 4;
        assert_eq!(
            &preview[sampled..sampled + 4],
            &rgb332_color(0xE3, [9, 17, 25])
        );
    }

    #[test]
    fn fill_mode_crops_without_letterboxing() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            12,
            8,
            image::Rgba([240, 80, 32, 255]),
        ));
        let frame = prepare_frame(source, [9, 17, 25], StandbyBackgroundScale::Fill).unwrap();

        assert_eq!(frame.get_pixel(0, 0).0, [240, 80, 32, 255]);
    }

    #[test]
    fn stretch_mode_scales_exactly_to_the_display() {
        let mut source = RgbaImage::from_pixel(2, 1, image::Rgba([240, 80, 32, 255]));
        source.put_pixel(1, 0, image::Rgba([20, 60, 220, 255]));
        let frame = prepare_frame(
            DynamicImage::ImageRgba8(source),
            [9, 17, 25],
            StandbyBackgroundScale::Stretch,
        )
        .unwrap();

        assert_eq!(frame.dimensions(), (WIDTH, HEIGHT));
        assert!(frame.get_pixel(0, HEIGHT / 2)[0] > frame.get_pixel(WIDTH - 1, HEIGHT / 2)[0]);
        assert!(frame.get_pixel(0, HEIGHT / 2)[2] < frame.get_pixel(WIDTH - 1, HEIGHT / 2)[2]);
    }

    #[test]
    fn legacy_encoder_remains_available_for_v3_firmware() {
        let image = RgbaImage::from_pixel(WIDTH, HEIGHT, image::Rgba([200, 100, 50, 0]));
        let mut encoded = Vec::new();
        let preview = encode_adaptive_4bit_legacy(&image, &mut encoded);

        assert_eq!(encoded.len(), LEGACY_ANIMATION_FRAME_SIZE);
        assert!(encoded[LEGACY_ANIMATION_PALETTE_SIZE..]
            .iter()
            .all(|byte| *byte == 0));
        assert!(preview.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
    }

    #[test]
    fn transparent_pixels_use_palette_index_zero() {
        let image = RgbaImage::from_pixel(WIDTH, HEIGHT, image::Rgba([200, 100, 50, 0]));
        let mut encoded = Vec::new();
        let preview = encode_adaptive_6bit(&image, &mut encoded);

        assert_eq!(encoded.len(), ANIMATION_FRAME_SIZE);
        assert!(
            encoded[ANIMATION_PALETTE_SIZE..ANIMATION_PALETTE_SIZE + ANIMATION_PIXEL_SIZE]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert!(encoded[ANIMATION_PALETTE_SIZE + ANIMATION_PIXEL_SIZE..]
            .iter()
            .all(|byte| *byte == 0xFF));
        assert!(preview.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
    }

    #[test]
    fn adaptive_preview_uses_the_exact_encoded_palette_color() {
        let mut image = RgbaImage::new(WIDTH, HEIGHT);
        for (index, pixel) in image.pixels_mut().enumerate() {
            let value = (index % 251) as u8;
            *pixel = image::Rgba([value, value.wrapping_mul(3), value.wrapping_mul(7), 255]);
        }
        let mut encoded = Vec::new();
        let preview = encode_adaptive_6bit(&image, &mut encoded);
        let first_index = encoded[ANIMATION_PALETTE_SIZE] >> 2;
        assert_ne!(first_index, 0);
        let palette_offset = usize::from(first_index) * 4;
        assert_eq!(
            preview.get_pixel(0, 0).0,
            [
                encoded[palette_offset + 2],
                encoded[palette_offset + 1],
                encoded[palette_offset],
                255,
            ]
        );
        let unique = preview
            .pixels()
            .map(|pixel| pixel.0)
            .collect::<std::collections::HashSet<_>>();
        assert!(unique.len() <= 63);
    }
}

#[cfg(test)]
pub(crate) fn test_background_upload_responses(commit_status: u8) -> Vec<[u8; 32]> {
    // One still frame on the legacy-sized channel: never enumerate fast HID.
    let mut query = [0; 32];
    query[0] = CMD_QUERY;
    query[2] = FORMAT_VERSION;
    query[5..9].copy_from_slice(&((HEADER_SIZE + IMAGE_FRAME_SIZE) as u32).to_le_bytes());
    query[13] = 1;
    query[14..16].copy_from_slice(&DEFAULT_SPEED_PERCENT.to_le_bytes());
    let mut begin = [0; 32];
    begin[0] = CMD_BEGIN;
    let mut replies = vec![query, begin];
    let packets = (HEADER_SIZE + IMAGE_FRAME_SIZE).div_ceil(29);
    for sequence in 0..packets {
        if (sequence + 1) % STREAM_ACK_INTERVAL == 0 || sequence + 1 == packets {
            let mut reply = [0; 32];
            reply[0] = CMD_DATA;
            reply[2..4].copy_from_slice(&next_sequence(sequence).to_le_bytes());
            replies.push(reply);
        }
    }
    let mut commit = [0; 32];
    commit[0] = CMD_COMMIT;
    commit[1] = commit_status;
    replies.push(commit);
    replies
}

#[cfg(test)]
mod legacy_preview_tests {
    use super::*;

    #[test]
    fn both_package_versions_restore_still_and_animation_pixels_and_delays() {
        let directory = tempfile::tempdir().unwrap();
        let image = RgbaImage::from_fn(12, 8, |x, y| {
            image::Rgba([
                (x * 20) as u8,
                (y * 30) as u8,
                120,
                if x < 2 { 0 } else { 255 },
            ])
        });
        let png = directory.path().join("still.png");
        image.save(&png).unwrap();
        let gif = directory.path().join("animation.gif");
        {
            let mut encoder =
                image::codecs::gif::GifEncoder::new(std::fs::File::create(&gif).unwrap());
            for delay in [70, 160] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image.clone(),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        for version in [LEGACY_FORMAT_VERSION, FORMAT_VERSION] {
            for path in [&png, &gif] {
                let prepared = prepare_background(
                    path,
                    [0, 0, 0],
                    super::super::StandbyBackgroundScale::Fit,
                    MAX_FRAMES,
                    version,
                    &AtomicU32::new(0),
                )
                .unwrap();
                let info = BackgroundInfo {
                    format_version: version,
                    kind: prepared.kind,
                    frame_count: prepared.frame_count,
                    total_size: prepared.package.len() as u32,
                    header_crc: read_u32(&prepared.package[24..28]),
                    ..Default::default()
                };
                let restored = decode_cached_background(&prepared.package, info).unwrap();
                assert_eq!(restored.preview_rgba, prepared.preview_rgba);
                assert_eq!(restored.preview_frames_rgba, prepared.preview_frames_rgba);
                assert_eq!(restored.preview_delays_ms, prepared.preview_delays_ms);
                assert!(restored
                    .preview_rgba
                    .chunks_exact(4)
                    .any(|pixel| pixel[3] == 0));
                let mut damaged = prepared.package.clone();
                damaged[HEADER_SIZE + 7] ^= 1;
                assert!(decode_cached_background(&damaged, info).is_none());
                assert!(decode_cached_background(
                    &prepared.package[..prepared.package.len() - 1],
                    info
                )
                .is_none());
                assert!(decode_cached_background(
                    &prepared.package,
                    BackgroundInfo {
                        format_version: 9,
                        ..info
                    }
                )
                .is_none());
                assert!(decode_cached_background(
                    &prepared.package,
                    BackgroundInfo {
                        format_version: if version == 3 { 4 } else { 3 },
                        ..info
                    }
                )
                .is_none());
                assert!(decode_cached_background(
                    &prepared.package,
                    BackgroundInfo {
                        frame_count: 0,
                        ..info
                    }
                )
                .is_none());
            }
        }
    }
}
