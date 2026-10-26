// SPDX-License-Identifier: GPL-3.0-or-later

use super::error::phase;
use super::{NetError, Operation, observe};
use crate::diagnostics::event;
use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
};

/// Filter for explicit hostname resolution. Resolver ordering is preserved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddressFamily {
    #[default]
    Any,
    Ipv4,
    Ipv6,
}
impl AddressFamily {
    fn accepts(self, address: &SocketAddr) -> bool {
        match self {
            Self::Any => true,
            Self::Ipv4 => address.is_ipv4(),
            Self::Ipv6 => address.is_ipv6(),
        }
    }
}

/// Explicit synchronous resolution with address-family filtering and deduplication.
/// Does not contact peers, bind sockets, sort addresses or choose retry policy.
/// DNS duration is controlled by the OS resolver, not the TCP connect timeout.
pub fn resolve(host: &str, port: u16, family: AddressFamily) -> Result<Vec<SocketAddr>, NetError> {
    observe("resolve", || {
        let candidates = phase(Operation::Resolve, (host, port).to_socket_addrs())?;
        let mut addresses = Vec::new();
        for address in candidates {
            if family.accepts(&address) && !addresses.contains(&address) {
                addresses.push(address);
            }
        }
        if addresses.is_empty() {
            return phase(
                Operation::Resolve,
                Err(io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "resolution produced no address in the requested family",
                )),
            );
        }
        event!(
            debug,
            "osmocore_rs::net",
            operation = "resolve",
            address_count = addresses.len()
        );
        Ok(addresses)
    })
}
