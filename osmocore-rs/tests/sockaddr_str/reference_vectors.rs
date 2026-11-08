use crate::sockaddr_str_fixture::{self, Body, Case, Conversion, FromOp, Record, Sockaddr};
use osmocore_rs::base::sockaddr_str::{Family, SocketAddrText, SocketAddrTextError, family_of};
use osmocore_rs::compat::sockaddr_str::{self as compat, OsmoSockaddrStr};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const FIXTURE: &str =
    include_str!("../compatibility/vectors/sockaddr_str/sockaddr_str_vectors.ndjson");

fn records() -> Vec<Record> {
    sockaddr_str_fixture::validate(FIXTURE)
}

fn bodies() -> impl Iterator<Item = Body> {
    records().into_iter().map(|record| record.body)
}

fn compat_struct(s: &Sockaddr) -> OsmoSockaddrStr {
    let mut out = OsmoSockaddrStr {
        af: s.af,
        port: s.port,
        ..OsmoSockaddrStr::default()
    };
    out.ip[..s.ip.len()].copy_from_slice(&s.ip);
    out
}

/// The destination the harness fills before each conversion into an address.
fn junk() -> OsmoSockaddrStr {
    compat_struct(&Sockaddr {
        af: 99,
        ip: b"junk".to_vec(),
        port: 4242,
    })
}

fn assert_struct(actual: &OsmoSockaddrStr, expected: &Sockaddr, what: &str) {
    assert_eq!(actual.af, expected.af, "{what}");
    assert_eq!(actual.ip_text(), expected.ip, "{what}");
    assert_eq!(actual.port, expected.port, "{what}");
    assert!(
        actual.ip[expected.ip.len()..].iter().all(|&byte| byte == 0),
        "{what}"
    );
}

fn family_af(family: Option<Family>) -> i32 {
    match family {
        Some(Family::Ipv4) => compat::AF_INET,
        Some(Family::Ipv6) => compat::AF_INET6,
        None => compat::AF_UNSPEC,
    }
}

#[test]
fn fixture_is_well_formed() {
    sockaddr_str_fixture::check_corruptions(FIXTURE);
}

#[test]
fn constants_match_c() {
    let expected = [
        ("AF_UNSPEC", i64::from(compat::AF_UNSPEC)),
        ("AF_INET", i64::from(compat::AF_INET)),
        ("AF_INET6", i64::from(compat::AF_INET6)),
        ("INET6_ADDRSTRLEN", compat::INET6_ADDRSTRLEN as i64),
        // The results the compat docs give for each error.
        ("EINVAL", 22),
        ("ENOSPC", 28),
        ("EIO", 5),
        ("EAFNOSUPPORT", 97),
    ];
    let constants: Vec<(String, i64)> = bodies()
        .filter_map(|body| match body {
            Body::Constant { name, value } => Some((name, value)),
            _ => None,
        })
        .collect();
    let expected: Vec<(String, i64)> = expected
        .iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect();
    assert_eq!(constants, expected);
    assert_eq!(
        osmocore_rs::base::sockaddr_str::MAX_IP_LEN,
        compat::INET6_ADDRSTRLEN - 1
    );
}

#[test]
fn compat_type_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::Type { ip, af } = body else {
            continue;
        };
        assert_eq!(compat::osmo_ip_str_type(ip.as_deref()), af, "{ip:?}");
        checked += 1;
    }
    assert_eq!(checked, 80);
}

#[test]
fn preferred_family_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::Type { ip: Some(ip), af } = body else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&ip) else {
            continue;
        };
        assert_eq!(family_af(family_of(text)), af, "{text:?}");
        checked += 1;
    }
    assert!(checked > 70, "{checked}");
}

