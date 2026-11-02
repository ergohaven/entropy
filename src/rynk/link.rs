//! One owner per Rynk board.
//!
//! The board's interface can be claimed by one handle only, so everything that
//! talks to a board shares the link of its [`PhysicalId`]: the editor session
//! and host-data bridges. A thread per link owns the client and runs one
//! request at a time: the editor first, requests nobody waits on next. The
//! last [`Arc`] dropped closes the link and releases the claim.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use rmk_types_rynk::protocol::rynk::command::Endpoint;
use rmk_types_rynk::protocol::rynk::Cmd;

use super::client::{RynkBoard, RynkClient, RynkClientError, RynkTimeouts, RynkTransport};
use super::transport;

/// How long a post may wait behind other requests before it goes first, so
/// host data (time, media, layout) never shows far behind on the board.
#[cfg(not(test))]
const POST_MAX_WAIT: Duration = Duration::from_secs(1);
#[cfg(test)]
const POST_MAX_WAIT: Duration = Duration::from_millis(150);
/// Retry budget when the interface is claimed by another handle: a short
/// session of another process, or our own previous owner still closing.
const BUSY_RETRY: Duration = Duration::from_millis(1500);
const BUSY_STEP: Duration = Duration::from_millis(100);

/// A board's identity on its bus, stable while it stays plugged in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PhysicalId(pub(crate) String);

/// Which slot a post replaces: the command plus a key chosen by the caller,
/// such as the kind of host data. Only the latest value of a slot is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PostKey {
    pub(crate) cmd: Cmd,
    pub(crate) slot: u16,
}

pub(crate) type BoxedTransport = Box<dyn RynkTransport + Send>;

impl<T: RynkTransport + ?Sized> RynkTransport for Box<T> {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        (**self).send(frame)
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        (**self).recv(buf, timeout)
    }

    fn pipelines(&self) -> bool {
        (**self).pipelines()
    }

    fn send_all(&mut self, frames: &[Vec<u8>]) -> io::Result<()> {
        (**self).send_all(frames)
    }
}

/// The client a link's owner holds, as [`RynkLink::run`] hands it out.
pub(crate) type LinkClient = RynkClient<BoxedTransport>;
type Client = LinkClient;
type Job = Box<dyn FnOnce(&mut Client) -> Result<(), RynkClientError> + Send>;

enum Event {
    Request(Job),
    Post {
        key: PostKey,
        queued: Instant,
        job: Job,
    },
}

/// The shared handle of one board's link.
pub(crate) struct RynkLink {
    id: PhysicalId,
    board: RynkBoard,
    events: mpsc::Sender<Event>,
    closed: Arc<Closed>,
}

/// Set by the owner thread once the client, and with it the claim, is gone.
#[derive(Default)]
struct Closed {
    done: Mutex<bool>,
    signal: Condvar,
}

impl Closed {
    fn is_done(&self) -> bool {
        *self.done.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mark(&self) {
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.signal.notify_all();
    }

    fn wait(&self, timeout: Duration) {
        let done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .signal
            .wait_timeout_while(done, timeout, |done| !*done)
            .unwrap_or_else(|e| e.into_inner());
    }
}

struct Registry {
    live: HashMap<PhysicalId, Weak<RynkLink>>,
    /// Owners still closing, so a reopen waits for the claim instead of
    /// failing on it.
    closing: HashMap<PhysicalId, Arc<Closed>>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            live: HashMap::new(),
            closing: HashMap::new(),
        })
    })
}

fn is_busy(error: &RynkClientError) -> bool {
    match error {
        RynkClientError::Transport(error) => {
            error.kind() == io::ErrorKind::ResourceBusy || error.raw_os_error() == Some(16)
        }
        _ => false,
    }
}

impl RynkLink {
    /// The link of the board `id` names, opened through its bus if nobody
    /// holds it yet.
    pub(crate) fn open(id: &PhysicalId) -> Result<Arc<Self>, RynkClientError> {
        Self::acquire(id.clone(), RynkTimeouts::default(), || transport::open(id))
    }

