use crate::gea_fixture::{self, Record};
use osmocore_rs::compat::gea as compat;
use osmocore_rs::gsm::gea::*;

const FIXTURE: &str = include_str!("../compatibility/vectors/gea/gea_vectors.ndjson");

fn records() -> Vec<Record> {
    gea_fixture::validate(FIXTURE)
}

#[test]
fn fixture_is_well_formed() {
    gea_fixture::check_corruptions(FIXTURE);
}

#[test]
fn the_frame_limit_matches_the_recording() {
    assert_eq!(MAX_FRAME_BYTES, usize::from(gea_fixture::MAX_BLOCK));
}

#[test]
fn algorithms_match_c() {
    for record in records() {
        let Record::Algorithm {
            algo,
            supported,
            key_length,
            name,
        } = record
        else {
            continue;
        };
        assert_eq!(compat::gprs_cipher_supported(algo), supported, "GEA{algo}");
        assert_eq!(
            compat::gprs_cipher_key_length(algo),
            key_length,
            "GEA{algo}"
        );
        assert_eq!(compat::gprs_cipher_name(algo), name.as_deref(), "GEA{algo}");

        let algorithm = u8::try_from(algo).ok().and_then(GprsAlgorithm::from_number);
        assert_eq!(algorithm.map(GprsAlgorithm::name), name.as_deref());
        if let Some(algorithm) = algorithm {
            assert_eq!(algorithm.key_bytes(), key_length as usize);
            assert_eq!(i32::from(algorithm.is_implemented()), supported);
        }
    }
}

#[test]
fn inputs_match_c() {
    for record in records() {
        match record {
            Record::InputUi {
                iov_ui,
                sapi,
                lfn,
                oc,
                input,
            } => {
                assert_eq!(input_ui(iov_ui, sapi, lfn, oc), Some(input));
                assert_eq!(
                    compat::gprs_cipher_gen_input_ui(iov_ui, sapi, lfn, oc),
                    input
                );
            }
            Record::InputI {
                iov_i,
                lfn,
                oc,
                input,
            } => {
                assert_eq!(input_i(iov_i, lfn, oc), input);
                assert_eq!(compat::gprs_cipher_gen_input_i(iov_i, lfn, oc), input);
            }
            _ => {}
        }
    }
}

#[test]
fn runs_match_c() {
    let mut preferred = 0;
    for record in records() {
        let Record::Run {
            algo,
            key,
            iv,
            dir,
            len,
            rc,
            out,
            ..
        } = record
        else {
            continue;
        };
        let mut c = vec![gea_fixture::GUARD; usize::from(len) + gea_fixture::GUARD_BYTES];
        let dir = u32::try_from(dir).unwrap();
        assert_eq!(
            compat::gprs_cipher_run(&mut c, len, algo, &key, iv, dir),
            rc,
            "GEA{algo} {len} bytes"
        );
        assert_eq!(c[..out.len()], out, "GEA{algo} {len} bytes");
        assert!(
            c[out.len()..]
                .iter()
                .all(|&byte| byte == gea_fixture::GUARD)
        );

        let direction = match dir {
            0 => Direction::Uplink,
            1 => Direction::Downlink,
            _ => continue,
        };
        if rc != 0 {
            continue;
        }
        let mut keystream = vec![0; usize::from(len)];
        match algo {
            3 => gea3(key[..].try_into().unwrap(), iv, direction, &mut keystream),
            4 => gea4(key[..].try_into().unwrap(), iv, direction, &mut keystream),
            _ => unreachable!("only GEA3 and GEA4 succeed"),
        }
        assert_eq!(keystream, out[..usize::from(len)]);
        preferred += 1;
    }
    assert!(preferred > 0);
}
