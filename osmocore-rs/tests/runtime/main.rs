// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "runtime")]
use osmocore_rs::{msgb::Msgb, runtime::*};
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    rc::Rc,
    sync::{Arc, Barrier, mpsc},
    time::{Duration, Instant},
};
mod mixed_drivers;
fn message(bytes: &[u8]) -> Msgb {
    let mut m = Msgb::new(bytes.len()).unwrap();
    m.put(bytes.len()).unwrap();
    m.data_mut().copy_from_slice(bytes);
    m
}
enum Command {
    Ping(mpsc::SyncSender<usize>),
    Pause(TcpHandle, bool),
    Send(TcpHandle, Msgb),
    Register(TcpStream),
    Stop,
}
#[derive(Default)]
struct App {
    count: Rc<RefCell<usize>>,
    tcp: Vec<u8>,
    udp: Vec<Vec<u8>>,
    sources: Vec<std::net::SocketAddr>,
    accepted: Vec<TcpHandle>,
    writes: Vec<WriteOutcome>,
    registered: Vec<TcpHandle>,
    oversized: usize,
    eof: usize,
    echo: bool,
    hooks: usize,
}
impl Application for App {
    type Command = Command;
    type Error = ();
    fn on_command(&mut self, cx: &mut RuntimeContext<'_>, cmd: Command) -> Result<(), ()> {
        match cmd {
            Command::Ping(reply) => {
                *self.count.borrow_mut() += 1;
                let _ = reply.try_send(*self.count.borrow());
            }
            Command::Pause(h, p) => cx.set_read_paused(h, p).unwrap(),
            Command::Send(h, m) => {
                cx.try_send_tcp(h, m).unwrap();
            }
            Command::Register(s) => {
                cx.register_tcp(s, TcpResourceOptions::default()).unwrap();
            }
            Command::Stop => cx
                .request_shutdown(ShutdownMode::Drain {
                    timeout: Duration::from_secs(1),
                })
                .unwrap(),
        }
        Ok(())
    }
    fn on_event(&mut self, cx: &mut RuntimeContext<'_>, event: RuntimeEvent<'_>) -> Result<(), ()> {
        match event {
            RuntimeEvent::Accepted { stream, .. } => {
                self.accepted.push(stream);
                cx.set_read_paused(stream, false).unwrap();
            }
            RuntimeEvent::TcpBytes { stream, bytes } => {
                self.tcp.extend_from_slice(bytes);
                if self.echo {
                    cx.try_send_tcp(stream, message(bytes)).unwrap();
                }
            }
            RuntimeEvent::UdpDatagram {
                socket,
                bytes,
                source,
            } => {
                self.udp.push(bytes.to_vec());
                self.sources.push(source);
                if self.echo {
                    cx.try_send_udp(
                        socket,
                        Datagram {
                            message: message(bytes),
                            destination: Some(source),
                        },
                    )
                    .unwrap();
                }
            }
            RuntimeEvent::OversizedDatagram(_) => self.oversized += 1,
            RuntimeEvent::PeerEof(_) => self.eof += 1,
            RuntimeEvent::Write(w) => self.writes.push(w),
            RuntimeEvent::Registered {
                result: Ok(ResourceHandle::Tcp(h)),
                ..
            } => self.registered.push(h),
            _ => {}
        }
        Ok(())
    }
    fn on_shutdown(&mut self, _: &mut RuntimeContext<'_>) -> Result<(), ()> {
        self.hooks += 1;
        Ok(())
    }
}
fn until(rt: &mut IoRuntime<Command>, app: &mut App, predicate: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut drivers = DriverSet::default();
    while !predicate(app) {
        assert!(Instant::now() < deadline, "runtime made no progress");
        rt.step(app, &mut drivers, Some(Duration::from_millis(5)))
            .unwrap();
    }
}
#[test]
fn tcp_listener_raw_echo_and_eof_preserve_output() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig {
        io_calls_per_resource: 1,
        bytes_per_resource: 2,
        ..RuntimeConfig::default()
    })
    .unwrap();
    rt.register_listener(listener, ListenerOptions::default())
        .unwrap();
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(b"abcdef").unwrap();
    client.shutdown(std::net::Shutdown::Write).unwrap();
    let mut app = App {
        echo: true,
        ..App::default()
    };
    until(&mut rt, &mut app, |a| a.eof == 1 && a.writes.len() == 3);
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    assert_eq!(response, b"abcdef");
    assert_eq!(app.tcp, response);
    assert!(
        app.writes
            .iter()
            .all(|w| matches!(w.status, WriteStatus::Complete))
    );
}
#[test]
fn udp_ipv4_ipv6_boundaries_empty_oversize_sources_and_output() {
    for host in ["127.0.0.1:0", "[::1]:0"] {
        let server = UdpSocket::bind(host).unwrap();
        let addr = server.local_addr().unwrap();
        let client = UdpSocket::bind(host).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut rt = IoRuntime::new(RuntimeConfig {
            max_datagram_size: 4,
            io_calls_per_resource: 1,
            ..RuntimeConfig::default()
        })
        .unwrap();
        rt.register_udp(server, UdpResourceOptions::default())
            .unwrap();
        for data in [b"abc".as_slice(), b"", b"oversized"] {
            client.send_to(data, addr).unwrap();
        }
        let mut app = App {
            echo: true,
            ..App::default()
        };
        until(&mut rt, &mut app, |a| {
            a.udp.len() == 2 && a.oversized == 1 && a.writes.len() == 2
        });
        assert_eq!(app.udp, [b"abc".to_vec(), Vec::new()]);
        assert!(
            app.sources
                .iter()
                .all(|a| *a == client.local_addr().unwrap())
        );
        let mut buf = [0; 8];
        for expected in [b"abc".as_slice(), b""] {
            let (n, source) = client.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..n], expected);
            assert_eq!(source, addr);
        }
    }
}
#[test]
fn deferred_registration_and_pause_resume_cannot_strand_readiness() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let control = rt.control_handle();
    let receipt = control
        .try_submit(Command::Register(server))
        .ok()
        .unwrap()
        .receipt;
    let mut app = App::default();
    until(&mut rt, &mut app, |a| !a.registered.is_empty());
    assert_eq!(receipt.recv().unwrap(), CommandStatus::Processed);
    let handle = app.registered[0];
    control
        .try_submit(Command::Pause(handle, true))
        .ok()
        .unwrap();
    rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
        .unwrap();
    peer.write_all(b"buffered").unwrap();
    rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
        .unwrap();
    assert!(app.tcp.is_empty());
    control
        .try_submit(Command::Pause(handle, false))
        .ok()
        .unwrap();
    until(&mut rt, &mut app, |a| a.tcp.len() == 8);
    assert_eq!(app.tcp, b"buffered");
}
#[test]
fn non_send_application_typed_reply_command_wakeup_coalescing_and_receipt_drop() {
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let control = rt.control_handle();
    let (reply, replies) = mpsc::sync_channel(4);
    let barrier = Arc::new(Barrier::new(2));
    let worker_barrier = barrier.clone();
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        for _ in 0..3 {
            drop(
                control
                    .try_submit(Command::Ping(reply.clone()))
                    .ok()
                    .unwrap(),
            );
        }
    });
    barrier.wait();
    let mut app = App::default();
    until(&mut rt, &mut app, |a| *a.count.borrow() == 3);
    worker.join().unwrap();
    assert_eq!(replies.try_iter().collect::<Vec<_>>(), [1, 2, 3]);
    rt.control_handle().try_submit(Command::Stop).ok().unwrap();
    until(&mut rt, &mut app, |a| a.hooks == 1);
    assert_eq!(rt.lifecycle(), Lifecycle::Stopped);
}
#[test]
fn connected_udp_rejects_explicit_destination_without_losing_datagram() {
    struct Connected {
        handle: UdpHandle,
        done: bool,
    }
    impl Application for Connected {
        type Command = ();
        type Error = ();
        fn on_event(
            &mut self,
            _: &mut RuntimeContext<'_>,
            event: RuntimeEvent<'_>,
        ) -> Result<(), ()> {
            if let RuntimeEvent::Write(w) = event {
                assert!(matches!(w.status, WriteStatus::Complete));
                self.done = true;
            }
            Ok(())
        }
        fn on_command(&mut self, cx: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
            let e = cx
                .try_send_udp(
                    self.handle,
                    Datagram {
                        message: message(b"x"),
                        destination: Some("127.0.0.1:9".parse().unwrap()),
                    },
                )
                .err()
                .unwrap();
            assert_eq!(e.reason, Rejection::InvalidDestination);
            assert_eq!(e.value.message.data(), b"x");
            cx.try_send_udp(
                self.handle,
                Datagram {
                    message: message(b"ok"),
                    destination: None,
                },
            )
            .unwrap();
            Ok(())
        }
    }
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect(peer.local_addr().unwrap()).unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let h = rt
        .register_udp(socket, UdpResourceOptions::default())
        .unwrap();
    let mut app = Connected {
        handle: h,
        done: false,
    };
    rt.control_handle().try_submit(()).ok().unwrap();
    while !app.done {
        rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            .unwrap();
    }
    let mut buf = [0; 8];
    assert_eq!(peer.recv(&mut buf).unwrap(), 2);
}

