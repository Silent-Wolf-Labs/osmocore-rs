//! Results the C recording can't hold, and where the preferred API differs from C.

use osmocore_rs::compat::gsm48_ie::*;
use osmocore_rs::gsm::gsm48_ie::{BcdNumber, BcdNumberError, MAX_CONTENT_BYTES};

fn number(text: &str) -> BcdNumber {
    text.parse().unwrap()
}

#[test]
fn every_digit_round_trips() {
    let all = "0123456789*#abc";
    for len in 0..=all.len() {
        for header in [&[][..], &[0x91], &[0x01, 0x80]] {
            let n = number(&all[..len]);
            let lv = n.encode(header).unwrap();
            assert_eq!(lv.len(), n.encoded_len(header.len()));
            assert_eq!(usize::from(lv[0]), lv.len() - 1);
            assert_eq!(&lv[1..1 + header.len()], header);
            assert_eq!(BcdNumber::decode(&lv, header.len()), Ok(n));
        }
    }
}

#[test]
fn an_odd_count_ends_with_a_filler() {
    assert_eq!(number("123").encode(&[]).unwrap(), [0x02, 0x21, 0xf3]);
    assert_eq!(number("12").encode(&[]).unwrap(), [0x01, 0x21]);
    assert_eq!(number("").encode(&[0x81]).unwrap(), [0x01, 0x81]);
}

#[test]
fn only_c_characters_are_digits() {
    for text in ["A", "B", "C", "d", "e", "f", "+49", "1 2", "12-3", "é"] {
        assert_eq!(
            text.parse::<BcdNumber>(),
            Err(BcdNumberError::Digits),
            "{text}"
        );
    }
    assert_eq!(number("*#31#").to_string(), "*#31#");
}

#[test]
fn the_length_byte_counts_at_most_255_bytes() {
    let digits = |count: usize| number(&"1".repeat(count));
    assert_eq!(
        digits(510).encode(&[]).unwrap().len(),
        1 + MAX_CONTENT_BYTES
    );
    assert_eq!(digits(511).encode(&[]), Err(BcdNumberError::TooLong));
    assert_eq!(digits(508).encode(&[0; 1]).unwrap()[0], 255);
    assert_eq!(digits(509).encode(&[0; 1]), Err(BcdNumberError::TooLong));
    assert_eq!(digits(0).encode(&[0; 256]), Err(BcdNumberError::TooLong));
}

#[test]
fn c_wraps_the_length_byte() {
    let input = "1".repeat(511);
    let mut buf = [0x55; 300];
    assert_eq!(
        gsm48_encode_bcd_number(&mut buf, 255, 0, input.as_bytes()),
        257
    );
    assert_eq!(buf[0], 0);
    assert_eq!(number(&input).encode(&[]), Err(BcdNumberError::TooLong));
}

#[test]
fn decoding_ends_at_the_first_filler() {
    assert_eq!(
        BcdNumber::decode(&[0x03, 0xf1, 0x21, 0x43], 0),
        Ok(number("1"))
    );
    assert_eq!(BcdNumber::decode(&[0x02, 0x21, 0xf3], 0), Ok(number("123")));
    assert_eq!(BcdNumber::decode(&[0x01, 0xff], 0), Ok(number("")));
}

#[test]
fn decoding_takes_what_c_takes() {
    // Bytes past the IE are ignored.
    assert_eq!(BcdNumber::decode(&[0x01, 0x21, 0x43], 0), Ok(number("12")));
    // A header past the IE leaves no digits.
    assert_eq!(BcdNumber::decode(&[0x01, 0x21], 2), Ok(number("")));
    assert_eq!(BcdNumber::decode(&[], 0), Err(BcdNumberError::Empty));
    assert_eq!(
        BcdNumber::decode(&[0x02, 0x21], 0),
        Err(BcdNumberError::Truncated)
    );
}

#[test]
fn c_max_len_bounds_the_length_byte_not_the_buffer() {
    // Two digits make a length byte of 1, which a max_len of 1 allows, and two bytes.
    let mut buf = [0x55; 3];
    assert_eq!(gsm48_encode_bcd_number(&mut buf, 1, 0, b"12"), 2);
    assert_eq!(buf, [0x01, 0x21, 0x55]);
}

#[test]
fn compat_encoding_stops_where_c_would_overflow() {
    // C would write three bytes here.
    let mut buf = [0x55; 2];
    assert_eq!(gsm48_encode_bcd_number(&mut buf, 255, 0, b"123"), -5);
    assert_eq!(buf, [0x02, 0x55]);
    assert_eq!(gsm48_encode_bcd_number(&mut [], 255, 0, b"1"), -5);
}

#[test]
fn compat_encoding_reads_input_to_its_nul() {
    let mut buf = [0x55; 4];
    assert_eq!(gsm48_encode_bcd_number(&mut buf, 255, 0, b"12\0x"), 2);
    assert_eq!(buf, [0x01, 0x21, 0x55, 0x55]);
}

#[test]
fn compat_legacy_decoding_stays_within_its_input() {
    let mut out = [0x55; 8];
    // C would read the length byte's three bytes from past the input.
    assert_eq!(gsm48_decode_bcd_number(&mut out, &[0x03, 0x21], 0), -22);
    assert_eq!(out[0], 0);
    assert_eq!(gsm48_decode_bcd_number(&mut out, &[], 0), -5);
}

#[test]
fn errors_describe_themselves() {
    for error in [
        BcdNumberError::Digits,
        BcdNumberError::TooLong,
        BcdNumberError::Empty,
        BcdNumberError::Truncated,
    ] {
        assert!(!error.to_string().is_empty());
    }
}
