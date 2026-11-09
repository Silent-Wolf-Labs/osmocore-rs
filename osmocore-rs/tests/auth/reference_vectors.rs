use crate::auth_fixture::{self, ENOENT, GUARD, Record};
use osmocore_rs::compat::auth::{self as compat, OsmoAuthVector, OsmoSubAuthData2};
use osmocore_rs::gsm::auth::{
    Algorithm, AuthError, Ck, GsmAlgorithm, Ik, K, Kc, Ki, Op, Opc, OperatorKey, Res, ResLength,
    SresDerivation, UmtsAlgorithm, UmtsSubscriber, c2, c3, c4, c5,
};

const FIXTURE: &str = include_str!("../compatibility/vectors/auth/auth_vectors.ndjson");

fn records() -> Vec<Record> {
    auth_fixture::validate(FIXTURE)
}

fn of_kind(kind: &str) -> Vec<Record> {
    let records: Vec<_> = records().into_iter().filter(|r| r.kind == kind).collect();
    assert!(!records.is_empty(), "no {kind} records");
    records
}

fn int<T: TryFrom<i128>>(record: &Record, key: &str) -> T {
    T::try_from(record.int(key))
        .unwrap_or_else(|_| panic!("{key} {} out of range", record.int(key)))
}

#[test]
fn fixture_is_well_formed() {
    auth_fixture::check_corruptions(FIXTURE);
}

#[test]
fn names_match_c() {
    for record in of_kind("alg_name") {
        let algo: i32 = int(&record, "algo");
        assert_eq!(compat::osmo_auth_alg_name(algo), record.text("name"));
        if let Some(algorithm) = u8::try_from(algo).ok().and_then(Algorithm::from_code) {
            assert_eq!(algorithm.name(), record.text("name"));
            assert_eq!(algorithm.to_string(), record.text("name"));
        }
    }
    for record in of_kind("alg_parse") {
        let (text, algo) = (record.text("text"), int::<i32>(&record, "algo"));
        assert_eq!(compat::osmo_auth_alg_parse(text), algo, "{text:?}");
        let expected = u8::try_from(algo).ok().and_then(Algorithm::from_code);
        assert_eq!(Algorithm::parse(text), expected, "{text:?}");
    }
    for record in of_kind("type_name") {
        let auth_type: i32 = int(&record, "type");
        assert_eq!(
            compat::osmo_sub_auth_type_name(auth_type),
            record.text("name")
        );
    }
}

#[test]
fn conversions_match_c() {
    for record in of_kind("c2") {
        let (res, func) = (record.bytes("res"), int::<u8>(&record, "func"));
        let mut sres = [GUARD; 4];
        assert_eq!(compat::osmo_auth_c2(&mut sres, res, func), 0);
        assert_eq!(sres, record.array::<4>("sres"), "{func} {res:02x?}");
        if let Some(res) = Res::new(res) {
            let derivation =
                [SresDerivation::Fold, SresDerivation::Truncate][usize::from(func) - 1];
            assert_eq!(c2(&res, derivation), record.array::<4>("sres"));
        }
    }
    for record in of_kind("c3") {
        let (ck, ik) = (record.array::<16>("ck"), record.array::<16>("ik"));
        let mut kc = [GUARD; 8];
        compat::osmo_auth_c3(&mut kc, &ck, &ik);
        assert_eq!(kc, record.array::<8>("kc"));
        assert_eq!(c3(&Ck::new(ck), &Ik::new(ik)), Kc::new(kc));
    }
    for record in of_kind("c4") {
        let kc = record.array::<8>("kc");
        let mut ck = [GUARD; 16];
        compat::osmo_c4(&mut ck, &kc);
        assert_eq!(ck, record.array::<16>("ck"));
        assert_eq!(c4(&Kc::new(kc)), Ck::new(ck));
    }
    for record in of_kind("from_2g") {
        let mut vec = OsmoAuthVector {
            ck: [GUARD; 16],
            ik: [GUARD; 16],
            kc: record.array("kc"),
            auth_types: int(&record, "auth_types"),
            ..OsmoAuthVector::default()
        };
        let rc = compat::osmo_auth_3g_from_2g(&mut vec);
        assert_eq!(rc, int::<i32>(&record, "rc"));
        assert_eq!(vec.ck, record.array::<16>("ck"));
        assert_eq!(vec.ik, record.array::<16>("ik"));
        if rc == 1 {
            let kc = Kc::new(vec.kc);
            assert_eq!(c4(&kc), Ck::new(vec.ck));
            assert_eq!(c5(&kc), Ik::new(vec.ik));
        }
    }
}

#[cfg(feature = "auth")]
mod milenage {
    use super::*;
    use osmocore_rs::gsm::milenage::{Check, Milenage, opc_from_op};

    fn milenage(record: &Record) -> Milenage {
        Milenage::new(&record.array("k"), &record.array("opc"))
    }

