// SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime limits, rejections and failure paths, through the public API.

use osmocore_rs::{
    base::msgb::Msgb,
    base::time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    base::timer::{TimerId, TimerScheduler},
    runtime::*,
};
use std::{
    cell::Cell,
    error::Error,
    io::Write,
    net::{TcpListener, TcpStream, UdpSocket},
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

type Callback = Box<dyn FnOnce(&mut RuntimeContext<'_>) -> Result<(), &'static str> + Send>;

/// Runs commands as closures, records events by name, and fails on the event it's told to.
#[derive(Default)]
struct Probe {
    fail_on: Option<&'static str>,
    fail_shutdown: bool,
    shutdown_hook_requests: Option<ShutdownMode>,
    events: Vec<String>,
    tcp: Vec<TcpHandle>,
    udp: Vec<UdpHandle>,
    listeners: Vec<ListenerHandle>,
    streams: Vec<TcpHandle>,
    bytes: Vec<u8>,
    datagrams: Vec<Vec<u8>>,
    failed_writes: usize,
    registration_errors: usize,
    shutdown_hooks: usize,
}

impl Application for Probe {
    type Command = Callback;
    type Error = &'static str;
    fn on_command(
        &mut self,
        cx: &mut RuntimeContext<'_>,
        run: Callback,
    ) -> Result<(), Self::Error> {
        run(cx)
    }
    fn on_event(
        &mut self,
        _: &mut RuntimeContext<'_>,
        event: RuntimeEvent<'_>,
    ) -> Result<(), Self::Error> {
        let name = match event {
            RuntimeEvent::Accepted { stream, .. } => {
                self.streams.push(stream);
                "accepted"
            }
            RuntimeEvent::TcpBytes { bytes, .. } => {
                self.bytes.extend_from_slice(bytes);
                "tcp_bytes"
            }
            RuntimeEvent::PeerEof(_) => "peer_eof",
            RuntimeEvent::UdpDatagram { bytes, .. } => {
                self.datagrams.push(bytes.to_vec());
                "datagram"
            }
            RuntimeEvent::OversizedDatagram(_) => "oversized",
            RuntimeEvent::TransportError { operation, .. } => {
                self.events.push(format!("transport:{operation}"));
                "transport"
            }
            RuntimeEvent::Write(write) => {
                if matches!(write.status, WriteStatus::Failed(_)) {
                    self.failed_writes += 1;
                }
                "write"
            }
            RuntimeEvent::Registered { result, .. } => {
                match result {
                    Ok(ResourceHandle::Tcp(handle)) => self.tcp.push(handle),
                    Ok(ResourceHandle::Udp(handle)) => self.udp.push(handle),
                    Ok(ResourceHandle::Listener(handle)) => self.listeners.push(handle),
                    Err(_) => self.registration_errors += 1,
                }
                "registered"
            }
            RuntimeEvent::EffectFailed { .. } => "effect_failed",
            RuntimeEvent::Closed(_) => "closed",
            RuntimeEvent::DriverProgress { .. } => "driver_progress",
            RuntimeEvent::DriverDisabled { label } => {
                self.events.push(format!("disabled:{label}"));
                "driver_disabled"
            }
        };
        self.events.push(name.to_owned());
        match self.fail_on {
            Some(failing) if failing == name => Err(failing),
            _ => Ok(()),
        }
    }
    fn on_shutdown(&mut self, cx: &mut RuntimeContext<'_>) -> Result<(), Self::Error> {
        self.shutdown_hooks += 1;
        if let Some(mode) = self.shutdown_hook_requests {
            cx.request_shutdown(mode).unwrap();
        }
        if self.fail_shutdown {
            Err("shutdown")
        } else {
            Ok(())
        }
    }
}

type Runtime = IoRuntime<Callback>;

fn runtime(config: RuntimeConfig) -> Runtime {
    IoRuntime::new(config).expect("valid configuration")
}

fn step(rt: &mut Runtime, app: &mut Probe) {
    rt.step(
        app,
        &mut DriverSet::default(),
        Some(Duration::from_millis(5)),
    )
    .unwrap();
}

fn until(rt: &mut Runtime, app: &mut Probe, done: impl Fn(&Probe) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(app) {
        assert!(Instant::now() < deadline, "no progress: {:?}", app.events);
        step(rt, app);
    }
}

/// Runs `body` as a command on the runtime thread and returns its result.
fn in_callback<R: Send + 'static>(
    rt: &mut Runtime,
    app: &mut Probe,
    body: impl FnOnce(&mut RuntimeContext<'_>) -> R + Send + 'static,
) -> R {
    let (sender, receiver) = mpsc::channel();
    let outcome = rt
        .control_handle()
        .try_submit(Box::new(move |cx| {
            sender.send(body(cx)).unwrap();
            Ok(())
        }))
        .ok()
        .expect("mailbox has room");
    assert!(outcome.wake_error.is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "command never ran");
        step(rt, app);
        if let Ok(status) = outcome.receipt.try_recv() {
            assert_eq!(status, CommandStatus::Processed);
            return receiver.recv().unwrap();
        }
    }
}

fn message(bytes: &[u8]) -> Msgb {
    let mut msg = Msgb::new(bytes.len()).unwrap();
    msg.put(bytes.len()).unwrap();
    msg.data_mut().copy_from_slice(bytes);
    msg
}

fn sized(capacity: usize) -> Msgb {
    Msgb::new(capacity).unwrap()
}

fn udp() -> UdpSocket {
    UdpSocket::bind("127.0.0.1:0").unwrap()
}

fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    (client, listener.accept().unwrap().0)
}

