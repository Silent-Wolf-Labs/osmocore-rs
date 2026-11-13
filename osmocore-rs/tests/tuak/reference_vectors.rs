use crate::tuak_fixture::{self, GUARD, Record};
use osmocore_rs::compat::auth::{self as compat, OsmoAuthVector, OsmoSubAuthData2};

const FIXTURE: &str = include_str!("../compatibility/vectors/tuak/tuak_vectors.ndjson");

fn of_kind(kind: &str) -> Vec<Record> {
    let records: Vec<_> = tuak_fixture::validate(FIXTURE)
        .into_iter()
        .filter(|r| r.kind == kind)
        .collect();
    assert!(!records.is_empty(), "no {kind} records");
    records
}

fn int<T: TryFrom<i128>>(record: &Record, key: &str) -> T {
    T::try_from(record.int(key))
        .unwrap_or_else(|_| panic!("{key} {} out of range", record.int(key)))
}

#[test]
fn fixture_is_well_formed() {
    tuak_fixture::check_corruptions(FIXTURE);
}

/// The C subscriber data a record describes.
fn aud(record: &Record) -> OsmoSubAuthData2 {
    let mut aud = OsmoSubAuthData2 {
        auth_type: int(record, "type"),
        algo: compat::OSMO_AUTH_ALG_TUAK,
        ..OsmoSubAuthData2::default()
    };
    aud.umts.k = record.array("key");
    aud.umts.k_len = int(record, "k_len");
    aud.umts.opc = record.array("opc");
    aud.umts.opc_len = int(record, "opc_len");
    aud.umts.opc_is_op = int(record, "opc_is_op");
    aud.umts.amf = record.array("amf");
    aud.umts.sqn = int(record, "sqn");
    aud.umts.ind_bitlen = int(record, "ind_bitlen");
    aud.umts.ind = int(record, "ind");
    aud
}

fn guarded_vector(record: &Record) -> OsmoAuthVector {
    OsmoAuthVector {
        rand: [GUARD; 16],
        autn: [GUARD; 16],
        ck: [GUARD; 16],
        ik: [GUARD; 16],
        res: [GUARD; 16],
        res_len: int(record, "in_res_len"),
        kc: [GUARD; 8],
        sres: [GUARD; 4],
        auth_types: 0x5555_5555,
    }
}

#[cfg_attr(not(feature = "auth"), allow(dead_code))]
fn assert_outcome(record: &Record, rc: i32, vec: &OsmoAuthVector, aud: &OsmoSubAuthData2) {
    let what = format!("{} {:?}", record.kind, record.fields);
    assert_eq!(rc, int::<i32>(record, "rc"), "{what}");
    assert_eq!(vec.rand, record.array::<16>("out_rand"), "{what}");
    assert_eq!(vec.autn, record.array::<16>("autn"), "{what}");
    assert_eq!(vec.ck, record.array::<16>("ck"), "{what}");
    assert_eq!(vec.ik, record.array::<16>("ik"), "{what}");
    assert_eq!(vec.res, record.array::<16>("res"), "{what}");
    assert_eq!(vec.res_len, int::<u8>(record, "res_len"), "{what}");
    assert_eq!(vec.kc, record.array::<8>("kc"), "{what}");
    assert_eq!(vec.sres, record.array::<4>("sres"), "{what}");
    assert_eq!(vec.auth_types, int::<u32>(record, "auth_types"), "{what}");
    assert_eq!(aud.umts.sqn, int::<u64>(record, "out_sqn"), "{what}");
    assert_eq!(aud.umts.sqn_ms, int::<u64>(record, "out_sqn_ms"), "{what}");
}

/// Without the `auth` feature, compat has no TUAK and changes nothing.
#[cfg(not(feature = "auth"))]
fn assert_missing(record: &Record, rc: i32, vec: &OsmoAuthVector, aud: &OsmoSubAuthData2) {
    assert_eq!(rc, -2);
    assert_eq!(*vec, guarded_vector(record));
    assert_eq!(aud.umts.sqn, int::<u64>(record, "sqn"));
}

