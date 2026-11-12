//! TIS objects and observers stay on the main queue; only owned evidence
//! crosses back to the shared desktop lane.
use super::{layout_code_index, LayoutEvents};
use crate::key_legend_detection::{
    DetectionFailure, KeyGlyph, LayoutObservation, NativeLayoutIdentity, SystemKeymap,
    SystemLayoutDescriptor,
};
use std::ffi::c_void;

type CFRef = *const c_void;
type NotificationCallback = unsafe extern "C" fn(CFRef, *mut c_void, CFRef, CFRef, CFRef);

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyInputSourceID: CFRef;
    static kTISPropertyLocalizedName: CFRef;
    static kTISPropertyInputSourceLanguages: CFRef;
    static kTISPropertyUnicodeKeyLayoutData: CFRef;
    static kTISNotifySelectedKeyboardInputSourceChanged: CFRef;
    fn TISCopyCurrentKeyboardInputSource() -> CFRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFRef;
    fn TISCreateInputSourceList(properties: CFRef, all_installed: bool) -> CFRef;
    fn TISGetInputSourceProperty(source: CFRef, key: CFRef) -> CFRef;
    fn LMGetKbdType() -> u8;
    fn KBGetLayoutType(keyboard_type: i16) -> u32;
    fn UCKeyTranslate(
        layout: CFRef,
        key: u16,
        action: u16,
        modifiers: u32,
        keyboard_type: u32,
        options: u32,
        dead: *mut u32,
        max_length: usize,
        length: *mut usize,
        output: *mut u16,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFRef);
    fn CFArrayGetCount(array: CFRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
    fn CFStringGetCString(string: CFRef, output: *mut i8, size: isize, encoding: u32) -> bool;
    fn CFDataGetBytePtr(data: CFRef) -> *const u8;
    fn CFNotificationCenterGetDistributedCenter() -> CFRef;
    fn CFNotificationCenterAddObserver(
        center: CFRef,
        observer: CFRef,
        callback: NotificationCallback,
        name: CFRef,
        object: CFRef,
        suspension_behavior: isize,
    );
    fn CFNotificationCenterRemoveObserver(
        center: CFRef,
        observer: CFRef,
        name: CFRef,
        object: CFRef,
    );
}

struct Source(CFRef);
impl Drop for Source {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

unsafe fn string(value: CFRef, capacity: usize) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut bytes = vec![0i8; capacity];
    if !CFStringGetCString(value, bytes.as_mut_ptr(), bytes.len() as isize, 0x08000100) {
        return None;
    }
    std::ffi::CStr::from_ptr(bytes.as_ptr())
        .to_str()
        .ok()
        .map(str::to_owned)
}

// HID physical positions -> Carbon virtual key codes. The ISO-only key is
// included separately; never treat a missing physical key as fixture proof.
const KEYS: [(u16, u16); 47] = [
    (4, 0),
    (5, 11),
    (6, 8),
    (7, 2),
    (8, 14),
    (9, 3),
    (10, 5),
    (11, 4),
    (12, 34),
    (13, 38),
    (14, 40),
    (15, 37),
    (16, 46),
    (17, 45),
    (18, 31),
    (19, 35),
    (20, 12),
    (21, 15),
    (22, 1),
    (23, 17),
    (24, 32),
    (25, 9),
    (26, 13),
    (27, 7),
    (28, 16),
    (29, 6),
    (30, 18),
    (31, 19),
    (32, 20),
    (33, 21),
    (34, 23),
    (35, 22),
    (36, 26),
    (37, 28),
    (38, 25),
    (39, 29),
    (45, 27),
    (46, 24),
    (47, 33),
    (48, 30),
    (49, 42),
    (51, 41),
    (52, 39),
    (53, 50),
    (54, 43),
    (55, 47),
    (56, 44),
];

fn physical_key_code(key: u16, iso: bool) -> u16 {
    // UCKeyTranslate reverses grave and the extra ISO position; SDL's Cocoa
    // UpdateKeymap applies the same physical-position correction.
    // https://github.com/libsdl-org/SDL/blob/main/src/video/cocoa/SDL_cocoakeyboard.m
    if iso && matches!(key, 10 | 50) {
        60 - key
    } else {
        key
    }
}

#[cfg(test)]
#[test]
fn key_legend_iso_positions_match_native_translation_contract() {
    assert_eq!(physical_key_code(50, true), 10);
    assert_eq!(physical_key_code(10, true), 50);
    assert_eq!(physical_key_code(50, false), 50);
    assert_eq!(physical_key_code(10, false), 10);
    assert_eq!(physical_key_code(42, true), 42);
}

