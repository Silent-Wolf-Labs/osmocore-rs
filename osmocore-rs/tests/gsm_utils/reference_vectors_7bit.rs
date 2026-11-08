use crate::gsm_utils_7bit_fixture::{self, DecodeMode, Record, Source};
use osmocore_rs::compat::gsm_utils as compat;
use osmocore_rs::gsm::gsm_utils::*;

const FIXTURE: &str =
    include_str!("../compatibility/vectors/gsm_utils/gsm_utils_7bit_vectors.ndjson");
const GUARD: u8 = 0xa5;

fn records() -> Vec<Record> {
    gsm_utils_7bit_fixture::validate(FIXTURE)
}

/// Runs a compat call on a guarded buffer of `size` bytes and checks that it wrote
/// nothing past `written`.
fn guarded(size: usize, written: usize, call: impl FnOnce(&mut [u8]) -> i32) -> (Vec<u8>, i32) {
    let mut buffer = vec![GUARD; size];
    let result = call(&mut buffer);
    assert!(buffer[written..].iter().all(|&byte| byte == GUARD));
    (buffer, result)
}

#[test]
fn fixture_is_well_formed() {
    gsm_utils_7bit_fixture::check_corruptions(FIXTURE);
}

#[test]
fn octet_lengths_match_c() {
    for record in records() {
        if let Record::OctetLen { septets, octets } = record {
            assert_eq!(compat::gsm_get_octet_len(septets), octets);
            assert_eq!(septets_to_octets(usize::from(septets)), usize::from(octets));
        }
    }
}

#[test]
fn septets_match_c() {
    for record in records() {
        if let Record::Septet { text, septets } = record {
            assert_eq!(encode_septets(&text), septets, "{text:02x?}");
            let (buffer, count) = guarded(septets.len() + 4, septets.len(), |buffer| {
                compat::gsm_septet_encode(buffer, &text)
            });
            assert_eq!(count as usize, septets.len());
            assert_eq!(buffer[..septets.len()], septets);
        }
    }
}

#[test]
fn packing_matches_c() {
    for record in records() {
        if let Record::Pack {
            septets,
            padding,
            octets,
            ..
        } = record
        {
            assert_eq!(
                pack_septets(&septets, padding).as_ref(),
                Some(&octets),
                "{septets:02x?} padding {padding}"
            );
            let (buffer, count) = guarded(octets.len() + 4, octets.len(), |buffer| {
                compat::gsm_septet_pack(buffer, &septets, padding)
            });
            assert_eq!(count as usize, octets.len());
            assert_eq!(buffer[..octets.len()], octets);
        }
    }
}

#[test]
fn encoding_matches_c() {
    for record in records() {
        if let Record::Encode {
            ussd,
            text,
            n,
            septets,
            octets,
            ..
        } = record
        {
            let packed = if ussd {
                encode_7bit_ussd(&text, n)
            } else {
                encode_7bit(&text, n)
            };
            assert_eq!(packed.octets, octets, "{text:02x?} n {n}");
            assert_eq!(packed.septets as i32, septets, "{text:02x?} n {n}");

            let mut buffer = vec![GUARD; n];
            let counts = if ussd {
                compat::gsm_7bit_encode_n_ussd(&mut buffer, &text)
            } else {
                compat::gsm_7bit_encode_n(&mut buffer, &text)
            };
            assert_eq!(counts, (septets, octets.len() as i32));
            assert_eq!(buffer[..octets.len()], octets);
            assert!(buffer[octets.len()..].iter().all(|&byte| byte == GUARD));
        }
    }
}

#[test]
fn compat_decoding_matches_c() {
    let mut seen = [0; 3];
    for record in records() {
        if let Record::Decode {
            mode,
            input,
            length,
            n,
            text,
            ..
        } = record
        {
            // Dropping a <CR> leaves the first terminator one byte further on.
            let written = (text.len() + 2).min(n);
            let (buffer, count) = guarded(n, written, |buffer| match mode {
                DecodeMode::Plain => {
                    seen[0] += 1;
                    compat::gsm_7bit_decode_n(buffer, &input, length)
                }
                DecodeMode::Ussd => {
                    seen[1] += 1;
                    compat::gsm_7bit_decode_n_ussd(buffer, &input, length)
                }
                DecodeMode::Header(indicator) => {
                    seen[2] += 1;
                    compat::gsm_7bit_decode_n_hdr(buffer, &input, length, indicator)
                }
            });
            assert_eq!(count as usize, text.len(), "{input:02x?} length {length}");
            assert_eq!(buffer[..text.len()], text, "{input:02x?} length {length}");
            assert_eq!(buffer[text.len()], 0);
        }
    }
    assert!(seen.iter().all(|&count| count > 0));
}

#[test]
fn preferred_decoding_matches_c() {
    for record in records() {
        let Record::Decode {
            mode,
            source,
            input,
            length,
            n,
            text,
        } = record
        else {
            continue;
        };
        let septets = usize::from(length);
        let decoded = match mode {
            DecodeMode::Plain => decode_7bit(&input, septets),
            DecodeMode::Ussd => {
                // C's limit applies before it drops a <CR>, so only a full output compares.
                if text.len() + 2 < n {
                    decode_7bit_ussd(&input, septets)
                } else {
                    continue;
                }
            }
            DecodeMode::Header(0) => decode_7bit(&input, septets),
            DecodeMode::Header(_) => decode_7bit_with_header(&input, septets),
        };
        if source == Source::Wrap {
            assert_eq!(decoded, None, "a header past the message is rejected");
            continue;
        }
        let decoded = decoded.expect("the recording holds enough octets");
        // C stops at its output size, and this API has none.
        assert_eq!(
            decoded[..decoded.len().min(n - 1)],
            text,
            "{input:02x?} length {length}"
        );
    }
}
