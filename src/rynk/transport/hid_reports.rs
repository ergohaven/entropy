//! Rynk over the vendor HID report (usage page 0xFF14) of a board paired over
//! Bluetooth, where Entropy reaches Bluetooth boards through the OS HID stack.
//!
//! A helper holds the endpoint, as it holds a Vial one, and moves raw reports;
//! the protocol stays with the link. Reports are 32 bytes and the last one of a
//! frame is zero-padded, which the deframer reads as empty frames.

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use rmk_types_rynk::protocol::rynk::RYNK_HID_REPORT_SIZE;

use super::super::client::RynkTransport;
use super::super::link::PhysicalId;
use crate::hid::RawReportChannel;

const PREFIX: &str = "hid:";

pub(crate) fn owns(id: &PhysicalId) -> bool {
    id.0.starts_with(PREFIX)
}

/// The link's id of the report endpoint at `path`.
#[cfg(any(not(target_os = "linux"), test))]
pub(crate) fn id(path: &str) -> PhysicalId {
    PhysicalId(format!("{PREFIX}{path}"))
}

/// Raw reports both ways: the helper's channel, or a test's.
pub(crate) trait Reports {
    fn write(&self, data: &[u8]) -> anyhow::Result<()>;
    fn read(&self, timeout: Duration) -> anyhow::Result<Vec<u8>>;
}

impl Reports for RawReportChannel {
    fn write(&self, data: &[u8]) -> anyhow::Result<()> {
        RawReportChannel::write(self, data)
    }

    fn read(&self, timeout: Duration) -> anyhow::Result<Vec<u8>> {
        RawReportChannel::read(self, timeout)
    }
}

fn io_error(error: anyhow::Error) -> io::Error {
    io::Error::other(format!("{error:#}"))
}

pub(crate) struct ReportTransport<R = RawReportChannel> {
    reports: R,
    pending: VecDeque<u8>,
}

impl<R> ReportTransport<R> {
    pub(crate) fn new(reports: R) -> Self {
        Self {
            reports,
            pending: VecDeque::new(),
        }
    }
}

impl<R: Reports> RynkTransport for ReportTransport<R> {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        for report in frame.chunks(RYNK_HID_REPORT_SIZE) {
            self.reports.write(report).map_err(io_error)?;
        }
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        if self.pending.is_empty() {
            let report = self.reports.read(timeout).map_err(io_error)?;
            self.pending.extend(report);
        }
        let n = buf.len().min(self.pending.len());
        for (slot, byte) in buf.iter_mut().zip(self.pending.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

/// Opens the endpoint `id` names, if the OS HID stack lists it now.
pub(crate) fn open(id: &PhysicalId) -> io::Result<ReportTransport> {
    let path = id.0.strip_prefix(PREFIX).unwrap_or_default();
    let device = {
        #[cfg(target_os = "macos")]
        let _hid_lock = crate::hid::macos_hid_operation_lock();
        let api = hidapi::HidApi::new().map_err(io::Error::other)?;
        let listed = api
            .device_list()
            .find(|info| crate::hid::is_rynk_report(info) && info.path().to_string_lossy() == path)
            .map(crate::hid::rynk_report_device);
        listed
    };
    let device = device.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("{} is not listed", id.0))
    })?;
    let channel = RawReportChannel::open(&device).map_err(io_error)?;
    Ok(ReportTransport::new(channel))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Reports written, and reports to read in turn.
    #[derive(Default)]
    struct Script {
        written: RefCell<Vec<Vec<u8>>>,
        to_read: RefCell<VecDeque<Vec<u8>>>,
    }

    impl Reports for Script {
        fn write(&self, data: &[u8]) -> anyhow::Result<()> {
            self.written.borrow_mut().push(data.to_vec());
            Ok(())
        }

        fn read(&self, _: Duration) -> anyhow::Result<Vec<u8>> {
            Ok(self.to_read.borrow_mut().pop_front().unwrap_or_default())
        }
    }

    #[test]
    fn a_frame_goes_out_in_reports() {
        let mut transport = ReportTransport::new(Script::default());
        transport.send(&[7; 70]).unwrap();
        let written = transport.reports.written.borrow();
        let sizes: Vec<usize> = written.iter().map(Vec::len).collect();
        assert_eq!(sizes, [32, 32, 6], "the helper pads the last one");
    }

    #[test]
    fn reports_read_as_one_stream() {
        let script = Script::default();
        script
            .to_read
            .borrow_mut()
            .extend([vec![1, 2, 3, 0, 0], vec![4]]);
        let mut transport = ReportTransport::new(script);
        let mut buf = [0u8; 4];
        assert_eq!(transport.recv(&mut buf, Duration::ZERO).unwrap(), 4);
        assert_eq!(buf, [1, 2, 3, 0]);
        assert_eq!(transport.recv(&mut buf, Duration::ZERO).unwrap(), 1);
        assert_eq!(transport.recv(&mut buf, Duration::ZERO).unwrap(), 1);
        assert_eq!(buf[0], 4);
        assert_eq!(
            transport.recv(&mut buf, Duration::ZERO).unwrap(),
            0,
            "a quiet board is a timeout"
        );
        assert!(owns(&id("/dev/hidraw7")));
    }
}