fn check_compat_case(case: &Case) {
    let s = compat_struct(&case.input);
    let what = format!("{:?}", case.input);
    assert_eq!(compat::osmo_sockaddr_str_fmt(Some(&s)), case.fmt, "{what}");
    assert_eq!(compat::osmo_sockaddr_str_is_set(&s), case.set, "{what}");
    assert_eq!(
        compat::osmo_sockaddr_str_is_nonzero(&s),
        case.nonzero,
        "{what}"
    );

    let mut in_addr = [0; 4];
    assert_eq!(
        compat::osmo_sockaddr_str_to_in_addr(&s, &mut in_addr),
        case.in_rc,
        "{what}"
    );
    assert_eq!(in_addr[..], case.in_addr, "{what}");
    let mut in6_addr = [0; 16];
    assert_eq!(
        compat::osmo_sockaddr_str_to_in6_addr(&s, &mut in6_addr),
        case.in6_rc,
        "{what}"
    );
    assert_eq!(in6_addr[..], case.in6_addr, "{what}");
    let mut value = 0;
    assert_eq!(
        compat::osmo_sockaddr_str_to_32(&s, &mut value),
        case.u32_rc,
        "{what}"
    );
    assert_eq!(value.to_ne_bytes()[..], case.u32_bytes, "{what}");
    let mut value = 0;
    assert_eq!(
        compat::osmo_sockaddr_str_to_32h(&s, &mut value),
        case.u32h_rc,
        "{what}"
    );
    assert_eq!(value, case.u32h, "{what}");

    let mut sin = compat::SockaddrIn::default();
    assert_eq!(
        compat::osmo_sockaddr_str_to_sockaddr_in(&s, &mut sin),
        case.sin_rc,
        "{what}"
    );
    assert_eq!(sin.to_bytes()[..], case.sin, "{what}");
    let mut sin6 = compat::SockaddrIn6::default();
    assert_eq!(
        compat::osmo_sockaddr_str_to_sockaddr_in6(&s, &mut sin6),
        case.sin6_rc,
        "{what}"
    );
    assert_eq!(sin6.to_bytes()[..], case.sin6, "{what}");
    let mut ss = compat::SockaddrStorage::default();
    assert_eq!(
        compat::osmo_sockaddr_str_to_sockaddr(&s, &mut ss),
        case.ss_rc,
        "{what}"
    );
    let (recorded, rest) = ss.0.split_at(sockaddr_str_fixture::STORAGE_RECORDED);
    assert_eq!(recorded, case.ss, "{what}");
    assert!(rest.iter().all(|&byte| byte == 0), "{what}");
}

#[test]
fn compat_cases_match_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::Case(case) = body else { continue };
        check_compat_case(&case);
        checked += 1;
    }
    assert_eq!(checked, 655);
}

fn run_compat_conversion(conversion: &Conversion) -> (i32, OsmoSockaddrStr) {
    let mut out = junk();
    let input = conversion.input.as_deref();
    let bytes = input.unwrap_or_default();
    let port = conversion.port.unwrap_or_default();
    let word = || u32::from_ne_bytes(bytes.try_into().unwrap());
    let rc = match conversion.op {
        FromOp::InAddr => {
            compat::osmo_sockaddr_str_from_in_addr(&mut out, bytes.try_into().unwrap(), port)
        }
        FromOp::In6Addr => {
            compat::osmo_sockaddr_str_from_in6_addr(&mut out, bytes.try_into().unwrap(), port)
        }
        FromOp::U32 => compat::osmo_sockaddr_str_from_32(&mut out, word(), port),
        FromOp::U32h => compat::osmo_sockaddr_str_from_32h(
            &mut out,
            u32::from_be_bytes(bytes.try_into().unwrap()),
            port,
        ),
        FromOp::SockaddrIn => compat::osmo_sockaddr_str_from_sockaddr_in(
            &mut out,
            &compat::SockaddrIn::from_bytes(bytes.try_into().unwrap()),
        ),
        FromOp::SockaddrIn6 => compat::osmo_sockaddr_str_from_sockaddr_in6(
            &mut out,
            &compat::SockaddrIn6::from_bytes(bytes.try_into().unwrap()),
        ),
        FromOp::Sockaddr => {
            let mut storage = compat::SockaddrStorage::default();
            storage.0[..bytes.len()].copy_from_slice(bytes);
            compat::osmo_sockaddr_str_from_sockaddr(&mut out, &storage)
        }
        FromOp::Str => compat::osmo_sockaddr_str_from_str(&mut out, input, port),
        FromOp::Str2 => compat::osmo_sockaddr_str_from_str2(&mut out, input),
    };
    (rc, out)
}

#[test]
fn compat_conversions_match_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::From(conversion) = body else {
            continue;
        };
        let (rc, out) = run_compat_conversion(&conversion);
        let what = format!("{conversion:?}");
        assert_eq!(rc, conversion.rc, "{what}");
        assert_struct(&out, &conversion.out, &what);
        checked += 1;
    }
    assert_eq!(checked, 701);
}

#[test]
fn compat_cmp_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::Cmp { a, b, sign } = body else {
            continue;
        };
        let (sa, sb) = (compat_struct(&a), compat_struct(&b));
        assert_eq!(
            compat::osmo_sockaddr_str_cmp(Some(&sa), Some(&sb)),
            sign,
            "{a:?} {b:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 1521);
}

