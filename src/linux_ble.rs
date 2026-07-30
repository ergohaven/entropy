use crate::device::Device;
use crate::firmware::FirmwareProtocol;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use zbus::blocking::connection::Builder as ConnectionBuilder;
use zbus::blocking::fdo::{ObjectManagerProxy, PropertiesProxy};
use zbus::blocking::{Connection, Proxy};
use zbus::fdo::ManagedObjects;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const BLUEZ_DESTINATION: &str = "org.bluez";
const BLUEZ_GATT_PREFIX: &str = "bluez-gatt:";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";
const SERVICE_INTERFACE: &str = "org.bluez.GattService1";
const CHARACTERISTIC_INTERFACE: &str = "org.bluez.GattCharacteristic1";
const HID_SERVICE_UUID: &str = "1812";
const REPORT_CHARACTERISTIC_UUID: &str = "2a4d";
const BLUEZ_METHOD_TIMEOUT: Duration = Duration::from_secs(5);
const BLUEZ_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const BLUEZ_REPLY_TIMEOUT: Duration = Duration::from_millis(2_500);
// RMK requests a 7.5 ms connection interval after pairing. BlueZ does not
// reliably forward the input notification to every desktop stack, so the
// direct-GATT fallback also reads the characteristic. Waiting 40 ms before
// every fallback read multiplied a single BLE round trip across every startup
// and keymap request. One connection interval is enough; a response that is
// not ready yet is rejected by `response_matches` and retried safely.
const BLUEZ_REPLY_POLL_INTERVAL: Duration = Duration::from_millis(8);

#[derive(Clone, Debug, PartialEq, Eq)]
struct CharacteristicSummary {
    path: String,
    service: String,
    uuid: String,
    flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VialGattEndpoints {
    service: String,
    input: String,
    output: String,
    output_write_types: Vec<GattWriteType>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GattWriteType {
    Request,
    Command,
}

impl GattWriteType {
    fn bluez_name(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Command => "command",
        }
    }
}

#[derive(Clone, Debug)]
struct BluezDeviceSummary {
    name: String,
    address: String,
    modalias: String,
    paired: bool,
}

pub(crate) fn is_bluez_gatt_path(path: &str) -> bool {
    path.starts_with(BLUEZ_GATT_PREFIX)
}

fn bluez_service_path(path: &str) -> Option<&str> {
    path.strip_prefix(BLUEZ_GATT_PREFIX)
        .filter(|path| path.starts_with('/'))
}

fn bluez_connection() -> Result<Connection> {
    ConnectionBuilder::system()
        .context("Failed to create BlueZ D-Bus connection builder")?
        .method_timeout(BLUEZ_METHOD_TIMEOUT)
        .build()
        .context("Failed to connect to the BlueZ system bus")
}

fn managed_objects(connection: &Connection) -> Result<ManagedObjects> {
    let proxy = ObjectManagerProxy::builder(connection)
        .destination(BLUEZ_DESTINATION)
        .context("Invalid BlueZ D-Bus destination")?
        .path("/")
        .context("Invalid BlueZ object manager path")?
        .build()
        .context("Failed to create the BlueZ object manager proxy")?;
    proxy
        .get_managed_objects()
        .context("Failed to enumerate BlueZ GATT objects")
}

fn property_string(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_owned)
}

fn property_bool(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<bool> {
    properties
        .get(name)
        .and_then(|value| bool::try_from(value).ok())
}

fn property_path(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|value| <&ObjectPath<'_>>::try_from(value).ok())
        .map(ToString::to_string)
}

fn property_strings(properties: &HashMap<String, OwnedValue>, name: &str) -> Vec<String> {
    properties
        .get(name)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default()
}

fn interface_properties<'a>(
    interfaces: &'a HashMap<zbus::names::OwnedInterfaceName, HashMap<String, OwnedValue>>,
    interface: &str,
) -> Option<&'a HashMap<String, OwnedValue>> {
    interfaces
        .iter()
        .find(|(name, _)| name.as_str() == interface)
        .map(|(_, properties)| properties)
}

fn uuid_matches(value: &str, short: &str) -> bool {
    value.eq_ignore_ascii_case(short)
        || value.eq_ignore_ascii_case(&format!("0000{short}-0000-1000-8000-00805f9b34fb"))
}

