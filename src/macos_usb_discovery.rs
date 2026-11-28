//! IORegistry metadata only: no IOHIDManager, IOHIDDeviceCreate/Open, or reports.
use super::macos_usb_metadata::{device_from_metadata, MacUsbMetadata};
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos_usb_registry_tests.rs"]
pub(crate) mod registry_tests;
use super::*;
use std::{
    ffi::{c_char, c_void, CString},
    ptr,
};
type CF = *const c_void;
#[repr(C)]
struct Range {
    location: isize,
    length: isize,
}
#[cfg_attr(target_os = "macos", link(name = "CoreFoundation", kind = "framework"))]
extern "C" {
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    fn CFRelease(v: CF);
    fn CFGetTypeID(v: CF) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFStringCreateWithCString(a: CF, s: *const c_char, encoding: u32) -> CF;
    fn CFStringGetLength(s: CF) -> isize;
    fn CFStringGetBytes(
        s: CF,
        range: Range,
        encoding: u32,
        loss: u8,
        external: u8,
        buffer: *mut u8,
        max: isize,
        used: *mut isize,
    ) -> isize;
    fn CFNumberGetValue(v: CF, kind: i32, out: *mut i64) -> u8;
    fn CFDictionaryGetValue(d: CF, key: CF) -> CF;
    fn CFDictionaryCreateMutable(a: CF, capacity: isize, keys: CF, values: CF) -> CF;
    fn CFDictionarySetValue(d: CF, k: CF, v: CF);
    fn CFArrayGetCount(a: CF) -> isize;
    fn CFArrayGetValueAtIndex(a: CF, i: isize) -> CF;
}
#[cfg_attr(target_os = "macos", link(name = "IOKit", kind = "framework"))]
extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CF;
    fn IOServiceGetMatchingServices(port: u32, matching: CF, iterator: *mut u32) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOIteratorIsValid(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOObjectConformsTo(object: u32, name: *const c_char) -> u32;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut CF,
        allocator: CF,
        options: u32,
    ) -> i32;
    fn IORegistryEntryGetRegistryEntryID(entry: u32, id: *mut u64) -> i32;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> i32;
}
struct OwnedCF(CF);
impl Drop for OwnedCF {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
struct Object(u32);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            IOObjectRelease(self.0);
        }
    }
}
fn error(what: &str) -> String {
    format!("USB IORegistry metadata incomplete: {what}")
}
unsafe fn string(s: &str) -> Result<OwnedCF, String> {
    let s = CString::new(s).unwrap();
    let p = CFStringCreateWithCString(ptr::null(), s.as_ptr(), 0x08000100);
    if p.is_null() {
        return Err(error("string allocation"));
    }
    Ok(OwnedCF(p))
}
unsafe fn properties(entry: u32) -> Result<OwnedCF, String> {
    let mut p = ptr::null();
    if IORegistryEntryCreateCFProperties(entry, &mut p, ptr::null(), 0) != 0 || p.is_null() {
        return Err(error("properties"));
    }
    Ok(OwnedCF(p))
}
unsafe fn get(d: CF, key: &str) -> Result<CF, String> {
    let k = string(key)?;
    Ok(CFDictionaryGetValue(d, k.0))
}
unsafe fn number(d: CF, key: &str) -> Result<u32, String> {
    let v = get(d, key)?;
    let mut n = 0i64;
    if v.is_null()
        || CFGetTypeID(v) != CFNumberGetTypeID()
        || CFNumberGetValue(v, 4, &mut n) == 0
        || !(0..=u32::MAX as i64).contains(&n)
    {
        return Err(error(key));
    }
    Ok(n as u32)
}
unsafe fn text(d: CF, key: &str, optional: bool, hidapi_limit: bool) -> Result<String, String> {
    let v = get(d, key)?;
    // A successful complete property dictionary distinguishes an absent optional
    // string from an unreadable snapshot. hidapi returns an empty string here.
    if v.is_null() && optional {
        return Ok(String::new());
    }
    if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
        return Err(error(key));
    }
    let len = CFStringGetLength(v);
    if len < 0 || len > 65536 {
        return Err(error(key));
    }
    let len = if hidapi_limit { len.min(255) } else { len };
    let mut bytes = vec![0u8; len as usize * 4];
    let mut used = 0;
    let copied = CFStringGetBytes(
        v,
        Range {
            location: 0,
            length: len,
        },
        0x1c000100,
        b'?',
        0,
        bytes.as_mut_ptr(),
        bytes.len() as isize,
        &mut used,
    );
    if copied != len || used < 0 || used as usize > bytes.len() || used % 4 != 0 {
        return Err(error(key));
    }
    let mut out = String::new();
    for chunk in bytes[..used as usize].chunks_exact(4) {
        let ch = char::from_u32(u32::from_le_bytes(chunk.try_into().unwrap()))
            .ok_or_else(|| error(key))?;
        if ch == '\0' {
            break;
        }
        out.push(ch);
    }
    Ok(out)
}
unsafe fn conforms(entry: u32, name: &[u8]) -> bool {
    IOObjectConformsTo(entry, name.as_ptr().cast()) != 0
}
unsafe fn usb_ancestry(entry: u32) -> Result<bool, String> {
    let mut current = entry;
    let mut owned: Option<Object> = None;
    for _ in 0..64 {
        // A USB Bluetooth adapter must never qualify its peripheral as USB.
        if conforms(current, b"IOBluetoothHIDDriver\0")
            || conforms(current, b"IOBluetoothDevice\0")
            || conforms(current, b"IOBluetoothHostController\0")
        {
            return Ok(false);
        }
        let props = properties(current)?;
        let transport = text(props.0, "Transport", true, false)?;
        if transport.starts_with("Bluetooth") {
            return Ok(false);
        }
        if conforms(current, b"IOUSBHostInterface\0") || conforms(current, b"IOUSBInterface\0") {
            return Ok(true);
        }
        if conforms(current, b"IORegistryRoot\0") {
            return Ok(false);
        }
        let mut parent = 0;
        if IORegistryEntryGetParentEntry(current, b"IOService\0".as_ptr().cast(), &mut parent) != 0
            || parent == 0
        {
            return Err(error("USB ancestry"));
        }
        owned = Some(Object(parent));
        current = owned.as_ref().unwrap().0;
    }
    drop(owned);
    Err(error("ancestry depth"))
}
unsafe fn usages(p: CF) -> Result<Vec<(u32, u32)>, String> {
    let mut result = vec![(number(p, "PrimaryUsagePage")?, number(p, "PrimaryUsage")?)];
    let pairs = get(p, "DeviceUsagePairs")?;
    if !pairs.is_null() {
        if CFGetTypeID(pairs) != CFArrayGetTypeID() {
            return Err(error("DeviceUsagePairs"));
        }
        for i in 0..CFArrayGetCount(pairs) {
            let pair = CFArrayGetValueAtIndex(pairs, i);
            if pair.is_null() || CFGetTypeID(pair) != CFDictionaryGetTypeID() {
                return Err(error("usage pair"));
            }
            result.push((
                number(pair, "DeviceUsagePage")?,
                number(pair, "DeviceUsage")?,
            ));
        }
    }
    Ok(result)
}
unsafe fn read(entry: u32) -> Result<Option<Device>, String> {
    let p = properties(entry)?;
    let transport = text(p.0, "Transport", false, false)?;
    if transport != "USB" {
        return Err(error("USB matching transport"));
    }
    let usages = usages(p.0)?;
    if !usages.contains(&(0xff60, 0x61)) {
        // Some non-Vial services advertise USB without a USB interface ancestor.
        // They must not prevent discovery of a genuine Vial endpoint.
        return Ok(None);
    }
    if !usb_ancestry(entry)? {
        return Err(error("USB ancestry mismatch"));
    }
    let mut entry_id = 0;
    if IORegistryEntryGetRegistryEntryID(entry, &mut entry_id) != 0 {
        return Err(error("service ID"));
    }
    let vendor = u16::try_from(number(p.0, "VendorID")?).map_err(|_| error("VendorID"))?;
    let product = u16::try_from(number(p.0, "ProductID")?).map_err(|_| error("ProductID"))?;
    device_from_metadata(MacUsbMetadata {
        entry_id,
        transport,
        usb_ancestor: true,
        bluetooth_ancestor: false,
        vendor,
        product,
        name: text(p.0, "Product", true, true)?,
        manufacturer: text(p.0, "Manufacturer", true, true)?,
        serial: text(p.0, "SerialNumber", true, true)?,
        usages,
        input_bytes: number(p.0, "MaxInputReportSize")?,
        output_bytes: number(p.0, "MaxOutputReportSize")?,
    })
}
pub(super) fn scan() -> Result<Vec<Device>, String> {
    unsafe {
        // Kernel-side matching only USB IOHIDDevice services. Never enumerate
        // Bluetooth services or create a global IOHID manager/run-loop source.
        let matching = IOServiceMatching(b"IOHIDDevice\0".as_ptr().cast());
        if matching.is_null() {
            return Err(error("matching allocation"));
        }
        let matching = OwnedCF(matching);
        // CF callbacks give string-value key equality (not pointer equality)
        // and retain nested match data until IOKit consumes the dictionary.
        let props = CFDictionaryCreateMutable(
            ptr::null(),
            0,
            ptr::addr_of!(kCFTypeDictionaryKeyCallBacks).cast(),
            ptr::addr_of!(kCFTypeDictionaryValueCallBacks).cast(),
        );
        if props.is_null() {
            return Err(error("property match allocation"));
        }
        let props = OwnedCF(props);
        let key = string("Transport")?;
        let value = string("USB")?;
        let property_match = string("IOPropertyMatch")?;
        CFDictionarySetValue(props.0, key.0, value.0);
        CFDictionarySetValue(matching.0, property_match.0, props.0);
        let mut iterator = 0;
        let matching_ptr = matching.0;
        std::mem::forget(matching); // consumed on success or failure
        if IOServiceGetMatchingServices(0, matching_ptr, &mut iterator) != 0 {
            return Err(error("service enumeration"));
        }
        // IOKit returns success with a null iterator when nothing matches
        // (xnu internal_io_service_get_matching_services / copyExistingServices).
        // Only a successful empty match proves USB absence; errors stay fail-closed.
        if iterator == 0 {
            return Ok(Vec::new());
        }
        let iterator = Object(iterator);
        let mut result = Vec::new();
        loop {
            let entry = IOIteratorNext(iterator.0);
            if entry == 0 {
                break;
            }
            let entry = Object(entry);
            if let Some(device) = read(entry.0)? {
                result.push(device);
            }
        }
        if IOIteratorIsValid(iterator.0) == 0 {
            return Err(error("invalidated iterator"));
        }
        Ok(result)
    }
}
