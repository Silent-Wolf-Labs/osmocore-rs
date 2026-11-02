use crate::gsm0503_fixture::{self, FILL, Record};
use osmocore_rs::compat::gsm0503 as compat;
use osmocore_rs::conv::Termination;
use osmocore_rs::gsm0503::*;

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm0503/xcch_vectors.ndjson");

fn records() -> Vec<Record> {
    gsm0503_fixture::validate(FIXTURE)
}

fn array<const N: usize, T: Copy>(values: &[T]) -> [T; N] {
    values.try_into().unwrap()
}

/// The tables as C lays them out: one output and one state pair per state.
fn flatten(table: &[[u8; 2]]) -> Vec<u8> {
    table.iter().flatten().copied().collect()
}

#[test]
fn code_and_parity_descriptors_match_c() {
    for record in records() {
        match record {
            Record::Code {
                n,
                k,
                len,
                term,
                next_output,
                next_state,
            } => {
                let spec = xcch_code().spec();
                assert_eq!((spec.n, spec.k, spec.len), (n, k, len));
                assert_eq!(spec.term, Termination(term));
                assert_eq!(flatten(&spec.next_output), next_output);
                assert_eq!(flatten(&spec.next_state), next_state);
                assert_eq!(spec.next_term_output, None);
                assert_eq!(spec.next_term_state, None);
                assert_eq!(spec.puncture, None);
            }
            Record::Crc {
                bits,
                poly,
                init,
                remainder,
            } => {
                assert_eq!(FIRE_CRC40.bits, bits);
                assert_eq!(FIRE_CRC40.poly, poly);
                assert_eq!(FIRE_CRC40.init, init);
                assert_eq!(FIRE_CRC40.remainder, remainder);
            }
            _ => {}
        }
    }
}

#[test]
fn interleaving_matches_c() {
    let mut compared = 0;
    for record in records() {
        match record {
            Record::Interleave { input, output } => {
                let result = xcch_interleave(&array::<XCCH_CODED_BITS, u8>(&input));
                assert_eq!(result, output[..XCCH_CODED_BITS]);
                compared += 1;
            }
            Record::Deinterleave { input, output } => {
                let result = xcch_deinterleave(&array::<XCCH_CODED_BITS, u8>(&input));
                assert_eq!(result, output[..XCCH_CODED_BITS]);
                compared += 1;
            }
            _ => {}
        }
    }
    assert_eq!(compared, 6);
}

#[test]
fn deinterleaving_reverses_interleaving() {
    let block: [u16; XCCH_CODED_BITS] = std::array::from_fn(|index| index as u16);
    assert_eq!(xcch_deinterleave(&xcch_interleave(&block)), block);
}

#[test]
fn burst_mapping_matches_c() {
    let mut mapped = 0;
    let mut unmapped = 0;
    for record in records() {
        match record {
            Record::Map {
                hl,
                hn,
                input,
                output,
            } => {
                let mut burst = [FILL; XCCH_BURST_BITS];
                let flag = |value: i32| u8::try_from(value).ok();
                xcch_burst_map(&array(&input), &mut burst, flag(hl), flag(hn));
                assert_eq!(burst, output[..XCCH_BURST_BITS], "hl {hl} hn {hn}");
                mapped += 1;
            }
            Record::Unmap {
                hl,
                hn,
                input,
                output,
            } => {
                let soft: Vec<i8> = input.iter().map(|byte| *byte as i8).collect();
                let (data, actual_hl, actual_hn) = xcch_burst_unmap(&array(&soft));
                let expected: Vec<i8> = output[..XCCH_BURST_DATA_BITS]
                    .iter()
                    .map(|byte| *byte as i8)
                    .collect();
                assert_eq!(data[..], expected[..]);
                assert_eq!((actual_hl, actual_hn), (hl, hn));
                unmapped += 1;
            }
            _ => {}
        }
    }
    assert_eq!((mapped, unmapped), (5, 4));
}

#[test]
fn encoding_matches_c() {
    let mut compared = 0;
    for record in records() {
        if let Record::Encode { case, l2, bursts } = record {
            let frame = array(&l2);
            assert_eq!(xcch_encode(&frame), bursts[..XCCH_BURSTS_BITS], "{case}");
            let mut output = [FILL; XCCH_BURSTS_BITS];
            assert_eq!(compat::gsm0503_xcch_encode(&mut output, &frame), 0);
            assert_eq!(output, bursts[..XCCH_BURSTS_BITS], "{case}");
            compared += 1;
        }
    }
    assert_eq!(compared, gsm0503_fixture::ENCODE_COUNT);
}

#[test]
fn decoding_matches_c() {
    let (mut decoded_frames, mut failures) = (0, 0);
    for record in records() {
        if let Record::Decode {
            case,
            input,
            rc,
            n_errors,
            n_bits_total,
            l2,
        } = record
        {
            let soft = array(&input);
            let decoded = xcch_decode(&soft);
            assert_eq!(decoded.bit_errors as i32, n_errors, "{case}");
            assert_eq!(decoded.total_bits as i32, n_bits_total, "{case}");
            let expected = &l2[..XCCH_L2_BYTES];
            match decoded.data {
                Some(data) => {
                    assert_eq!(rc, 0, "{case}");
                    assert_eq!(data, expected, "{case}");
                    decoded_frames += 1;
                }
                None => {
                    assert_eq!(rc, -1, "{case}");
                    failures += 1;
                }
            }

            let mut frame = [FILL; XCCH_L2_BYTES];
            let (mut errors, mut total) = (-1, -1);
            let status = compat::gsm0503_xcch_decode(&mut frame, &soft, &mut errors, &mut total);
            assert_eq!(
                (status, errors, total),
                (rc, n_errors, n_bits_total),
                "{case}"
            );
            assert_eq!(frame, expected, "{case}");
        }
    }
    assert!(decoded_frames > 0 && failures > 0);
    assert_eq!(decoded_frames + failures, gsm0503_fixture::DECODE_COUNT);
}

#[test]
fn an_encoded_frame_survives_the_noise_upstream_applies() {
    let frame: [u8; XCCH_L2_BYTES] = std::array::from_fn(|index| index as u8 + 1);
    let mut soft = xcch_encode(&frame).map(|bit| if bit == 0 { 127 } else { -127 });
    soft[..30].fill(0);
    soft[XCCH_BURST_BITS..XCCH_BURST_BITS + 30].fill(0);
    let decoded = xcch_decode(&soft);
    assert_eq!(decoded.data, Some(frame));
    assert_eq!((decoded.bit_errors, decoded.total_bits), (60, 456));
}

#[test]
fn any_soft_bits_decode_without_panicking() {
    // -128 is outside the C range, so the fixture doesn't record it. The decoder
    // still accepts every i8.
    for soft in [-128i8, -127, 0, 127] {
        let decoded = xcch_decode(&[soft; XCCH_BURSTS_BITS]);
        assert_eq!(decoded.total_bits, 456);
    }
    let frame = [0x55; XCCH_L2_BYTES];
    let soft = xcch_encode(&frame).map(|bit| if bit == 0 { 127 } else { -128 });
    assert_eq!(xcch_decode(&soft).data, Some(frame));
    let mixed: [i8; XCCH_BURSTS_BITS] =
        std::array::from_fn(|index| if index % 3 == 0 { -128 } else { 127 });
    assert_eq!(xcch_decode(&mixed).total_bits, 456);
}

#[test]
fn corrupted_fixtures_are_rejected() {
    gsm0503_fixture::check_corruptions(FIXTURE);
}
