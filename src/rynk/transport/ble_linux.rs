//! Rynk over BLE on Linux: the board's own GATT service, through BlueZ.
//!
//! Requests go to the output characteristic in chunks the link's MTU takes;
//! replies and topics come back as notifications on the input one. Both carry
//! the byte stream of COBS frames, so chunks need no framing of their own.
//! The board must be paired: the characteristics want an encrypted link.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_lite::{future, StreamExt};
use rmk_types_rynk::protocol::rynk::{
    RYNK_BLE_CHUNK_SIZE, RYNK_INPUT_CHAR_UUID, RYNK_OUTPUT_CHAR_UUID, RYNK_SERVICE_UUID,
};
use zbus::blocking::Connection;
use zbus::fdo::ManagedObjects;
use zbus::zvariant::Value;

use super::super::client::RynkTransport;
use super::super::link::PhysicalId;
use super::FoundBoard;
use crate::linux_ble::{
    bluez_connection, characteristic_proxy, interface_properties, managed_objects,
    parse_bluez_modalias, property_bool, property_path, property_string, property_strings,
    value_bytes, BLUEZ_DESTINATION, CHARACTERISTIC_INTERFACE, DEVICE_INTERFACE, SERVICE_INTERFACE,
};

const PREFIX: &str = "ble:";
/// ATT's default MTU, for a BlueZ that does not report the link's.
const DEFAULT_MTU: u16 = 23;
/// How long the listener may take to subscribe.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a board that just connected may take to show its services.
const SERVICES_TIMEOUT: Duration = Duration::from_secs(10);

/// A UUID as BlueZ spells it.
fn uuid(value: u128) -> String {
    let hex = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// A device BlueZ knows, as far as finding Rynk boards needs.
#[derive(Clone, Debug, Default)]
struct BluezDevice {
    path: String,
    address: String,
    name: String,
    modalias: String,
    paired: bool,
    connected: bool,
    services_resolved: bool,
    uuids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct BluezService {
    path: String,
    device: String,
    uuid: String,
}

#[derive(Clone, Debug, Default)]
struct BluezCharacteristic {
    path: String,
    service: String,
    uuid: String,
    flags: Vec<String>,
    mtu: Option<u16>,
}

fn devices(objects: &ManagedObjects) -> Vec<BluezDevice> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interface_properties(interfaces, DEVICE_INTERFACE)?;
            Some(BluezDevice {
                path: path.to_string(),
                address: property_string(properties, "Address")?,
                name: property_string(properties, "Alias")
                    .or_else(|| property_string(properties, "Name"))
                    .unwrap_or_default(),
                modalias: property_string(properties, "Modalias").unwrap_or_default(),
                paired: property_bool(properties, "Paired").unwrap_or(false),
                connected: property_bool(properties, "Connected").unwrap_or(false),
                services_resolved: property_bool(properties, "ServicesResolved").unwrap_or(false),
                uuids: property_strings(properties, "UUIDs"),
            })
        })
        .collect()
}

fn services(objects: &ManagedObjects) -> Vec<BluezService> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interface_properties(interfaces, SERVICE_INTERFACE)?;
            Some(BluezService {
                path: path.to_string(),
                device: property_path(properties, "Device")?,
                uuid: property_string(properties, "UUID")?,
            })
        })
        .collect()
}

fn characteristics(objects: &ManagedObjects) -> Vec<BluezCharacteristic> {
    objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interface_properties(interfaces, CHARACTERISTIC_INTERFACE)?;
            Some(BluezCharacteristic {
                path: path.to_string(),
                service: property_path(properties, "Service")?,
                uuid: property_string(properties, "UUID")?,
                flags: property_strings(properties, "Flags"),
                mtu: properties
                    .get("MTU")
                    .and_then(|value| u16::try_from(value).ok()),
            })
        })
        .collect()
}

/// Paired boards BlueZ is connected to that list the Rynk service. BlueZ keeps
/// a bonded device's services while it is off or away, and connects to one
/// that is on by itself, so only a connected board can be opened; a board
/// still resolving its services is listed, as opening waits for them.
fn rynk_boards(devices: &[BluezDevice]) -> Vec<FoundBoard> {
    let service = uuid(RYNK_SERVICE_UUID);
    devices
        .iter()
        .filter(|device| {
            device.paired
                && device.connected
                && device
                    .uuids
                    .iter()
                    .any(|uuid| uuid.eq_ignore_ascii_case(&service))
        })
        .map(|device| {
            let (vendor_id, product_id) = parse_bluez_modalias(&device.modalias);
            FoundBoard {
                id: PhysicalId(format!("{PREFIX}{}", device.address)),
                vendor_id,
                product_id,
                manufacturer: None,
                product: Some(device.name.clone()),
                // The address is what tells a Bluetooth board apart, as for Vial.
                serial: Some(device.address.clone()),
                bluetooth: true,
            }
        })
        .collect()
}

