//! Results the C recording can't hold, and where the preferred API differs from C.

use osmocore_rs::base::sockaddr_str::*;
use osmocore_rs::compat::sockaddr_str::{self as compat, OsmoSockaddrStr};
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

fn compat_from(ip: &[u8], port: u16) -> OsmoSockaddrStr {
    let mut s = OsmoSockaddrStr::default();
    compat::osmo_sockaddr_str_from_str(&mut s, Some(ip), port);
    s
}

#[test]
fn compat_text_ends_at_nul_but_preferred_keeps_it() {
    let s = compat_from(b"1.2.3.4\0junk", 5);
    assert_eq!(s.ip_text(), b"1.2.3.4");
    assert_eq!(compat::osmo_ip_str_type(Some(b"1\0:")), compat::AF_UNSPEC);

    let addr = SocketAddrText::new("1.2.3.4\0junk", 5).unwrap();
    assert_eq!(addr.ip(), "1.2.3.4\0junk");
    assert_eq!(addr.to_ipv4(), Err(SocketAddrTextError::Invalid));
}

#[test]
fn compat_cmp_orders_missing_addresses_first() {
    let s = compat_from(b"1.2.3.4", 5);
    assert_eq!(compat::osmo_sockaddr_str_cmp(None, None), 0);
    assert_eq!(compat::osmo_sockaddr_str_cmp(None, Some(&s)), -1);
    assert_eq!(compat::osmo_sockaddr_str_cmp(Some(&s), None), 1);
    assert_eq!(compat::osmo_sockaddr_str_cmp(Some(&s), Some(&s)), 0);
    assert_eq!(compat::osmo_sockaddr_str_fmt(None), b"NULL:0");
}

#[test]
fn compat_cmp_is_not_a_total_order() {
    let [a, b, c] = [b"2.0.0.0".as_slice(), b"10.0.0.0", b"1x"].map(|ip| {
        let mut s = compat_from(ip, 1);
        s.af = compat::AF_INET;
        s
    });
    // a < b by address, b < c and c < a by text, because "1x" isn't an address.
    let cmp = |x, y| compat::osmo_sockaddr_str_cmp(Some(x), Some(y));
    assert_eq!((cmp(&a, &b), cmp(&b, &c), cmp(&c, &a)), (-1, -1, -1));
}

#[test]
fn preferred_equality_is_by_text_where_c_compares_addresses() {
    let (short, long) = (compat_from(b"::", 5), compat_from(b"0::", 5));
    assert_eq!(compat::osmo_sockaddr_str_cmp(Some(&short), Some(&long)), 0);

    let (short, long) = (
        SocketAddrText::new("::", 5).unwrap(),
        SocketAddrText::new("0::", 5).unwrap(),
    );
    assert_ne!(short, long);
    assert_eq!(short.to_socket_addr(), long.to_socket_addr());
}

#[test]
fn from_str2_keeps_the_port_and_writes_on_failure() {
    let mut s = compat_from(b"1.2.3.4", 77);
    assert_eq!(
        compat::osmo_sockaddr_str_from_str2(&mut s, Some(b"nothing")),
        -22
    );
    assert_eq!(
        (s.af, s.ip_text(), s.port),
        (compat::AF_UNSPEC, &b"nothing"[..], 77)
    );
    assert_eq!(compat::osmo_sockaddr_str_from_str2(&mut s, None), -5);
    assert_eq!(s.ip_text(), b"");
}

#[test]
fn compat_to_in_addr_leaves_the_destination_on_failure() {
    let s = compat_from(b"1.2.3.256", 1);
    let mut addr = [9; 4];
    assert_eq!(compat::osmo_sockaddr_str_to_in_addr(&s, &mut addr), -22);
    assert_eq!(addr, [9; 4]);
    let mut value = 7;
    assert_eq!(compat::osmo_sockaddr_str_to_32h(&s, &mut value), -22);
    assert_eq!(value, 7);
}

#[test]
fn compat_to_sockaddr_leaves_the_rest_of_the_storage() {
    let s = compat_from(b"1.2.3.4", 0x1234);
    let mut storage = compat::SockaddrStorage([0xee; compat::SOCKADDR_STORAGE_SIZE]);
    assert_eq!(compat::osmo_sockaddr_str_to_sockaddr(&s, &mut storage), 0);
    assert_eq!(storage.sockaddr_in().sin_addr, [1, 2, 3, 4]);
    assert_eq!(u16::from_be(storage.sockaddr_in().sin_port), 0x1234);
    assert!(
        storage.0[compat::SockaddrIn::SIZE..]
            .iter()
            .all(|&byte| byte == 0xee)
    );
}

#[test]
fn sockaddr_bytes_round_trip() {
    let sin6 = compat::SockaddrIn6 {
        sin6_family: compat::AF_INET6 as u16,
        sin6_port: 80u16.to_be(),
        sin6_flowinfo: 0x01020304,
        sin6_addr: [7; 16],
        sin6_scope_id: 9,
    };
    assert_eq!(compat::SockaddrIn6::from_bytes(&sin6.to_bytes()), sin6);
    let sin = compat::SockaddrIn {
        sin_family: compat::AF_INET as u16,
        sin_port: 80u16.to_be(),
        sin_addr: [1, 2, 3, 4],
        sin_zero: [5; 8],
    };
    assert_eq!(compat::SockaddrIn::from_bytes(&sin.to_bytes()), sin);
}

#[test]
fn preferred_drops_flow_label_and_scope() {
    let socket = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 80, 7, 3);
    let addr = SocketAddrText::from(socket);
    assert_eq!(addr.to_string(), "[::1]:80");
    assert_eq!(
        addr.to_socket_addr(),
        Ok(SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::LOCALHOST,
            80,
            0,
            0
        )))
    );
}

#[test]
fn preferred_writes_ipv4_compatible_addresses_as_c_does() {
    let ip = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0x0102, 0x0304);
    assert_eq!(ip.to_string(), "::102:304");
    assert_eq!(SocketAddrText::from_ip(ip.into(), 1).ip(), "::1.2.3.4");
    // The same text parses back to the same address.
    assert_eq!(
        SocketAddrText::new("::1.2.3.4", 1).unwrap().to_ipv6(),
        Ok(ip)
    );
}

#[test]
fn preferred_family_comes_from_the_text() {
    assert_eq!(
        SocketAddrText::new("1.2.3.4", 0).unwrap().family(),
        Family::Ipv4
    );
    assert_eq!(
        SocketAddrText::new("::ffff:1.2.3.4", 0).unwrap().family(),
        Family::Ipv6
    );
    assert!(!SocketAddrText::new("1.2.3.4", 0).unwrap().is_set());
    assert_eq!(SocketAddrText::new("", 1), Err(SocketAddrTextError::Empty));
    assert_eq!(
        SocketAddrText::new("x", 1),
        Err(SocketAddrTextError::NoFamily)
    );
    assert_eq!(
        SocketAddrText::new("1:".repeat(23), 1),
        Err(SocketAddrTextError::TooLong)
    );
}