    #[test]
    fn opc_matches_c() {
        for record in of_kind("opc") {
            let (k, op) = (record.array::<16>("k"), record.array::<16>("op"));
            assert_eq!(opc_from_op(&k, &op), record.array::<16>("opc"));
            // OP and the OPc derived from it give the same functions.
            let (via_op, via_opc) = (
                Milenage::with_op(&k, &op),
                Milenage::new(&k, &record.array("opc")),
            );
            assert_eq!(via_op.f2345(&[0; 16]), via_opc.f2345(&[0; 16]));
        }
    }

    #[test]
    fn functions_match_c() {
        for record in of_kind("milenage") {
            let milenage = milenage(&record);
            let rand = record.array::<16>("rand");
            let macs = milenage.f1(&rand, &record.array("sqn"), &record.array("amf"));
            assert_eq!(macs.mac_a, record.array::<8>("mac_a"));
            assert_eq!(macs.mac_s, record.array::<8>("mac_s"));
            let outputs = milenage.f2345(&rand);
            assert_eq!(outputs.res, record.array::<8>("res"));
            assert_eq!(outputs.ck, Ck::new(record.array("ck")));
            assert_eq!(outputs.ik, Ik::new(record.array("ik")));
            assert_eq!(outputs.ak, record.array::<6>("ak"));
            assert_eq!(outputs.ak_star, record.array::<6>("ak_star"));
            let gsm = milenage.gsm(&rand);
            assert_eq!(gsm.sres, record.array::<4>("sres"));
            assert_eq!(gsm.kc, Kc::new(record.array("kc")));
        }
    }

    #[test]
    fn auts_matches_c() {
        for record in of_kind("auts") {
            let milenage = milenage(&record);
            let (rand, auts) = (record.array::<16>("rand"), record.array::<14>("auts"));
            let result = milenage.auts_sqn(&rand, &auts);
            if record.int("rc") == 0 {
                let sqn = record.array::<6>("sqn");
                assert_eq!(result, Ok(sqn));
                // A USIM at that SQN sends the same AUTS.
                assert_eq!(milenage.auts(&rand, &sqn), auts);
            } else {
                assert!(result.is_err(), "{auts:02x?}");
            }
        }
    }

    #[test]
    fn check_matches_c() {
        for record in of_kind("check") {
            let check = milenage(&record).check(
                &record.array("rand"),
                &record.array("autn"),
                &record.array("sqn"),
            );
            let expected = match record.int("rc") {
                0 => Check::Accepted {
                    res: record.array("res"),
                    ck: Ck::new(record.array("ck")),
                    ik: Ik::new(record.array("ik")),
                },
                -2 => Check::Resync {
                    auts: record.array("auts"),
                },
                _ => Check::MacFailure,
            };
            assert_eq!(check, expected);
        }
    }
}

/// The C subscriber data a record describes.
fn aud(record: &Record) -> OsmoSubAuthData2 {
    let mut aud = OsmoSubAuthData2 {
        auth_type: int(record, "type"),
        algo: int(record, "algo"),
        ..OsmoSubAuthData2::default()
    };
    let key = record.array::<32>("key");
    if aud.auth_type == compat::OSMO_AUTH_TYPE_GSM {
        aud.gsm.ki.copy_from_slice(&key[..16]);
        return aud;
    }
    aud.umts.k = key;
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
    if record.has("out_key") {
        assert_eq!(aud.gsm.ki[..], record.bytes("out_key")[..], "{what}");
    } else {
        assert_eq!(aud.umts.sqn, int::<u64>(record, "out_sqn"), "{what}");
        assert_eq!(aud.umts.sqn_ms, int::<u64>(record, "out_sqn_ms"), "{what}");
    }
}

/// Milenage records need the `auth` feature; without it, compat reports no algorithm.
fn needs_missing_feature(record: &Record) -> bool {
    !cfg!(feature = "auth") && record.int("algo") == i128::from(compat::OSMO_AUTH_ALG_MILENAGE)
}

#[test]
fn compat_vectors_match_c() {
    for record in of_kind("gen_vec") {
        let (mut vec, mut aud) = (guarded_vector(&record), aud(&record));
        let rc = compat::osmo_auth_gen_vec2(&mut vec, &mut aud, &record.array("rand"));
        if needs_missing_feature(&record) {
            assert_eq!(i128::from(rc), ENOENT);
            continue;
        }
        assert_outcome(&record, rc, &vec, &aud);
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
        if needs_missing_feature(&record) {
            assert_eq!(i128::from(rc), ENOENT);
            continue;
        }
        assert_outcome(&record, rc, &vec, &aud);
    }
}

