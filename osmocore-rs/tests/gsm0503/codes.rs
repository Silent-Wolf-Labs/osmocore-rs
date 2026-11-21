//! The GSM 05.03 convolutional codes and parity descriptors against C.

use crate::gsm0503_codes_fixture::{self, FILL, GUARD, Record, UPSTREAM_LENGTHS};
use osmocore_rs::base::conv::{self, Termination};
use osmocore_rs::base::crc::{Crc8Code, Crc16Code, Crc64Code};
use osmocore_rs::coding::gsm0503::*;
use osmocore_rs::compat::conv::{osmo_conv_get_input_length, osmo_conv_get_output_length};

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm0503/codes_vectors.ndjson");

fn records() -> Vec<Record> {
    gsm0503_codes_fixture::validate(FIXTURE)
}

fn flatten(table: &[[u8; 2]]) -> Vec<u8> {
    table.iter().flatten().copied().collect()
}

fn code_named(name: &str) -> Gsm0503Code {
    Gsm0503Code::ALL
        .into_iter()
        .find(|code| code.c_name() == name)
        .unwrap_or_else(|| panic!("no code named {name}"))
}

#[test]
fn every_c_code_is_listed_in_order() {
    let names: Vec<_> = Gsm0503Code::ALL.map(Gsm0503Code::c_name).into();
    let upstream: Vec<_> = UPSTREAM_LENGTHS.map(|(name, _, _)| name).into();
    assert_eq!(names, upstream);
    for (index, code) in Gsm0503Code::ALL.into_iter().enumerate() {
        assert_eq!(code as usize, index);
    }
    assert!(std::ptr::eq(xcch_code(), Gsm0503Code::Xcch.code()));
}

/// Replays `conv_gsm0503_test.ok`, which records only these lengths.
#[test]
fn lengths_match_upstream_test_output() {
    for (code, (name, input, output)) in Gsm0503Code::ALL.into_iter().zip(UPSTREAM_LENGTHS) {
        let code = code.code();
        assert_eq!(osmo_conv_get_input_length(code, 0), Ok(input), "{name}");
        assert_eq!(osmo_conv_get_output_length(code, 0), Ok(output), "{name}");
        assert_eq!(code.output_capacity(), Ok(output as usize), "{name}");
        assert_eq!(code.decode_input_capacity(), Ok(output as usize), "{name}");
    }
}

#[test]
fn descriptors_match_c() {
    let mut compared = 0;
    for record in records() {
        let Record::Code {
            code,
            n,
            k,
            len,
            term,
            next_output,
            next_state,
            next_term_output,
            next_term_state,
            puncture,
            ..
        } = record
        else {
            continue;
        };
        let spec = code_named(&code).code().spec();
        assert_eq!((spec.n, spec.k, spec.len), (n, k, len), "{code}");
        assert_eq!(spec.term, Termination(term), "{code}");
        assert_eq!(flatten(&spec.next_output), next_output, "{code}");
        assert_eq!(flatten(&spec.next_state), next_state, "{code}");
        assert_eq!(spec.next_term_output, next_term_output, "{code}");
        assert_eq!(spec.next_term_state, next_term_state, "{code}");
        assert_eq!(spec.puncture, puncture, "{code}");
        compared += 1;
    }
    assert_eq!(compared, Gsm0503Code::ALL.len());
}

#[test]
fn encoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        let Record::Encode {
            code,
            case,
            input,
            rc,
            output: expected,
        } = record
        else {
            continue;
        };
        let mut output = vec![FILL; expected.len()];
        let written = conv::encode(code_named(&code).code(), &input, &mut output);
        assert_eq!(written, Ok(rc as usize), "{code} {case}");
        assert_eq!(output, expected, "{code} {case}");
        compared += 1;
    }
    assert_eq!(compared, 9 * Gsm0503Code::ALL.len());
}

#[test]
fn decoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        let Record::Decode {
            code,
            case,
            input,
            rc,
            output: expected,
        } = record
        else {
            continue;
        };
        let mut output = vec![FILL; expected.len()];
        let result = conv::decode(code_named(&code).code(), &input, &mut output);
        assert_eq!(result, Ok(rc), "{code} {case}");
        assert_eq!(output, expected, "{code} {case}");
        compared += 1;
    }
    assert_eq!(compared, 17 * Gsm0503Code::ALL.len());
}

