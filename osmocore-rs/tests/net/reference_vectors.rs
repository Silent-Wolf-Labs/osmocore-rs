// SPDX-License-Identifier: GPL-3.0-or-later

#[path = "../support/network_fixture.rs"]
mod fixture;
use super::*;
const VECTORS: &str = include_str!("../compatibility/vectors/net/network_vectors.tsv");

#[test]
fn native_establishment_matches_bounded_c_observations() {
    for observation in fixture::parse(VECTORS).unwrap() {
        if observation.case == "invalid_dscp" {
            let rejected = net::bind_udp(
                loopback(),
                None,
                &SocketOptions {
                    dscp: Some(64),
                    ..SocketOptions::default()
                },
            )
            .is_err_and(|error| {
                error.operation() == Operation::Validate
                    && error.io_error().kind() == io::ErrorKind::InvalidInput
            });
            assert_eq!(vec![u8::from(rejected)], observation.values);
            continue;
        }
        let tcp = observation.case.starts_with("tcp_");
        let connected = observation.case.ends_with("connected");
        let configured = observation.case == "udp_configured";
        let socket = SocketOptions {
            reuse_address: tcp || configured,
            nonblocking: configured,
            dscp: if configured { Some(46) } else { None },
            ..SocketOptions::default()
        };
        let (bound, peer_matches, nonblocking, reuse, dscp, payload) = if tcp {
            let options = TcpOptions {
                socket,
                ..TcpOptions::default()
            };
            let listener = net::listen_tcp(loopback(), 16, &options).unwrap();
            if connected {
                let mut client = net::connect_tcp(
                    Some(loopback()),
                    listener.local_addr().unwrap(),
                    TIMEOUT,
                    &options,
                )
                .unwrap();
                let (mut accepted, _) = listener.accept().unwrap();
                accepted.set_read_timeout(Some(TIMEOUT)).unwrap();
                client.set_write_timeout(Some(TIMEOUT)).unwrap();
                client.write_all(b"tcp").unwrap();
                let mut data = [0; 3];
                accepted.read_exact(&mut data).unwrap();
                let local = client.local_addr().unwrap();
                (
                    local.ip().is_loopback() && local.port() != 0,
                    client.peer_addr().unwrap() == listener.local_addr().unwrap(),
                    SockRef::from(&client).nonblocking().unwrap(),
                    SockRef::from(&client).reuse_address().unwrap(),
                    (SockRef::from(&client).tos_v4().unwrap() >> 2) as u8,
                    data == *b"tcp",
                )
            } else {
                let local = listener.local_addr().unwrap();
                (
                    local.ip().is_loopback() && local.port() != 0,
                    SockRef::from(&listener).peer_addr().is_ok(),
                    SockRef::from(&listener).nonblocking().unwrap(),
                    SockRef::from(&listener).reuse_address().unwrap(),
                    (SockRef::from(&listener).tos_v4().unwrap() >> 2) as u8,
                    false,
                )
            }
        } else {
            let receiver = net::bind_udp(loopback(), None, &SocketOptions::default()).unwrap();
            let peer = connected.then(|| receiver.local_addr().unwrap());
            let sender = net::bind_udp(loopback(), peer, &socket).unwrap();
            let payload = if connected {
                receiver.set_read_timeout(Some(TIMEOUT)).unwrap();
                sender.send(b"udp").unwrap();
                let mut data = [0; 3];
                receiver.recv_from(&mut data).unwrap();
                data == *b"udp"
            } else {
                false
            };
            let nonblocking = SockRef::from(&sender).nonblocking().unwrap();
            let local = sender.local_addr().unwrap();
            (
                local.ip().is_loopback() && local.port() != 0,
                match peer {
                    Some(peer) => sender.peer_addr().unwrap() == peer,
                    None => sender.peer_addr().is_ok(),
                },
                nonblocking,
                SockRef::from(&sender).reuse_address().unwrap(),
                SockRef::from(&sender).tos_v4().unwrap() as u8 >> 2,
                payload,
            )
        };
        assert_eq!(
            vec![
                u8::from(bound),
                u8::from(peer_matches),
                u8::from(nonblocking),
                u8::from(reuse),
                dscp,
                u8::from(payload)
            ],
            observation.values,
            "{}",
            observation.case
        );
    }
}

#[test]
fn fixture_reader_rejects_missing_reordered_or_invalid_observations() {
    assert!(fixture::parse(VECTORS).is_ok());
    assert!(fixture::parse(&VECTORS.replace("udp_configured", "udp_bound")).is_err());
    assert!(fixture::parse(&VECTORS.replace("\t46\t", "\t64\t")).is_err());
    assert!(fixture::parse(VECTORS.trim_end()).is_err());
    assert!(fixture::parse(&format!("{VECTORS}extra\n")).is_err());
    assert!(fixture::parse(&VECTORS.replace("\t46\t", "\t+46\t")).is_err());
}
