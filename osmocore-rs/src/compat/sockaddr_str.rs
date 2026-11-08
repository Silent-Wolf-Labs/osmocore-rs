// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::base::sockaddr_str::ipv6_text;

/// Linux's `AF_UNSPEC`.
pub const AF_UNSPEC: i32 = 0;
/// Linux's `AF_INET`.
pub const AF_INET: i32 = 2;
/// Linux's `AF_INET6`.
pub const AF_INET6: i32 = 10;
/// The size of C's address text field, including its NUL.
pub const INET6_ADDRSTRLEN: usize = 46;
/// The size of Linux's `struct sockaddr_storage`.
pub const SOCKADDR_STORAGE_SIZE: usize = 128;

const EIO: i32 = 5;
const EINVAL: i32 = 22;
const ENOSPC: i32 = 28;
const EAFNOSUPPORT: i32 = 97;

/// C's `struct osmo_sockaddr_str`. The text in `ip` ends at its first NUL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OsmoSockaddrStr {
    /// `AF_INET`, `AF_INET6`, or anything else, which no conversion accepts.
    pub af: i32,
    /// The address text, NUL-terminated.
    pub ip: [u8; INET6_ADDRSTRLEN],
    /// The port.
    pub port: u16,
}

impl Default for OsmoSockaddrStr {
    fn default() -> Self {
        Self {
            af: AF_UNSPEC,
            ip: [0; INET6_ADDRSTRLEN],
            port: 0,
        }
    }
}

impl OsmoSockaddrStr {
    /// Returns the address text, up to its first NUL.
    pub fn ip_text(&self) -> &[u8] {
        until_nul(&self.ip)
    }
}

/// Linux's `struct sockaddr_in`. As in C, `sin_port` holds the port in network byte order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SockaddrIn {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: [u8; 4],
    pub sin_zero: [u8; 8],
}

impl SockaddrIn {
    /// The struct's size in C.
    pub const SIZE: usize = 16;

    /// Returns the bytes of the struct in memory on this host.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0; Self::SIZE];
        bytes[0..2].copy_from_slice(&self.sin_family.to_ne_bytes());
        bytes[2..4].copy_from_slice(&self.sin_port.to_ne_bytes());
        bytes[4..8].copy_from_slice(&self.sin_addr);
        bytes[8..16].copy_from_slice(&self.sin_zero);
        bytes
    }

    /// Reads the struct from its bytes in memory on this host.
    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        Self {
            sin_family: u16::from_ne_bytes([bytes[0], bytes[1]]),
            sin_port: u16::from_ne_bytes([bytes[2], bytes[3]]),
            sin_addr: [bytes[4], bytes[5], bytes[6], bytes[7]],
            sin_zero: core::array::from_fn(|i| bytes[8 + i]),
        }
    }
}

/// Linux's `struct sockaddr_in6`. As in C, `sin6_port` and `sin6_flowinfo` hold network
/// byte order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SockaddrIn6 {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: [u8; 16],
    pub sin6_scope_id: u32,
}

impl SockaddrIn6 {
    /// The struct's size in C.
    pub const SIZE: usize = 28;

    /// Returns the bytes of the struct in memory on this host.
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0; Self::SIZE];
        bytes[0..2].copy_from_slice(&self.sin6_family.to_ne_bytes());
        bytes[2..4].copy_from_slice(&self.sin6_port.to_ne_bytes());
        bytes[4..8].copy_from_slice(&self.sin6_flowinfo.to_ne_bytes());
        bytes[8..24].copy_from_slice(&self.sin6_addr);
        bytes[24..28].copy_from_slice(&self.sin6_scope_id.to_ne_bytes());
        bytes
    }

    /// Reads the struct from its bytes in memory on this host.
    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        let word = |at: usize| {
            u32::from_ne_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        Self {
            sin6_family: u16::from_ne_bytes([bytes[0], bytes[1]]),
            sin6_port: u16::from_ne_bytes([bytes[2], bytes[3]]),
            sin6_flowinfo: word(4),
            sin6_addr: core::array::from_fn(|i| bytes[8 + i]),
            sin6_scope_id: word(24),
        }
    }
}