#[test]
fn compat_vectors_match_c() {
    for record in of_kind("gen_vec") {
        let (mut vec, mut aud) = (guarded_vector(&record), aud(&record));
        let rc = compat::osmo_auth_gen_vec2(&mut vec, &mut aud, &record.array("rand"));
        #[cfg(feature = "auth")]
        assert_outcome(&record, rc, &vec, &aud);
        #[cfg(not(feature = "auth"))]
        assert_missing(&record, rc, &vec, &aud);
    }
}

#[test]
fn compat_resyncs_match_c() {
    for record in of_kind("gen_vec_auts") {
        let (mut vec, mut aud) = (guarded_vector(&record), aud(&record));
        let rc = compat::osmo_auth_gen_vec_auts2(
            &mut vec,
            &mut aud,
            &record.array("auts"),
            &record.array("rand_auts"),
            &record.array("rand"),
        );
        #[cfg(feature = "auth")]
        assert_outcome(&record, rc, &vec, &aud);
        #[cfg(not(feature = "auth"))]
        assert_missing(&record, rc, &vec, &aud);
    }
}

#[cfg(feature = "auth")]
mod functions {
    use super::*;
    use osmocore_rs::gsm::auth::K;
    use osmocore_rs::gsm::tuak::{Tuak, topc_from_top};
    use std::num::NonZeroU32;

    fn key(record: &Record) -> Option<K> {
        let len: usize = int(record, "k_len");
        K::new(record.bytes("k").get(..len)?).ok()
    }

    fn iterations(record: &Record) -> NonZeroU32 {
        NonZeroU32::new(int(record, "iterations")).expect("at least one")
    }

    fn tuak(record: &Record) -> Option<Tuak> {
        Some(Tuak::new(&key(record)?, &record.array("opc")).with_iterations(iterations(record)))
    }

    /// Checks a record C rejected: the Rust types can't hold its lengths.
    fn assert_rejected(record: &Record, lengths: &[(&str, &[i128])]) {
        assert_eq!(record.int("rc"), tuak_fixture::EINVAL);
        let unrepresentable = key(record).is_none()
            || lengths
                .iter()
                .any(|(field, allowed)| !allowed.contains(&record.int(field)));
        assert!(unrepresentable, "{:?}", record.fields);
    }

    #[test]
    fn topc_matches_c() {
        for record in of_kind("opc") {
            let Some(k) = key(&record) else {
                assert_rejected(&record, &[]);
                continue;
            };
            let topc = topc_from_top(&k, &record.array("top"), iterations(&record));
            assert_eq!(topc[..], record.written("topc")[..], "{:?}", record.fields);
        }
    }

    #[test]
    fn f1_matches_c() {
        for record in of_kind("f1") {
            let (rand, sqn, amf) = (
                record.array("rand"),
                record.array("sqn"),
                record.array("amf"),
            );
            let star = record.flag("star");
            macro_rules! mac {
                ($tuak:expr, $n:literal) => {{
                    let mac: [u8; $n] = if star {
                        $tuak.f1_star(&rand, &sqn, &amf)
                    } else {
                        $tuak.f1(&rand, &sqn, &amf)
                    };
                    mac.to_vec()
                }};
            }
            let mac = match (tuak(&record), record.int("mac_len")) {
                (Some(tuak), 8) => mac!(tuak, 8),
                (Some(tuak), 16) => mac!(tuak, 16),
                (Some(tuak), 32) => mac!(tuak, 32),
                _ => {
                    assert_rejected(&record, &[("mac_len", &[8, 16, 32])]);
                    continue;
                }
            };
            assert_eq!(mac[..], record.written("mac")[..], "{:?}", record.fields);
        }
    }