fn datagram(bytes: &[u8]) -> Datagram {
    Datagram {
        message: message(bytes),
        destination: Some("127.0.0.1:9".parse().unwrap()),
    }
}

#[test]
fn each_invalid_setting_is_rejected() {
    let edits: [fn(&mut RuntimeConfig); 20] = [
        |c| c.max_resources = 0,
        |c| c.poll_capacity = 0,
        |c| c.command_capacity = 0,
        |c| c.commands_per_step = 0,
        |c| c.effects_per_callback = 0,
        |c| c.shutdown_effects = 0,
        |c| c.notifications_per_step = 0,
        |c| c.notification_capacity = 0,
        |c| c.io_calls_per_resource = 0,
        |c| c.bytes_per_resource = 0,
        |c| c.accepts_per_resource = 0,
        |c| c.max_datagram_size = 0,
        |c| c.per_resource_bytes = 0,
        |c| c.per_resource_messages = 0,
        |c| c.total_bytes = 0,
        |c| c.total_messages = 0,
        |c| c.driver_invocations_per_step = 0,
        |c| c.notification_capacity = 3,
        |c| c.max_poll_interval = Duration::ZERO,
        |c| c.max_datagram_size = 65536,
    ];
    for (index, edit) in edits.iter().enumerate() {
        let mut config = RuntimeConfig::default();
        edit(&mut config);
        assert!(
            matches!(Runtime::new(config), Err(InitError::Configuration)),
            "edit {index}"
        );
    }
    let mut config = RuntimeConfig {
        shutdown_effects: usize::MAX,
        ..RuntimeConfig::default()
    };
    assert!(matches!(
        Runtime::new(config.clone()),
        Err(InitError::Configuration)
    ));
    config.shutdown_effects = 1;
    config.notification_capacity = 4;
    config.max_datagram_size = 65535;
    assert!(Runtime::new(config).is_ok());
}

#[test]
fn errors_describe_and_expose_their_source() {
    let io_error = || std::io::Error::other("boom");
    assert_eq!(
        InitError::Configuration.to_string(),
        "invalid runtime configuration"
    );
    assert!(InitError::Configuration.source().is_none());
    assert!(
        InitError::IdentityExhausted
            .to_string()
            .contains("identity")
    );
    let init = InitError::Io(io_error());
    assert!(init.to_string().contains("boom") && init.source().is_some());

    let rejected = RegistrationError::Rejected(Rejection::Capacity);
    assert!(rejected.to_string().contains("Capacity") && rejected.source().is_none());
    let phase = RegistrationError::Io {
        phase: "register",
        source: io_error(),
    };
    assert!(phase.to_string().contains("register") && phase.source().is_some());

    let application: DriverFailure<std::io::Error> = DriverFailure::Application(io_error());
    assert!(application.to_string().contains("application") && application.source().is_some());
    let component: DriverFailure<std::io::Error> = DriverFailure::Component(Box::new(io_error()));
    assert!(component.to_string().contains("component") && component.source().is_some());
    let clock: DriverFailure<std::io::Error> = DriverFailure::Clock(TimeError::SystemError(1));
    assert!(clock.to_string().contains("clock") && clock.source().is_some());

    let cause: FailureCause<std::io::Error> = FailureCause::Application(io_error());
    assert!(cause.to_string().contains("application") && cause.source().is_some());
    let cause: FailureCause<std::io::Error> = FailureCause::Driver {
        label: "timer",
        failure: DriverFailure::Application(io_error()),
    };
    assert!(cause.to_string().contains("timer") && cause.source().is_some());
    let cause: FailureCause<std::io::Error> = FailureCause::Poll(io_error());
    assert!(cause.to_string().contains("poll") && cause.source().is_some());
    let cause: FailureCause<std::io::Error> = FailureCause::Internal(io_error());
    assert!(cause.to_string().contains("runtime") && cause.source().is_some());

    let enqueue = EnqueueError {
        reason: Rejection::Closed,
        value: 7,
    };
    assert!(format!("{enqueue:?}").contains("Closed"));
}

