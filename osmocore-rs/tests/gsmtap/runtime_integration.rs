// SPDX-License-Identifier: GPL-3.0-or-later

use osmocore_rs::{gsmtap::GsmtapMessage, runtime::*};
use std::{net::UdpSocket, time::Duration};

struct Sender {
    handle: UdpHandle,
    rejected: Option<EnqueueError<Datagram>>,
    completed: usize,
}
impl Application for Sender {
    type Command = Datagram;
    type Error = ();

    fn on_command(&mut self, cx: &mut RuntimeContext<'_>, packet: Datagram) -> Result<(), ()> {
        self.rejected = cx.try_send_udp(self.handle, packet).err();
        Ok(())
    }

    fn on_event(&mut self, _: &mut RuntimeContext<'_>, event: RuntimeEvent<'_>) -> Result<(), ()> {
        if let RuntimeEvent::Write(outcome) = event {
            assert!(
                matches!(outcome.status, WriteStatus::Complete),
                "{outcome:?}"
            );
            self.completed += 1;
        }
        Ok(())
    }
}

fn socket(peer: Option<std::net::SocketAddr>) -> UdpSocket {
    #[cfg(feature = "net")]
    {
        osmocore_rs::net::bind_udp(
            "127.0.0.1:0".parse().unwrap(),
            peer,
            &osmocore_rs::net::SocketOptions::default(),
        )
        .unwrap()
    }
    #[cfg(not(feature = "net"))]
    {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        if let Some(peer) = peer {
            socket.connect(peer).unwrap();
        }
        socket
    }
}

fn sender(connected: bool, config: RuntimeConfig) -> (IoRuntime<Datagram>, Sender, UdpSocket) {
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let peer = connected.then(|| receiver.local_addr().unwrap());
    let mut rt = IoRuntime::new(config).unwrap();
    let handle = rt
        .register_udp(socket(peer), UdpResourceOptions::default())
        .unwrap();
    (
        rt,
        Sender {
            handle,
            rejected: None,
            completed: 0,
        },
        receiver,
    )
}

fn submit(rt: &mut IoRuntime<Datagram>, app: &mut Sender, packet: Datagram) {
    rt.control_handle().try_submit(packet).ok().unwrap();
    for _ in 0..200 {
        rt.step(
            app,
            &mut DriverSet::default(),
            Some(Duration::from_millis(10)),
        )
        .unwrap();
        if app.rejected.is_some() || app.completed > 0 {
            return;
        }
    }
    panic!("GSMTAP datagram neither completed nor rejected");
}

fn send_packets(connected: bool) {
    let (mut rt, mut app, receiver) = sender(connected, RuntimeConfig::default());
    let destination = (!connected).then(|| receiver.local_addr().unwrap());
    for payload in [b"".as_slice(), b"GSMTAP\0\xff"] {
        let mut message = GsmtapMessage::um(payload);
        message.arfcn = 0xffff;
        message.frame_number = u32::MAX;
        let expected = message.encode();
        let datagram = message.into_datagram(destination).unwrap();
        assert_eq!(datagram.destination, destination);
        app.completed = 0;
        submit(&mut rt, &mut app, datagram);
        assert!(app.rejected.is_none());
        assert_eq!(app.completed, 1);
        let mut bytes = [0; 256];
        let (length, _) = receiver.recv_from(&mut bytes).unwrap();
        assert_eq!(&bytes[..length], expected);
        assert_eq!(length, 16 + payload.len());
    }
    receiver.set_nonblocking(true).unwrap();
    assert_eq!(
        receiver.recv_from(&mut [0; 1]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn connected_udp_sends_owned_gsmtap_datagrams() {
    send_packets(true);
}

#[test]
fn unconnected_udp_sends_owned_gsmtap_datagrams() {
    send_packets(false);
}

#[test]
fn runtime_capacity_rejection_returns_original_packet_and_destination() {
    let (mut rt, mut app, receiver) = sender(
        false,
        RuntimeConfig {
            per_resource_bytes: 8,
            ..RuntimeConfig::default()
        },
    );
    let destination = Some(receiver.local_addr().unwrap());
    let packet = GsmtapMessage::um(b"retry me")
        .into_datagram(destination)
        .unwrap();
    let expected = packet.message.data().to_vec();
    let allocation = packet.message.data().as_ptr();
    submit(&mut rt, &mut app, packet);
    let error = app.rejected.take().unwrap();
    assert_eq!(error.reason, Rejection::Capacity);
    assert_eq!(error.value.destination, destination);
    assert_eq!(error.value.message.data(), expected);
    assert_eq!(error.value.message.data().as_ptr(), allocation);
    assert_eq!(app.completed, 0);
}

#[test]
fn generic_runtime_validates_destination_against_socket_state() {
    for connected in [true, false] {
        let (mut rt, mut app, receiver) = sender(connected, RuntimeConfig::default());
        // Preparation accepts both policies; only the runtime knows socket state.
        let destination = connected.then(|| receiver.local_addr().unwrap());
        let packet = GsmtapMessage::um(&[]).into_datagram(destination).unwrap();
        assert_eq!(packet.destination, destination);
        submit(&mut rt, &mut app, packet);
        let error = app.rejected.take().unwrap();
        assert_eq!(error.reason, Rejection::InvalidDestination);
        assert_eq!(error.value.destination, destination);
        assert_eq!(error.value.message.data(), GsmtapMessage::um(&[]).encode());
        assert_eq!(app.completed, 0);
    }
}