    #[test]
    fn f2345_matches_c() {
        for record in of_kind("f2345") {
            let rand = record.array("rand");
            let lengths = (
                record.int("res_len"),
                record.int("ck_len"),
                record.int("ik_len"),
            );
            macro_rules! outputs {
                ($tuak:expr, $(($res:literal, $ck:literal, $ik:literal)),*) => {
                    match lengths {
                        $(($res, $ck, $ik) => {
                            let out = $tuak.f2345::<$res, $ck, $ik>(&rand);
                            Some((out.res.to_vec(), out.ck.to_vec(), out.ik.to_vec(), out.ak))
                        })*
                        _ => None,
                    }
                };
            }
            let outputs = tuak(&record).and_then(|tuak| {
                outputs!(
                    tuak,
                    (4, 16, 16),
                    (4, 16, 32),
                    (4, 32, 16),
                    (4, 32, 32),
                    (8, 16, 16),
                    (8, 16, 32),
                    (8, 32, 16),
                    (8, 32, 32),
                    (16, 16, 16),
                    (16, 16, 32),
                    (16, 32, 16),
                    (16, 32, 32),
                    (32, 16, 16),
                    (32, 16, 32),
                    (32, 32, 16),
                    (32, 32, 32)
                )
            });
            let Some((res, ck, ik, ak)) = outputs else {
                assert_rejected(
                    &record,
                    &[
                        ("res_len", &[4, 8, 16, 32]),
                        ("ck_len", &[16, 32]),
                        ("ik_len", &[16, 32]),
                    ],
                );
                continue;
            };
            let what = format!("{:?}", record.fields);
            assert_eq!(res[..], record.written("res")[..], "{what}");
            assert_eq!(ck[..], record.written("ck")[..], "{what}");
            assert_eq!(ik[..], record.written("ik")[..], "{what}");
            assert_eq!(ak[..], record.written("ak")[..], "{what}");
        }
    }

    #[test]
    fn f5_star_matches_c() {
        for record in of_kind("f5star") {
            let Some(tuak) = tuak(&record) else {
                assert_rejected(&record, &[]);
                continue;
            };
            let ak = tuak.f5_star(&record.array("rand"));
            assert_eq!(ak[..], record.written("ak")[..], "{:?}", record.fields);
        }
    }
}

#[cfg(feature = "auth")]
mod preferred {
    use super::*;
    use osmocore_rs::gsm::auth::{
        AuthError, Ck, Ik, K, Kc, Op, Opc, OperatorKey, ResLength, UmtsAlgorithm, UmtsSubscriber,
        UmtsVector,
    };

    /// The preferred subscriber for a UMTS record, if the preferred API can express it.
    fn subscriber(record: &Record) -> Option<(UmtsSubscriber, ResLength)> {
        if record.int("type") != i128::from(compat::OSMO_AUTH_TYPE_UMTS) {
            return None;
        }
        let k = K::new(record.bytes("key").get(..int::<usize>(record, "k_len"))?).ok()?;
        let opc = &record.bytes("opc")[..int::<usize>(record, "opc_len")];
        let operator = if record.int("opc_is_op") != 0 {
            OperatorKey::Op(Op::new(opc).ok()?)
        } else {
            OperatorKey::Opc(Opc::new(opc).ok()?)
        };
        let res_length = match record.int("in_res_len") {
            4 => ResLength::Four,
            8 => ResLength::Eight,
            16 => ResLength::Sixteen,
            _ => return None,
        };
        let mut subscriber = UmtsSubscriber::new(UmtsAlgorithm::Tuak, k, operator);
        subscriber.amf = record.array("amf");
        subscriber.sqn = int(record, "sqn");
        subscriber.ind_bitlen = int(record, "ind_bitlen");
        subscriber.ind = int(record, "ind");
        Some((subscriber, res_length))
    }

    fn assert_vector(record: &Record, vector: &UmtsVector) {
        let what = format!("{:?}", record.fields);
        let res_len: usize = int(record, "in_res_len");
        assert_eq!(vector.autn, record.array::<16>("autn"), "{what}");
        assert_eq!(vector.ck, Ck::new(record.array("ck")), "{what}");
        assert_eq!(vector.ik, Ik::new(record.array("ik")), "{what}");
        assert_eq!(
            vector.res.as_bytes(),
            &record.bytes("res")[..res_len],
            "{what}"
        );
        assert_eq!(vector.sres, record.array::<4>("sres"), "{what}");
        assert_eq!(vector.kc, Kc::new(record.array("kc")), "{what}");
    }

    /// The preferred error for a C result where the inputs reach TUAK.
    fn expected_error(record: &Record) -> AuthError {
        match record.int("rc") {
            -1 => AuthError::AutsMismatch,
            -2 => AuthError::IndBitlen(int(record, "ind_bitlen")),
            -3 => AuthError::IndRange {
                ind: int(record, "ind"),
                ind_bitlen: int(record, "ind_bitlen"),
            },
            _ => AuthError::OperatorKeyLength,
        }
    }