fn collect_bluez_summaries(
    objects: &ManagedObjects,
) -> (
    HashMap<String, BluezDeviceSummary>,
    HashMap<String, String>,
    Vec<CharacteristicSummary>,
) {
    let mut devices = HashMap::new();
    let mut hid_services = HashMap::new();
    let mut characteristics = Vec::new();

    for (path, interfaces) in objects {
        let path = path.to_string();
        if let Some(properties) = interface_properties(interfaces, DEVICE_INTERFACE) {
            let name = property_string(properties, "Alias")
                .or_else(|| property_string(properties, "Name"))
                .unwrap_or_else(|| "RMK Keyboard".to_owned());
            devices.insert(
                path.clone(),
                BluezDeviceSummary {
                    name,
                    address: property_string(properties, "Address").unwrap_or_default(),
                    modalias: property_string(properties, "Modalias").unwrap_or_default(),
                    paired: property_bool(properties, "Paired").unwrap_or(false),
                },
            );
        }

        if let Some(properties) = interface_properties(interfaces, SERVICE_INTERFACE) {
            let uuid = property_string(properties, "UUID").unwrap_or_default();
            if uuid_matches(&uuid, HID_SERVICE_UUID) {
                if let Some(device) = property_path(properties, "Device") {
                    hid_services.insert(path.clone(), device);
                }
            }
        }

        if let Some(properties) = interface_properties(interfaces, CHARACTERISTIC_INTERFACE) {
            characteristics.push(CharacteristicSummary {
                path,
                service: property_path(properties, "Service").unwrap_or_default(),
                uuid: property_string(properties, "UUID").unwrap_or_default(),
                flags: property_strings(properties, "Flags"),
            });
        }
    }

    (devices, hid_services, characteristics)
}

fn select_vial_endpoints(
    hid_services: &HashMap<String, String>,
    characteristics: &[CharacteristicSummary],
) -> Vec<VialGattEndpoints> {
    let mut endpoints = Vec::new();
    for service in hid_services.keys() {
        let reports: Vec<&CharacteristicSummary> = characteristics
            .iter()
            .filter(|characteristic| {
                characteristic.service == *service
                    && uuid_matches(&characteristic.uuid, REPORT_CHARACTERISTIC_UUID)
            })
            .collect();

        // RMK exposes a dedicated Vial HID service with exactly one input and
        // one output report. Its normal keyboard HID service has five reports.
        if reports.len() != 2 {
            continue;
        }

        let input = reports.iter().find(|characteristic| {
            characteristic
                .flags
                .iter()
                .any(|flag| flag.eq_ignore_ascii_case("notify"))
        });
        let output = reports.iter().find(|characteristic| {
            characteristic.flags.iter().any(|flag| {
                flag.eq_ignore_ascii_case("write")
                    || flag.eq_ignore_ascii_case("write-without-response")
            })
        });
        let (Some(input), Some(output)) = (input, output) else {
            continue;
        };
        // Prefer an acknowledged ATT Write Request when available. Some BlueZ
        // and RMK combinations only authorize Write Without Response, so keep
        // every advertised mode and let the live transport remember the first
        // one that completes a Vial round trip.
        let mut output_write_types = Vec::with_capacity(2);
        if output
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("write"))
        {
            output_write_types.push(GattWriteType::Request);
        }
        if output
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("write-without-response"))
        {
            output_write_types.push(GattWriteType::Command);
        }
        endpoints.push(VialGattEndpoints {
            service: service.clone(),
            input: input.path.clone(),
            output: output.path.clone(),
            output_write_types,
        });
    }
    endpoints
}

fn parse_bluez_modalias(modalias: &str) -> (u16, u16) {
    let lower = modalias.to_ascii_lowercase();
    let Some(vendor_start) = lower.find(":v").map(|index| index + 2) else {
        return (0, 0);
    };
    let Some(product_marker) = lower[vendor_start..].find('p') else {
        return (0, 0);
    };
    let product_marker = vendor_start + product_marker;
    let vendor = lower[vendor_start..product_marker]
        .get(..4)
        .and_then(|value| u16::from_str_radix(value, 16).ok())
        .unwrap_or(0);
    let product = lower[product_marker + 1..]
        .get(..4)
        .and_then(|value| u16::from_str_radix(value, 16).ok())
        .unwrap_or(0);
    (vendor, product)
}

