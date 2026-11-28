//! TCH/FS, TCH/EFS and FACCH/F coding against C.

use crate::gsm0503_tch_fr_fixture::{self, FILL, GUARD, Record};
use osmocore_rs::codec::{GSM_EFR_BYTES, GSM_FR_BYTES, GSM610_BITORDER, GSM660_BITORDER};
use osmocore_rs::coding::gsm0503::*;

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm0503/tch_fr_vectors.ndjson");

fn records() -> Vec<Record> {
    gsm0503_tch_fr_fixture::validate(FIXTURE)
}

fn order(net_order: bool) -> FrBitOrder {
    if net_order {
        FrBitOrder::Network
    } else {
        FrBitOrder::Reversed
    }
}

/// Checks a C destination against Rust's output and its untouched guard.
fn assert_destination<T: Copy + PartialEq + std::fmt::Debug>(
    what: &str,
    c: &[T],
    rust: &[T],
    fill: T,
) {
    assert_eq!(&c[..rust.len()], rust, "{what}");
    assert_eq!(c.len(), rust.len() + GUARD, "{what}");
    assert!(c[rust.len()..].iter().all(|value| *value == fill), "{what}");
}

#[test]
fn bit_orders_match_c() {
    let mut compared = 0;
    for record in records() {
        if let Record::Table { name, values } = record {
            let rust: Option<&[u16]> = match name.as_str() {
                "gsm610_bitorder" => Some(&GSM610_BITORDER),
                "gsm660_bitorder" => Some(&GSM660_BITORDER),
                // Private to the coder, and checked through every encode and decode.
                _ => None,
            };
            if let Some(rust) = rust {
                assert_eq!(rust, values, "{name}");
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 2);
    assert_eq!((GSM_FR_BYTES, GSM_EFR_BYTES), (33, 31));
}

#[test]
fn interleaving_matches_c() {
    let mut compared = 0;
    for record in records() {
        let Record::Interleave {
            inverse,
            case,
            input,
            output,
        } = record
        else {
            continue;
        };
        if inverse {
            let rust = tch_fr_deinterleave(input.as_slice().try_into().unwrap());
            assert_destination(&case, &output, &rust, FILL);
        } else {
            let mut rust = [FILL; TCH_FR_INTERLEAVED_BITS];
            tch_fr_interleave(input.as_slice().try_into().unwrap(), &mut rust);
            assert_destination(&case, &output, &rust, FILL);
        }
        compared += 1;
    }
    assert_eq!(compared, 6);
}

#[test]
fn burst_mapping_matches_c() {
    let mut compared = 0;
    for record in records() {
        match record {
            Record::Map {
                odd,
                h,
                input,
                output,
            } => {
                let mut burst = [FILL; TCH_BURST_BITS];
                let h = u8::try_from(h).ok();
                tch_burst_map(input.as_slice().try_into().unwrap(), &mut burst, h, odd);
                assert_destination("map", &output, &burst, FILL);
            }
            Record::Unmap {
                odd,
                h,
                input,
                output,
            } => {
                let mut data = [FILL as i8; TCH_BURST_DATA_BITS];
                let flag = tch_burst_unmap(input.as_slice().try_into().unwrap(), &mut data, odd);
                assert_eq!(flag, h);
                assert_destination("unmap", &output, &data, FILL as i8);
            }
            _ => continue,
        }
        compared += 1;
    }
    assert_eq!(compared, 12);
}

/// Encodes like C's length dispatch, or returns `None` for a length C rejects.
fn encode(data: &[u8], net_order: bool) -> Option<[u8; TCH_FR_BURSTS_BITS]> {
    let mut bursts = [FILL; TCH_FR_BURSTS_BITS];
    if data.is_empty() {
        tch_fr_encode(&mut bursts, None, order(net_order));
    } else {
        let frame = TchFrFrame::from_bytes(data)?;
        assert_eq!(frame.as_bytes(), data);
        tch_fr_encode(&mut bursts, Some(&frame), order(net_order));
    }
    Some(bursts)
}

#[test]
fn encoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        match record {
            Record::Encode {
                case,
                net_order,
                data,
                rc,
                bursts,
            } => match encode(&data, net_order) {
                Some(rust) => {
                    assert_eq!(rc, 0, "{case}");
                    assert_destination(&case, &bursts, &rust, FILL);
                }
                None => {
                    assert_eq!(rc, -1, "{case}");
                    assert!(bursts.iter().all(|bit| *bit == FILL), "{case}");
                }
            },
            Record::FacchEncode {
                case, data, bursts, ..
            } => {
                let mut rust = [FILL; TCH_FR_BURSTS_BITS];
                tch_fr_facch_encode(&mut rust, data.as_slice().try_into().unwrap());
                assert_destination(&case, &bursts, &rust, FILL);
            }
            _ => continue,
        }
        compared += 1;
    }
    assert_eq!(compared, 823 + 8);
}