    #[test]
    fn vectors_match_c() {
        let mut compared = 0;
        for record in of_kind("gen_vec") {
            let Some((mut subscriber, res_length)) = subscriber(&record) else {
                continue;
            };
            match subscriber.vector(&record.array("rand"), res_length) {
                Ok(vector) => {
                    assert_eq!(record.int("rc"), 0, "{:?}", record.fields);
                    assert_vector(&record, &vector);
                }
                Err(error) => assert_eq!(error, expected_error(&record), "{:?}", record.fields),
            }
            assert_eq!(subscriber.sqn, int::<u64>(&record, "out_sqn"));
            compared += 1;
        }
        assert_eq!(compared, 167);
    }

    /// C checks AUTS with a 32-byte key even when K has 16 bytes; the preferred API
    /// doesn't, so those records are compared separately.
    fn short_key(record: &Record) -> bool {
        record.int("k_len") == 16
    }

    #[test]
    fn resyncs_match_c_with_a_32_byte_k() {
        let mut compared = 0;
        for record in of_kind("gen_vec_auts").iter().filter(|r| !short_key(r)) {
            let Some((mut subscriber, res_length)) = subscriber(record) else {
                continue;
            };
            let before = subscriber.sqn;
            let result = subscriber.resync(
                &record.array("auts"),
                &record.array("rand_auts"),
                &record.array("rand"),
                res_length,
            );
            match result {
                Ok(resync) => {
                    assert_eq!(record.int("rc"), 0, "{:?}", record.fields);
                    assert_eq!(resync.sqn_ms, int::<u64>(record, "out_sqn_ms"));
                    assert_vector(record, &resync.vector);
                    assert_eq!(subscriber.sqn, int::<u64>(record, "out_sqn"));
                }
                Err(error) => {
                    assert_eq!(error, expected_error(record), "{:?}", record.fields);
                    // Unlike C, a failed resync changes nothing.
                    assert_eq!(subscriber.sqn, before);
                }
            }
            compared += 1;
        }
        assert_eq!(compared, 32);
    }

    #[test]
    fn resyncs_with_a_16_byte_k_use_k_as_it_is() {
        let records: Vec<_> = of_kind("gen_vec_auts")
            .into_iter()
            .filter(short_key)
            .collect();
        // The harness made each AUTS twice: from the 16-byte K, which C rejects, then
        // from the whole 32-byte buffer, which C accepts. The preferred API does the
        // opposite, and its vector is the one C gives for the other AUTS.
        let pairs: Vec<_> = records.iter().filter(|r| r.int("opc_is_op") == 0).collect();
        assert_eq!(pairs.len(), 4);
        for pair in pairs.chunks(2) {
            let (from_k, from_buffer) = (pair[0], pair[1]);
            assert_eq!((from_k.int("rc"), from_buffer.int("rc")), (-1, 0));
            let (mut preferred, res_length) = subscriber(from_k).expect("a subscriber");
            let resync = preferred
                .resync(
                    &from_k.array("auts"),
                    &from_k.array("rand_auts"),
                    &from_k.array("rand"),
                    res_length,
                )
                .expect("the AUTS of a 16-byte K");
            assert_eq!(resync.sqn_ms, int::<u64>(from_buffer, "out_sqn_ms"));
            assert_vector(from_buffer, &resync.vector);
            assert_eq!(preferred.sqn, int::<u64>(from_buffer, "out_sqn"));
            let (mut preferred, _) = subscriber(from_buffer).expect("a subscriber");
            let result = preferred.resync(
                &from_buffer.array("auts"),
                &from_buffer.array("rand_auts"),
                &from_buffer.array("rand"),
                res_length,
            );
            assert_eq!(result, Err(AuthError::AutsMismatch));
        }
        // With OP, C accepts an AUTS from the buffer, and the preferred API rejects it.
        let with_op = records
            .iter()
            .find(|r| r.int("opc_is_op") == 1)
            .expect("OP");
        assert_eq!(with_op.int("rc"), 0);
        let (mut preferred, res_length) = subscriber(with_op).expect("a subscriber");
        let result = preferred.resync(
            &with_op.array("auts"),
            &with_op.array("rand_auts"),
            &with_op.array("rand"),
            res_length,
        );
        assert_eq!(result, Err(AuthError::AutsMismatch));
    }
}