#[test]
fn a_full_effect_budget_rejects_every_effect_and_returns_values() {
    let mut rt = runtime(RuntimeConfig {
        effects_per_callback: 1,
        ..RuntimeConfig::default()
    });
    let handle = rt
        .register_udp(udp(), UdpResourceOptions::default())
        .unwrap();
    let mut app = Probe::default();
    in_callback(&mut rt, &mut app, move |cx| {
        cx.set_read_paused(handle, true).unwrap();
        assert_eq!(cx.close(handle), Err(Rejection::Capacity));
        assert_eq!(cx.set_read_paused(handle, false), Err(Rejection::Capacity));
        assert_eq!(
            cx.request_shutdown(ShutdownMode::Immediate),
            Err(Rejection::Capacity)
        );
        let send = cx.try_send_udp(handle, datagram(b"x")).err().unwrap();
        assert_eq!(send.reason, Rejection::Capacity);
        assert_eq!(send.value.message.data(), b"x");
        let socket = udp();
        let local = socket.local_addr().unwrap();
        let udp = cx
            .register_udp(socket, UdpResourceOptions::default())
            .err()
            .unwrap();
        assert_eq!(udp.reason, Rejection::Capacity);
        assert_eq!(udp.value.local_addr().unwrap(), local);
        let (client, _server) = tcp_pair();
        let tcp = cx
            .register_tcp(client, TcpResourceOptions::default())
            .err()
            .unwrap();
        assert_eq!(tcp.reason, Rejection::Capacity);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let listen = cx
            .register_listener(listener, ListenerOptions::default())
            .err()
            .unwrap();
        assert_eq!(listen.reason, Rejection::Capacity);
    });
}

#[test]
fn a_shutdown_request_closes_the_callback_to_new_work() {
    let mut rt = runtime(RuntimeConfig::default());
    let handle = rt
        .register_udp(udp(), UdpResourceOptions::default())
        .unwrap();
    let mut app = Probe::default();
    in_callback(&mut rt, &mut app, move |cx| {
        assert_eq!(
            cx.request_shutdown(ShutdownMode::Drain {
                timeout: Duration::MAX
            }),
            Err(Rejection::Exhausted)
        );
        cx.request_shutdown(ShutdownMode::Immediate).unwrap();
        let send = cx.try_send_udp(handle, datagram(b"x")).err().unwrap();
        assert_eq!(send.reason, Rejection::Lifecycle);
        assert_eq!(cx.set_read_paused(handle, true), Err(Rejection::Lifecycle));
        let register = cx
            .register_udp(udp(), UdpResourceOptions::default())
            .err()
            .unwrap();
        assert_eq!(register.reason, Rejection::Lifecycle);
    });
}

#[test]
fn each_output_quota_rejects_and_returns_the_message() {
    let cases: [(RuntimeConfig, usize); 4] = [
        (
            RuntimeConfig {
                per_resource_bytes: 8,
                ..RuntimeConfig::default()
            },
            8,
        ),
        (
            RuntimeConfig {
                total_bytes: 8,
                ..RuntimeConfig::default()
            },
            8,
        ),
        (
            RuntimeConfig {
                per_resource_messages: 1,
                ..RuntimeConfig::default()
            },
            1,
        ),
        (
            RuntimeConfig {
                total_messages: 1,
                ..RuntimeConfig::default()
            },
            1,
        ),
    ];
    for (config, first) in cases {
        let mut rt = runtime(config);
        let (client, _server) = tcp_pair();
        let handle = rt
            .register_tcp(client, TcpResourceOptions::default())
            .unwrap();
        let mut app = Probe::default();
        in_callback(&mut rt, &mut app, move |cx| {
            cx.try_send_tcp(handle, sized(first)).unwrap();
            let rejected = cx.try_send_tcp(handle, sized(1)).err().unwrap();
            assert_eq!(rejected.reason, Rejection::Capacity);
            assert_eq!(rejected.value.capacity(), 1);
        });
    }
}

#[test]
fn resource_limit_applies_to_direct_and_deferred_registration() {
    let mut rt = runtime(RuntimeConfig {
        max_resources: 2,
        ..RuntimeConfig::default()
    });
    rt.register_udp(udp(), UdpResourceOptions::default())
        .unwrap();
    let mut app = Probe::default();
    in_callback(&mut rt, &mut app, |cx| {
        cx.register_udp(udp(), UdpResourceOptions::default())
            .unwrap();
        // One live resource and one pending registration fill the limit.
        let rejected = cx
            .register_udp(udp(), UdpResourceOptions::default())
            .err()
            .unwrap();
        assert_eq!(rejected.reason, Rejection::Capacity);
    });
    until(&mut rt, &mut app, |app| app.udp.len() == 1);
    assert!(matches!(
        rt.register_udp(udp(), UdpResourceOptions::default()),
        Err(RegistrationError::Rejected(Rejection::Capacity))
    ));
}