#[path = "../../examples/runtime_service.rs"]
mod acceptance_service;
#[test]
fn service_composition_and_round_trips() {
    let (report, requests) = acceptance_service::exercise();
    assert_eq!(requests, 2);
    assert!(!report.timed_out);
}

#[cfg(feature = "net")]
#[test]
fn optional_establishment_helpers_compose_without_runtime_dependency() {
    use osmocore_rs::net::{TcpOptions, connect_tcp, listen_tcp};
    let listener = listen_tcp("127.0.0.1:0".parse().unwrap(), 8, &TcpOptions::default()).unwrap();
    let mut peer = connect_tcp(
        None,
        listener.local_addr().unwrap(),
        Duration::from_secs(1),
        &TcpOptions::default(),
    )
    .unwrap();
    let (stream, _) = listener.accept().unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    rt.register_tcp(stream, TcpResourceOptions::default())
        .unwrap();
    peer.write_all(b"composed").unwrap();
    let mut app = App::default();
    until(&mut rt, &mut app, |a| a.tcp.len() == 8);
    assert_eq!(app.tcp, b"composed");
}

#[test]
fn hot_resources_and_commands_all_progress_with_small_quanta() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig {
        io_calls_per_resource: 1,
        bytes_per_resource: 1,
        commands_per_step: 1,
        ..RuntimeConfig::default()
    })
    .unwrap();
    let mut peers = Vec::new();
    for _ in 0..3 {
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (s, _) = listener.accept().unwrap();
        rt.register_tcp(s, TcpResourceOptions::default()).unwrap();
        peer.write_all(b"xxxx").unwrap();
        peers.push(peer);
    }
    let (reply, _) = mpsc::sync_channel(1);
    for _ in 0..8 {
        rt.control_handle()
            .try_submit(Command::Ping(reply.clone()))
            .ok()
            .unwrap();
    }
    let mut app = App::default();
    rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
        .unwrap();
    assert_eq!(app.tcp.len(), 3);
    assert!(*app.count.borrow() > 0);
    assert!(*app.count.borrow() < 8);
    until(&mut rt, &mut app, |a| {
        a.tcp.len() == 12 && *a.count.borrow() == 8
    });
}