/// The parity descriptors, widened so that one comparison covers every width.
fn crc_named(name: &str) -> (u32, i32, u64, u64, u64) {
    let narrow = |code: Crc8Code| {
        let Crc8Code {
            bits,
            poly,
            init,
            remainder,
        } = code;
        (8, bits, poly.into(), init.into(), remainder.into())
    };
    let middle = |code: Crc16Code| {
        let Crc16Code {
            bits,
            poly,
            init,
            remainder,
        } = code;
        (16, bits, poly.into(), init.into(), remainder.into())
    };
    let wide = |code: Crc64Code| {
        let Crc64Code {
            bits,
            poly,
            init,
            remainder,
        } = code;
        (64, bits, poly, init, remainder)
    };
    match name {
        "gsm0503_fire_crc40" => wide(FIRE_CRC40),
        "gsm0503_cs234_crc16" => middle(CS234_CRC16),
        "gsm0503_mcs_crc8_hdr" => narrow(MCS_CRC8_HDR),
        "gsm0503_mcs_crc12" => middle(MCS_CRC12),
        "gsm0503_rach_crc6" => narrow(RACH_CRC6),
        "gsm0503_sch_crc10" => middle(SCH_CRC10),
        "gsm0503_tch_fr_crc3" => narrow(TCH_FR_CRC3),
        "gsm0503_tch_efr_crc8" => narrow(TCH_EFR_CRC8),
        "gsm0503_amr_crc6" => narrow(AMR_CRC6),
        "gsm0503_amr_crc14" => middle(AMR_CRC14),
        other => panic!("no parity descriptor named {other}"),
    }
}

/// Runs compute, set and check on one descriptor, as C's `crcXXgen` calls do.
fn crc_bits(name: &str, input: &[u8], parity: &mut [u8]) -> (u64, bool, bool) {
    fn run<T: Into<u64>>(
        compute: impl Fn(&[u8]) -> T,
        set: impl Fn(&[u8], &mut [u8]),
        check: impl Fn(&[u8], &[u8]) -> bool,
        input: &[u8],
        parity: &mut [u8],
    ) -> (u64, bool, bool) {
        let value = compute(input).into();
        set(input, parity);
        let good = check(input, parity);
        parity[0] ^= 1;
        let flipped = check(input, parity);
        parity[0] ^= 1;
        (value, good, flipped)
    }
    macro_rules! with {
        ($code:expr) => {
            run(
                |input| $code.compute_bits(input).unwrap(),
                |input, parity| $code.set_bits(input, parity).unwrap(),
                |input, parity| $code.check_bits(input, parity).unwrap(),
                input,
                parity,
            )
        };
    }
    match name {
        "gsm0503_fire_crc40" => with!(FIRE_CRC40),
        "gsm0503_cs234_crc16" => with!(CS234_CRC16),
        "gsm0503_mcs_crc8_hdr" => with!(MCS_CRC8_HDR),
        "gsm0503_mcs_crc12" => with!(MCS_CRC12),
        "gsm0503_rach_crc6" => with!(RACH_CRC6),
        "gsm0503_sch_crc10" => with!(SCH_CRC10),
        "gsm0503_tch_fr_crc3" => with!(TCH_FR_CRC3),
        "gsm0503_tch_efr_crc8" => with!(TCH_EFR_CRC8),
        "gsm0503_amr_crc6" => with!(AMR_CRC6),
        "gsm0503_amr_crc14" => with!(AMR_CRC14),
        other => panic!("no parity descriptor named {other}"),
    }
}

#[test]
fn parity_matches_c() {
    let (mut descriptors, mut cases) = (0, 0);
    for record in records() {
        match record {
            Record::Crc {
                crc,
                width,
                bits,
                poly,
                init,
                remainder,
            } => {
                assert_eq!(
                    crc_named(&crc),
                    (width, bits, poly, init, remainder),
                    "{crc}"
                );
                descriptors += 1;
            }
            Record::CrcBits {
                crc,
                case,
                input,
                value,
                set,
                check,
                check_flipped,
            } => {
                let mut parity = vec![FILL; set.len()];
                let (actual, good, flipped) = crc_bits(&crc, &input, &mut parity);
                assert_eq!(actual, value, "{crc} {case}");
                assert_eq!(parity, set, "{crc} {case}");
                assert_eq!(good, check == 0, "{crc} {case}");
                assert_eq!(flipped, check_flipped == 0, "{crc} {case}");
                assert!(
                    parity[parity.len() - GUARD..]
                        .iter()
                        .all(|byte| *byte == FILL)
                );
                cases += 1;
            }
            _ => {}
        }
    }
    assert_eq!((descriptors, cases), (10, 70));
}

/// Soft bits outside the compared range, -128, must not panic, even though the C
/// backends disagree there.
#[test]
fn minus_128_does_not_panic() {
    for code in Gsm0503Code::ALL {
        let code = code.code();
        let input = vec![-128; code.decode_input_capacity().unwrap()];
        let mut output = vec![0; code.spec().len as usize];
        conv::decode(code, &input, &mut output).unwrap();
        assert!(output.iter().all(|bit| *bit <= 1));
    }
}

#[test]
fn corrupted_fixtures_are_rejected() {
    gsm0503_codes_fixture::check_corruptions(FIXTURE);
}
