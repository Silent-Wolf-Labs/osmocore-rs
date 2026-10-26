// SPDX-License-Identifier: GPL-3.0-or-later

use super::error::phase;
use super::{NetError, Operation};
use socket2::{Domain, Protocol, Socket, Type};
use std::{io, net::SocketAddr};

/// Configuration shared by new TCP/UDP sockets. Nothing is silently best-effort.
#[derive(Clone, Debug, Default)]
pub struct SocketOptions {
    /// Set SO_REUSEADDR before bind. Default false, including TCP listeners.
    pub reuse_address: bool,
    /// Set IPV6_V6ONLY before bind/connect; `None` leaves the OS default.
    /// Specifying this for an IPv4 socket is an input error.
    pub ipv6_only: Option<bool>,
    /// Mode of the returned socket; TCP establishment still waits for completion.
    pub nonblocking: bool,
    /// Optional six-bit DSCP value, 0..=63. ECN bits are preserved.
    /// An unsupported option/platform is an error, not a silently skipped setting.
    pub dscp: Option<u8>,
}
impl SocketOptions {
    pub(super) fn validate(
        &self,
        local: Option<SocketAddr>,
        address: SocketAddr,
    ) -> Result<(), NetError> {
        if self.dscp.is_some_and(|dscp| dscp > 63) {
            return Err(NetError::invalid("DSCP must be in 0..=63"));
        }
        if address.is_ipv4() && self.ipv6_only.is_some() {
            return Err(NetError::invalid("ipv6_only requires an IPv6 socket"));
        }
        if local.is_some_and(|local| local.is_ipv4() != address.is_ipv4()) {
            return Err(NetError::invalid(
                "local and remote address families must match",
            ));
        }
        if address.is_ipv6() && self.ipv6_only == Some(true) {
            let mapped =
                |a: SocketAddr| matches!(a, SocketAddr::V6(a) if a.ip().to_ipv4_mapped().is_some());
            if mapped(address) || local.is_some_and(mapped) {
                return Err(NetError::invalid(
                    "IPv4-mapped addresses conflict with ipv6_only",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn create(
        &self,
        address: SocketAddr,
        kind: Type,
        protocol: Protocol,
    ) -> Result<Socket, NetError> {
        let socket = phase(
            Operation::Create,
            Socket::new(Domain::for_address(address), kind, Some(protocol)),
        )?;
        phase(
            Operation::Configure,
            socket.set_reuse_address(self.reuse_address),
        )?;
        if let Some(only) = self.ipv6_only {
            phase(Operation::Configure, socket.set_only_v6(only))?;
        }
        if let Some(dscp) = self.dscp {
            phase(Operation::Configure, set_dscp(&socket, address, dscp))?;
        }
        Ok(socket)
    }
    pub(super) fn finish(&self, socket: &Socket) -> Result<(), NetError> {
        phase(
            Operation::SetNonblocking,
            socket.set_nonblocking(self.nonblocking),
        )
    }
}

/// TCP-specific pre-establishment options; applicable to streams and listeners.
/// Accepted stream option/mode inheritance remains OS-defined.
#[derive(Clone, Debug, Default)]
pub struct TcpOptions {
    pub socket: SocketOptions,
    pub nodelay: bool,
    pub keepalive: bool,
}
impl TcpOptions {
    pub(super) fn create(&self, address: SocketAddr) -> Result<Socket, NetError> {
        let socket = self.socket.create(address, Type::STREAM, Protocol::TCP)?;
        phase(Operation::Configure, socket.set_tcp_nodelay(self.nodelay))?;
        phase(Operation::Configure, socket.set_keepalive(self.keepalive))?;
        Ok(socket)
    }
}

// socket2 exposes IPv6 traffic class only on these Unix targets with `all`.
// Unsupported configurations fail explicitly without adding handwritten FFI.
fn set_dscp(socket: &Socket, address: SocketAddr, dscp: u8) -> io::Result<()> {
    if address.is_ipv4() {
        set_dscp_v4(socket, dscp)
    } else {
        set_dscp_v6(socket, dscp)
    }
}
fn set_dscp_v4(socket: &Socket, dscp: u8) -> io::Result<()> {
    #[cfg(not(any(
        target_os = "fuchsia",
        target_os = "redox",
        target_os = "solaris",
        target_os = "haiku",
        target_os = "wasi"
    )))]
    {
        socket.set_tos_v4((socket.tos_v4()? & 3) | (u32::from(dscp) << 2))
    }
    #[cfg(any(
        target_os = "fuchsia",
        target_os = "redox",
        target_os = "solaris",
        target_os = "haiku",
        target_os = "wasi"
    ))]
    {
        let _ = (socket, dscp);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "IPv4 DSCP unsupported on this platform",
        ))
    }
}
fn set_dscp_v6(socket: &Socket, dscp: u8) -> io::Result<()> {
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "fuchsia",
        target_os = "linux",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "cygwin",
        target_os = "illumos"
    ))]
    {
        socket.set_tclass_v6((socket.tclass_v6()? & 3) | (u32::from(dscp) << 2))
    }
    #[cfg(not(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "fuchsia",
        target_os = "linux",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "cygwin",
        target_os = "illumos"
    )))]
    {
        let _ = (socket, dscp);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "IPv6 DSCP unsupported on this platform",
        ))
    }
}
