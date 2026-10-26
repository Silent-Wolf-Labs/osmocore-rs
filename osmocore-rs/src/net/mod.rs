// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("README.md")]

mod error;
mod options;
mod resolve;

pub use error::{NetError, Operation};
pub use options::{SocketOptions, TcpOptions};
pub use resolve::{AddressFamily, resolve};

use crate::diagnostics::event;
use error::phase;
use socket2::{Protocol, Type};
#[cfg(feature = "tracing")]
use std::io;
use std::{
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    time::Duration,
};

/// Configure, bind and listen on exactly one numeric endpoint. Port zero requests
/// an ephemeral port. Backlog must be positive and fit an OS signed integer;
/// the OS may cap it. No implicit DNS, fallback, registration or accept loop.
pub fn listen_tcp(
    local: SocketAddr,
    backlog: u32,
    options: &TcpOptions,
) -> Result<TcpListener, NetError> {
    observe("tcp_listen", || {
        options.socket.validate(None, local)?;
        let backlog = i32::try_from(backlog)
            .ok()
            .filter(|b| *b > 0)
            .ok_or_else(|| NetError::invalid("backlog must be in 1..=i32::MAX"))?;
        let socket = options.create(local)?;
        phase(Operation::Bind, socket.bind(&local.into()))?;
        phase(Operation::Listen, socket.listen(backlog))?;
        options.socket.finish(&socket)?;
        success("tcp_listen", local, options.socket.nonblocking);
        Ok(socket.into())
    })
}

/// Establish a TCP connection with a required nonzero timeout, optionally binding
/// a local address/source port first. No socket escapes on failure. The timeout
/// applies to this one connection attempt, not DNS or application I/O. Even when
/// the returned stream is nonblocking, establishment waits for completion.
pub fn connect_tcp(
    local: Option<SocketAddr>,
    remote: SocketAddr,
    timeout: Duration,
    options: &TcpOptions,
) -> Result<TcpStream, NetError> {
    observe("tcp_connect", || {
        options.socket.validate(local, remote)?;
        if timeout.is_zero() {
            return Err(NetError::invalid("connect timeout must be nonzero"));
        }
        let socket = options.create(remote)?;
        if let Some(local) = local {
            phase(Operation::Bind, socket.bind(&local.into()))?;
        }
        phase(
            Operation::Connect,
            socket.connect_timeout(&remote.into(), timeout),
        )?;
        options.socket.finish(&socket)?;
        success("tcp_connect", remote, options.socket.nonblocking);
        Ok(socket.into())
    })
}

/// Configure and bind a UDP socket, optionally selecting its default peer.
/// UDP connect is not a handshake or evidence of peer reachability. Without a
/// peer, use the returned UdpSocket's send_to/recv_from; with one, use send/recv.
/// Broadcast and multicast membership can be configured through std::net after
/// establishment; no implicit group joins or broadcast enablement occur here.
pub fn bind_udp(
    local: SocketAddr,
    peer: Option<SocketAddr>,
    options: &SocketOptions,
) -> Result<UdpSocket, NetError> {
    observe("udp_bind", || {
        options.validate(Some(local), peer.unwrap_or(local))?;
        let socket = options.create(local, Type::DGRAM, Protocol::UDP)?;
        phase(Operation::Bind, socket.bind(&local.into()))?;
        if let Some(peer) = peer {
            phase(Operation::Connect, socket.connect(&peer.into()))?;
        }
        options.finish(&socket)?;
        success("udp_bind", local, options.nonblocking);
        Ok(socket.into())
    })
}

fn observe<T>(
    _operation: &'static str,
    f: impl FnOnce() -> Result<T, NetError>,
) -> Result<T, NetError> {
    #[cfg(feature = "tracing")]
    let span = tracing::trace_span!(target: "osmocore_rs::net", "net_establishment", operation = _operation);
    #[cfg(feature = "tracing")]
    let _guard = span.enter();
    let result = f();
    #[cfg(feature = "tracing")]
    if let Err(error) = &result {
        let category = match error.io_error().kind() {
            io::ErrorKind::InvalidInput => "invalid_input",
            io::ErrorKind::AddrInUse => "address_in_use",
            io::ErrorKind::AddrNotAvailable => "address_not_available",
            io::ErrorKind::ConnectionRefused => "connection_refused",
            io::ErrorKind::TimedOut => "timeout",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::Unsupported => "unsupported",
            _ => "io",
        };
        tracing::warn!(target: "osmocore_rs::net", operation = _operation, phase = error.operation().as_str(),
            error_category = category, os_error = error.io_error().raw_os_error());
    }
    result
}
fn success(_operation: &'static str, _address: SocketAddr, _nonblocking: bool) {
    event!(
        debug,
        "osmocore_rs::net",
        operation = _operation,
        family = if _address.is_ipv4() { "ipv4" } else { "ipv6" },
        nonblocking = _nonblocking
    );
}
