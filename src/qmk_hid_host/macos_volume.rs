//! Native output-volume reads avoid launching AppleScript on every bridge tick.
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const VOLUME: u32 = u32::from_be_bytes(*b"volm");
const VIRTUAL_VOLUME: u32 = u32::from_be_bytes(*b"vmvc");

#[cfg(target_os = "macos")]
#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const std::ffi::c_void,
        size: *mut u32,
        data: *mut std::ffi::c_void,
    ) -> i32;
}

// Audio Hardware Service exposes the system's virtual main volume for
// devices with separate channel controls, preserving their balance.
#[cfg(target_os = "macos")]
#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioHardwareServiceGetPropertyData(
        object: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const std::ffi::c_void,
        size: *mut u32,
        data: *mut std::ffi::c_void,
    ) -> i32;
}

#[cfg(target_os = "macos")]
fn property(object: u32, address: PropertyAddress) -> Option<u32> {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        let read = if address.selector == VIRTUAL_VOLUME {
            AudioHardwareServiceGetPropertyData
        } else {
            AudioObjectGetPropertyData
        };
        read(
            object,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut u32).cast(),
        )
    };
    (status == 0 && size == 4).then_some(value)
}

fn scalar_percent(bits: u32) -> Option<u8> {
    let scalar = f32::from_bits(bits);
    scalar
        .is_finite()
        .then(|| (scalar.clamp(0.0, 1.0) * 100.0).round() as u8)
}

fn native_volume(
    mut read: impl FnMut(u32, PropertyAddress) -> Option<u32>,
) -> (Option<u32>, Option<u8>) {
    let Some(device) = read(
        1,
        PropertyAddress {
            selector: DEFAULT_OUTPUT,
            scope: GLOBAL,
            element: 0,
        },
    )
    .filter(|id| *id != 0) else {
        return (None, None);
    };
    // Prefer system main volume; never estimate it by averaging channels.
    let value = read(
        device,
        PropertyAddress {
            selector: VIRTUAL_VOLUME,
            scope: OUTPUT,
            element: 0,
        },
    )
    .and_then(scalar_percent)
    .or_else(|| {
        read(
            device,
            PropertyAddress {
                selector: VOLUME,
                scope: OUTPUT,
                element: 0,
            },
        )
        .and_then(scalar_percent)
    });
    (Some(device), value)
}

#[derive(Default)]
struct FallbackCache {
    device: Option<u32>,
    last_read: Option<Instant>,
    value: Option<u8>,
}

impl FallbackCache {
    fn get(
        &mut self,
        device: Option<u32>,
        now: Instant,
        read: impl FnOnce() -> Option<u8>,
    ) -> Option<u8> {
        if self.device != device
            || self
                .last_read
                .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(250))
        {
            self.device = device;
            self.last_read = Some(now);
            self.value = read();
        }
        self.value
    }
}

#[cfg(target_os = "macos")]
pub(super) fn volume_percent() -> Option<u8> {
    let (device, value) = native_volume(property);
    if value.is_some() {
        return value;
    }
    // Some HDMI/USB/virtual devices do not expose a main hardware control.
    // Retain the previous system-volume query, rate-limited independently so
    // we never start 25 osascript processes a second on such devices.
    thread_local! {
        static FALLBACK: std::cell::RefCell<FallbackCache> = std::cell::RefCell::new(FallbackCache::default());
    }
    FALLBACK.with(|cache| {
        cache.borrow_mut().get(device, Instant::now(), || {
            super::macos_automation_stdout(&["-e", "output volume of (get volume settings)"])
                .and_then(|out| out.trim().parse::<u8>().ok())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_default_output_and_rejects_missing_or_invalid_values() {
        for (id, scalar, expected) in [
            (7, 0.42, Some(42)),
            (12, 0.0, Some(0)),
            (23, 1.0, Some(100)),
            (23, f32::NAN, None),
        ] {
            let mut calls = 0;
            let result = native_volume(|object, address| {
                calls += 1;
                if calls == 1 {
                    assert_eq!(
                        (object, address.selector, address.scope, address.element),
                        (1, DEFAULT_OUTPUT, GLOBAL, 0)
                    );
                    Some(id)
                } else {
                    assert_eq!((object, address.scope, address.element), (id, OUTPUT, 0));
                    assert_eq!(
                        address.selector,
                        if calls == 2 { VIRTUAL_VOLUME } else { VOLUME }
                    );
                    Some(scalar.to_bits())
                }
            });
            assert_eq!(result, (Some(id), expected));
            assert_eq!(calls, if expected.is_some() { 2 } else { 3 });
        }
        assert_eq!(native_volume(|_, _| None), (None, None));
        assert_eq!(native_volume(|_, _| Some(0)), (None, None));
    }

    #[test]
    fn falls_back_to_hardware_main_control_without_guessing_channel_volume() {
        assert_eq!(
            native_volume(|_, address| match address.selector {
                DEFAULT_OUTPUT => Some(9),
                VIRTUAL_VOLUME => None,
                VOLUME => Some(0.56f32.to_bits()),
                _ => panic!("unexpected property"),
            }),
            (Some(9), Some(56))
        );
    }

    #[test]
    fn unsupported_device_fallback_is_bounded_and_invalidated_on_device_switch() {
        let mut cache = FallbackCache::default();
        let start = Instant::now();
        assert_eq!(cache.get(Some(1), start, || Some(42)), Some(42));
        for ms in (40..250).step_by(40) {
            assert_eq!(
                cache.get(Some(1), start + Duration::from_millis(ms), || panic!(
                    "fallback queried too frequently"
                )),
                Some(42)
            );
        }
        assert_eq!(
            cache.get(Some(1), start + Duration::from_millis(250), || Some(44)),
            Some(44)
        );
        assert_eq!(
            cache.get(Some(2), start + Duration::from_millis(260), || Some(18)),
            Some(18)
        );
        assert_eq!(
            cache.get(None, start + Duration::from_millis(270), || None),
            None
        );
    }
}