/// Where a connected board takes requests and sends replies.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Endpoints {
    input: String,
    output: String,
    write_type: &'static str,
    chunk: usize,
}

fn endpoints(
    device: &str,
    services: &[BluezService],
    characteristics: &[BluezCharacteristic],
) -> Option<Endpoints> {
    let service = uuid(RYNK_SERVICE_UUID);
    let rynk = services
        .iter()
        .find(|found| found.device == device && found.uuid.eq_ignore_ascii_case(&service))?;
    let of_rynk = |id: u128| {
        let wanted = uuid(id);
        characteristics.iter().find(move |found| {
            found.service == rynk.path && found.uuid.eq_ignore_ascii_case(&wanted)
        })
    };
    let input = of_rynk(RYNK_INPUT_CHAR_UUID)
        .filter(|input| input.flags.iter().any(|flag| flag == "notify"))?;
    let output = of_rynk(RYNK_OUTPUT_CHAR_UUID)?;
    // A write the board acknowledges cannot be lost under load; the board
    // offers both kinds, a write without response only as a fallback.
    let write_type = if output.flags.iter().any(|flag| flag == "write") {
        "request"
    } else if output
        .flags
        .iter()
        .any(|flag| flag == "write-without-response")
    {
        "command"
    } else {
        return None;
    };
    let mtu = output.mtu.unwrap_or(DEFAULT_MTU) as usize;
    Some(Endpoints {
        input: input.path.clone(),
        output: output.path.clone(),
        write_type,
        chunk: RYNK_BLE_CHUNK_SIZE.min(mtu.saturating_sub(3)).max(1),
    })
}

fn io_error(error: anyhow::Error) -> io::Error {
    io::Error::other(format!("{error:#}"))
}

fn gone() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "the board is disconnected")
}

/// One system bus connection for every scan, as the Vial scan keeps one.
fn scan_connection() -> io::Result<Connection> {
    static CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
    let mut connection = CONNECTION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if connection.is_none() {
        *connection = Some(bluez_connection().map_err(io_error)?);
    }
    Ok(connection.clone().expect("connected above"))
}

pub(crate) fn owns(id: &PhysicalId) -> bool {
    id.0.starts_with(PREFIX)
}

pub(crate) fn scan() -> io::Result<Vec<FoundBoard>> {
    let objects = managed_objects(&scan_connection()?).map_err(io_error)?;
    Ok(rynk_boards(&devices(&objects)))
}

/// Whether BlueZ is connected to the board now. Asking pages nothing, while
/// opening a bonded board that is away would.
pub(crate) fn connected(id: &PhysicalId) -> io::Result<bool> {
    let objects = managed_objects(&scan_connection()?).map_err(io_error)?;
    Ok(is_connected(&devices(&objects), id))
}

fn is_connected(devices: &[BluezDevice], id: &PhysicalId) -> bool {
    let address = id.0.strip_prefix(PREFIX).unwrap_or_default();
    devices
        .iter()
        .any(|device| device.connected && device.address.eq_ignore_ascii_case(address))
}

/// What opening a paired board takes, from what BlueZ says of it now.
#[derive(Debug, PartialEq, Eq)]
enum Opening {
    /// BlueZ is not connected to it. It is not paged: a bonded board that is
    /// on connects by itself, and one that is off or away only times out.
    Away,
    /// Just connected, its services not resolved yet.
    AwaitServices,
    Ready,
}

fn opening(device: &BluezDevice) -> Opening {
    match (device.connected, device.services_resolved) {
        (false, _) => Opening::Away,
        (true, false) => Opening::AwaitServices,
        (true, true) => Opening::Ready,
    }
}

fn not_connected(device: &BluezDevice) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        super::NotConnected {
            board: device.name.clone(),
        },
    )
}