#[test]
fn deferred_registration_covers_every_resource_kind() {
    let mut rt = runtime(RuntimeConfig::default());
    let mut app = Probe::default();
    let socket = udp();
    let target = socket.local_addr().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (client, mut peer) = tcp_pair();
    in_callback(&mut rt, &mut app, move |cx| {
        cx.register_udp(socket, UdpResourceOptions::default())
            .unwrap();
        cx.register_listener(
            listener,
            ListenerOptions {
                accepted: TcpResourceOptions {
                    nodelay: true,
                    ..TcpResourceOptions::default()
                },
            },
        )
        .unwrap();
        cx.register_tcp(
            client,
            TcpResourceOptions {
                nodelay: true,
                ..TcpResourceOptions::default()
            },
        )
        .unwrap();
    });
    until(&mut rt, &mut app, |app| {
        app.udp.len() == 1 && app.listeners.len() == 1 && app.tcp.len() == 1
    });

    udp().send_to(b"hello", target).unwrap();
    until(&mut rt, &mut app, |app| {
        app.datagrams == [b"hello".to_vec()]
    });
    peer.write_all(b"tcp").unwrap();
    until(&mut rt, &mut app, |app| app.bytes == b"tcp");
    let _accepted = TcpStream::connect(address).unwrap();
    until(&mut rt, &mut app, |app| app.streams.len() == 1);

    let listener = app.listeners[0];
    in_callback(&mut rt, &mut app, move |cx| cx.close(listener).unwrap());
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "closed")
    });
    assert_eq!(app.registration_errors, 0);
}

#[test]
fn the_accept_budget_spreads_a_backlog_over_steps() {
    let mut rt = runtime(RuntimeConfig {
        accepts_per_resource: 1,
        ..RuntimeConfig::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    rt.register_listener(listener, ListenerOptions::default())
        .unwrap();
    let _clients: Vec<_> = (0..3)
        .map(|_| TcpStream::connect(address).unwrap())
        .collect();
    let mut app = Probe::default();
    until(&mut rt, &mut app, |app| !app.streams.is_empty());
    assert_eq!(app.streams.len(), 1);
    until(&mut rt, &mut app, |app| app.streams.len() == 3);
}

#[test]
fn a_tiny_notification_budget_still_delivers_everything_in_order() {
    let mut rt = runtime(RuntimeConfig {
        notification_capacity: 4,
        notifications_per_step: 1,
        io_calls_per_resource: 2,
        ..RuntimeConfig::default()
    });
    let socket = udp();
    let target = socket.local_addr().unwrap();
    rt.register_udp(socket, UdpResourceOptions::default())
        .unwrap();
    let sender = udp();
    let expected: Vec<Vec<u8>> = (0..20).map(|index| vec![index]).collect();
    for datagram in &expected {
        sender.send_to(datagram, target).unwrap();
    }
    let mut app = Probe::default();
    until(&mut rt, &mut app, |app| app.datagrams.len() == 20);
    assert_eq!(app.datagrams, expected);
}

#[test]
fn many_resources_share_a_tiny_notification_budget_with_a_driver() {
    let mut rt = runtime(RuntimeConfig {
        notification_capacity: 4,
        notifications_per_step: 1,
        ..RuntimeConfig::default()
    });
    let sender = udp();
    for index in 0..8 {
        let socket = udp();
        sender
            .send_to(&[index], socket.local_addr().unwrap())
            .unwrap();
        rt.register_udp(socket, UdpResourceOptions::default())
            .unwrap();
    }
    let drives = Rc::new(Cell::new(0));
    let mut drivers = DriverSet::new(1);
    drivers
        .add_driver(
            "script",
            Script {
                drives: drives.clone(),
                ..Script::new()
            },
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    let mut app = Probe::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.datagrams.len() < 8 {
        assert!(Instant::now() < deadline, "no progress: {:?}", app.events);
        rt.step(&mut app, &mut drivers, Some(Duration::from_millis(5)))
            .unwrap();
    }
    let mut received = app.datagrams.clone();
    received.sort();
    assert_eq!(
        received,
        (0..8).map(|index| vec![index]).collect::<Vec<_>>()
    );
    assert!(drives.get() > 0);
}

#[test]
fn a_shutdown_hook_can_shorten_or_escalate_a_drain() {
    for (mode, stops_at_once) in [
        (ShutdownMode::Immediate, true),
        (
            ShutdownMode::Drain {
                timeout: Duration::from_secs(60),
            },
            false,
        ),
    ] {
        let mut rt = runtime(RuntimeConfig {
            io_calls_per_resource: 1,
            bytes_per_resource: 1,
            ..RuntimeConfig::default()
        });
        let (client, _server) = tcp_pair();
        let handle = rt
            .register_tcp(client, TcpResourceOptions::default())
            .unwrap();
        let mut app = Probe::default();
        in_callback(&mut rt, &mut app, move |cx| {
            cx.try_send_tcp(handle, message(b"abcdef")).unwrap();
        });
        app.shutdown_hook_requests = Some(mode);
        rt.control_handle()
            .request_shutdown(ShutdownMode::Drain {
                timeout: Duration::from_secs(30),
            })
            .unwrap();
        let mut steps = 0;
        while rt
            .step(
                &mut app,
                &mut DriverSet::default(),
                Some(Duration::from_millis(1)),
            )
            .unwrap()
            != StepOutcome::Stopped
        {
            steps += 1;
            assert!(steps < 1000, "never stopped");
        }
        assert_eq!(app.shutdown_hooks, 1);
        // An immediate stop cancels the output, while a drain finishes it.
        let report = rt.take_shutdown_report().unwrap();
        let cancelled = report
            .writes
            .iter()
            .any(|write| matches!(write.status, WriteStatus::Cancelled));
        assert_eq!(cancelled, stops_at_once);
    }
}

#[test]
fn a_reset_connection_reports_a_read_failure_and_closes() {
    let (server, client) = tcp_pair();
    let mut rt = runtime(RuntimeConfig::default());
    let handle = rt
        .register_tcp(
            server,
            TcpResourceOptions {
                read_paused: true,
                ..TcpResourceOptions::default()
            },
        )
        .unwrap();
    let mut app = Probe::default();
    // The server's bytes are unread when the client closes, which resets the connection.
    in_callback(&mut rt, &mut app, move |cx| {
        cx.try_send_tcp(handle, message(b"x")).unwrap();
    });
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "write")
    });
    std::thread::sleep(Duration::from_millis(20));
    drop(client);
    std::thread::sleep(Duration::from_millis(20));
    in_callback(&mut rt, &mut app, move |cx| {
        cx.set_read_paused(handle, false).unwrap();
    });
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "closed")
    });
    assert!(
        app.events.iter().any(|event| event == "transport:read"),
        "{:?}",
        app.events
    );
}