#[test]
fn submission_shutdown_race_resolves_every_accepted_receipt() {
    let mut rt = IoRuntime::<Command>::new(RuntimeConfig::default()).unwrap();
    let control = rt.control_handle();
    let stopping = control.clone();
    let barrier = Arc::new(Barrier::new(2));
    let other = barrier.clone();
    let worker = std::thread::spawn(move || {
        let (reply, _) = mpsc::sync_channel(1);
        other.wait();
        (0..32)
            .filter_map(|_| {
                control
                    .try_submit(Command::Ping(reply.clone()))
                    .ok()
                    .map(|o| o.receipt)
            })
            .collect::<Vec<_>>()
    });
    barrier.wait();
    stopping.request_shutdown(ShutdownMode::Immediate).unwrap();
    let mut app = App::default();
    rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
        .unwrap();
    for receipt in worker.join().unwrap() {
        assert_eq!(receipt.recv().unwrap(), CommandStatus::Rejected);
    }
    assert_eq!(app.hooks, 1);
}

#[test]
fn explicit_half_close_policy_retains_write_half_after_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (stream, _) = listener.accept().unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let handle = rt
        .register_tcp(
            stream,
            TcpResourceOptions {
                retain_write_half: true,
                ..TcpResourceOptions::default()
            },
        )
        .unwrap();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    let mut app = App::default();
    until(&mut rt, &mut app, |a| a.eof == 1);
    rt.control_handle()
        .try_submit(Command::Send(handle, message(b"after")))
        .ok()
        .unwrap();
    until(&mut rt, &mut app, |a| !a.writes.is_empty());
    let mut response = [0; 5];
    peer.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"after");
}