#[test]
fn preferred_new_matches_from_str() {
    let mut checked = 0;
    for body in bodies() {
        let Body::From(Conversion {
            op: FromOp::Str,
            input: Some(input),
            port: Some(port),
            rc,
            out,
        }) = body
        else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&input) else {
            continue;
        };
        let result = SocketAddrText::new(text, port);
        match rc {
            0 => {
                let addr = result.unwrap();
                assert_eq!(addr.ip().as_bytes(), out.ip, "{text:?}");
                assert_eq!(family_af(Some(addr.family())), out.af, "{text:?}");
                assert_eq!(addr.port(), port, "{text:?}");
            }
            -5 => assert_eq!(result, Err(SocketAddrTextError::Empty), "{text:?}"),
            -28 => assert_eq!(result, Err(SocketAddrTextError::TooLong), "{text:?}"),
            -22 => assert_eq!(result, Err(SocketAddrTextError::NoFamily), "{text:?}"),
            _ => panic!("unexpected result {rc} for {text:?}"),
        }
        checked += 1;
    }
    assert!(checked > 80, "{checked}");
}

/// Cases whose family is the one the text looks like, which is all the preferred type holds.
#[test]
fn preferred_conversions_match_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::Case(case) = body else { continue };
        let Ok(text) = std::str::from_utf8(&case.input.ip) else {
            continue;
        };
        let Some(family) = family_of(text) else {
            continue;
        };
        if family_af(Some(family)) != case.input.af {
            continue;
        }
        let what = format!("{:?}", case.input);
        let addr = SocketAddrText::new(text, case.input.port).unwrap();
        assert_eq!(addr.to_string().as_bytes(), case.fmt, "{what}");
        assert_eq!(addr.is_set(), case.set, "{what}");
        assert_eq!(addr.is_nonzero(), case.nonzero, "{what}");
        match addr.family() {
            Family::Ipv4 => {
                assert_eq!(addr.to_ipv6(), Err(SocketAddrTextError::WrongFamily));
                match addr.to_ipv4() {
                    Ok(ip) => {
                        assert_eq!(case.in_rc, 0, "{what}");
                        assert_eq!(ip.octets()[..], case.in_addr, "{what}");
                    }
                    Err(error) => {
                        assert_eq!(error, SocketAddrTextError::Invalid, "{what}");
                        assert_ne!(case.in_rc, 0, "{what}");
                    }
                }
            }
            Family::Ipv6 => {
                assert_eq!(addr.to_ipv4(), Err(SocketAddrTextError::WrongFamily));
                match addr.to_ipv6() {
                    Ok(ip) => {
                        assert_eq!(case.in6_rc, 0, "{what}");
                        assert_eq!(ip.octets()[..], case.in6_addr, "{what}");
                    }
                    Err(error) => {
                        assert_eq!(error, SocketAddrTextError::Invalid, "{what}");
                        assert_ne!(case.in6_rc, 0, "{what}");
                    }
                }
            }
        }
        let socket = addr.to_socket_addr();
        assert_eq!(socket.is_ok(), case.ss_rc == 0, "{what}");
        if let Ok(socket) = socket {
            assert_eq!(socket.port(), case.input.port, "{what}");
        }
        checked += 1;
    }
    assert!(checked > 150, "{checked}");
}

/// Conversions from addresses that succeed in C give the same text in the preferred API.
#[test]
fn preferred_from_addresses_matches_c() {
    let mut checked = 0;
    for body in bodies() {
        let Body::From(conversion) = body else {
            continue;
        };
        if conversion.rc != 0 {
            continue;
        }
        let bytes = conversion.input.clone().unwrap_or_default();
        let ip = match conversion.op {
            FromOp::InAddr | FromOp::U32 | FromOp::U32h => {
                IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(&bytes[..]).unwrap()))
            }
            FromOp::In6Addr => {
                IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[..]).unwrap()))
            }
            FromOp::SockaddrIn => {
                IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(&bytes[4..8]).unwrap()))
            }
            FromOp::SockaddrIn6 | FromOp::Sockaddr if conversion.out.af == compat::AF_INET6 => {
                IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(&bytes[8..24]).unwrap()))
            }
            FromOp::Sockaddr => {
                IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(&bytes[4..8]).unwrap()))
            }
            FromOp::SockaddrIn6 | FromOp::Str | FromOp::Str2 => continue,
        };
        let addr = SocketAddrText::from_ip(ip, conversion.out.port);
        assert_eq!(addr.ip().as_bytes(), conversion.out.ip, "{conversion:?}");
        assert_eq!(
            family_af(Some(addr.family())),
            conversion.out.af,
            "{conversion:?}"
        );
        checked += 1;
    }
    assert!(checked > 400, "{checked}");
}

#[test]
fn upstream_records_are_present() {
    let upstream = records().iter().filter(|record| record.upstream).count();
    assert_eq!(upstream, sockaddr_str_fixture::UPSTREAM_COUNT);
}
