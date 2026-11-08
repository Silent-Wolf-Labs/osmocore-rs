// SPDX-License-Identifier: GPL-3.0-or-later

//! An IP address kept as text with a port, like C's `osmo_sockaddr_str`, for protocols such
//! as MGCP that carry addresses as text.
//!
//! ```
//! use osmocore_rs::sockaddr_str::{Family, SocketAddrText};
//! use std::net::{Ipv4Addr, SocketAddr};
//! let addr = SocketAddrText::new("1:2:3::4", 5)?;
//! assert_eq!(addr.family(), Family::Ipv6);
//! assert_eq!(addr.to_string(), "[1:2:3::4]:5");
//!
//! let from_socket = SocketAddrText::from(SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 80)));
//! assert_eq!(from_socket.ip(), "10.0.0.1");
//! assert_eq!(from_socket.to_socket_addr()?, "10.0.0.1:80".parse::<SocketAddr>().unwrap());
//! # Ok::<(), osmocore_rs::sockaddr_str::SocketAddrTextError>(())
//! ```

use std::error::Error;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

/// The longest address text kept, which fits the longest IPv6 address with an IPv4 tail.
pub const MAX_IP_LEN: usize = 45;

/// The address family that text looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// IPv4, C's `AF_INET`.
    Ipv4,
    /// IPv6, C's `AF_INET6`.
    Ipv6,
}

/// Errors from making or converting a [`SocketAddrText`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketAddrTextError {
    /// The address text is empty. C's `-EIO`.
    Empty,
    /// The address text is longer than [`MAX_IP_LEN`] bytes. C's `-ENOSPC`.
    TooLong,
    /// The text has neither `:` nor `.`, so it has no family. C's `-EINVAL`.
    NoFamily,
    /// The address is of the other family than the one asked for.
    WrongFamily,
    /// The text isn't a valid address of its family.
    Invalid,
}

impl fmt::Display for SocketAddrTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "empty address text",
            Self::TooLong => "address text too long",
            Self::NoFamily => "address text has no family",
            Self::WrongFamily => "address of the wrong family",
            Self::Invalid => "invalid address text",
        })
    }
}

impl Error for SocketAddrTextError {}

/// Returns the family that text looks like, from whether it holds `:` (IPv6) or else `.`
/// (IPv4). It doesn't check that the text is a valid address. C's `osmo_ip_str_type`.
pub fn family_of(text: &str) -> Option<Family> {
    if text.contains(':') {
        Some(Family::Ipv6)
    } else if text.contains('.') {
        Some(Family::Ipv4)
    } else {
        None
    }
}

/// Writes an IPv6 address as glibc's `inet_ntop` does. That is the standard form, except
/// that an address whose first 96 bits are zero, with a nonzero seventh group, ends in
/// dotted IPv4 (`::1.2.3.4`).
pub(crate) fn ipv6_text(addr: Ipv6Addr) -> String {
    let octets = addr.octets();
    if octets[..12].iter().all(|&byte| byte == 0) && octets[12..14] != [0, 0] {
        format!(
            "::{}",
            Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15])
        )
    } else {
        addr.to_string()
    }
}

pub(crate) fn ip_text(addr: IpAddr) -> String {
    match addr {
        IpAddr::V4(addr) => addr.to_string(),
        IpAddr::V6(addr) => ipv6_text(addr),
    }
}

/// An IP address as text, its family and a port.
///
/// The family always comes from the text, so the two can't disagree. The text needn't be
/// a valid address until it's converted.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SocketAddrText {
    ip: String,
    family: Family,
    port: u16,
}

impl SocketAddrText {
    /// Keeps the address text and port, with the family the text looks like. It doesn't check
    /// that the text is a valid address. A NUL is an ordinary character here.
    pub fn new(ip: impl Into<String>, port: u16) -> Result<Self, SocketAddrTextError> {
        let ip = ip.into();
        if ip.is_empty() {
            return Err(SocketAddrTextError::Empty);
        }
        if ip.len() > MAX_IP_LEN {
            return Err(SocketAddrTextError::TooLong);
        }
        let family = family_of(&ip).ok_or(SocketAddrTextError::NoFamily)?;
        Ok(Self { ip, family, port })
    }