#[test]
fn writing_to_a_reset_connection_fails_the_write_and_closes() {
    let (server, client) = tcp_pair();
    let mut rt = runtime(RuntimeConfig::default());
    let handle = rt
        .register_tcp(
            server,
            TcpResourceOptions {
                read_paused: true,
                ..TcpResourceOptions::default()
            },
        )
        .unwrap();
    let mut app = Probe::default();
    in_callback(&mut rt, &mut app, move |cx| {
        cx.try_send_tcp(handle, message(b"unread")).unwrap();
    });
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "write")
    });
    std::thread::sleep(Duration::from_millis(20));
    drop(client);
    std::thread::sleep(Duration::from_millis(20));
    in_callback(&mut rt, &mut app, move |cx| {
        cx.try_send_tcp(handle, message(b"late")).unwrap();
    });
    until(&mut rt, &mut app, |app| app.failed_writes == 1);
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "closed")
    });
}

/// What each event needs in order to happen.
fn provoke(event: &'static str) -> (Runtime, Vec<TcpStream>) {
    let mut rt = runtime(RuntimeConfig {
        max_datagram_size: 4,
        ..RuntimeConfig::default()
    });
    let mut keep = Vec::new();
    match event {
        "accepted" => {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            keep.push(TcpStream::connect(listener.local_addr().unwrap()).unwrap());
            rt.register_listener(listener, ListenerOptions::default())
                .unwrap();
        }
        "tcp_bytes" => {
            let (server, mut client) = tcp_pair();
            client.write_all(b"x").unwrap();
            rt.register_tcp(server, TcpResourceOptions::default())
                .unwrap();
            keep.push(client);
        }
        "peer_eof" => {
            let (server, client) = tcp_pair();
            drop(client);
            rt.register_tcp(server, TcpResourceOptions::default())
                .unwrap();
        }
        "datagram" | "oversized" => {
            let socket = udp();
            let bytes: &[u8] = if event == "datagram" {
                b"x"
            } else {
                b"too long"
            };
            udp().send_to(bytes, socket.local_addr().unwrap()).unwrap();
            rt.register_udp(socket, UdpResourceOptions::default())
                .unwrap();
        }
        other => panic!("unknown event {other}"),
    }
    (rt, keep)
}

fn failure_of(rt: &mut Runtime, app: &mut Probe) -> RunFailure<&'static str> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "no failure: {:?}", app.events);
        match rt.step(
            app,
            &mut DriverSet::default(),
            Some(Duration::from_millis(5)),
        ) {
            Ok(StepOutcome::Progress) => {}
            Ok(StepOutcome::Stopped) => panic!("stopped without a failure"),
            Err(failure) => return failure,
        }
    }
}

#[test]
fn a_failing_callback_stops_the_runtime_for_each_input_event() {
    for event in ["accepted", "tcp_bytes", "peer_eof", "datagram", "oversized"] {
        let (mut rt, _keep) = provoke(event);
        let control = rt.control_handle();
        let mut app = Probe {
            fail_on: Some(event),
            ..Probe::default()
        };
        let failure = failure_of(&mut rt, &mut app);
        assert!(
            matches!(failure.cause, FailureCause::Application(name) if name == event),
            "{event}"
        );
        assert!(matches!(
            control.try_submit(Box::new(|_| Ok(()))),
            Err(SubmitError::Stopped(_))
        ));
        assert!(matches!(
            control.request_shutdown(ShutdownMode::Immediate),
            Err(ControlError::Stopped)
        ));
    }
}