/// Waits for BlueZ to resolve the services of a board it just connected to.
fn await_services(connection: &Connection, device: &BluezDevice) -> io::Result<()> {
    let proxy = zbus::blocking::Proxy::new(
        connection,
        BLUEZ_DESTINATION,
        device.path.as_str(),
        DEVICE_INTERFACE,
    )
    .map_err(|error| io_error(error.into()))?;
    let deadline = Instant::now() + SERVICES_TIMEOUT;
    loop {
        if !proxy.get_property::<bool>("Connected").unwrap_or(false) {
            return Err(not_connected(device));
        }
        if proxy
            .get_property::<bool>("ServicesResolved")
            .unwrap_or(false)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "BlueZ did not resolve the board's services",
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub(crate) fn open(id: &PhysicalId) -> io::Result<BleTransport> {
    let address = id.0.strip_prefix(PREFIX).unwrap_or_default();
    let connection = bluez_connection().map_err(io_error)?;
    let device = devices(&managed_objects(&connection).map_err(io_error)?)
        .into_iter()
        .find(|device| device.paired && device.address.eq_ignore_ascii_case(address))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not paired over Bluetooth", id.0),
            )
        })?;
    match opening(&device) {
        Opening::Away => return Err(not_connected(&device)),
        Opening::AwaitServices => await_services(&connection, &device)?,
        Opening::Ready => {}
    }
    // The services show once the connection resolved them.
    let objects = managed_objects(&connection).map_err(io_error)?;
    let endpoints = endpoints(
        &device.path,
        &services(&objects),
        &characteristics(&objects),
    )
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} has no usable Rynk service", id.0),
        )
    })?;
    let (sender, events) = mpsc::channel();
    let listener = Listener::spawn(&device.path, &endpoints.input, sender).map_err(io_error)?;
    Ok(BleTransport {
        connection,
        endpoints,
        inbox: Inbox::new(events),
        _listener: listener,
    })
}

enum Event {
    Data(Vec<u8>),
    Gone,
}

/// Notified chunks waiting to be read, in order.
struct Inbox {
    events: mpsc::Receiver<Event>,
    pending: VecDeque<u8>,
    gone: bool,
}

impl Inbox {
    fn new(events: mpsc::Receiver<Event>) -> Self {
        Self {
            events,
            pending: VecDeque::new(),
            gone: false,
        }
    }

    fn take(&mut self, event: Event) {
        match event {
            Event::Data(bytes) => self.pending.extend(bytes),
            Event::Gone => self.gone = true,
        }
    }