    /// Keeps an address and port, with the address written as C writes it: the standard
    /// form, except that IPv6 addresses whose first 96 bits are zero, with a nonzero seventh
    /// group, end in dotted IPv4 (`::1.2.3.4`).
    pub fn from_ip(ip: IpAddr, port: u16) -> Self {
        let family = match ip {
            IpAddr::V4(_) => Family::Ipv4,
            IpAddr::V6(_) => Family::Ipv6,
        };
        Self {
            ip: ip_text(ip),
            family,
            port,
        }
    }

    /// Returns the address text.
    pub fn ip(&self) -> &str {
        &self.ip
    }

    /// Returns the family the text looks like.
    pub const fn family(&self) -> Family {
        self.family
    }

    /// Returns the port.
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns whether the port is nonzero. The text and family are always set.
    pub const fn is_set(&self) -> bool {
        self.port != 0
    }

    /// Returns whether the port is nonzero and the text is a valid address that isn't all
    /// zero bits.
    pub fn is_nonzero(&self) -> bool {
        self.is_set()
            && match self.to_ip() {
                Ok(IpAddr::V4(addr)) => !addr.is_unspecified(),
                Ok(IpAddr::V6(addr)) => !addr.is_unspecified(),
                Err(_) => false,
            }
    }

    /// Returns the IPv4 address.
    pub fn to_ipv4(&self) -> Result<Ipv4Addr, SocketAddrTextError> {
        match self.family {
            Family::Ipv4 => self.ip.parse().map_err(|_| SocketAddrTextError::Invalid),
            Family::Ipv6 => Err(SocketAddrTextError::WrongFamily),
        }
    }

    /// Returns the IPv6 address.
    pub fn to_ipv6(&self) -> Result<Ipv6Addr, SocketAddrTextError> {
        match self.family {
            Family::Ipv6 => self.ip.parse().map_err(|_| SocketAddrTextError::Invalid),
            Family::Ipv4 => Err(SocketAddrTextError::WrongFamily),
        }
    }

    /// Returns the address, of the family the text looks like.
    pub fn to_ip(&self) -> Result<IpAddr, SocketAddrTextError> {
        match self.family {
            Family::Ipv4 => self.to_ipv4().map(IpAddr::V4),
            Family::Ipv6 => self.to_ipv6().map(IpAddr::V6),
        }
    }

    /// Returns the address and port. An IPv6 result has no flow label or scope.
    pub fn to_socket_addr(&self) -> Result<SocketAddr, SocketAddrTextError> {
        self.to_ip().map(|ip| SocketAddr::new(ip, self.port))
    }
}

/// Drops the flow label and scope of an IPv6 address, as C does.
impl From<SocketAddr> for SocketAddrText {
    fn from(addr: SocketAddr) -> Self {
        Self::from_ip(addr.ip(), addr.port())
    }
}

impl From<SocketAddrV4> for SocketAddrText {
    fn from(addr: SocketAddrV4) -> Self {
        SocketAddr::V4(addr).into()
    }
}

/// Drops the flow label and scope, as C does.
impl From<SocketAddrV6> for SocketAddrText {
    fn from(addr: SocketAddrV6) -> Self {
        SocketAddr::V6(addr).into()
    }
}

/// Writes `ip:port`, with the text of an IPv6 address in brackets, as C's
/// `OSMO_SOCKADDR_STR_FMT` does.
impl fmt::Display for SocketAddrText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.family {
            Family::Ipv4 => write!(formatter, "{}:{}", self.ip, self.port),
            Family::Ipv6 => write!(formatter, "[{}]:{}", self.ip, self.port),
        }
    }
}
