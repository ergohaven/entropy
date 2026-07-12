#[cfg(not(target_arch = "wasm32"))]
const VISIBLE_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
const HIDDEN_TO_TRAY_REPAINT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const BLUETOOTH_VISIBLE_REPAINT_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(16);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const CONNECT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
#[cfg(not(target_arch = "wasm32"))]
pub(super) const UPDATE_CHECK_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(100);

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn native_repaint_interval(
    hidden_to_tray: bool,
    high_frequency_bluetooth: bool,
    connect_pending: bool,
    update_check_pending: bool,
) -> std::time::Duration {
    let baseline = if hidden_to_tray {
        HIDDEN_TO_TRAY_REPAINT_INTERVAL
    } else if high_frequency_bluetooth {
        BLUETOOTH_VISIBLE_REPAINT_INTERVAL
    } else {
        VISIBLE_REPAINT_INTERVAL
    };

    let connect_interval = connect_pending.then_some(CONNECT_POLL_INTERVAL);
    let update_check_interval = update_check_pending.then_some(UPDATE_CHECK_POLL_INTERVAL);

    [Some(baseline), connect_interval, update_check_interval]
        .into_iter()
        .flatten()
        .min()
        .expect("baseline repaint interval is always present")
}

#[cfg(any(target_os = "windows", all(test, not(target_arch = "wasm32"))))]
fn repaint_interval_update(
    current: std::time::Duration,
    next: std::time::Duration,
) -> Option<std::time::Duration> {
    if current == next {
        None
    } else {
        Some(next)
    }
}

#[cfg(any(target_os = "windows", all(test, not(target_arch = "wasm32"))))]
enum RepaintSchedulerCommand {
    SetInterval(std::time::Duration),
    Stop,
}

#[cfg(target_os = "windows")]
// Delaying on this worker keeps eframe in ControlFlow::Wait while idle. Calling
// request_repaint_after from the UI thread registers WaitUntil, which can spin
// for a hidden Win32 window (issue #59).
pub(crate) struct WindowsRepaintScheduler {
    command_sender: std::sync::mpsc::Sender<RepaintSchedulerCommand>,
    worker: Option<std::thread::JoinHandle<()>>,
    interval: std::time::Duration,
}

#[cfg(target_os = "windows")]
impl WindowsRepaintScheduler {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let interval = native_repaint_interval(false, false, false, false);
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let worker = std::thread::Builder::new()
            .name("entropy-repaint-scheduler".to_owned())
            .spawn(move || {
                run_repaint_scheduler(command_receiver, interval, move || ctx.request_repaint())
            })
            .expect("failed to start native repaint scheduler");

        Self {
            command_sender,
            worker: Some(worker),
            interval,
        }
    }

    pub(super) fn set_schedule(
        &mut self,
        hidden_to_tray: bool,
        connect_pending: bool,
        update_check_pending: bool,
    ) {
        let interval =
            native_repaint_interval(hidden_to_tray, false, connect_pending, update_check_pending);
        let Some(interval) = repaint_interval_update(self.interval, interval) else {
            return;
        };
        self.interval = interval;
        let _ = self
            .command_sender
            .send(RepaintSchedulerCommand::SetInterval(interval));
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsRepaintScheduler {
    fn drop(&mut self) {
        let _ = self.command_sender.send(RepaintSchedulerCommand::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(any(target_os = "windows", all(test, not(target_arch = "wasm32"))))]
fn run_repaint_scheduler(
    command_receiver: std::sync::mpsc::Receiver<RepaintSchedulerCommand>,
    mut interval: std::time::Duration,
    mut request_repaint: impl FnMut(),
) {
    loop {
        match command_receiver.recv_timeout(interval) {
            Ok(RepaintSchedulerCommand::SetInterval(next_interval)) => interval = next_interval,
            Ok(RepaintSchedulerCommand::Stop)
            | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => request_repaint(),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn native_repaint_cadence_uses_shortest_pending_interval() {
        assert_eq!(
            native_repaint_interval(false, false, false, false),
            std::time::Duration::from_millis(250)
        );
        assert_eq!(
            native_repaint_interval(false, true, false, false),
            BLUETOOTH_VISIBLE_REPAINT_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, true, false, false),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            native_repaint_interval(true, false, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, false, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(true, false, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, true, false),
            CONNECT_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, false, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
        assert_eq!(
            native_repaint_interval(false, false, true, true),
            UPDATE_CHECK_POLL_INTERVAL
        );
    }

    #[test]
    fn unchanged_repaint_interval_is_a_no_op() {
        let interval = std::time::Duration::from_millis(250);

        assert_eq!(repaint_interval_update(interval, interval), None);
        assert_eq!(
            repaint_interval_update(interval, std::time::Duration::from_secs(5)),
            Some(std::time::Duration::from_secs(5))
        );
    }

    #[test]
    fn scheduler_requests_repaint_after_timeout() {
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let (repaint_sender, repaint_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_repaint_scheduler(
                command_receiver,
                std::time::Duration::from_millis(10),
                move || {
                    let _ = repaint_sender.send(());
                },
            );
        });

        repaint_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("scheduler should request repaint after timeout");
        command_sender.send(RepaintSchedulerCommand::Stop).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn scheduler_interval_change_resets_deadline() {
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let (repaint_sender, repaint_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            run_repaint_scheduler(
                command_receiver,
                std::time::Duration::from_secs(5),
                move || {
                    let _ = repaint_sender.send(());
                },
            );
        });

        command_sender
            .send(RepaintSchedulerCommand::SetInterval(
                std::time::Duration::from_millis(10),
            ))
            .unwrap();
        repaint_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("new interval should replace existing deadline");
        command_sender.send(RepaintSchedulerCommand::Stop).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn scheduler_stops_on_command_and_disconnect() {
        for disconnect in [false, true] {
            let (command_sender, command_receiver) = std::sync::mpsc::channel();
            let (done_sender, done_receiver) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || {
                run_repaint_scheduler(command_receiver, std::time::Duration::from_secs(5), || {});
                let _ = done_sender.send(());
            });

            if disconnect {
                drop(command_sender);
            } else {
                command_sender.send(RepaintSchedulerCommand::Stop).unwrap();
            }
            done_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("scheduler should stop without waiting for timeout");
            worker.join().unwrap();
        }
    }
}