#[test]
fn a_failing_callback_stops_the_runtime_for_each_notification() {
    for event in ["write", "registered", "closed"] {
        let mut rt = runtime(RuntimeConfig::default());
        let (client, _server) = tcp_pair();
        let handle = rt
            .register_tcp(client, TcpResourceOptions::default())
            .unwrap();
        let mut app = Probe {
            fail_on: Some(event),
            ..Probe::default()
        };
        rt.control_handle()
            .try_submit(Box::new(move |cx| {
                match event {
                    "write" => {
                        cx.try_send_tcp(handle, message(b"x")).ok().unwrap();
                    }
                    "registered" => {
                        cx.register_udp(udp(), UdpResourceOptions::default())
                            .ok()
                            .unwrap();
                    }
                    _ => cx.close(handle).unwrap(),
                }
                Ok(())
            }))
            .ok()
            .unwrap();
        let failure = failure_of(&mut rt, &mut app);
        assert!(
            matches!(failure.cause, FailureCause::Application(name) if name == event),
            "{event}"
        );
    }
}

#[test]
fn a_failing_command_resolves_its_receipt_as_failed() {
    let mut rt = runtime(RuntimeConfig::default());
    let mut app = Probe::default();
    let outcome = rt
        .control_handle()
        .try_submit(Box::new(|_| Err("command")))
        .ok()
        .unwrap();
    assert!(matches!(
        outcome.receipt.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let failure = failure_of(&mut rt, &mut app);
    assert!(matches!(
        failure.cause,
        FailureCause::Application("command")
    ));
    assert_eq!(outcome.receipt.try_recv(), Ok(CommandStatus::Failed));
}

#[test]
fn a_failing_shutdown_hook_stops_with_its_error() {
    let mut rt = runtime(RuntimeConfig::default());
    let mut app = Probe {
        fail_shutdown: true,
        ..Probe::default()
    };
    rt.control_handle()
        .request_shutdown(ShutdownMode::Immediate)
        .unwrap();
    let failure = failure_of(&mut rt, &mut app);
    assert!(matches!(
        failure.cause,
        FailureCause::Application("shutdown")
    ));
    assert_eq!(app.shutdown_hooks, 1);
}

#[test]
fn run_returns_the_report_once_the_runtime_stops() {
    let mut rt = runtime(RuntimeConfig {
        max_poll_interval: Duration::from_millis(5),
        ..RuntimeConfig::default()
    });
    let control = rt.control_handle();
    let mut app = Probe::default();
    control
        .try_submit(Box::new(|cx| {
            cx.request_shutdown(ShutdownMode::Immediate)
                .map_err(|_| "x")
        }))
        .ok()
        .unwrap();
    let report = rt.run(&mut app, &mut DriverSet::default()).ok().unwrap();
    assert!(!report.timed_out && report.writes.is_empty());
    assert_eq!(app.shutdown_hooks, 1);
    assert!(matches!(
        control.request_shutdown(ShutdownMode::Immediate),
        Err(ControlError::Stopped)
    ));
}

#[test]
fn control_rejects_commands_once_shutdown_is_requested_and_overlong_deadlines() {
    let rt = runtime(RuntimeConfig::default());
    let control = rt.control_handle();
    assert!(matches!(
        control.request_shutdown(ShutdownMode::Drain {
            timeout: Duration::MAX
        }),
        Err(ControlError::DeadlineOverflow)
    ));
    let shutdown = control
        .request_shutdown(ShutdownMode::Drain {
            timeout: Duration::from_secs(1),
        })
        .unwrap();
    assert!(shutdown.wake_error.is_none());
    assert!(matches!(
        control.try_submit(Box::new(|_| Ok(()))),
        Err(SubmitError::Stopped(_))
    ));
}

#[test]
fn queue_usage_tracks_output_as_it_drains() {
    let mut rt = runtime(RuntimeConfig {
        io_calls_per_resource: 1,
        bytes_per_resource: 1,
        ..RuntimeConfig::default()
    });
    let (client, _server) = tcp_pair();
    let handle = rt
        .register_tcp(client, TcpResourceOptions::default())
        .unwrap();
    let mut app = Probe::default();
    let usage = rt.queue_usage();
    assert_eq!(
        (
            usage.retained_bytes,
            usage.messages,
            usage.pending_wire_bytes
        ),
        (0, 0, 0)
    );
    let mut msg = sized(16);
    msg.put(4).unwrap();
    in_callback(&mut rt, &mut app, move |cx| {
        cx.try_send_tcp(handle, msg).unwrap();
    });
    let usage = rt.queue_usage();
    assert_eq!((usage.retained_bytes, usage.messages), (16, 1));
    assert!(usage.pending_wire_bytes < 4, "{}", usage.pending_wire_bytes);
    until(&mut rt, &mut app, |app| {
        app.events.iter().any(|event| event == "write")
    });
    assert_eq!(rt.queue_usage().pending_wire_bytes, 0);
}

/// A clock that succeeds for its first `reads`, then fails.
#[derive(Clone)]
struct LimitedClock(Rc<Cell<u32>>);

impl LimitedClock {
    fn read(&self) -> Result<(), TimeError> {
        match self.0.get() {
            0 => Err(TimeError::SystemError(1)),
            left => {
                self.0.set(left - 1);
                Ok(())
            }
        }
    }
}

impl Clock for LimitedClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        self.read().map(|()| TimeVal::new(100, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        self.read().map(|()| TimeSpec::new(100, 0))
    }
}

/// Fails as told at each stage of the driver contract.
struct Script {
    prepare_failures_after: Option<u32>,
    drive_fails: bool,
    drive_idle: bool,
    hint: WakeHint,
    drives: Rc<Cell<u32>>,
}

impl Script {
    fn new() -> Self {
        Self {
            prepare_failures_after: None,
            drive_fails: false,
            drive_idle: false,
            hint: WakeHint::Idle,
            drives: Rc::default(),
        }
    }
}

impl RuntimeDriver<Probe> for Script {
    fn prepare(&mut self, _: &mut Probe) -> Result<WakeHint, DriverFailure<&'static str>> {
        match &mut self.prepare_failures_after {
            Some(0) => Err(DriverFailure::Application("prepare")),
            Some(left) => {
                *left -= 1;
                Ok(self.hint)
            }
            None => Ok(self.hint),
        }
    }
    fn drive(
        &mut self,
        _: &mut Probe,
        _: &mut RuntimeContext<'_>,
    ) -> Result<DriverProgress, DriverFailure<&'static str>> {
        self.drives.set(self.drives.get() + 1);
        if self.drive_fails {
            Err(DriverFailure::Application("drive"))
        } else if self.drive_idle {
            Ok(DriverProgress::Idle)
        } else {
            Ok(DriverProgress::Progress)
        }
    }
}