    /// The link of `id`: the live one if anyone holds it, otherwise a new one
    /// from `open`. A claim that is busy is retried for a short while.
    pub(crate) fn acquire(
        id: PhysicalId,
        timeouts: RynkTimeouts,
        mut open: impl FnMut() -> io::Result<BoxedTransport>,
    ) -> Result<Arc<Self>, RynkClientError> {
        let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
        // A link whose owner stopped (the board went away under it) is not
        // handed out again; its holders see their requests fail.
        if let Some(link) = registry
            .live
            .get(&id)
            .and_then(Weak::upgrade)
            .filter(|link| !link.closed.is_done())
        {
            return Ok(link);
        }
        if let Some(closing) = registry.closing.remove(&id) {
            drop(registry);
            closing.wait(BUSY_RETRY);
            registry = self::registry().lock().unwrap_or_else(|e| e.into_inner());
        }
        let started = Instant::now();
        let (client, board) = loop {
            let opened = open()
                .map_err(RynkClientError::from)
                .and_then(|transport| Client::connect(transport, timeouts));
            match opened {
                Err(error) if is_busy(&error) && started.elapsed() < BUSY_RETRY => {
                    std::thread::sleep(BUSY_STEP)
                }
                other => break other?,
            }
        };
        let (sender, events) = mpsc::channel();
        let closed = Arc::new(Closed::default());
        let link = Arc::new(Self {
            id: id.clone(),
            board,
            events: sender,
            closed: closed.clone(),
        });
        let name = format!("entropy-rynk-{}", id.0);
        std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                Owner::new(client).run(&events);
                // The claim is gone with the owner; only then do holders
                // learn that the link closed.
                closed.mark();
                drop(events);
            })
            .map_err(RynkClientError::from)?;
        registry.live.insert(id, Arc::downgrade(&link));
        Ok(link)
    }

    pub(crate) fn id(&self) -> &PhysicalId {
        &self.id
    }

    /// Whether the link can still carry requests: false once the board went
    /// away under it.
    pub(crate) fn is_open(&self) -> bool {
        !self.closed.is_done()
    }

    /// What the board reported when the link opened.
    pub(crate) fn board(&self) -> &RynkBoard {
        &self.board
    }

    /// A request for the editor: runs before posts.
    pub(crate) fn request<E>(&self, request: E::Request) -> Result<E::Response, RynkClientError>
    where
        E: Endpoint,
        E::Request: Send + 'static,
        E::Response: Send + 'static,
    {
        self.run(move |client| client.request::<E>(&request))
    }

    /// Runs `operation` on the client as one editor request, so the requests
    /// it makes in a row (a paged read, a bulk write) are not interleaved.
    pub(crate) fn run<R>(
        &self,
        operation: impl FnOnce(&mut LinkClient) -> Result<R, RynkClientError> + Send + 'static,
    ) -> Result<R, RynkClientError>
    where
        R: Send + 'static,
    {
        let (reply, answer) = mpsc::channel();
        let job: Job = Box::new(move |client| {
            let result = operation(client);
            let failed = result.as_ref().err().map(link_failed).unwrap_or(false);
            let _ = reply.send(result);
            if failed {
                return Err(RynkClientError::Transport(io::ErrorKind::BrokenPipe.into()));
            }
            Ok(())
        });
        self.send(Event::Request(job))?;
        let result = answer.recv().map_err(|_| closed_error())?;
        // The owner stops on a broken link; its caller hears of it only once
        // the link reads as closed, so the next step already sees it gone.
        if result.as_ref().err().is_some_and(link_failed) {
            self.closed.wait(BUSY_RETRY);
        }
        result
    }

    /// Sends `request` without the caller waiting for its reply. A newer post
    /// with the same `slot` replaces one not sent yet.
    pub(crate) fn post<E>(&self, slot: u16, request: E::Request) -> Result<(), RynkClientError>
    where
        E: Endpoint,
        E::Request: Send + 'static,
    {
        // The owner still reads the reply: over USB the board takes no frame
        // while its last reply waits for the host. What it answers matters to
        // no one; only a link that broke ends the owner.
        let job: Job = Box::new(move |client| match client.request::<E>(&request) {
            Err(error) if link_failed(&error) => Err(error),
            _ => Ok(()),
        });
        self.send(Event::Post {
            key: PostKey { cmd: E::CMD, slot },
            queued: Instant::now(),
            job,
        })
    }

    fn send(&self, event: Event) -> Result<(), RynkClientError> {
        self.events.send(event).map_err(|_| closed_error())
    }
}