#[test]
fn udp_send_failure_is_datagram_local_and_fifo_continues() {
    struct Sender {
        handle: UdpHandle,
        destination: std::net::SocketAddr,
        outcomes: Vec<WriteStatus>,
    }
    impl Application for Sender {
        type Command = ();
        type Error = ();
        fn on_command(&mut self, cx: &mut RuntimeContext<'_>, _: ()) -> Result<(), ()> {
            for destination in ["[::1]:9".parse().unwrap(), self.destination] {
                cx.try_send_udp(
                    self.handle,
                    Datagram {
                        message: message(b"next"),
                        destination: Some(destination),
                    },
                )
                .unwrap();
            }
            Ok(())
        }
        fn on_event(
            &mut self,
            _: &mut RuntimeContext<'_>,
            event: RuntimeEvent<'_>,
        ) -> Result<(), ()> {
            if let RuntimeEvent::Write(w) = event {
                self.outcomes.push(w.status);
            }
            Ok(())
        }
    }
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut rt = IoRuntime::new(RuntimeConfig::default()).unwrap();
    let handle = rt
        .register_udp(
            UdpSocket::bind("127.0.0.1:0").unwrap(),
            UdpResourceOptions::default(),
        )
        .unwrap();
    let mut app = Sender {
        handle,
        destination: receiver.local_addr().unwrap(),
        outcomes: Vec::new(),
    };
    rt.control_handle().try_submit(()).ok().unwrap();
    for _ in 0..8 {
        if app.outcomes.len() == 2 {
            break;
        }
        rt.step(&mut app, &mut DriverSet::default(), Some(Duration::ZERO))
            .unwrap();
    }
    assert_eq!(app.outcomes.len(), 2);
    assert!(matches!(app.outcomes[0], WriteStatus::Failed(_)));
    assert!(matches!(app.outcomes[1], WriteStatus::Complete));
    let mut bytes = [0; 4];
    assert_eq!(receiver.recv(&mut bytes).unwrap(), 4);
    assert_eq!(&bytes, b"next");
}

// Subprocesses contain the example's deliberate panic and process-wide watchdog.
#[test]
fn acceptance_failure_paths_terminate() {
    const CHILD_MODE: &str = "OSMOCORE_ACCEPTANCE_FAILURE_MODE";
    if let Ok(mode) = std::env::var(CHILD_MODE) {
        acceptance_service::exercise_mode(&mode);
        panic!("injected scenario unexpectedly succeeded");
    }
    for (mode, expected_code, message) in [
        (
            "--worker-panic",
            101,
            "intentional acceptance worker assertion failure",
        ),
        (
            "--deadline-probe",
            2,
            "acceptance scenario deadline exceeded",
        ),
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "acceptance_failure_paths_terminate",
                "--nocapture",
            ])
            .env(CHILD_MODE, mode)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "acceptance child hung: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(expected_code));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{stderr}");
        if mode == "--worker-panic" {
            assert!(
                stderr.contains("acceptance runtime shutdown completed"),
                "{stderr}"
            );
        }
    }
}
