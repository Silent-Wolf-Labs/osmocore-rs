//! Real loopback establishment and configuration contracts; no public services.
#![cfg(feature = "net")]

use osmocore_rs::net::{self, AddressFamily, NetError, Operation, SocketOptions, TcpOptions};
use socket2::SockRef;
use std::{
    error::Error,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, UdpSocket},
    time::Duration,
};

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}
const TIMEOUT: Duration = Duration::from_secs(2);
fn expect_error<T>(result: Result<T, NetError>, operation: Operation, kind: io::ErrorKind) {
    let error = match result {
        Ok(_) => panic!("unexpected success"),
        Err(error) => error,
    };
    assert_eq!(error.operation(), operation);
    assert_eq!(error.io_error().kind(), kind);
    assert!(error.source().is_some());
    assert!(error.to_string().starts_with(operation.as_str()));
    assert_eq!(error.into_io_error().kind(), kind);
}

#[test]
fn explicit_resolution_preserves_port_and_filters_address_family() {
    assert_eq!(
        net::resolve("127.0.0.1", 1234, AddressFamily::Any).unwrap(),
        ["127.0.0.1:1234".parse::<SocketAddr>().unwrap()]
    );
    assert_eq!(
        net::resolve("::1", 4321, AddressFamily::Ipv6).unwrap(),
        ["[::1]:4321".parse::<SocketAddr>().unwrap()]
    );
    expect_error(
        net::resolve("127.0.0.1", 1, AddressFamily::Ipv6),
        Operation::Resolve,
        io::ErrorKind::AddrNotAvailable,
    );
    let addresses = net::resolve("localhost", 3210, AddressFamily::Any).unwrap();
    assert!(!addresses.is_empty());
    assert!(addresses.iter().all(|a| a.port() == 3210));
    for (i, address) in addresses.iter().enumerate() {
        assert!(!addresses[..i].contains(address));
    }
    assert!(
        net::resolve("localhost", 3210, AddressFamily::Ipv4)
            .unwrap()
            .iter()
            .all(SocketAddr::is_ipv4)
    );
}

#[test]
fn tcp_connect_binds_local_endpoint_and_returns_configured_native_stream() {
    let listener = net::listen_tcp(loopback(), 16, &TcpOptions::default()).unwrap();
    let local_reservation = TcpListener::bind(loopback()).unwrap();
    let source = local_reservation.local_addr().unwrap();
    drop(local_reservation);
    let options = TcpOptions {
        nodelay: true,
        keepalive: true,
        ..TcpOptions::default()
    };
    let mut client = net::connect_tcp(
        Some(source),
        listener.local_addr().unwrap(),
        TIMEOUT,
        &options,
    )
    .unwrap();
    client.set_read_timeout(Some(TIMEOUT)).unwrap();
    client.set_write_timeout(Some(TIMEOUT)).unwrap();
    let (mut server, peer) = listener.accept().unwrap();
    server.set_read_timeout(Some(TIMEOUT)).unwrap();
    server.set_write_timeout(Some(TIMEOUT)).unwrap();
    assert_eq!(client.local_addr().unwrap(), source);
    assert_eq!(peer, source);
    assert!(client.nodelay().unwrap());
    assert!(SockRef::from(&client).keepalive().unwrap());
    client.write_all(b"tcp").unwrap();
    let mut data = [0; 3];
    server.read_exact(&mut data).unwrap();
    assert_eq!(&data, b"tcp");
    server.write_all(b"ack").unwrap();
    client.read_exact(&mut data).unwrap();
    assert_eq!(&data, b"ack");
}

#[test]
fn udp_bound_and_connected_sockets_exchange_datagrams_and_keep_source_binding() {
    let receiver = net::bind_udp(loopback(), None, &SocketOptions::default()).unwrap();
    let sender = net::bind_udp(
        loopback(),
        Some(receiver.local_addr().unwrap()),
        &SocketOptions::default(),
    )
    .unwrap();
    sender.set_read_timeout(Some(TIMEOUT)).unwrap();
    receiver.set_read_timeout(Some(TIMEOUT)).unwrap();
    assert_eq!(sender.peer_addr().unwrap(), receiver.local_addr().unwrap());
    assert!(sender.local_addr().unwrap().port() != 0);
    assert_eq!(sender.send(b"udp").unwrap(), 3);
    let mut buffer = [0; 16];
    let (len, from) = receiver.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..len], b"udp");
    assert_eq!(from, sender.local_addr().unwrap());
    receiver.send_to(b"reply", from).unwrap();
    let len = sender.recv(&mut buffer).unwrap();
    assert_eq!(&buffer[..len], b"reply");
}