fn devices_from_objects(objects: &ManagedObjects) -> Vec<Device> {
    let (devices, hid_services, characteristics) = collect_bluez_summaries(objects);
    select_vial_endpoints(&hid_services, &characteristics)
        .into_iter()
        .filter_map(|endpoints| {
            let device_path = hid_services.get(&endpoints.service)?;
            let summary = devices.get(device_path)?;
            if !summary.paired {
                return None;
            }
            let (vendor_id, product_id) = parse_bluez_modalias(&summary.modalias);
            Some(Device {
                name: summary.name.clone(),
                vendor_id,
                product_id,
                manufacturer: String::new(),
                serial_number: summary.address.clone(),
                bus_type: "Bluetooth".to_owned(),
                path: format!("{BLUEZ_GATT_PREFIX}{}", endpoints.service),
                firmware: FirmwareProtocol::Vial,
            })
        })
        .collect()
}

pub(crate) fn scan_devices() -> Vec<Device> {
    let result = (|| -> Result<Vec<Device>> {
        static SCAN_CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
        let mut connection = SCAN_CONNECTION
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if connection.is_none() {
            *connection = Some(bluez_connection()?);
        }
        let active_connection = connection
            .as_ref()
            .context("BlueZ scan connection was not initialized")?;
        let objects = match managed_objects(active_connection) {
            Ok(objects) => objects,
            Err(error) => {
                *connection = None;
                return Err(error);
            }
        };
        Ok(devices_from_objects(&objects))
    })();
    match result {
        Ok(devices) => devices,
        Err(error) => {
            log::debug!("BlueZ Vial scan unavailable: {error:#}");
            Vec::new()
        }
    }
}

fn service_device_path(objects: &ManagedObjects, service: &str) -> Option<String> {
    let service_path = objects.keys().find(|path| path.as_str() == service)?;
    let properties = interface_properties(objects.get(service_path)?, SERVICE_INTERFACE)?;
    property_path(properties, "Device")
}

