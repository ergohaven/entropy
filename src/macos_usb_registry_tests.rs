//! Synthetic CoreFoundation/IORegistry ABI, only on non-Mac test hosts.
//! Production scan/read/ancestry/decoder are compiled unchanged above this seam.
use super::*;
use std::{cell::RefCell, collections::BTreeMap, ffi::CStr};
#[derive(Clone)]
enum Value {
    Text(String),
    Number(i64),
    Dict(BTreeMap<String, CF>),
    Array(Vec<CF>),
}
struct State {
    arena: Vec<Box<RefCell<Value>>>,
    properties: CF,
    parent: CF,
    non_vial: CF,
    case: u8,
    next: usize,
    valid_filter: bool,
    inspected: Vec<u32>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            arena: vec![],
            properties: ptr::null(),
            parent: ptr::null(),
            non_vial: ptr::null(),
            case: 0,
            next: 0,
            valid_filter: false,
            inspected: vec![],
        }
    }
}
thread_local! { static OS: RefCell<State> = RefCell::new(State::default()); }
fn alloc(v: Value) -> CF {
    OS.with(|s| {
        let mut s = s.borrow_mut();
        let b = Box::new(RefCell::new(v));
        let p = (&*b as *const RefCell<Value>).cast();
        s.arena.push(b);
        p
    })
}
unsafe fn value(p: CF) -> Value {
    (*(p as *const RefCell<Value>)).borrow().clone()
}
fn dict(items: Vec<(&str, Value)>) -> CF {
    let mut map = BTreeMap::new();
    for (k, v) in items {
        map.insert(k.into(), alloc(v));
    }
    alloc(Value::Dict(map))
}
fn num(n: i64) -> Value {
    Value::Number(n)
}
fn txt(s: &str) -> Value {
    Value::Text(s.into())
}
#[export_name = "kCFTypeDictionaryKeyCallBacks"]
static KEYS: u8 = 0;
#[export_name = "kCFTypeDictionaryValueCallBacks"]
static VALUES: u8 = 0;
#[no_mangle]
unsafe extern "C" fn CFRelease(_: CF) {}
#[no_mangle]
unsafe extern "C" fn CFGetTypeID(v: CF) -> usize {
    match value(v) {
        Value::Text(_) => 1,
        Value::Number(_) => 2,
        Value::Dict(_) => 3,
        Value::Array(_) => 4,
    }
}
#[no_mangle]
extern "C" fn CFStringGetTypeID() -> usize {
    1
}
#[no_mangle]
extern "C" fn CFNumberGetTypeID() -> usize {
    2
}
#[no_mangle]
extern "C" fn CFDictionaryGetTypeID() -> usize {
    3
}
#[no_mangle]
extern "C" fn CFArrayGetTypeID() -> usize {
    4
}
#[no_mangle]
unsafe extern "C" fn CFStringCreateWithCString(_: CF, s: *const c_char, _: u32) -> CF {
    alloc(txt(CStr::from_ptr(s).to_str().unwrap()))
}
#[no_mangle]
unsafe extern "C" fn CFStringGetLength(s: CF) -> isize {
    if let Value::Text(s) = value(s) {
        s.encode_utf16().count() as isize
    } else {
        0
    }
}
#[no_mangle]
unsafe extern "C" fn CFStringGetBytes(
    s: CF,
    r: Range,
    e: u32,
    _: u8,
    _: u8,
    b: *mut u8,
    max: isize,
    used: *mut isize,
) -> isize {
    if e != 0x1c000100 || r.location != 0 {
        return 0;
    }
    let Value::Text(s) = value(s) else {
        return 0;
    };
    let utf16: Vec<_> = s.encode_utf16().take(r.length as usize).collect();
    let s = String::from_utf16_lossy(&utf16);
    let bytes: Vec<u8> = s.chars().flat_map(|c| (c as u32).to_le_bytes()).collect();
    if bytes.len() > max as usize {
        return 0;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), b, bytes.len());
    *used = bytes.len() as isize;
    r.length
}
#[no_mangle]
unsafe extern "C" fn CFNumberGetValue(v: CF, k: i32, out: *mut i64) -> u8 {
    if k != 4 {
        return 0;
    }
    if let Value::Number(n) = value(v) {
        *out = n;
        1
    } else {
        0
    }
}
#[no_mangle]
unsafe extern "C" fn CFDictionaryGetValue(d: CF, k: CF) -> CF {
    if let (Value::Dict(d), Value::Text(k)) = (value(d), value(k)) {
        d.get(&k).copied().unwrap_or(ptr::null())
    } else {
        ptr::null()
    }
}
#[no_mangle]
unsafe extern "C" fn CFDictionaryCreateMutable(_: CF, _: isize, k: CF, v: CF) -> CF {
    if k.is_null() || v.is_null() {
        return ptr::null();
    }
    alloc(Value::Dict(Default::default()))
}
#[no_mangle]
unsafe extern "C" fn CFDictionarySetValue(d: CF, k: CF, v: CF) {
    if let Value::Text(k) = value(k) {
        if let Value::Dict(ref mut map) = *(*(d as *const RefCell<Value>)).borrow_mut() {
            map.insert(k, v);
        }
    }
}
#[no_mangle]
unsafe extern "C" fn CFArrayGetCount(a: CF) -> isize {
    if let Value::Array(a) = value(a) {
        a.len() as isize
    } else {
        0
    }
}
#[no_mangle]
unsafe extern "C" fn CFArrayGetValueAtIndex(a: CF, i: isize) -> CF {
    if let Value::Array(a) = value(a) {
        a[i as usize]
    } else {
        ptr::null()
    }
}
#[no_mangle]
unsafe extern "C" fn IOServiceMatching(n: *const c_char) -> CF {
    if CStr::from_ptr(n).to_bytes() != b"IOHIDDevice" {
        return ptr::null();
    }
    alloc(Value::Dict(Default::default()))
}
#[no_mangle]
unsafe extern "C" fn IOServiceGetMatchingServices(_: u32, m: CF, i: *mut u32) -> i32 {
    let key = alloc(txt("IOPropertyMatch"));
    let props = CFDictionaryGetValue(m, key);
    let key = alloc(txt("Transport"));
    let ok = !props.is_null() && {
        let t = CFDictionaryGetValue(props, key);
        !t.is_null() && matches!(value(t),Value::Text(s) if s=="USB")
    };
    OS.with(|s| s.borrow_mut().valid_filter = ok);
    if !ok {
        return -1;
    }
    // Apple xnu: no matching services => kIOReturnSuccess + null iterator.
    // Cases 11/12 are genuine enumeration failures, never empty snapshots.
    let case = OS.with(|s| s.borrow().case);
    *i = if [10, 11].contains(&case) { 0 } else { 100 };
    if [11, 12].contains(&case) {
        -1
    } else {
        0
    }
}
#[no_mangle]
extern "C" fn IOIteratorNext(_: u32) -> u32 {
    OS.with(|s| {
        let mut s = s.borrow_mut();
        let entries: &[u32] = match s.case {
            9 => &[],
            13 => &[12, 10],
            _ => &[10],
        };
        let entry = entries.get(s.next).copied().unwrap_or(0);
        if entry != 0 {
            s.next += 1;
        }
        entry
    })
}
#[no_mangle]
extern "C" fn IOIteratorIsValid(_: u32) -> u32 {
    OS.with(|s| u32::from(s.borrow().case != 6))
}
#[no_mangle]
extern "C" fn IOObjectRelease(_: u32) -> i32 {
    0
}
#[no_mangle]
unsafe extern "C" fn IOObjectConformsTo(o: u32, n: *const c_char) -> u32 {
    let n = CStr::from_ptr(n).to_bytes();
    OS.with(|s| {
        let s = s.borrow();
        u32::from(
            (o == 11 && s.case != 3 && n == b"IOUSBHostInterface")
                || (o == 11 && s.case == 3 && n == b"IOBluetoothDevice"),
        )
    })
}
#[no_mangle]
unsafe extern "C" fn IORegistryEntryCreateCFProperties(o: u32, p: *mut CF, _: CF, _: u32) -> i32 {
    OS.with(|s| {
        let mut s = s.borrow_mut();
        s.inspected.push(o);
        if s.case == 4 {
            return -1;
        }
        *p = match o {
            10 => s.properties,
            12 => s.non_vial,
            _ => s.parent,
        };
        0
    })
}
#[no_mangle]
unsafe extern "C" fn IORegistryEntryGetRegistryEntryID(_: u32, id: *mut u64) -> i32 {
    *id = 4294968123;
    0
}
#[no_mangle]
unsafe extern "C" fn IORegistryEntryGetParentEntry(o: u32, _: *const c_char, p: *mut u32) -> i32 {
    OS.with(|s| {
        if o == 12 || s.borrow().case == 5 {
            -1
        } else {
            *p = 11;
            0
        }
    })
}