/// The preferred subscriber for a UMTS record, if the preferred API can express it.
fn subscriber(record: &Record) -> Option<(UmtsSubscriber, ResLength)> {
    if record.int("type") != i128::from(compat::OSMO_AUTH_TYPE_UMTS) {
        return None;
    }
    let algorithm = match int::<i32>(record, "algo") {
        compat::OSMO_AUTH_ALG_XOR_3G => UmtsAlgorithm::Xor3g,
        #[cfg(feature = "auth")]
        compat::OSMO_AUTH_ALG_MILENAGE => UmtsAlgorithm::Milenage,
        _ => return None,
    };
    let key = record.bytes("key");
    let k = K::new(&key[..int::<usize>(record, "k_len")]).ok()?;
    let opc = &record.bytes("opc")[..int::<usize>(record, "opc_len")];
    let operator = if record.int("opc_is_op") != 0 {
        OperatorKey::Op(Op::new(opc).ok()?)
    } else {
        OperatorKey::Opc(Opc::new(opc).ok()?)
    };
    // XOR-3G ignores the RES length.
    let res_length = if algorithm == UmtsAlgorithm::Xor3g {
        ResLength::Eight
    } else {
        match record.int("in_res_len") {
            4 => ResLength::Four,
            8 => ResLength::Eight,
            _ => return None,
        }
    };
    let mut subscriber = UmtsSubscriber::new(algorithm, k, operator);
    subscriber.amf = record.array("amf");
    subscriber.sqn = int(record, "sqn");
    subscriber.ind_bitlen = int(record, "ind_bitlen");
    subscriber.ind = int(record, "ind");
    Some((subscriber, res_length))
}

fn assert_vector(record: &Record, vector: &osmocore_rs::gsm::auth::UmtsVector) {
    let res_len: usize = if record.int("algo") == i128::from(compat::OSMO_AUTH_ALG_XOR_3G) {
        16
    } else {
        int(record, "in_res_len")
    };
    assert_eq!(vector.autn, record.array::<16>("autn"));
    assert_eq!(vector.ck, Ck::new(record.array("ck")));
    assert_eq!(vector.ik, Ik::new(record.array("ik")));
    assert_eq!(vector.res.as_bytes(), &record.bytes("res")[..res_len]);
    assert_eq!(vector.sres, record.array::<4>("sres"));
    assert_eq!(vector.kc, Kc::new(record.array("kc")));
}

/// The preferred error for a C result where the inputs reach the algorithm.
fn expected_error(record: &Record) -> AuthError {
    match record.int("rc") {
        -1 => AuthError::AutsMismatch,
        -2 => AuthError::IndBitlen(int(record, "ind_bitlen")),
        -3 => AuthError::IndRange {
            ind: int(record, "ind"),
            ind_bitlen: int(record, "ind_bitlen"),
        },
        _ if record.int("k_len") != 16 => AuthError::KeyLength,
        _ => AuthError::OperatorKeyLength,
    }
}

#[test]
fn preferred_vectors_match_c() {
    let mut compared = 0;
    for record in of_kind("gen_vec") {
        if record.int("type") == i128::from(compat::OSMO_AUTH_TYPE_GSM)
            && record.int("algo") == i128::from(compat::OSMO_AUTH_ALG_XOR_3G)
        {
            let ki = Ki::new(record.bytes("key")[..16].try_into().unwrap());
            let gsm = GsmAlgorithm::Xor3g.vector(&ki, &record.array("rand"));
            assert_eq!(gsm.sres, record.array::<4>("sres"));
            assert_eq!(gsm.kc, Kc::new(record.array("kc")));
            compared += 1;
            continue;
        }
        let Some((mut subscriber, res_length)) = subscriber(&record) else {
            continue;
        };
        let result = subscriber.vector(&record.array("rand"), res_length);
        match result {
            Ok(vector) => {
                assert_eq!(record.int("rc"), 0, "{:?}", record.fields);
                assert_vector(&record, &vector);
            }
            Err(error) => assert_eq!(error, expected_error(&record), "{:?}", record.fields),
        }
        assert_eq!(subscriber.sqn, int::<u64>(&record, "out_sqn"));
        compared += 1;
    }
    // Without the `auth` feature, only the XOR-3G records apply.
    let expected = if cfg!(feature = "auth") { 198 } else { 33 };
    assert_eq!(compared, expected);
}

#[test]
fn preferred_resyncs_match_c() {
    let mut compared = 0;
    for record in of_kind("gen_vec_auts") {
        let Some((mut subscriber, res_length)) = subscriber(&record) else {
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
                assert_eq!(resync.sqn_ms, int::<u64>(&record, "out_sqn_ms"));
                assert_vector(&record, &resync.vector);
                assert_eq!(subscriber.sqn, int::<u64>(&record, "out_sqn"));
            }
            Err(error) => {
                assert_eq!(error, expected_error(&record), "{:?}", record.fields);
                // Unlike C, a failed resync changes nothing.
                assert_eq!(subscriber.sqn, before);
            }
        }
        compared += 1;
    }
    let expected = if cfg!(feature = "auth") { 39 } else { 10 };
    assert_eq!(compared, expected);
}