    /// Bytes that arrived before the board went away are still read.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        if self.pending.is_empty() && !self.gone {
            match self.events.recv_timeout(timeout) {
                Ok(event) => self.take(event),
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(0),
                Err(mpsc::RecvTimeoutError::Disconnected) => self.gone = true,
            }
        }
        while let Ok(event) = self.events.try_recv() {
            self.take(event);
        }
        if self.pending.is_empty() {
            return if self.gone { Err(gone()) } else { Ok(0) };
        }
        let n = buf.len().min(self.pending.len());
        for (slot, byte) in buf.iter_mut().zip(self.pending.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

/// Forwards the input characteristic's notifications and the device's
/// disconnect until dropped.
struct Listener {
    connection: Connection,
    input: String,
    stop: async_channel::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Listener {
    fn spawn(device: &str, input: &str, events: mpsc::Sender<Event>) -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop, stopped) = async_channel::bounded(1);
        let (device_path, input_path) = (device.to_owned(), input.to_owned());
        let thread = std::thread::Builder::new()
            .name("entropy-rynk-ble".to_owned())
            .spawn(move || {
                let listened = future::block_on(async {
                    let connection = bluez_connection()?;
                    let changes = |path: String| {
                        let connection = connection.inner().clone();
                        async move {
                            zbus::fdo::PropertiesProxy::builder(&connection)
                                .destination(BLUEZ_DESTINATION)?
                                .path(path)?
                                .build()
                                .await?
                                .receive_properties_changed()
                                .await
                        }
                    };
                    let mut values = changes(input_path.clone())
                        .await
                        .context("Failed to listen for Rynk notifications")?;
                    let mut device_changes = changes(device_path)
                        .await
                        .context("Failed to watch the Bluetooth connection")?;
                    zbus::Proxy::new(
                        connection.inner(),
                        BLUEZ_DESTINATION,
                        input_path.as_str(),
                        CHARACTERISTIC_INTERFACE,
                    )
                    .await?
                    .call::<_, _, ()>("StartNotify", &())
                    .await
                    .context("Failed to subscribe to Rynk notifications")?;
                    let _ = ready_tx.send(Ok(connection.clone()));
                    loop {
                        let stop = async {
                            let _ = stopped.recv().await;
                            None
                        };
                        let value = async { values.next().await.map(Ok) };
                        let device = async { device_changes.next().await.map(Err) };
                        let Some(change) = future::race(stop, future::race(value, device)).await
                        else {
                            return anyhow::Ok(());
                        };
                        let (args, from_device) = match &change {
                            Ok(change) => (change.args()?, false),
                            Err(change) => (change.args()?, true),
                        };
                        let changed = args.changed_properties();
                        let event = if from_device {
                            match changed.get("Connected").map(bool::try_from) {
                                Some(Ok(false)) => Event::Gone,
                                _ => continue,
                            }
                        } else {
                            match changed.get("Value").and_then(value_bytes) {
                                Some(bytes) => Event::Data(bytes),
                                None => continue,
                            }
                        };
                        let gone = matches!(event, Event::Gone);
                        if events.send(event).is_err() || gone {
                            return anyhow::Ok(());
                        }
                    }
                });
                if let Err(error) = listened {
                    let _ = ready_tx.send(Err(format!("{error:#}")));
                    let _ = events.send(Event::Gone);
                    log::debug!("Rynk BLE listener stopped: {error:#}");
                }
            })
            .context("Failed to start the Rynk BLE listener")?;
        let connection = ready_rx
            .recv_timeout(LISTEN_TIMEOUT)
            .context("The Rynk BLE listener did not start")?
            .map_err(anyhow::Error::msg)?;
        Ok(Self {
            connection,
            input: input.to_owned(),
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        if let Ok(input) = characteristic_proxy(&self.connection, &self.input) {
            let _ = input.call::<_, _, ()>("StopNotify", &());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A connected board's Rynk service. Dropping it stops the notifications;
/// the Bluetooth connection itself stays, the keyboard types over it.
pub(crate) struct BleTransport {
    connection: Connection,
    endpoints: Endpoints,
    inbox: Inbox,
    _listener: Listener,
}

impl RynkTransport for BleTransport {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.write(frame)
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        self.inbox.recv(buf, timeout)
    }

    /// The board's BLE writes land in a pipe it reads while it answers.
    fn pipelines(&self) -> bool {
        true
    }

    /// The frames in as few writes as the MTU allows: each write waits for
    /// the board's acknowledgement, and the stream needs no boundaries.
    fn send_all(&mut self, frames: &[Vec<u8>]) -> io::Result<()> {
        self.write(&frames.concat())
    }
}

impl BleTransport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.inbox.gone {
            return Err(gone());
        }
        let output =
            characteristic_proxy(&self.connection, &self.endpoints.output).map_err(io_error)?;
        let mut options = HashMap::<&str, Value<'_>>::new();
        options.insert("type", Value::from(self.endpoints.write_type));
        for chunk in bytes.chunks(self.endpoints.chunk) {
            output
                .call::<_, _, ()>("WriteValue", &(chunk, &options))
                .map_err(|error| match error {
                    zbus::Error::MethodError(name, _, _)
                        if name.as_str() == "org.bluez.Error.NotConnected" =>
                    {
                        gone()
                    }
                    other => io::Error::other(other),
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "E0:2A:4F:10:22:33";

    fn rynk_device(paired: bool) -> BluezDevice {
        BluezDevice {
            path: "/org/bluez/hci0/dev_E0_2A_4F_10_22_33".into(),
            address: ADDRESS.into(),
            name: "K:04".into(),
            modalias: "usb:vE126p0074d0001".into(),
            paired,
            connected: true,
            services_resolved: true,
            uuids: vec![
                "00001812-0000-1000-8000-00805f9b34fb".into(),
                uuid(RYNK_SERVICE_UUID).to_uppercase(),
            ],
        }
    }

    #[test]
    fn uuids_read_as_bluez_spells_them() {
        assert_eq!(
            uuid(RYNK_SERVICE_UUID),
            "10900067-537f-4f0a-9b55-929e271f61ab"
        );
    }

    #[test]
    fn only_paired_boards_with_the_rynk_service_are_found() {
        let mut vial = rynk_device(true);
        vial.address = "11:22:33:44:55:66".into();
        vial.uuids = vec!["8cfa65ff-3b6d-55f3-8b67-49693930420d".into()];
        let found = rynk_boards(&[rynk_device(true), rynk_device(false), vial]);

        assert_eq!(found.len(), 1);
        let board = &found[0];
        assert_eq!(board.id.0, format!("ble:{ADDRESS}"));
        assert!(owns(&board.id));
        assert_eq!((board.vendor_id, board.product_id), (0xE126, 0x0074));
        assert_eq!(board.serial.as_deref(), Some(ADDRESS));
        assert!(board.bluetooth);
    }

    #[test]
    fn a_bonded_board_is_listed_only_while_bluez_is_connected_to_it() {
        let mut board = rynk_device(true);
        let id = PhysicalId(format!("ble:{}", ADDRESS.to_lowercase()));
        assert_eq!(rynk_boards(&[board.clone()]).len(), 1);
        assert!(is_connected(&[board.clone()], &id));

        // Just switched on: connected, its services not resolved yet.
        board.services_resolved = false;
        assert_eq!(rynk_boards(&[board.clone()]).len(), 1);

        // Off or away: bonded, its services kept, not connected.
        board.connected = false;
        assert!(rynk_boards(&[board.clone()]).is_empty());
        assert!(!is_connected(&[board], &id));
    }

    #[test]
    fn opening_a_board_waits_for_its_services_but_never_pages_it() {
        let mut board = rynk_device(true);
        assert_eq!(opening(&board), Opening::Ready);
        board.services_resolved = false;
        assert_eq!(opening(&board), Opening::AwaitServices);

        // Off or away: opening fails at once, naming the board.
        board.connected = false;
        assert_eq!(opening(&board), Opening::Away);
        let away = not_connected(&board);
        assert_eq!(away.kind(), io::ErrorKind::NotConnected);
        assert_eq!(away.to_string(), "K:04 is not connected over Bluetooth");
    }

    fn gatt(
        output_flags: &[&str],
        mtu: Option<u16>,
    ) -> (Vec<BluezService>, Vec<BluezCharacteristic>) {
        let device = rynk_device(true).path;
        let services = vec![
            BluezService {
                path: format!("{device}/service0010"),
                device: device.clone(),
                uuid: "00001812-0000-1000-8000-00805f9b34fb".into(),
            },
            BluezService {
                path: format!("{device}/service0020"),
                device: device.clone(),
                uuid: uuid(RYNK_SERVICE_UUID),
            },
        ];
        let characteristic = |service: &str, id: u128, flags: &[&str], mtu| BluezCharacteristic {
            path: format!("{device}/{service}/char{id:x}"),
            service: format!("{device}/{service}"),
            uuid: uuid(id),
            flags: flags.iter().map(|flag| flag.to_string()).collect(),
            mtu,
        };
        let characteristics = vec![
            // The same UUIDs under another service are not the board's Rynk ones.
            characteristic(
                "service0010",
                RYNK_INPUT_CHAR_UUID,
                &["read", "notify"],
                None,
            ),
            characteristic(
                "service0020",
                RYNK_INPUT_CHAR_UUID,
                &["read", "notify"],
                mtu,
            ),
            characteristic("service0020", RYNK_OUTPUT_CHAR_UUID, output_flags, mtu),
        ];
        (services, characteristics)
    }

    #[test]
    fn writes_are_acknowledged_and_chunked_to_the_link() {
        let device = rynk_device(true).path;
        let (services, characteristics) =
            gatt(&["read", "write", "write-without-response"], Some(247));
        let found = endpoints(&device, &services, &characteristics).unwrap();
        assert!(found.input.contains("service0020"));
        assert!(found.output.contains("service0020"));
        assert_eq!(found.write_type, "request");
        assert_eq!(found.chunk, 244);

        let (services, characteristics) = gatt(&["write-without-response"], None);
        let found = endpoints(&device, &services, &characteristics).unwrap();
        assert_eq!(found.write_type, "command");
        assert_eq!(found.chunk, 20, "the default MTU when BlueZ reports none");

        let (services, characteristics) = gatt(&["read"], None);
        assert_eq!(endpoints(&device, &services, &characteristics), None);
        assert_eq!(
            endpoints("/org/bluez/hci0/dev_other", &services, &characteristics),
            None
        );
    }

    #[test]
    fn the_inbox_reads_chunks_as_one_stream_and_then_the_disconnect() {
        let (sender, events) = mpsc::channel();
        let mut inbox = Inbox::new(events);
        let mut buf = [0u8; 8];
        assert_eq!(inbox.recv(&mut buf, Duration::ZERO).unwrap(), 0);

        sender.send(Event::Data(vec![1, 2, 3])).unwrap();
        sender.send(Event::Data(vec![4, 5, 6, 7, 8, 9])).unwrap();
        sender.send(Event::Gone).unwrap();
        assert_eq!(inbox.recv(&mut buf, Duration::ZERO).unwrap(), 8);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(inbox.recv(&mut buf, Duration::ZERO).unwrap(), 1);
        assert_eq!(buf[0], 9);
        let error = inbox.recv(&mut buf, Duration::ZERO).unwrap_err();
        assert!(crate::hid::is_disconnect_error_message(&error.to_string()));
    }
}