impl Drop for RynkLink {
    fn drop(&mut self) {
        let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
        if registry
            .live
            .get(&self.id)
            .is_some_and(|link| link.strong_count() == 0)
        {
            registry.live.remove(&self.id);
        }
        // The owner closes once its channel does, which is now.
        registry
            .closing
            .insert(self.id.clone(), self.closed.clone());
    }
}

/// Worded so the UI reads it as a disconnect, as it does a Vial board's.
fn closed_error() -> RynkClientError {
    RynkClientError::Transport(io::Error::new(
        io::ErrorKind::NotConnected,
        "the board is disconnected",
    ))
}

/// Errors after which the link cannot carry another request.
fn link_failed(error: &RynkClientError) -> bool {
    matches!(error, RynkClientError::Transport(_))
}

struct Owner {
    client: Client,
    editor: VecDeque<Job>,
    /// Posts in the order their slots were first queued.
    posts: VecDeque<(PostKey, Instant)>,
    post_jobs: HashMap<PostKey, Job>,
}

impl Owner {
    fn new(client: Client) -> Self {
        Self {
            client,
            editor: VecDeque::new(),
            posts: VecDeque::new(),
            post_jobs: HashMap::new(),
        }
    }

    fn run(mut self, events: &mpsc::Receiver<Event>) {
        loop {
            // Take everything that arrived; sleep only when there is no work.
            let idle = self.editor.is_empty() && self.posts.is_empty();
            let event = if !idle {
                events
                    .try_recv()
                    .map_err(|error| error == mpsc::TryRecvError::Disconnected)
            } else {
                events.recv().map_err(|_| true)
            };
            match event {
                Ok(event) => {
                    self.queue(event);
                    continue;
                }
                Err(true) => {
                    self.flush_posts();
                    return;
                }
                Err(false) => {}
            }
            let Some(job) = self.next_job() else {
                continue;
            };
            if job(&mut self.client).is_err() {
                return;
            }
        }
    }

    fn queue(&mut self, event: Event) {
        match event {
            Event::Request(job) => self.editor.push_back(job),
            // A newer value takes the slot but not its place in line: the
            // wait counts from the oldest value not sent yet.
            Event::Post { key, queued, job } => {
                if self.post_jobs.insert(key, job).is_none() {
                    self.posts.push_back((key, queued));
                }
            }
        }
    }

    fn next_job(&mut self) -> Option<Job> {
        let post_overdue = self
            .posts
            .front()
            .is_some_and(|(_, queued)| queued.elapsed() >= POST_MAX_WAIT);
        if !post_overdue {
            if let Some(job) = self.editor.pop_front() {
                return Some(job);
            }
        }
        let (key, _) = self.posts.pop_front()?;
        self.post_jobs.remove(&key)
    }