#[test]
fn driver_set_has_a_limit() {
    let mut drivers = DriverSet::<Probe>::new(1);
    drivers
        .add_driver("first", Script::new(), DriverFailurePolicy::Critical)
        .unwrap();
    assert_eq!(
        drivers.add_driver("second", Script::new(), DriverFailurePolicy::Critical),
        Err(DriverRegistrationError::Capacity)
    );
}

#[test]
fn an_optional_driver_is_disabled_once_and_keeps_its_failure() {
    // Failing in the first prepare, the second prepare of a step, and in drive.
    for (name, prepare_failures_after, drive_fails, failing) in [
        ("first prepare", Some(0), false, "prepare"),
        ("wait prepare", Some(1), false, "prepare"),
        ("drive", None, true, "drive"),
    ] {
        let drives = Rc::new(Cell::new(0));
        let mut drivers = DriverSet::new(2);
        drivers
            .add_driver(
                "script",
                Script {
                    prepare_failures_after,
                    drive_fails,
                    hint: WakeHint::Recheck,
                    drives: drives.clone(),
                    ..Script::new()
                },
                DriverFailurePolicy::Optional,
            )
            .unwrap();
        let mut rt = runtime(RuntimeConfig::default());
        let mut app = Probe::default();
        for _ in 0..4 {
            rt.step(&mut app, &mut drivers, Some(Duration::ZERO))
                .ok()
                .unwrap();
        }
        let disabled = app
            .events
            .iter()
            .filter(|event| *event == "disabled:script")
            .count();
        assert_eq!(disabled, 1, "{name}: {:?}", app.events);
        let driven = drives.get();
        assert!(driven <= 1, "{name}: driven {driven} times");
        let failures: Vec<_> = drivers.take_failures().collect();
        assert_eq!(failures.len(), 1, "{name}");
        assert_eq!(failures[0].0, "script");
        assert!(
            matches!(failures[0].1, DriverFailure::Application(text) if text == failing),
            "{name}"
        );
        assert_eq!(drivers.take_failures().count(), 0, "{name}");
    }
}