#[test]
fn nonblocking_mode_is_applied_to_listener_connected_stream_and_udp() {
    let socket = SocketOptions {
        nonblocking: true,
        ..SocketOptions::default()
    };
    let listener = net::listen_tcp(
        loopback(),
        4,
        &TcpOptions {
            socket: socket.clone(),
            ..TcpOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut stream = net::connect_tcp(
        None,
        listener.local_addr().unwrap(),
        TIMEOUT,
        &TcpOptions {
            socket: socket.clone(),
            ..TcpOptions::default()
        },
    )
    .unwrap();
    let mut byte = [0];
    assert_eq!(
        stream.read(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let udp = net::bind_udp(loopback(), None, &socket).unwrap();
    assert_eq!(
        udp.recv_from(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn invalid_configuration_is_rejected_before_any_socket_binding() {
    let remote = "[::1]:1234".parse().unwrap();
    expect_error(
        net::connect_tcp(Some(loopback()), remote, TIMEOUT, &TcpOptions::default()),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
    expect_error(
        net::bind_udp(loopback(), Some(remote), &SocketOptions::default()),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
    expect_error(
        net::connect_tcp(None, loopback(), Duration::ZERO, &TcpOptions::default()),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
    for backlog in [0, u32::MAX] {
        expect_error(
            net::listen_tcp(loopback(), backlog, &TcpOptions::default()),
            Operation::Validate,
            io::ErrorKind::InvalidInput,
        );
    }
    let invalid_dscp = SocketOptions {
        dscp: Some(64),
        ..SocketOptions::default()
    };
    expect_error(
        net::bind_udp(loopback(), None, &invalid_dscp),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
    let only_v6 = SocketOptions {
        ipv6_only: Some(true),
        ..SocketOptions::default()
    };
    expect_error(
        net::bind_udp(loopback(), None, &only_v6),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
    expect_error(
        net::bind_udp("[::ffff:127.0.0.1]:0".parse().unwrap(), None, &only_v6),
        Operation::Validate,
        io::ErrorKind::InvalidInput,
    );
}

#[test]
fn bind_failure_keeps_its_phase_and_os_error() {
    let occupied = net::bind_udp(loopback(), None, &SocketOptions::default()).unwrap();
    let error = net::bind_udp(
        occupied.local_addr().unwrap(),
        None,
        &SocketOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error.operation(), Operation::Bind);
    assert_eq!(error.io_error().kind(), io::ErrorKind::AddrInUse);
    assert!(error.io_error().raw_os_error().is_some());
}

#[test]
fn local_binding_is_released_on_connection_failure() {
    // A bound, non-listening TCP socket reserves a port without accepting connects.
    let remote = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    remote.bind(&loopback().into()).unwrap();
    let peer = remote.local_addr().unwrap().as_socket().unwrap();
    let reservation = TcpListener::bind(loopback()).unwrap();
    let local = reservation.local_addr().unwrap();
    drop(reservation);
    expect_error(
        net::connect_tcp(Some(local), peer, TIMEOUT, &TcpOptions::default()),
        Operation::Connect,
        io::ErrorKind::ConnectionRefused,
    );
    let rebound = TcpListener::bind(local).unwrap();
    assert_eq!(rebound.local_addr().unwrap(), local);
}

#[test]
fn prebind_options_are_visible_on_returned_sockets() {
    let options = SocketOptions {
        reuse_address: true,
        ..SocketOptions::default()
    };
    let udp = net::bind_udp(loopback(), None, &options).unwrap();
    assert!(SockRef::from(&udp).reuse_address().unwrap());
    let listener = net::listen_tcp(
        loopback(),
        16,
        &TcpOptions {
            socket: options,
            ..TcpOptions::default()
        },
    )
    .unwrap();
    assert!(SockRef::from(&listener).reuse_address().unwrap());
}

#[cfg(target_os = "linux")]
#[test]
fn ipv4_dscp_supports_zero_and_full_six_bit_range() {
    for dscp in [0, 1, 46, 63] {
        let udp = net::bind_udp(
            loopback(),
            None,
            &SocketOptions {
                dscp: Some(dscp),
                ..SocketOptions::default()
            },
        )
        .unwrap();
        assert_eq!(SockRef::from(&udp).tos_v4().unwrap() >> 2, u32::from(dscp));
    }
}

#[test]
fn ipv6_tcp_udp_and_explicit_v6_only_work_when_loopback_is_available() {
    let local: SocketAddr = "[::1]:0".parse().unwrap();
    let probe = match UdpSocket::bind(local) {
        Ok(socket) => socket,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::AddrNotAvailable
            ) =>
        {
            eprintln!("IPv6 loopback unavailable: {e}");
            return;
        }
        Err(e) => panic!("IPv6 probe failed: {e}"),
    };
    drop(probe);
    let socket = SocketOptions {
        ipv6_only: Some(true),
        ..SocketOptions::default()
    };
    let listener = net::listen_tcp(
        local,
        8,
        &TcpOptions {
            socket: socket.clone(),
            ..TcpOptions::default()
        },
    )
    .unwrap();
    assert!(SockRef::from(&listener).only_v6().unwrap());
    let stream = net::connect_tcp(
        Some(local),
        listener.local_addr().unwrap(),
        TIMEOUT,
        &TcpOptions {
            socket: socket.clone(),
            ..TcpOptions::default()
        },
    )
    .unwrap();
    assert!(stream.local_addr().unwrap().is_ipv6());
    let (accepted, _) = listener.accept().unwrap();
    assert!(accepted.peer_addr().unwrap().is_ipv6());
    let receiver = net::bind_udp(local, None, &socket).unwrap();
    receiver.set_read_timeout(Some(TIMEOUT)).unwrap();
    let sender = net::bind_udp(local, Some(receiver.local_addr().unwrap()), &socket).unwrap();
    sender.send(b"v6").unwrap();
    let mut data = [0; 2];
    assert_eq!(receiver.recv_from(&mut data).unwrap().0, 2);
    assert_eq!(&data, b"v6");
    #[cfg(target_os = "linux")]
    {
        let configured = net::bind_udp(
            local,
            None,
            &SocketOptions {
                dscp: Some(46),
                ..socket
            },
        )
        .unwrap();
        assert_eq!(SockRef::from(&configured).tclass_v6().unwrap() >> 2, 46);
    }
}

#[cfg(target_os = "linux")]
mod reference_vectors;