fn ensure_device_connected(connection: &Connection, device_path: &str) -> Result<()> {
    let proxy = Proxy::new(connection, BLUEZ_DESTINATION, device_path, DEVICE_INTERFACE)
        .context("Failed to create the BlueZ device proxy")?;
    let connected = proxy.get_property::<bool>("Connected").unwrap_or(false);
    if !connected {
        proxy
            .call::<_, _, ()>("Connect", &())
            .context("Failed to connect the Bluetooth keyboard")?;
    }

    let deadline = Instant::now() + BLUEZ_CONNECT_TIMEOUT;
    loop {
        let connected = proxy.get_property::<bool>("Connected").unwrap_or(false);
        let resolved = proxy
            .get_property::<bool>("ServicesResolved")
            .unwrap_or(false);
        if connected && resolved {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("Bluetooth keyboard connected, but its GATT services were not resolved");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn endpoints_for_service(objects: &ManagedObjects, service: &str) -> Option<VialGattEndpoints> {
    let (_, hid_services, characteristics) = collect_bluez_summaries(objects);
    select_vial_endpoints(&hid_services, &characteristics)
        .into_iter()
        .find(|endpoints| endpoints.service == service)
}

fn characteristic_proxy<'a>(connection: &'a Connection, path: &'a str) -> Result<Proxy<'a>> {
    Proxy::new(
        connection,
        BLUEZ_DESTINATION,
        path,
        CHARACTERISTIC_INTERFACE,
    )
    .context("Failed to create a BlueZ GATT characteristic proxy")
}

fn value_bytes(value: &Value<'_>) -> Option<Vec<u8>> {
    let array: &zbus::zvariant::Array<'_> = value.downcast_ref().ok()?;
    array.iter().map(|item| u8::try_from(item).ok()).collect()
}

fn spawn_notification_listener(
    input_path: String,
    notification_tx: mpsc::Sender<Vec<u8>>,
) -> Result<Connection> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("entropy-bluez-vial-notify".to_owned())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let connection = bluez_connection()?;
                let properties = PropertiesProxy::builder(&connection)
                    .destination(BLUEZ_DESTINATION)
                    .context("Invalid BlueZ D-Bus destination")?
                    .path(input_path.as_str())
                    .context("Invalid BlueZ Vial input path")?
                    .build()
                    .context("Failed to create the BlueZ properties proxy")?;
                let mut changes = properties
                    .receive_properties_changed()
                    .context("Failed to listen for BlueZ Vial notifications")?;
                characteristic_proxy(&connection, &input_path)?
                    .call::<_, _, ()>("StartNotify", &())
                    .context("Failed to subscribe to BlueZ Vial replies")?;
                ready_tx
                    .send(Ok(connection.clone()))
                    .map_err(|_| anyhow::anyhow!("BlueZ listener startup receiver disappeared"))?;
                for change in &mut changes {
                    let arguments = change
                        .args()
                        .context("Malformed BlueZ PropertiesChanged signal")?;
                    let Some(value) = arguments.changed_properties().get("Value") else {
                        continue;
                    };
                    let Some(bytes) = value_bytes(value) else {
                        continue;
                    };
                    if notification_tx.send(bytes).is_err() {
                        break;
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = ready_tx.send(Err(format!("{error:#}")));
                log::debug!("BlueZ Vial notification listener stopped: {error:#}");
            }
        })
        .context("Failed to start the BlueZ notification listener")?;

    ready_rx
        .recv_timeout(BLUEZ_METHOD_TIMEOUT)
        .context("BlueZ notification listener startup timed out")?
        .map_err(anyhow::Error::msg)
}

fn normalize_notification(bytes: &[u8]) -> Option<[u8; 32]> {
    let payload = match bytes.len() {
        32 => bytes,
        33 if bytes[0] == 0 => &bytes[1..],
        _ => return None,
    };
    let mut response = [0u8; 32];
    response.copy_from_slice(payload);
    Some(response)
}

pub(crate) struct LinuxBleDevice {
    connection: Connection,
    input_path: String,
    output_path: String,
    output_write_types: Vec<GattWriteType>,
    preferred_write_type: AtomicUsize,
    notifications: Mutex<mpsc::Receiver<Vec<u8>>>,
    listener_control: Mutex<Option<Connection>>,
    command_lock: Mutex<()>,
}

impl LinuxBleDevice {
    pub(crate) fn open(device: &Device) -> Result<Self> {
        let service = bluez_service_path(&device.path)
            .context("Invalid BlueZ Vial device path")?
            .to_owned();
        let connection = bluez_connection()?;
        let initial_objects = managed_objects(&connection)?;
        let device_path = service_device_path(&initial_objects, &service)
            .context("BlueZ Vial service no longer belongs to a Bluetooth device")?;
        ensure_device_connected(&connection, &device_path)?;

        let objects = managed_objects(&connection)?;
        let endpoints = endpoints_for_service(&objects, &service)
            .context("BlueZ Vial GATT characteristics are unavailable")?;
        log::info!(
            "BlueZ Vial endpoints: input={}, output={}, write_types={:?}",
            endpoints.input,
            endpoints.output,
            endpoints.output_write_types
        );
        let (notification_tx, notification_rx) = mpsc::channel();
        let listener_control =
            spawn_notification_listener(endpoints.input.clone(), notification_tx)?;

        Ok(Self {
            connection,
            input_path: endpoints.input,
            output_path: endpoints.output,
            output_write_types: endpoints.output_write_types,
            preferred_write_type: AtomicUsize::new(0),
            notifications: Mutex::new(notification_rx),
            listener_control: Mutex::new(Some(listener_control)),
            command_lock: Mutex::new(()),
        })
    }

    fn write_value_with_type(&self, data: &[u8], write_type: GattWriteType) -> Result<()> {
        if data.len() > 32 {
            bail!(
                "Bluetooth GATT command too long — {} bytes, max 32 bytes",
                data.len()
            );
        }

        let mut payload = [0u8; 32];
        payload[..data.len()].copy_from_slice(data);
        let mut options = HashMap::<&str, Value<'_>>::new();
        options.insert("type", Value::from(write_type.bluez_name()));
        characteristic_proxy(&self.connection, &self.output_path)?
            .call::<_, _, ()>("WriteValue", &(payload.as_slice(), options))
            .context("Failed to write the BlueZ Vial request")
    }

    pub(crate) fn write_output_report(&self, data: &[u8]) -> Result<()> {
        let _command_guard = self
            .command_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("BlueZ Vial command lock poisoned"))?;
        let preferred = self
            .preferred_write_type
            .load(Ordering::Relaxed)
            .min(self.output_write_types.len().saturating_sub(1));
        self.write_value_with_type(data, self.output_write_types[preferred])
    }

    fn send_with_write_type(
        &self,
        data: &[u8],
        notifications: &mpsc::Receiver<Vec<u8>>,
        write_type: GattWriteType,
        response_matches: &impl Fn(&[u8; 32]) -> bool,
    ) -> Result<[u8; 32]> {
        while notifications.try_recv().is_ok() {}

        self.write_value_with_type(data, write_type)?;

        let deadline = Instant::now() + BLUEZ_REPLY_TIMEOUT;
        let mut last_unrelated = None;
        let mut last_read_error = None;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match notifications.recv_timeout(remaining.min(BLUEZ_REPLY_POLL_INTERVAL)) {
                Ok(bytes) => {
                    let Some(response) = normalize_notification(&bytes) else {
                        last_unrelated = Some(format!(
                            "invalid Bluetooth Vial reply length {}",
                            bytes.len()
                        ));
                        continue;
                    };
                    if response_matches(&response) {
                        return Ok(response);
                    }
                    last_unrelated = Some(format!(
                        "stale Bluetooth Vial notification {:02X?}",
                        &response[..data.len().clamp(3, 8)]
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("Bluetooth GATT disconnected while waiting for a Vial reply")
                }
            }

            let read_options = HashMap::<&str, Value<'_>>::new();
            match characteristic_proxy(&self.connection, &self.input_path)?
                .call::<_, _, Vec<u8>>("ReadValue", &(read_options,))
            {
                Ok(bytes) => {
                    let Some(response) = normalize_notification(&bytes) else {
                        last_unrelated = Some(format!(
                            "invalid Bluetooth Vial read length {}",
                            bytes.len()
                        ));
                        continue;
                    };
                    if response_matches(&response) {
                        return Ok(response);
                    }
                    last_unrelated = Some(format!(
                        "stale Bluetooth Vial read {:02X?}",
                        &response[..data.len().clamp(3, 8)]
                    ));
                }
                Err(error) => {
                    last_read_error = Some(format!("{error:#}"));
                }
            }
        }

        bail!(
            "{}",
            last_unrelated
                .or_else(|| last_read_error.map(|error| format!(
                    "Bluetooth Vial timeout; direct reply read failed: {error}"
                )))
                .unwrap_or_else(|| {
                    "Bluetooth Vial timeout — keyboard did not respond".to_owned()
                })
        )
    }

    pub(crate) fn send(
        &self,
        data: &[u8],
        response_matches: impl Fn(&[u8; 32]) -> bool,
    ) -> Result<[u8; 32]> {
        let _command_guard = self
            .command_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("BlueZ Vial command lock poisoned"))?;
        let notifications = self
            .notifications
            .lock()
            .map_err(|_| anyhow::anyhow!("BlueZ Vial notification lock poisoned"))?;
        let preferred = self
            .preferred_write_type
            .load(Ordering::Relaxed)
            .min(self.output_write_types.len().saturating_sub(1));
        let mut errors = Vec::with_capacity(self.output_write_types.len());

        for index in std::iter::once(preferred)
            .chain((0..self.output_write_types.len()).filter(|index| *index != preferred))
        {
            let write_type = self.output_write_types[index];
            match self.send_with_write_type(data, &notifications, write_type, &response_matches) {
                Ok(response) => {
                    if index != preferred {
                        log::info!(
                            "BlueZ Vial switched to {} writes after {} did not complete",
                            write_type.bluez_name(),
                            self.output_write_types[preferred].bluez_name()
                        );
                    }
                    self.preferred_write_type.store(index, Ordering::Relaxed);
                    return Ok(response);
                }
                Err(error) => {
                    errors.push(format!("{}: {error:#}", write_type.bluez_name()));
                }
            }
        }

        bail!(
            "Bluetooth Vial request failed over all advertised write modes: {}",
            errors.join("; ")
        )
    }
}