/// Failures: bad caps, bad transport, BT ancestor, failed properties,
/// failed parent, invalid iterator, missing optional serial, malformed identity.
pub(crate) fn snapshot(case: u8) -> Result<Vec<Device>, String> {
    OS.with(|s| *s.borrow_mut() = State::default());
    let pair = dict(vec![
        ("DeviceUsagePage", num(0xff60)),
        ("DeviceUsage", num(0x61)),
    ]);
    let mut items = vec![
        (
            "Transport",
            txt(if case == 2 { "Bluetooth" } else { "USB" }),
        ),
        ("PrimaryUsagePage", num(1)),
        ("PrimaryUsage", num(6)),
        ("DeviceUsagePairs", Value::Array(vec![pair])),
        ("VendorID", num(0xe126)),
        ("ProductID", num(9)),
        ("Product", if case == 8 { num(1) } else { txt("Macropad") }),
        ("Manufacturer", txt("Ergohaven")),
        ("MaxInputReportSize", num(if case == 1 { 0 } else { 32 })),
        ("MaxOutputReportSize", num(32)),
    ];
    if case != 7 {
        items.push(("SerialNumber", txt("native-serial")));
    }
    let p = dict(items);
    let parent = dict(vec![]);
    // Apple service observed ahead of the Phenom on macOS: USB by its
    // Transport property, non-Vial, and without a valid USB ancestry chain.
    let non_vial = dict(vec![
        ("Transport", txt("USB")),
        ("PrimaryUsagePage", num(0xff00)),
        ("PrimaryUsage", num(0x0f)),
        ("VendorID", num(0x05ac)),
        ("ProductID", num(0)),
    ]);
    OS.with(|s| {
        let mut s = s.borrow_mut();
        s.properties = p;
        s.parent = parent;
        s.non_vial = non_vial;
        s.case = case;
    });
    let result = super::scan();
    OS.with(|s| {
        let s = s.borrow();
        assert!(s.valid_filter, "kernel-side USB property filter required");
        assert!(
            s.inspected.iter().all(|n| [10, 11].contains(n) || (case == 13 && *n == 12)),
            "must not inspect any BT service"
        );
    });
    result
}
#[test]
fn macos_registry_production_boundary_uses_usb_filter_and_complete_metadata() {
    let d = snapshot(0).unwrap().remove(0);
    assert_eq!(d.path, "DevSrvsID:4294968123");
    assert_eq!(d.serial_number, "native-serial");
    for case in 1..=6 {
        assert!(snapshot(case).is_err(), "case {case}");
    }
    assert_eq!(snapshot(7).unwrap()[0].serial_number, "");
    assert!(snapshot(8).is_err());
}

#[test]
fn macos_registry_skips_non_vial_service_with_broken_usb_ancestry() {
    let devices = snapshot(13).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].path, "DevSrvsID:4294968123");
    // A Vial candidate with an ancestry failure must still fail closed.
    assert!(snapshot(5).unwrap_err().contains("USB ancestry"));
}
