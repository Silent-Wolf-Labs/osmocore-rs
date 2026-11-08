//! Results the C recording can't hold: inputs where C crashes or reads out of bounds,
//! and inputs its 8-bit lengths and C strings can't express.

use osmocore_rs::compat::gsm_utils as compat;
use osmocore_rs::gsm::gsm_utils::*;

#[test]
fn byte_0xff_is_a_byte_with_no_septet() {
    // C reads one past the end of its table here.
    assert_eq!(encode_septets(&[0xff]), [0xff]);
    assert_eq!(encode_7bit(&[0xff], 8).octets, [0x7f]);
}

#[test]
fn preferred_encoding_takes_nul_as_a_byte_and_compat_stops_at_it() {
    assert_eq!(encode_septets(b"a\0b"), [0x61, 0xff, 0x62]);
    let mut buffer = [0; 8];
    assert_eq!(compat::gsm_septet_encode(&mut buffer, b"a\0b"), 1);
    assert_eq!(compat::gsm_7bit_encode_n(&mut buffer, b"a\0b"), (1, 1));
    assert_eq!(compat::gsm_7bit_encode_n_ussd(&mut buffer, b"a\0b"), (1, 1));
}

#[test]
fn padding_above_seven_is_rejected() {
    // C shifts by a negative amount here.
    assert_eq!(pack_septets(&[1], 8), None);
    assert_eq!(pack_septets(&[1], 255), None);
    assert_eq!(compat::gsm_septet_pack(&mut [0; 8], &[1], 8), -22);
}

#[test]
fn compat_reports_a_result_buffer_that_is_too_small() {
    // C has no size to check here and overruns.
    let mut buffer = [0xaa; 1];
    assert_eq!(compat::gsm_septet_encode(&mut buffer, b"ab"), -22);
    assert_eq!(compat::gsm_septet_pack(&mut buffer, &[1, 2, 3], 0), -22);
    assert_eq!(buffer, [0xaa]);
}

#[test]
fn compat_decoding_with_no_output_returns_zero() {
    // C aborts here.
    assert_eq!(compat::gsm_7bit_decode_n(&mut [], &[0x41], 1), 0);
    assert_eq!(compat::gsm_7bit_decode_n_ussd(&mut [], &[0x1a], 1), 0);
    assert_eq!(compat::gsm_7bit_decode_n_hdr(&mut [], &[0], 2, 1), 0);
}

#[test]
fn compat_decoding_past_the_input_reads_zeros() {
    // C reads out of bounds here. A zero septet decodes as '@'.
    let mut text = [0xaa; 8];
    assert_eq!(compat::gsm_7bit_decode_n(&mut text, &[], 2), 2);
    assert_eq!(&text[..3], b"@@\0");
    assert_eq!(compat::gsm_7bit_decode_n_hdr(&mut text, &[], 2, 1), 0);
    assert_eq!(compat::gsm_7bit_decode_n_ussd(&mut text, &[], 1), 1);
}

#[test]
fn preferred_decoding_rejects_input_shorter_than_the_septets() {
    assert_eq!(decode_7bit(&[0x41], 2), None);
    assert_eq!(decode_7bit_ussd(&[0x41], 2), None);
    assert_eq!(decode_7bit_with_header(&[], 0), None);
    assert_eq!(decode_7bit_with_header(&[0, 0], 3), None);
    assert_eq!(decode_7bit(&[], 0).as_deref(), Some(&b""[..]));
    assert_eq!(decode_7bit_ussd(&[], 0).as_deref(), Some(&b""[..]));
}

#[test]
fn header_length_that_just_fits_leaves_no_text() {
    // A header of one octet plus its length octet takes 3 septets.
    assert_eq!(
        decode_7bit_with_header(&[1, 0, 0], 3).as_deref(),
        Some(&b""[..])
    );
    assert_eq!(decode_7bit_with_header(&[1, 0, 0], 2), None);
}

#[test]
fn messages_over_255_septets_round_trip() {
    // C's decoders take an 8-bit septet count, so this can't be recorded.
    let text: Vec<u8> = b"abc ".iter().copied().cycle().take(2048).collect();
    let packed = encode_7bit(&text, usize::MAX);
    assert_eq!(packed.septets, 2048);
    assert_eq!(packed.octets.len(), 1792);
    assert_eq!(decode_7bit(&packed.octets, packed.septets).unwrap(), text);
}

#[test]
fn text_with_escapes_round_trips_within_the_limit() {
    let text = b"{[|]} ~ ^ \\ test $@_";
    let packed = encode_7bit(text, 140);
    assert_eq!(decode_7bit(&packed.octets, packed.septets).unwrap(), text);
}

#[test]
fn a_limit_can_split_an_escape() {
    let packed = encode_7bit(b"aaaaaaa^", 7);
    // Seven characters and the escape fill the limit, and the '^' is lost.
    assert_eq!(packed.septets, 8);
    assert_eq!(decode_7bit(&packed.octets, 8).unwrap(), b"aaaaaaa\xff");
}

#[test]
fn ussd_padding_round_trips_where_seven_septets_would_read_as_eight() {
    let packed = encode_7bit_ussd(b"aaaaaaa", 16);
    assert_eq!(packed.septets, 7);
    assert_eq!(packed.octets.last().map(|octet| octet >> 1), Some(b'\r'));
    // Read as 8 septets, as a receiver can't tell, the padding is dropped.
    assert_eq!(decode_7bit_ussd(&packed.octets, 8).unwrap(), b"aaaaaaa");
}

#[test]
fn ussd_second_cr_needs_a_spare_octet_past_the_one_it_uses() {
    let text = b"aaaaaaa\r";
    assert_eq!(encode_7bit_ussd(text, 8).septets, 8);
    assert_eq!(encode_7bit_ussd(text, 9).septets, 9);
}

#[test]
fn encoding_with_the_largest_limit_does_not_overflow() {
    assert_eq!(encode_7bit(b"abc", usize::MAX).septets, 3);
    assert_eq!(encode_7bit_ussd(b"abc", usize::MAX).septets, 3);
    assert_eq!(septets_to_octets(usize::MAX) / 7, usize::MAX / 8 + 1);
}