    /// Sends the posts still queued once the last holder let go, so what a
    /// closing consumer posted last (clearing the media, say) still arrives.
    fn flush_posts(&mut self) {
        while let Some((key, _)) = self.posts.pop_front() {
            let Some(job) = self.post_jobs.remove(&key) else {
                continue;
            };
            if job(&mut self.client).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use rmk_types_rynk::protocol::rynk::{command, RynkHeader};

    use super::super::client::testing::*;
    use super::*;

    /// The commands a board saw after the handshake, with their first
    /// payload byte.
    type CommandLog = Arc<Mutex<Vec<(Cmd, u8)>>>;

    /// Unique per test: the registry is shared by tests running in parallel.
    fn id(name: &str) -> PhysicalId {
        PhysicalId(format!("test-{name}"))
    }

    fn open(board: &FakeBoard) -> impl FnMut() -> io::Result<BoxedTransport> + '_ {
        move || Ok(Box::new(board.transport()) as BoxedTransport)
    }

    /// A board that also answers `GetCurrentLayer` with 3 and records the
    /// commands after the handshake, with their first payload byte.
    fn layer_board() -> (FakeBoard, CommandLog) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        let board = FakeBoard::new(move |header: RynkHeader, payload: &[u8]| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            seen.lock()
                .unwrap()
                .push((header.cmd, payload.first().copied().unwrap_or_default()));
            match header.cmd {
                Cmd::GetCurrentLayer => vec![ok(header, 3u8)],
                Cmd::GetDefaultLayer => vec![ok(header, 0u8)],
                _ => vec![ok(header, ())],
            }
        });
        (board, log)
    }

    fn eventually(what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn everyone_shares_one_link_and_one_claim() {
        let (board, _) = layer_board();
        let opened = AtomicUsize::new(0);
        let mut counted = || {
            opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(board.transport()) as BoxedTransport)
        };
        let first = RynkLink::acquire(id("shared"), quick(), &mut counted).unwrap();
        let second = RynkLink::acquire(id("shared"), quick(), &mut counted).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        assert_eq!(
            first.request::<command::GetCurrentLayer>(()).unwrap(),
            3,
            "requests go through the owner"
        );

        drop(first);
        assert_eq!(board.dropped(), 0, "a holder keeps the claim");
        drop(second);
        eventually("the last holder releases the claim", || {
            board.dropped() == 1
        });
    }

    #[test]
    fn a_reopen_waits_until_the_previous_owner_lets_go() {
        let (board, _) = layer_board();
        let link = RynkLink::acquire(id("reopen"), quick(), open(&board)).unwrap();
        drop(link);
        let released_before_open = AtomicBool::new(false);
        let link = RynkLink::acquire(id("reopen"), quick(), || {
            released_before_open.store(board.dropped() == 1, Ordering::SeqCst);
            Ok(Box::new(board.transport()) as BoxedTransport)
        })
        .unwrap();
        assert!(released_before_open.load(Ordering::SeqCst));
        assert_eq!(link.request::<command::GetCurrentLayer>(()).unwrap(), 3);
    }

    #[test]
    fn a_busy_claim_is_retried_before_it_fails() {
        let (board, _) = layer_board();
        let attempts = AtomicUsize::new(0);
        let link = RynkLink::acquire(id("busy"), quick(), || {
            if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                return Err(io::Error::from_raw_os_error(16));
            }
            Ok(Box::new(board.transport()) as BoxedTransport)
        })
        .unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert_eq!(link.request::<command::GetCurrentLayer>(()).unwrap(), 3);

        let refused = RynkLink::acquire(id("busy-for-good"), quick(), || {
            Err(io::Error::from_raw_os_error(16))
        });
        assert!(matches!(refused, Err(RynkClientError::Transport(_))));
    }

    /// Holds the owner inside its first default layer read until `release` is
    /// sent, so the test can queue work behind it. `held` says it is there.
    /// The board's own state stays locked meanwhile, so the test waits on
    /// `held` rather than asking the board.
    fn gated_board() -> (FakeBoard, CommandLog, mpsc::Sender<()>, mpsc::Receiver<()>) {
        let (release, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let (holding, held) = mpsc::channel::<()>();
        let holding = Mutex::new(holding);
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        let board = FakeBoard::new(move |header: RynkHeader, payload: &[u8]| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            if header.cmd == Cmd::GetDefaultLayer && payload.is_empty() {
                let gated = seen.lock().unwrap().is_empty();
                if gated {
                    holding.lock().unwrap().send(()).unwrap();
                    gate.lock().unwrap().recv().unwrap();
                }
            }
            seen.lock()
                .unwrap()
                .push((header.cmd, payload.first().copied().unwrap_or_default()));
            match header.cmd {
                Cmd::GetCurrentLayer => vec![ok(header, 3u8)],
                Cmd::GetDefaultLayer => vec![ok(header, 0u8)],
                _ => vec![ok(header, ())],
            }
        });
        (board, log, release, held)
    }

    fn commands(log: &Mutex<Vec<(Cmd, u8)>>) -> Vec<(Cmd, u8)> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn the_editor_goes_before_posts() {
        let (board, log, release, held) = gated_board();
        let link = RynkLink::acquire(id("order"), quick(), open(&board)).unwrap();
        // The first read holds the owner at the gate.
        let blocker = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetDefaultLayer>(()))
        };
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        link.post::<command::SetDefaultLayer>(0, 7).unwrap();
        let editor = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetCurrentLayer>(()))
        };
        std::thread::sleep(Duration::from_millis(50));
        release.send(()).unwrap();
        assert_eq!(editor.join().unwrap().unwrap(), 3);
        blocker.join().unwrap().unwrap();
        eventually("the post is sent", || commands(&log).len() == 3);

        assert_eq!(
            commands(&log),
            [
                (Cmd::GetDefaultLayer, 0),
                (Cmd::GetCurrentLayer, 0),
                (Cmd::SetDefaultLayer, 7),
            ]
        );
    }

    #[test]
    fn a_run_keeps_its_requests_together() {
        let (board, log, release, held) = gated_board();
        let link = RynkLink::acquire(id("run"), quick(), open(&board)).unwrap();
        let blocker = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetDefaultLayer>(()))
        };
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        link.post::<command::SetDefaultLayer>(0, 7).unwrap();
        let run = {
            let link = link.clone();
            std::thread::spawn(move || {
                link.run(|client| {
                    let first = client.request::<command::GetCurrentLayer>(&())?;
                    let second = client.request::<command::GetCurrentLayer>(&())?;
                    Ok(first + second)
                })
            })
        };
        std::thread::sleep(Duration::from_millis(20));
        release.send(()).unwrap();
        assert_eq!(run.join().unwrap().unwrap(), 6);
        blocker.join().unwrap().unwrap();
        eventually("the post is sent", || commands(&log).len() == 4);

        assert_eq!(
            commands(&log)[1..],
            [
                (Cmd::GetCurrentLayer, 0),
                (Cmd::GetCurrentLayer, 0),
                (Cmd::SetDefaultLayer, 7),
            ]
        );
    }

    #[test]
    fn posts_queued_when_the_last_holder_lets_go_still_go_out() {
        let (release, gate) = mpsc::channel::<()>();
        let gate = Mutex::new(gate);
        let (holding, held) = mpsc::channel::<()>();
        let holding = Mutex::new(holding);
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        let board = FakeBoard::new(move |header: RynkHeader, payload: &[u8]| {
            if let Some(reply) = handshake(header, 512) {
                return vec![reply];
            }
            let value = payload.first().copied().unwrap_or_default();
            // The first post holds the owner until the link is let go.
            if header.cmd == Cmd::SetDefaultLayer && value == 1 {
                holding.lock().unwrap().send(()).unwrap();
                gate.lock().unwrap().recv().unwrap();
            }
            seen.lock().unwrap().push((header.cmd, value));
            vec![ok(header, ())]
        });
        let link = RynkLink::acquire(id("flush"), quick(), open(&board)).unwrap();
        link.post::<command::SetDefaultLayer>(0, 1).unwrap();
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        link.post::<command::SetDefaultLayer>(1, 2).unwrap();
        link.post::<command::SetDefaultLayer>(2, 3).unwrap();
        drop(link);
        release.send(()).unwrap();

        eventually("the board is let go", || board.dropped() == 1);
        assert_eq!(
            commands(&log),
            [
                (Cmd::SetDefaultLayer, 1),
                (Cmd::SetDefaultLayer, 2),
                (Cmd::SetDefaultLayer, 3),
            ]
        );
    }

    #[test]
    fn a_post_keeps_only_the_latest_value_of_its_slot() {
        let (board, log, release, held) = gated_board();
        let link = RynkLink::acquire(id("coalesce"), quick(), open(&board)).unwrap();
        let blocker = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetDefaultLayer>(()))
        };
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        for value in [1, 2, 3] {
            link.post::<command::SetDefaultLayer>(0, value).unwrap();
        }
        link.post::<command::SetDefaultLayer>(1, 9).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        release.send(()).unwrap();
        blocker.join().unwrap().unwrap();
        eventually("both slots are sent", || commands(&log).len() == 3);

        assert_eq!(
            commands(&log)[1..],
            [(Cmd::SetDefaultLayer, 3), (Cmd::SetDefaultLayer, 9)]
        );
        // The posts' replies are dropped, not mistaken for the next answer.
        assert_eq!(link.request::<command::GetCurrentLayer>(()).unwrap(), 3);
    }

    #[test]
    fn an_overdue_post_goes_before_the_editor() {
        let (board, log, release, held) = gated_board();
        let link = RynkLink::acquire(id("overdue"), quick(), open(&board)).unwrap();
        let blocker = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetDefaultLayer>(()))
        };
        held.recv_timeout(Duration::from_secs(3)).unwrap();
        link.post::<command::SetDefaultLayer>(0, 5).unwrap();
        std::thread::sleep(POST_MAX_WAIT + Duration::from_millis(50));
        let editor = {
            let link = link.clone();
            std::thread::spawn(move || link.request::<command::GetCurrentLayer>(()))
        };
        std::thread::sleep(Duration::from_millis(20));
        release.send(()).unwrap();
        editor.join().unwrap().unwrap();
        blocker.join().unwrap().unwrap();

        assert_eq!(
            commands(&log)[1..],
            [(Cmd::SetDefaultLayer, 5), (Cmd::GetCurrentLayer, 0)]
        );
    }

    /// Phenom over USB kept the first layout it was sent: nothing read the
    /// post's reply, so the board took no further frame and the link broke.
    #[test]
    fn posts_after_posts_reach_a_board_over_usb() {
        let (board, log) = layer_board();
        let board = board.usb();
        let link = RynkLink::acquire(id("usb-posts"), quick(), open(&board)).unwrap();
        for value in 1..=3u8 {
            link.post::<command::SetDefaultLayer>(0, value).unwrap();
            eventually("the post is sent", || {
                commands(&log).len() == value as usize
            });
        }
        assert!(link.is_open());
        assert_eq!(
            commands(&log),
            [
                (Cmd::SetDefaultLayer, 1),
                (Cmd::SetDefaultLayer, 2),
                (Cmd::SetDefaultLayer, 3),
            ]
        );
        assert_eq!(link.request::<command::GetCurrentLayer>(()).unwrap(), 3);
    }

    /// A transport that can be unplugged.
    struct Unpluggable {
        inner: FakeTransport,
        unplugged: Arc<AtomicBool>,
    }

    impl RynkTransport for Unpluggable {
        fn send(&mut self, frame: &[u8]) -> io::Result<()> {
            if self.unplugged.load(Ordering::SeqCst) {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            self.inner.send(frame)
        }

        fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
            self.inner.recv(buf, timeout)
        }
    }

    #[test]
    fn a_link_that_broke_is_not_handed_out_again() {
        let (board, _) = layer_board();
        let unplugged = Arc::new(AtomicBool::new(false));
        let opened = AtomicUsize::new(0);
        let mut open = || {
            opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Unpluggable {
                inner: board.transport(),
                unplugged: unplugged.clone(),
            }) as BoxedTransport)
        };
        let broken = RynkLink::acquire(id("broken"), quick(), &mut open).unwrap();
        unplugged.store(true, Ordering::SeqCst);
        assert!(broken.request::<command::GetCurrentLayer>(()).is_err());
        assert!(!broken.is_open(), "the request that broke it saw it closed");
        let stopped = broken
            .request::<command::GetCurrentLayer>(())
            .expect_err("the stopped owner answers nothing more");
        assert!(
            crate::hid::is_disconnect_error(&anyhow::Error::new(stopped)),
            "the UI sees the board gone"
        );

        unplugged.store(false, Ordering::SeqCst);
        let fresh = RynkLink::acquire(id("broken"), quick(), &mut open).unwrap();
        assert!(!Arc::ptr_eq(&broken, &fresh));
        assert_eq!(opened.load(Ordering::SeqCst), 2);
        assert_eq!(fresh.request::<command::GetCurrentLayer>(()).unwrap(), 3);
    }
}