unsafe fn keymap(source: CFRef) -> Option<std::sync::Arc<SystemKeymap>> {
    let id = string(
        TISGetInputSourceProperty(source, kTISPropertyInputSourceID),
        4096,
    )?;
    let name = string(
        TISGetInputSourceProperty(source, kTISPropertyLocalizedName),
        4096,
    )
    .unwrap_or_else(|| id.clone());
    let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
    if data.is_null() {
        return None;
    }
    let bytes = CFDataGetBytePtr(data);
    if bytes.is_null() {
        return None;
    }
    let keyboard_type = LMGetKbdType() as u32;
    let iso = KBGetLayoutType(keyboard_type as i16) == u32::from_be_bytes(*b"ISO ");
    let mut keys = std::collections::BTreeMap::new();
    for (hid, key) in KEYS.iter().copied().chain(iso.then_some((100, 10))).chain([
        (50, 42),
        (44, 49),
        (135, 94),
        (137, 93),
    ]) {
        let key = physical_key_code(key, iso);
        let mut levels: [KeyGlyph; 4] = Default::default();
        for (level, modifiers) in [0, 2, 8, 10].into_iter().enumerate() {
            let mut dead = 0;
            let mut length = 0;
            let mut output = [0u16; 16];
            if UCKeyTranslate(
                bytes.cast(),
                key,
                0,
                modifiers,
                keyboard_type,
                0,
                &mut dead,
                output.len(),
                &mut length,
                output.as_mut_ptr(),
            ) != 0
            {
                continue;
            }
            let is_dead = dead != 0;
            if is_dead {
                dead = 0;
                if UCKeyTranslate(
                    bytes.cast(),
                    key,
                    3,
                    modifiers,
                    keyboard_type,
                    1,
                    &mut dead,
                    output.len(),
                    &mut length,
                    output.as_mut_ptr(),
                ) != 0
                {
                    continue;
                }
            }
            if length <= output.len() {
                if let Ok(text) = String::from_utf16(&output[..length]) {
                    levels[level] = KeyGlyph::new(text, is_dead);
                }
            }
        }
        keys.insert(hid, levels);
    }
    Some(std::sync::Arc::new(SystemKeymap { id, name, keys }))
}

pub(super) fn installed_layouts() -> Vec<SystemLayoutDescriptor> {
    dispatch2::run_on_main(|_| unsafe {
        let sources = TISCreateInputSourceList(std::ptr::null(), true);
        if sources.is_null() {
            return Vec::new();
        }
        let sources = Source(sources);
        (0..CFArrayGetCount(sources.0))
            .filter_map(|i| {
                let source = CFArrayGetValueAtIndex(sources.0, i);
                if TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData).is_null() {
                    return None;
                }
                let id = string(
                    TISGetInputSourceProperty(source, kTISPropertyInputSourceID),
                    4096,
                )?;
                let name = string(
                    TISGetInputSourceProperty(source, kTISPropertyLocalizedName),
                    4096,
                )
                .unwrap_or_else(|| id.clone());
                Some(SystemLayoutDescriptor { id, name })
            })
            .collect()
    })
}

pub(super) fn load_layout(id: &str) -> Option<std::sync::Arc<SystemKeymap>> {
    let id = id.to_owned();
    dispatch2::run_on_main(move |_| unsafe {
        let sources = TISCreateInputSourceList(std::ptr::null(), true);
        if sources.is_null() {
            return None;
        }
        let sources = Source(sources);
        (0..CFArrayGetCount(sources.0)).find_map(|i| {
            let source = CFArrayGetValueAtIndex(sources.0, i);
            (string(
                TISGetInputSourceProperty(source, kTISPropertyInputSourceID),
                4096,
            )
            .as_deref()
                == Some(&id))
            .then(|| keymap(source))
            .flatten()
        })
    })
}

pub(super) fn current_layout() -> LayoutObservation {
    dispatch2::run_on_main(|_| unsafe {
        let source = TISCopyCurrentKeyboardInputSource();
        if source.is_null() {
            return None.into();
        }
        let source = Source(source);
        let languages = TISGetInputSourceProperty(source.0, kTISPropertyInputSourceLanguages);
        let language = if !languages.is_null() && CFArrayGetCount(languages) > 0 {
            string(CFArrayGetValueAtIndex(languages, 0), 64).and_then(|s| layout_code_index(&s))
        } else {
            None
        };
        let mapping = keymap(source.0).or_else(|| {
            let layout_source = TISCopyCurrentKeyboardLayoutInputSource();
            if layout_source.is_null() {
                return None;
            }
            let layout_source = Source(layout_source);
            keymap(layout_source.0)
        });
        LayoutObservation {
            language,
            identity: mapping.map(NativeLayoutIdentity::Mapped).unwrap_or(
                NativeLayoutIdentity::Unavailable(DetectionFailure::Unavailable),
            ),
        }
    })
}

pub(super) struct Observer(usize);

impl Observer {
    pub(super) fn new(events: LayoutEvents) -> Option<Self> {
        dispatch2::run_on_main(move |_| unsafe {
            let center = CFNotificationCenterGetDistributedCenter();
            if center.is_null() {
                return None;
            }
            let state = Box::into_raw(Box::new(events));
            CFNotificationCenterAddObserver(
                center,
                state.cast(),
                changed,
                kTISNotifySelectedKeyboardInputSourceChanged,
                std::ptr::null(),
                4,
            );
            (*state).following(true);
            Some(Self(state as usize))
        })
    }
}

unsafe extern "C" fn changed(_: CFRef, observer: *mut c_void, _: CFRef, _: CFRef, _: CFRef) {
    // Registration, callbacks and removal share the main queue, so removal
    // completes before its boxed callback state can be freed.
    if let Some(events) = observer.cast::<LayoutEvents>().as_ref() {
        events.changed(false);
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        let state = self.0;
        dispatch2::run_on_main(move |_| unsafe {
            let pointer = state as *mut LayoutEvents;
            CFNotificationCenterRemoveObserver(
                CFNotificationCenterGetDistributedCenter(),
                pointer.cast(),
                kTISNotifySelectedKeyboardInputSourceChanged,
                std::ptr::null(),
            );
            let events = Box::from_raw(pointer);
            events.following(false);
        });
    }
}