/// Linux's `struct sockaddr_storage`, as its bytes in memory. Its family decides whether C
/// reads it as a [`SockaddrIn`] or a [`SockaddrIn6`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SockaddrStorage(pub [u8; SOCKADDR_STORAGE_SIZE]);

impl Default for SockaddrStorage {
    fn default() -> Self {
        Self([0; SOCKADDR_STORAGE_SIZE])
    }
}

impl SockaddrStorage {
    /// Returns the family field, which every socket address starts with.
    pub fn family(&self) -> u16 {
        u16::from_ne_bytes([self.0[0], self.0[1]])
    }

    /// Reads the start of the storage as a [`SockaddrIn`].
    pub fn sockaddr_in(&self) -> SockaddrIn {
        let mut bytes = [0; SockaddrIn::SIZE];
        bytes.copy_from_slice(&self.0[..SockaddrIn::SIZE]);
        SockaddrIn::from_bytes(&bytes)
    }

    /// Reads the start of the storage as a [`SockaddrIn6`].
    pub fn sockaddr_in6(&self) -> SockaddrIn6 {
        let mut bytes = [0; SockaddrIn6::SIZE];
        bytes.copy_from_slice(&self.0[..SockaddrIn6::SIZE]);
        SockaddrIn6::from_bytes(&bytes)
    }

    /// Writes a [`SockaddrIn`] over the start of the storage, leaving the rest as it was.
    pub fn set_sockaddr_in(&mut self, addr: &SockaddrIn) {
        self.0[..SockaddrIn::SIZE].copy_from_slice(&addr.to_bytes());
    }