#[test]
fn a_critical_driver_failure_stops_the_runtime() {
    for (name, prepare_failures_after, drive_fails, failing) in [
        ("first prepare", Some(0), false, "prepare"),
        ("wait prepare", Some(1), false, "prepare"),
        ("drive", None, true, "drive"),
    ] {
        let mut drivers = DriverSet::new(1);
        drivers
            .add_driver(
                "script",
                Script {
                    prepare_failures_after,
                    drive_fails,
                    ..Script::new()
                },
                DriverFailurePolicy::Critical,
            )
            .unwrap();
        let mut rt = runtime(RuntimeConfig::default());
        let mut app = Probe::default();
        let failure = rt
            .step(&mut app, &mut drivers, Some(Duration::ZERO))
            .err()
            .unwrap_or_else(|| panic!("{name}: no failure"));
        match failure.cause {
            FailureCause::Driver {
                label: "script",
                failure: DriverFailure::Application(text),
            } => assert_eq!(text, failing, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn a_driver_wake_hint_shortens_the_poll() {
    let mut drivers = DriverSet::new(1);
    drivers
        .add_driver(
            "soon",
            Script {
                hint: WakeHint::After(Duration::from_millis(1)),
                // Progress would queue a notification, and the poll wouldn't wait at all.
                drive_idle: true,
                ..Script::new()
            },
            DriverFailurePolicy::Critical,
        )
        .unwrap();
    let mut rt = runtime(RuntimeConfig {
        max_poll_interval: Duration::from_secs(10),
        ..RuntimeConfig::default()
    });
    let mut app = Probe::default();
    let started = Instant::now();
    rt.step(&mut app, &mut drivers, None).ok().unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

fn expire(_: &mut TimerScheduler<LimitedClock, ()>, _: TimerId, _: &mut ()) {}

/// Application state for the typed drivers.
struct Typed {
    probe: Probe,
    scheduler: TimerScheduler<LimitedClock, ()>,
    context: (),
    rate: osmocore_rs::base::rate_ctr::RateCtrPeriodicDriver<LimitedClock>,
    counters: osmocore_rs::base::rate_ctr::RateCtrRuntime,
}

impl Application for Typed {
    type Command = Callback;
    type Error = &'static str;
    fn on_command(
        &mut self,
        cx: &mut RuntimeContext<'_>,
        run: Callback,
    ) -> Result<(), Self::Error> {
        self.probe.on_command(cx, run)
    }
    fn on_event(
        &mut self,
        cx: &mut RuntimeContext<'_>,
        event: RuntimeEvent<'_>,
    ) -> Result<(), Self::Error> {
        self.probe.on_event(cx, event)
    }
}

fn typed(reads: u32) -> (Typed, Rc<Cell<u32>>) {
    let budget = Rc::new(Cell::new(reads));
    let clock = LimitedClock(budget.clone());
    (
        Typed {
            probe: Probe::default(),
            scheduler: TimerScheduler::new(clock.clone()),
            context: (),
            rate: osmocore_rs::base::rate_ctr::RateCtrPeriodicDriver::new(clock),
            counters: osmocore_rs::base::rate_ctr::RateCtrRuntime::new(),
        },
        budget,
    )
}

fn timer_driver() -> TimerDriver<LimitedClock, (), Typed> {
    TimerDriver::new(|app| (&mut app.scheduler, &mut app.context))
}

fn rate_driver() -> RateDriver<LimitedClock, Typed> {
    RateDriver::new(|app| (&mut app.rate, &mut app.counters))
}

#[test]
fn idle_typed_drivers_hint_idle() {
    // A rate driver that hasn't started doesn't read its clock.
    let (mut app, _) = typed(0);
    assert!(matches!(
        rate_driver().prepare(&mut app),
        Ok(WakeHint::Idle)
    ));
    // A timer driver reads it even with nothing scheduled.
    assert!(matches!(
        timer_driver().prepare(&mut app),
        Err(DriverFailure::Component(_))
    ));
    let (mut app, _) = typed(1);
    assert!(matches!(
        timer_driver().prepare(&mut app),
        Ok(WakeHint::Idle)
    ));
}

#[test]
fn typed_drivers_report_clock_failures() {
    // A running timer whose clock fails on the next read.
    let (mut app, budget) = typed(10);
    let id = app.scheduler.create(Some(expire));
    app.scheduler.schedule(&id, 60, 0).unwrap();
    budget.set(0);
    assert!(matches!(
        timer_driver().prepare(&mut app),
        Err(DriverFailure::Component(_))
    ));

    let (mut app, budget) = typed(10);
    app.rate.start().unwrap();
    budget.set(0);
    assert!(matches!(
        rate_driver().prepare(&mut app),
        Err(DriverFailure::Clock(_))
    ));
}

#[test]
fn typed_drivers_report_failures_while_driving() {
    let mut rt: IoRuntime<Callback> = IoRuntime::new(RuntimeConfig::default()).ok().unwrap();
    let (mut app, budget) = typed(10);
    let id = app.scheduler.create(Some(expire));
    app.scheduler.schedule(&id, 60, 0).unwrap();
    let mut drivers = DriverSet::new(1);
    drivers
        .add_driver("timer", timer_driver(), DriverFailurePolicy::Critical)
        .unwrap();
    // Enough for the first prepare, and not for the update that follows it.
    budget.set(1);
    let failure = rt
        .step(&mut app, &mut drivers, Some(Duration::ZERO))
        .expect_err("clock failure");
    assert!(matches!(
        failure.cause,
        FailureCause::Driver {
            label: "timer",
            failure: DriverFailure::Component(_)
        }
    ));

    let mut rt: IoRuntime<Callback> = IoRuntime::new(RuntimeConfig::default()).ok().unwrap();
    let (mut app, budget) = typed(10);
    app.rate.start().unwrap();
    let mut drivers = DriverSet::new(1);
    drivers
        .add_driver("rate", rate_driver(), DriverFailurePolicy::Critical)
        .unwrap();
    budget.set(1);
    let failure = rt
        .step(&mut app, &mut drivers, Some(Duration::ZERO))
        .expect_err("clock failure");
    assert!(matches!(
        failure.cause,
        FailureCause::Driver {
            label: "rate",
            failure: DriverFailure::Component(_)
        }
    ));
}
