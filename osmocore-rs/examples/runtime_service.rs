// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end composition and round-trip acceptance example: bounded application
//! framing, TCP/UDP, FSM timer response, typed worker status and clean shutdown.
//! Accounting/rate drivers compose here; focused tests verify their timed effects.
//! Run with `cargo run --no-default-features --features runtime --example runtime_service`.
use osmocore_rs::{
    fsm::{EventData, Fsm, FsmId, FsmRuntime, FsmState},
    msgb::Msgb,
    rate_ctr::{
        RateCtr, RateCtrDesc, RateCtrGroupDesc, RateCtrGroupId, RateCtrPeriodicDriver,
        RateCtrRuntime,
    },
    runtime::*,
    time::SystemClock,
    time_accounting::TimeCc,
    timer::TimerScheduler,
};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::mpsc,
    time::Duration,
};
#[derive(Clone, Copy)]
enum Reply {
    Tcp(TcpHandle),
    Udp(UdpHandle, SocketAddr),
}
struct Engine {
    pending: VecDeque<Reply>,
    fired: bool,
}
type FsmRt = FsmRuntime<SystemClock, Engine, Reply>;
type FsmTimers = TimerScheduler<SystemClock, FsmRt>;
fn request(s: &mut FsmTimers, r: &mut FsmRt, id: FsmId, _: u32, data: &EventData<'_, Reply>) {
    if let EventData::User(reply) = data {
        assert!(r.data.pending.len() < 16);
        r.data.pending.push_back(**reply);
        // A second request replaces/cancels the previous arm, preserving the
        // existing FSM transition and scheduler contracts.
        r.state_change_ms(s, &id, 0, 1, 1).unwrap();
    }
}
fn timeout(_: &mut FsmTimers, r: &mut FsmRt, _: FsmId) -> i32 {
    r.data.fired = true;
    0
}
enum Command {
    Status(mpsc::SyncSender<u64>),
}
struct Service {
    fsm: (FsmTimers, FsmRt),
    id: FsmId,
    accounting: (
        TimerScheduler<SystemClock, TimeCc<'static, RateCtr>>,
        TimeCc<'static, RateCtr>,
    ),
    periodic: RateCtrPeriodicDriver<SystemClock>,
    counters: RateCtrRuntime,
    group: RateCtrGroupId,
    frames: HashMap<TcpHandle, Vec<u8>>,
    completions: usize,
    failures: usize,
}
impl Service {
    fn new() -> Self {
        let mut scheduler = TimerScheduler::new(SystemClock);
        let mut fsm = FsmRt::new(Engine {
            pending: VecDeque::new(),
            fired: false,
        });
        let definition = fsm
            .register(Fsm {
                name: "runtime_service".into(),
                states: vec![FsmState {
                    name: Some("ready".into()),
                    in_event_mask: 1,
                    out_state_mask: 1,
                    action: Some(request),
                    onenter: None,
                    onleave: None,
                }],
                allstate_event_mask: 0,
                allstate_action: None,
                pre_term: None,
                cleanup: None,
                timer_cb: Some(timeout),
                event_names: None,
            })
            .unwrap();
        let id = fsm.alloc(&mut scheduler, &definition, None, ()).unwrap();
        let mut counters = RateCtrRuntime::new();
        let group = counters
            .allocate(
                RateCtrGroupDesc {
                    group_name_prefix: "service".into(),
                    group_description: None,
                    class_id: 0,
                    counters: vec![RateCtrDesc {
                        name: "requests".into(),
                        description: None,
                    }],
                },
                0,
            )
            .unwrap();
        let mut periodic = RateCtrPeriodicDriver::new(SystemClock);
        periodic.start().unwrap();
        let mut accounting = (TimerScheduler::new(SystemClock), TimeCc::new());
        accounting.1.cfg.rate_ctr = Some(RateCtr::default());
        accounting.1.set_flag(&mut accounting.0, true).unwrap();
        Self {
            fsm: (scheduler, fsm),
            id,
            accounting,
            periodic,
            counters,
            group,
            frames: HashMap::new(),
            completions: 0,
            failures: 0,
        }
    }
    fn dispatch(&mut self, reply: Reply) {
        self.counters.add(&self.group, 0, 1).unwrap();
        self.fsm
            .1
            .dispatch(&mut self.fsm.0, Some(&self.id), 0, &EventData::User(&reply))
            .unwrap();
    }
}
fn response() -> Msgb {
    let mut m = Msgb::new(16).unwrap();
    m.put(5).unwrap();
    m.data_mut().copy_from_slice(b"pong\n");
    m
}
impl Application for Service {
    type Command = Command;
    type Error = ();
    fn on_command(&mut self, _: &mut RuntimeContext<'_>, command: Command) -> Result<(), ()> {
        match command {
            Command::Status(reply) => {
                let _ = reply.try_send(
                    self.counters
                        .group(&self.group)
                        .unwrap()
                        .counter(0)
                        .unwrap()
                        .current,
                );
            }
        }
        Ok(())
    }
    fn on_event(&mut self, cx: &mut RuntimeContext<'_>, event: RuntimeEvent<'_>) -> Result<(), ()> {
        match event {
            RuntimeEvent::Accepted { stream, .. } => {
                self.frames.insert(stream, Vec::new());
                cx.set_read_paused(stream, false).unwrap();
            }
            RuntimeEvent::TcpBytes { stream, bytes } => {
                // Application framing has a hard per-connection bound.
                let frame = self.frames.get_mut(&stream).unwrap();
                if frame.len().saturating_add(bytes.len()) > 256 {
                    self.failures += 1;
                    cx.close(stream).unwrap();
                    self.frames.remove(&stream);
                    return Ok(());
                }
                frame.extend_from_slice(bytes);
                let mut requests = 0;
                while let Some(end) = frame.iter().position(|b| *b == b'\n') {
                    if &frame[..=end] == b"ping\n" {
                        requests += 1;
                    }
                    frame.drain(..=end);
                }
                for _ in 0..requests {
                    if self.fsm.1.data.pending.len() < 16 {
                        self.dispatch(Reply::Tcp(stream));
                    } else {
                        self.failures += 1;
                        cx.close(stream).unwrap();
                        break;
                    }
                }
            }
            RuntimeEvent::UdpDatagram {
                socket,
                bytes: b"ping\n",
                source,
            } => {
                if self.fsm.1.data.pending.len() < 16 {
                    self.dispatch(Reply::Udp(socket, source));
                }
            }
            RuntimeEvent::DriverProgress { label: "fsm" } if self.fsm.1.data.fired => {
                self.fsm.1.data.fired = false;
                while let Some(reply) = self.fsm.1.data.pending.pop_front() {
                    let result = match reply {
                        Reply::Tcp(h) => cx.try_send_tcp(h, response()).map_err(|e| e.reason),
                        Reply::Udp(h, source) => cx
                            .try_send_udp(
                                h,
                                Datagram {
                                    message: response(),
                                    destination: Some(source),
                                },
                            )
                            .map_err(|e| e.reason),
                    };
                    if result.is_err() {
                        self.failures += 1;
                    }
                }
            }
            RuntimeEvent::Write(w) => {
                if matches!(w.status, WriteStatus::Complete) {
                    self.completions += 1;
                } else {
                    self.failures += 1;
                }
            }
            RuntimeEvent::Closed(ResourceHandle::Tcp(h)) => {
                self.frames.remove(&h);
            }
            RuntimeEvent::TransportError { .. } => self.failures += 1,
            _ => {}
        }
        Ok(())
    }
    fn on_shutdown(&mut self, _: &mut RuntimeContext<'_>) -> Result<(), ()> {
        self.accounting
            .1
            .set_flag(&mut self.accounting.0, false)
            .unwrap();
        self.periodic.stop();
        Ok(())
    }
}
/// Runs a finite local service scenario, returning inspectable final state.
pub fn exercise() -> (ShutdownReport, u64) {
    exercise_mode("")
}

