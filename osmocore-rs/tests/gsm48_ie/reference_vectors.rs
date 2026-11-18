use crate::gsm48_ie_fixture::{self as fixture, Body, GUARD, Record};
use osmocore_rs::compat::gsm48_ie::*;
use osmocore_rs::gsm::gsm48_ie::{BcdNumber, BcdNumberError};

const FIXTURE: &str = include_str!("../compatibility/vectors/gsm48_ie/gsm48_ie_vectors.ndjson");
/// Larger than any buffer the harness records, as C's were.
const BUF_SIZE: usize = 1024;

fn records() -> Vec<Record> {
    fixture::validate(FIXTURE)
}

/// The text C's caller sees: the output up to its first NUL. The record's last byte is past
/// the output, and with no output there is no text.
fn c_text(out: &[u8]) -> &str {
    let written = &out[..out.len() - 1];
    let end = written.iter().position(|&byte| byte == 0).unwrap_or(0);
    std::str::from_utf8(&written[..end]).expect("decoded digits are ASCII")
}

/// Calls `decode` on an output of `output_len` guard bytes inside a larger buffer, and checks
/// its result and every byte against C's.
fn check_decode(
    output_len: usize,
    rc: i32,
    out: &[u8],
    decode: impl FnOnce(&mut [u8]) -> i32,
    what: &str,
) {
    let mut buf = [GUARD; BUF_SIZE];
    assert_eq!(decode(&mut buf[..output_len]), rc, "{what}");
    assert_eq!(&buf[..out.len()], out, "{what}");
    assert!(buf[out.len()..].iter().all(|&byte| byte == GUARD), "{what}");
}

/// The preferred decoder against C's: the same failures, and C's text where C succeeds or
/// runs out of room, which then holds the start of the number.
fn check_preferred_decode(lv: &[u8], h_len: usize, rc: i32, out: &[u8]) {
    let decoded = BcdNumber::decode(lv, h_len);
    match rc {
        -5 => assert_eq!(decoded, Err(BcdNumberError::Empty)),
        -22 => assert_eq!(decoded, Err(BcdNumberError::Truncated)),
        0 => assert_eq!(decoded.unwrap().as_str(), c_text(out), "{lv:02x?}"),
        -28 => assert!(decoded.unwrap().as_str().starts_with(c_text(out))),
        _ => unreachable!("validated result"),
    }
}

#[test]
fn fixture_is_well_formed() {
    let total: usize = fixture::SECTIONS.iter().map(|(_, count)| count).sum();
    assert_eq!(records().len(), total);
    fixture::check_corruptions(FIXTURE);
}

#[test]
fn encoding_matches_c() {
    let mut checked = 0;
    for record in records() {
        let Body::Encode {
            input,
            h_len,
            max_len,
            rc,
            out,
        } = record.body
        else {
            continue;
        };
        let mut buf = [GUARD; BUF_SIZE];
        assert_eq!(
            gsm48_encode_bcd_number(&mut buf, max_len, h_len, &input),
            rc
        );
        assert_eq!(&buf[..out.len()], out.as_slice(), "{input:02x?}");
        assert!(buf[out.len()..].iter().all(|&byte| byte == GUARD));

        let number = std::str::from_utf8(&input)
            .ok()
            .and_then(|text| text.parse::<BcdNumber>().ok());
        let extent = out.len() - 1;
        match rc {
            -22 => assert!(number.is_none(), "{input:02x?}"),
            -5 => assert!(number.is_none_or(|n| n.encoded_len(h_len) - 1 > usize::from(max_len))),
            _ => {
                let number = number.expect("C encoded it");
                assert_eq!(number.encoded_len(h_len), extent);
                // C leaves the header alone, so the guard bytes stand in for it.
                match number.encode(&vec![GUARD; h_len]) {
                    Ok(lv) => assert_eq!(lv, out[..extent]),
                    Err(error) => {
                        assert_eq!(error, BcdNumberError::TooLong);
                        assert!(extent - 1 > 255);
                    }
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[0].1);
}

#[test]
fn decoding_matches_c() {
    let mut checked = 0;
    for record in records() {
        let Body::Decode {
            input,
            input_len,
            h_len,
            output_len,
            rc,
            out,
        } = record.body
        else {
            continue;
        };
        let lv = &input[..input_len];
        check_decode(
            output_len,
            rc,
            &out,
            |output| gsm48_decode_bcd_number2(output, lv, h_len),
            &format!("{lv:02x?} {h_len} {output_len}"),
        );
        check_preferred_decode(lv, h_len, rc, &out);
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[1].1);
}

#[test]
fn legacy_decoding_matches_c() {
    let mut checked = 0;
    for record in records() {
        let Body::DecodeLegacy {
            input,
            h_len,
            output_len,
            rc,
            out,
        } = record.body
        else {
            continue;
        };
        check_decode(
            output_len,
            rc,
            &out,
            |output| gsm48_decode_bcd_number(output, &input, h_len),
            &format!("{input:02x?} {h_len} {output_len}"),
        );
        checked += 1;
    }
    assert_eq!(checked, fixture::SECTIONS[2].1);
}

#[test]
fn upstream_test_is_replayed() {
    let upstream = records().into_iter().filter(|record| record.upstream);
    assert_eq!(upstream.count(), fixture::UPSTREAM_COUNT);
}