    /// Writes a [`SockaddrIn6`] over the start of the storage, leaving the rest as it was.
    pub fn set_sockaddr_in6(&mut self, addr: &SockaddrIn6) {
        self.0[..SockaddrIn6::SIZE].copy_from_slice(&addr.to_bytes());
    }
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&byte| byte == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn parse<T: std::str::FromStr>(s: &OsmoSockaddrStr) -> Option<T> {
    std::str::from_utf8(s.ip_text()).ok()?.parse().ok()
}

fn set_text(s: &mut OsmoSockaddrStr, text: &[u8]) {
    s.ip = [0; INET6_ADDRSTRLEN];
    let kept = text.len().min(INET6_ADDRSTRLEN - 1);
    s.ip[..kept].copy_from_slice(&text[..kept]);
}

const fn ordering(order: Ordering) -> i32 {
    order as i32
}

/// Writes `ip:port`, with IPv6 text in brackets, as C's `OSMO_SOCKADDR_STR_FMT` with
/// `OSMO_SOCKADDR_STR_FMT_ARGS` does. A missing address gives `NULL:0`.
pub fn osmo_sockaddr_str_fmt(sockaddr_str: Option<&OsmoSockaddrStr>) -> Vec<u8> {
    let Some(s) = sockaddr_str else {
        return b"NULL:0".to_vec();
    };
    let bracketed = s.af == AF_INET6;
    let mut text = Vec::with_capacity(INET6_ADDRSTRLEN + 8);
    if bracketed {
        text.push(b'[');
    }
    text.extend_from_slice(s.ip_text());
    if bracketed {
        text.push(b']');
    }
    text.extend_from_slice(format!(":{}", s.port).as_bytes());
    text
}

/// Returns `AF_INET6` if the text holds `:`, else `AF_INET` if it holds `.`, else
/// `AF_UNSPEC`, which is also the result for `None`. The text ends at its first NUL.
pub fn osmo_ip_str_type(ip: Option<&[u8]>) -> i32 {
    let Some(ip) = ip.map(until_nul) else {
        return AF_UNSPEC;
    };
    if ip.contains(&b':') {
        AF_INET6
    } else if ip.contains(&b'.') {
        AF_INET
    } else {
        AF_UNSPEC
    }
}

/// Returns whether the text is nonempty, the port nonzero and the family `AF_INET` or
/// `AF_INET6`.
pub fn osmo_sockaddr_str_is_set(sockaddr_str: &OsmoSockaddrStr) -> bool {
    sockaddr_str.ip[0] != 0
        && sockaddr_str.port != 0
        && matches!(sockaddr_str.af, AF_INET | AF_INET6)
}

/// Returns whether the address is set and converts to an address that isn't all zero bits.
pub fn osmo_sockaddr_str_is_nonzero(sockaddr_str: &OsmoSockaddrStr) -> bool {
    if !osmo_sockaddr_str_is_set(sockaddr_str) {
        return false;
    }
    match sockaddr_str.af {
        AF_INET => {
            let mut ip = 0;
            osmo_sockaddr_str_to_32(sockaddr_str, &mut ip) == 0 && ip != 0
        }
        _ => {
            let mut addr = [0; 16];
            osmo_sockaddr_str_to_in6_addr(sockaddr_str, &mut addr) == 0 && addr != [0; 16]
        }
    }
}

/// Compares two addresses: a missing one first, then by family, then by address and port.
/// When either text doesn't convert, or the family is neither `AF_INET` nor `AF_INET6`,
/// the texts are compared as bytes instead, then the ports. As in C, IPv4 addresses
/// compare as the 32-bit value whose bytes in memory are in network order, which depends
/// on the host. This isn't a total order.
///
/// Returns -1, 0 or 1. C's result has the same sign, with a size that comes from libc.
pub fn osmo_sockaddr_str_cmp(a: Option<&OsmoSockaddrStr>, b: Option<&OsmoSockaddrStr>) -> i32 {
    let (a, b) = match (a, b) {
        (None, None) => return 0,
        (None, Some(_)) => return -1,
        (Some(_), None) => return 1,
        (Some(a), Some(b)) => (a, b),
    };
    let by_text = || a.ip_text().cmp(b.ip_text()).then(a.port.cmp(&b.port));
    let order = a.af.cmp(&b.af).then_with(|| match a.af {
        AF_INET => {
            let (mut ip_a, mut ip_b) = (0, 0);
            if osmo_sockaddr_str_to_32(a, &mut ip_a) != 0
                || osmo_sockaddr_str_to_32(b, &mut ip_b) != 0
            {
                by_text()
            } else {
                ip_a.cmp(&ip_b).then(a.port.cmp(&b.port))
            }
        }
        AF_INET6 => {
            let (mut ip_a, mut ip_b) = ([0; 16], [0; 16]);
            if osmo_sockaddr_str_to_in6_addr(a, &mut ip_a) != 0
                || osmo_sockaddr_str_to_in6_addr(b, &mut ip_b) != 0
            {
                by_text()
            } else {
                ip_a.cmp(&ip_b).then(a.port.cmp(&b.port))
            }
        }
        _ => by_text(),
    });
    ordering(order)
}

/// Copies the text, which ends at its first NUL, and sets the family it looks like. The
/// port stays as it was. As in C, the struct is written even when this fails. `None` is
/// empty text.
///
/// Returns 0, -5 (`-EIO`) for empty text, -28 (`-ENOSPC`) for text that doesn't fit, which
/// is cut short, or -22 (`-EINVAL`) when the family is `AF_UNSPEC`.
pub fn osmo_sockaddr_str_from_str2(sockaddr_str: &mut OsmoSockaddrStr, ip: Option<&[u8]>) -> i32 {
    let text = until_nul(ip.unwrap_or_default());
    sockaddr_str.af = osmo_ip_str_type(Some(text));
    set_text(sockaddr_str, text);
    if text.is_empty() {
        -EIO
    } else if text.len() >= INET6_ADDRSTRLEN {
        -ENOSPC
    } else if sockaddr_str.af == AF_UNSPEC {
        -EINVAL
    } else {
        0
    }
}

/// [`osmo_sockaddr_str_from_str2`], then sets the port, even when that failed.
pub fn osmo_sockaddr_str_from_str(
    sockaddr_str: &mut OsmoSockaddrStr,
    ip: Option<&[u8]>,
    port: u16,
) -> i32 {
    let rc = osmo_sockaddr_str_from_str2(sockaddr_str, ip);
    sockaddr_str.port = port;
    rc
}

/// Sets an IPv4 address, given as its bytes in network order, and the port. Returns 0.
pub fn osmo_sockaddr_str_from_in_addr(
    sockaddr_str: &mut OsmoSockaddrStr,
    addr: &[u8; 4],
    port: u16,
) -> i32 {
    *sockaddr_str = OsmoSockaddrStr {
        af: AF_INET,
        port,
        ..OsmoSockaddrStr::default()
    };
    set_text(sockaddr_str, Ipv4Addr::from(*addr).to_string().as_bytes());
    0
}

/// Sets an IPv6 address and the port, writing the text as glibc's `inet_ntop` does.
/// Returns 0.
pub fn osmo_sockaddr_str_from_in6_addr(
    sockaddr_str: &mut OsmoSockaddrStr,
    addr: &[u8; 16],
    port: u16,
) -> i32 {
    *sockaddr_str = OsmoSockaddrStr {
        af: AF_INET6,
        port,
        ..OsmoSockaddrStr::default()
    };
    set_text(sockaddr_str, ipv6_text(Ipv6Addr::from(*addr)).as_bytes());
    0
}

/// Sets an IPv4 address from a value whose bytes in memory are in network order. Returns 0.
pub fn osmo_sockaddr_str_from_32(sockaddr_str: &mut OsmoSockaddrStr, ip: u32, port: u16) -> i32 {
    osmo_sockaddr_str_from_in_addr(sockaddr_str, &ip.to_ne_bytes(), port)
}

/// Sets an IPv4 address from its value in host byte order. Returns 0.
pub fn osmo_sockaddr_str_from_32h(sockaddr_str: &mut OsmoSockaddrStr, ip: u32, port: u16) -> i32 {
    osmo_sockaddr_str_from_in_addr(sockaddr_str, &ip.to_be_bytes(), port)
}

/// Sets the address and port. Returns 0, or -22 (`-EINVAL`) if the family isn't `AF_INET`,
/// leaving the struct as it was.
pub fn osmo_sockaddr_str_from_sockaddr_in(
    sockaddr_str: &mut OsmoSockaddrStr,
    src: &SockaddrIn,
) -> i32 {
    if i32::from(src.sin_family) != AF_INET {
        return -EINVAL;
    }
    osmo_sockaddr_str_from_in_addr(sockaddr_str, &src.sin_addr, u16::from_be(src.sin_port))
}

/// Sets the address and port, dropping the flow label and scope. Returns 0, or -22
/// (`-EINVAL`) if the family isn't `AF_INET6`, leaving the struct as it was.
pub fn osmo_sockaddr_str_from_sockaddr_in6(
    sockaddr_str: &mut OsmoSockaddrStr,
    src: &SockaddrIn6,
) -> i32 {
    if i32::from(src.sin6_family) != AF_INET6 {
        return -EINVAL;
    }
    osmo_sockaddr_str_from_in6_addr(sockaddr_str, &src.sin6_addr, u16::from_be(src.sin6_port))
}

/// Reads the storage as the address of its family. Returns 0, or -22 (`-EINVAL`) if the
/// family is neither `AF_INET` nor `AF_INET6`, leaving the struct as it was.
pub fn osmo_sockaddr_str_from_sockaddr(
    sockaddr_str: &mut OsmoSockaddrStr,
    src: &SockaddrStorage,
) -> i32 {
    match i32::from(src.family()) {
        AF_INET => osmo_sockaddr_str_from_sockaddr_in(sockaddr_str, &src.sockaddr_in()),
        AF_INET6 => osmo_sockaddr_str_from_sockaddr_in6(sockaddr_str, &src.sockaddr_in6()),
        _ => -EINVAL,
    }
}

/// Writes the IPv4 address's bytes in network order. Returns 0, -97 (`-EAFNOSUPPORT`) if
/// the family isn't `AF_INET`, or -22 (`-EINVAL`) if the text isn't an IPv4 address. `dst`
/// changes only on success.
pub fn osmo_sockaddr_str_to_in_addr(sockaddr_str: &OsmoSockaddrStr, dst: &mut [u8; 4]) -> i32 {
    if sockaddr_str.af != AF_INET {
        return -EAFNOSUPPORT;
    }
    match parse::<Ipv4Addr>(sockaddr_str) {
        Some(addr) => {
            *dst = addr.octets();
            0
        }
        None => -EINVAL,
    }
}

/// Writes the IPv6 address's bytes. Returns 0, or -22 (`-EINVAL`) if the family isn't
/// `AF_INET6` or the text isn't an IPv6 address. `dst` changes only on success.
pub fn osmo_sockaddr_str_to_in6_addr(sockaddr_str: &OsmoSockaddrStr, dst: &mut [u8; 16]) -> i32 {
    if sockaddr_str.af != AF_INET6 {
        return -EINVAL;
    }
    match parse::<Ipv6Addr>(sockaddr_str) {
        Some(addr) => {
            *dst = addr.octets();
            0
        }
        None => -EINVAL,
    }
}

/// Writes the IPv4 address as a value whose bytes in memory are in network order. Returns
/// as [`osmo_sockaddr_str_to_in_addr`] does.
pub fn osmo_sockaddr_str_to_32(sockaddr_str: &OsmoSockaddrStr, ip: &mut u32) -> i32 {
    let mut addr = [0; 4];
    let rc = osmo_sockaddr_str_to_in_addr(sockaddr_str, &mut addr);
    if rc == 0 {
        *ip = u32::from_ne_bytes(addr);
    }
    rc
}

/// Writes the IPv4 address as a value in host byte order. Returns as
/// [`osmo_sockaddr_str_to_in_addr`] does.
pub fn osmo_sockaddr_str_to_32h(sockaddr_str: &OsmoSockaddrStr, ip: &mut u32) -> i32 {
    let mut addr = [0; 4];
    let rc = osmo_sockaddr_str_to_in_addr(sockaddr_str, &mut addr);
    if rc == 0 {
        *ip = u32::from_be_bytes(addr);
    }
    rc
}

/// Writes the address and port. Returns 0, or -22 (`-EINVAL`) if the family isn't
/// `AF_INET`, leaving `dst` as it was, or if the text isn't an IPv4 address. In that case,
/// as in C, `dst` keeps the family and port with a zero address.
pub fn osmo_sockaddr_str_to_sockaddr_in(
    sockaddr_str: &OsmoSockaddrStr,
    dst: &mut SockaddrIn,
) -> i32 {
    if sockaddr_str.af != AF_INET {
        return -EINVAL;
    }
    *dst = SockaddrIn {
        sin_family: sockaddr_str.af as u16,
        sin_port: sockaddr_str.port.to_be(),
        ..SockaddrIn::default()
    };
    osmo_sockaddr_str_to_in_addr(sockaddr_str, &mut dst.sin_addr)
}

/// Writes the address and port, with no flow label or scope. Returns 0, or -22 (`-EINVAL`)
/// if the family isn't `AF_INET6`, leaving `dst` as it was, or if the text isn't an IPv6
/// address. In that case, as in C, `dst` keeps the family and port with a zero address.
pub fn osmo_sockaddr_str_to_sockaddr_in6(
    sockaddr_str: &OsmoSockaddrStr,
    dst: &mut SockaddrIn6,
) -> i32 {
    if sockaddr_str.af != AF_INET6 {
        return -EINVAL;
    }
    *dst = SockaddrIn6 {
        sin6_family: sockaddr_str.af as u16,
        sin6_port: sockaddr_str.port.to_be(),
        ..SockaddrIn6::default()
    };
    osmo_sockaddr_str_to_in6_addr(sockaddr_str, &mut dst.sin6_addr)
}

/// Writes the address of the struct's family over the start of the storage, as
/// [`osmo_sockaddr_str_to_sockaddr_in`] or [`osmo_sockaddr_str_to_sockaddr_in6`] does.
/// Returns -22 (`-EINVAL`) for any other family, leaving `dst` as it was.
pub fn osmo_sockaddr_str_to_sockaddr(
    sockaddr_str: &OsmoSockaddrStr,
    dst: &mut SockaddrStorage,
) -> i32 {
    match sockaddr_str.af {
        AF_INET => {
            let mut addr = dst.sockaddr_in();
            let rc = osmo_sockaddr_str_to_sockaddr_in(sockaddr_str, &mut addr);
            dst.set_sockaddr_in(&addr);
            rc
        }
        AF_INET6 => {
            let mut addr = dst.sockaddr_in6();
            let rc = osmo_sockaddr_str_to_sockaddr_in6(sockaddr_str, &mut addr);
            dst.set_sockaddr_in6(&addr);
            rc
        }
        _ => -EINVAL,
    }
}