impl Drop for LinuxBleDevice {
    fn drop(&mut self) {
        if let Ok(mut control) = self.listener_control.lock() {
            if let Some(connection) = control.take() {
                if let Ok(input) = characteristic_proxy(&connection, &self.input_path) {
                    let _ = input.call::<_, _, ()>("StopNotify", &());
                }
                let _ = connection.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn characteristic(
        path: &str,
        service: &str,
        uuid: &str,
        flags: &[&str],
    ) -> CharacteristicSummary {
        CharacteristicSummary {
            path: path.to_owned(),
            service: service.to_owned(),
            uuid: uuid.to_owned(),
            flags: flags.iter().map(|flag| (*flag).to_owned()).collect(),
        }
    }

    #[test]
    fn selects_dedicated_rmk_vial_hid_service() {
        let services = HashMap::from([
            ("/service/keyboard".to_owned(), "/device".to_owned()),
            ("/service/vial".to_owned(), "/device".to_owned()),
        ]);
        let mut characteristics = vec![
            characteristic(
                "/service/vial/input",
                "/service/vial",
                "00002a4d-0000-1000-8000-00805f9b34fb",
                &["read", "notify"],
            ),
            characteristic(
                "/service/vial/output",
                "/service/vial",
                "2a4d",
                &["read", "write", "write-without-response"],
            ),
        ];
        for index in 0..5 {
            characteristics.push(characteristic(
                &format!("/service/keyboard/report{index}"),
                "/service/keyboard",
                "2a4d",
                if index == 1 {
                    &["read", "write"]
                } else {
                    &["read", "notify"]
                },
            ));
        }

        let endpoints = select_vial_endpoints(&services, &characteristics);

        assert_eq!(
            endpoints,
            vec![VialGattEndpoints {
                service: "/service/vial".to_owned(),
                input: "/service/vial/input".to_owned(),
                output: "/service/vial/output".to_owned(),
                output_write_types: vec![GattWriteType::Request, GattWriteType::Command],
            }]
        );
    }

    #[test]
    fn uses_only_the_write_modes_advertised_by_bluez() {
        let services = HashMap::from([("/service/vial".to_owned(), "/device".to_owned())]);
        let request_characteristics = vec![
            characteristic(
                "/service/vial/input",
                "/service/vial",
                "2a4d",
                &["read", "notify"],
            ),
            characteristic(
                "/service/vial/output",
                "/service/vial",
                "2a4d",
                &["read", "write"],
            ),
        ];
        let command_characteristics = vec![
            characteristic(
                "/service/vial/input",
                "/service/vial",
                "2a4d",
                &["read", "notify"],
            ),
            characteristic(
                "/service/vial/output",
                "/service/vial",
                "2a4d",
                &["read", "write-without-response"],
            ),
        ];

        assert_eq!(
            select_vial_endpoints(&services, &request_characteristics)[0].output_write_types,
            vec![GattWriteType::Request]
        );
        assert_eq!(
            select_vial_endpoints(&services, &command_characteristics)[0].output_write_types,
            vec![GattWriteType::Command]
        );
    }

    #[test]
    fn ignores_non_rmk_hid_services() {
        let services = HashMap::from([("/service/keyboard".to_owned(), "/device".to_owned())]);
        let characteristics = vec![characteristic(
            "/service/keyboard/input",
            "/service/keyboard",
            "2a4d",
            &["read", "notify"],
        )];

        assert!(select_vial_endpoints(&services, &characteristics).is_empty());
    }

    #[test]
    fn parses_bluez_bluetooth_modalias() {
        assert_eq!(
            parse_bluez_modalias("bluetooth:vE126p0041d0100"),
            (0xE126, 0x0041)
        );
        assert_eq!(parse_bluez_modalias(""), (0, 0));
    }

    #[test]
    fn normalizes_report_id_less_bluez_notifications() {
        let response = [7u8; 32];
        assert_eq!(normalize_notification(&response), Some(response));

        let mut prefixed = [0u8; 33];
        prefixed[1..].copy_from_slice(&response);
        assert_eq!(normalize_notification(&prefixed), Some(response));
        assert_eq!(normalize_notification(&[0u8; 31]), None);
    }
}