// The watchdog bounds the process even if a callback or worker never returns.
// Cancellation on drop also covers unwinding in the orchestration thread.
struct Watchdog(mpsc::Sender<()>, Option<std::thread::JoinHandle<()>>);
impl Watchdog {
    fn start(bound: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if matches!(
                receiver.recv_timeout(bound),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                eprintln!("acceptance scenario deadline exceeded");
                std::process::exit(2);
            }
        });
        Self(sender, Some(thread))
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        let _ = self.0.send(());
        let _ = self.1.take().unwrap().join();
    }
}
struct WorkerShutdown(ControlHandle<Command>);
impl Drop for WorkerShutdown {
    fn drop(&mut self) {
        let _ = self.0.request_shutdown(ShutdownMode::Drain {
            timeout: Duration::from_secs(1),
        });
    }
}
/// Fault injection is confined to acceptance orchestration, outside the runtime API.
pub fn exercise_mode(mode: &str) -> (ShutdownReport, u64) {
    let deadline_probe = mode == "--deadline-probe";
    let worker_panic = mode == "--worker-panic";
    let _watchdog = Watchdog::start(if deadline_probe {
        Duration::from_millis(200)
    } else {
        Duration::from_secs(10)
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_addr = listener.local_addr().unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_addr = socket.local_addr().unwrap();
    let mut runtime = IoRuntime::new(RuntimeConfig {
        max_resources: 16,
        max_datagram_size: 256,
        ..RuntimeConfig::default()
    })
    .unwrap();
    runtime
        .register_listener(listener, ListenerOptions::default())
        .unwrap();
    runtime
        .register_udp(socket, UdpResourceOptions::default())
        .unwrap();
    let control = runtime.control_handle();
    let worker = std::thread::spawn(move || -> std::io::Result<()> {
        let _shutdown = WorkerShutdown(control.clone());
        assert!(
            !worker_panic,
            "intentional acceptance worker assertion failure"
        );
        if deadline_probe {
            loop {
                std::thread::park();
            }
        }
        let mut bad = TcpStream::connect(tcp_addr)?;
        bad.write_all(&[b'x'; 257])?;
        bad.set_read_timeout(Some(Duration::from_secs(2)))?;
        let mut eof = [0];
        assert_eq!(bad.read(&mut eof)?, 0);
        let mut tcp = TcpStream::connect(tcp_addr)?;
        tcp.set_read_timeout(Some(Duration::from_secs(2)))?;
        tcp.write_all(b"pi")?;
        tcp.write_all(b"ng\n")?;
        let mut response = [0; 5];
        tcp.read_exact(&mut response)?;
        assert_eq!(&response, b"pong\n");
        let udp = UdpSocket::bind("127.0.0.1:0")?;
        udp.set_read_timeout(Some(Duration::from_secs(2)))?;
        udp.send_to(b"ping\n", udp_addr)?;
        let (n, from) = udp.recv_from(&mut response)?;
        assert_eq!(n, 5);
        assert_eq!(from, udp_addr);
        assert_eq!(&response, b"pong\n");
        let (reply, receiver) = mpsc::sync_channel(1);
        let receipt = control
            .try_submit(Command::Status(reply))
            .ok()
            .unwrap()
            .receipt;
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        assert_eq!(receipt.recv().unwrap(), CommandStatus::Processed);
        Ok(())
    });
    let mut service = Service::new();
    let mut drivers = DriverSet::default();
    drivers
        .add_driver(
            "fsm",
            FsmDriver::new(|a: &mut Service| &mut a.fsm),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    drivers
        .add_driver(
            "accounting",
            AccountingDriver::new(|a: &mut Service| &mut a.accounting),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    drivers
        .add_driver(
            "rate",
            RateDriver::new(|a: &mut Service| (&mut a.periodic, &mut a.counters)),
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    let report = runtime.run(&mut service, &mut drivers).unwrap();
    if worker_panic {
        eprintln!("acceptance runtime shutdown completed");
    }
    match worker.join() {
        Ok(result) => result.unwrap(),
        Err(payload) => std::panic::resume_unwind(payload),
    }
    assert_eq!(service.failures, 1);
    assert_eq!(service.completions, 2);
    assert!(!report.timed_out);
    (
        report,
        service
            .counters
            .group(&service.group)
            .unwrap()
            .counter(0)
            .unwrap()
            .current,
    )
}
#[allow(dead_code)]
fn main() {
    let (_, requests) = exercise_mode(&std::env::args().nth(1).unwrap_or_default());
    println!("runtime acceptance service completed {requests} requests");
}