#[test]
fn decoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        let Record::Decode {
            case,
            net_order,
            efr,
            input,
            rc,
            n_errors,
            n_bits_total,
            data,
        } = record
        else {
            continue;
        };
        let codec = if efr {
            TchFrCodec::Efr
        } else {
            TchFrCodec::Fr(order(net_order))
        };
        let decoded = tch_fr_decode(input.as_slice().try_into().unwrap(), codec);
        let frame = decoded.frame.as_ref().map_or(&[][..], TchFrFrame::as_bytes);
        let expected_rc = decoded
            .frame
            .map_or(-1, |frame| frame.as_bytes().len() as i32);
        assert_eq!(rc, expected_rc, "{case}");
        assert_eq!(&data[..frame.len()], frame, "{case}");
        assert!(
            data[frame.len()..].iter().all(|byte| *byte == FILL),
            "{case}"
        );
        assert_eq!(
            (decoded.bit_errors as i32, decoded.total_bits as i32),
            (n_errors, n_bits_total),
            "{case}"
        );
        assert_eq!(decoded.stolen, n_bits_total == 456, "{case}");
        if decoded.stolen {
            assert!(matches!(decoded.frame, None | Some(TchFrFrame::Facch(_))));
        }
        compared += 1;
    }
    assert_eq!(compared, 562);
}

#[test]
fn facch_decoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        let Record::FacchDecode {
            case,
            input,
            rc,
            n_errors,
            n_bits_total,
            data,
        } = record
        else {
            continue;
        };
        match tch_fr_facch_decode(input.as_slice().try_into().unwrap()) {
            None => {
                assert_eq!((rc, n_errors, n_bits_total), (-1, -1, -1), "{case}");
                assert!(data.iter().all(|byte| *byte == FILL), "{case}");
            }
            Some(decoded) => {
                assert_eq!(
                    (decoded.bit_errors as i32, decoded.total_bits as i32),
                    (n_errors, n_bits_total),
                    "{case}"
                );
                match decoded.data {
                    Some(l2) => {
                        assert_eq!(rc, 23, "{case}");
                        assert_destination(&case, &data, &l2, FILL);
                    }
                    None => {
                        assert_eq!(rc, -1, "{case}");
                        assert!(data.iter().all(|byte| *byte == FILL), "{case}");
                    }
                }
            }
        }
        compared += 1;
    }
    assert_eq!(compared, 88);
}

/// Replays upstream's `test_facch`: blocks four bursts apart share their bursts.
#[test]
fn neighbouring_blocks_share_bursts_like_c() {
    let mut compared = 0;
    for record in records() {
        let Record::FacchOffsets { data, bursts } = record else {
            continue;
        };
        let l2 = data.as_slice().try_into().unwrap();
        let mut rust = vec![0u8; bursts.len()];
        for block in 0..3 {
            let start = 4 * TCH_BURST_BITS * block;
            let window = &mut rust[start..start + TCH_FR_BURSTS_BITS];
            tch_fr_facch_encode(window.try_into().unwrap(), l2);
        }
        assert_eq!(rust, bursts);
        let soft: Vec<i8> = rust
            .iter()
            .map(|bit| if *bit == 0 { 127 } else { -127 })
            .collect();
        for block in 0..3 {
            let start = 4 * TCH_BURST_BITS * block;
            let window = soft[start..start + TCH_FR_BURSTS_BITS].try_into().unwrap();
            let decoded = tch_fr_facch_decode(window).unwrap();
            assert_eq!(decoded.data.as_ref().map(|l2| &l2[..]), Some(&data[..]));
            assert_eq!((decoded.bit_errors, decoded.total_bits), (0, 456));
        }
        compared += 1;
    }
    assert_eq!(compared, 3);
}

/// C's `sbit_t` stops at -127; Rust takes -128 without panicking.
#[test]
fn minimum_soft_bits_decode_without_panicking() {
    let bursts = [-128; TCH_FR_BURSTS_BITS];
    for codec in [TchFrCodec::Efr, TchFrCodec::Fr(FrBitOrder::Reversed)] {
        let decoded = tch_fr_decode(&bursts, codec);
        assert!(decoded.stolen);
    }
    assert!(tch_fr_facch_decode(&bursts).is_some());
}

#[test]
fn corrupted_fixtures_are_rejected() {
    gsm0503_tch_fr_fixture::check_corruptions(FIXTURE);
}
