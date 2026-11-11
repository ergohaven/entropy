// Included in device::tests; portable, no HID access or persistence.
#[test]
fn sm62_prefixed_hardware_serial_has_the_same_strict_half_key() {
    let mut device = test_device("Usb", "identity-fixture");
    device.vendor_id = ERGOHAVEN_VENDOR_ID;
    device.product_id = SM62_PRODUCT_ID;
    device.firmware = FirmwareProtocol::Vial;
    device.serial_number = "0123456789ABCDEF0000000000000000".into();
    let bare = device.sm62_host_rule_key().expect("valid RP2040 ID");
    device.serial_number = "vial:f64c2b3c:0123456789abcdef0000000000000000".into();
    assert_eq!(device.sm62_host_rule_key(), Some(bare));
    for serial in ["", "00000000000000000000000000000000",
        "FFFFFFFFFFFFFFFF0000000000000000", "vial:f64c2b3c", "vial:f64c2b3c:",
        "vial:f64c2b3c:00000000000000000000000000000000",
        "vial:f64c2b3c:FFFFFFFFFFFFFFFF0000000000000000",
        "vial:f64c2b3c:0123456789ABCDEF0000000000000001",
        "vial:wrong:0123456789ABCDEF0000000000000000",
        "xvial:f64c2b3c:0123456789ABCDEF0000000000000000",
        "vial:f64c2b3c:vial:f64c2b3c:0123456789ABCDEF0000000000000000",
        "vial:f64c2b3c:0123456789ABCDEG0000000000000000",
        "vial:f64c2b3c:0123456789ABCDEF000000000000000",
        "vial:f64c2b3c:0123456789ABCDEF00000000000000000",
        "vial:f64c2b3c:é123456789ABCDEF0000000000000000"] {
        device.serial_number = serial.into();
        assert_eq!(device.sm62_host_rule_key(), None, "{serial}");
    }
}
