//! Results the C recording can't hold, and where the preferred API differs from C.

use osmocore_rs::compat::gsm23236::*;
use osmocore_rs::gsm::gsm23236::{
    NRI_MAX, NriBitlen, NriError, NriRange, NriRanges, parse_nri, set_tmsi_nri, tmsi_nri,
};

fn r(first: u16, last: u16) -> NriRange {
    NriRange::new(first, last).unwrap()
}

fn set(ranges: &[(u16, u16)]) -> NriRanges {
    ranges.iter().map(|&(first, last)| r(first, last)).collect()
}

#[test]
fn bit_lengths_are_one_to_fifteen() {
    assert_eq!(NriBitlen::new(0), None);
    assert_eq!(NriBitlen::new(16), None);
    assert_eq!(NriBitlen::new(1), Some(NriBitlen::MIN));
    assert_eq!(NriBitlen::new(15), Some(NriBitlen::MAX));
    assert_eq!(NriBitlen::default().get(), OSMO_NRI_BITLEN_DEFAULT);
    assert_eq!(NriBitlen::MIN.get(), OSMO_NRI_BITLEN_MIN);
    assert_eq!(NriBitlen::MAX.get(), OSMO_NRI_BITLEN_MAX);
    assert_eq!(NriBitlen::MAX.max_nri(), NRI_MAX);
}

#[test]
fn every_tmsi_nri_round_trips() {
    for bits in 1..=15 {
        let bitlen = NriBitlen::new(bits).unwrap();
        for tmsi in [0, u32::MAX, 0x0123_4567, 0xc0ff_ee00] {
            for nri in [0, 1, bitlen.max_nri() / 3, bitlen.max_nri()] {
                let changed = set_tmsi_nri(tmsi, nri, bitlen).unwrap();
                assert_eq!(tmsi_nri(changed, bitlen), nri);
                // Only the NRI bits change, and the top byte never does.
                let mask = u32::from(bitlen.max_nri()) << (24 - bits);
                assert_eq!(changed & !mask, tmsi & !mask);
                assert_eq!(changed >> 24, tmsi >> 24);
            }
            assert_eq!(
                set_tmsi_nri(tmsi, bitlen.max_nri() + 1, bitlen),
                Err(NriError::TooLarge)
            );
        }
    }
}

#[test]
fn ranges_check_their_ends() {
    assert_eq!(NriRange::new(2, 1), Err(NriError::Reversed));
    assert_eq!(NriRange::new(0, NRI_MAX + 1), Err(NriError::TooLarge));
    assert_eq!(NriRange::single(NRI_MAX).map(NriRange::last), Ok(NRI_MAX));
    assert!(r(0, 1023).fits(NriBitlen::DEFAULT));
    assert!(!r(0, 1024).fits(NriBitlen::DEFAULT));
    assert!(r(3, 5).overlaps(r(5, 9)));
    assert!(!r(3, 5).overlaps(r(6, 9)));
    assert_eq!(r(23, 42).to_string(), "23..42");
}

#[test]
fn the_vty_text_is_decimal_or_hex() {
    assert_eq!(parse_nri("511"), Ok(511));
    assert_eq!(parse_nri("0x1ff"), Ok(511));
    assert_eq!(parse_nri("0x1FF"), Ok(511));
    assert_eq!(parse_nri("08"), Ok(8));
    assert_eq!(parse_nri("32767"), Ok(NRI_MAX));
    assert_eq!(parse_nri("32768"), Err(NriError::TooLarge));
    assert_eq!(parse_nri("0x8000"), Err(NriError::TooLarge));
    assert_eq!(parse_nri("99999999999999999999"), Err(NriError::TooLarge));
    // C takes the first five, as strtoll does.
    for text in [
        " 1", "\t1", "+1", "-0", "-1", "0X1f", "0x", "", "1 ", "1.5", "0x-1", "abc",
    ] {
        assert_eq!(parse_nri(text), Err(NriError::Syntax), "{text:?}");
    }
    assert_eq!(NriRange::parse("1", Some("0x10")), Ok(r(1, 16)));
    assert_eq!(NriRange::parse("5", None), Ok(r(5, 5)));
    assert_eq!(NriRange::parse("5", Some("4")), Err(NriError::Reversed));
}

#[test]
fn lists_stay_sorted_and_apart() {
    let mut ranges = NriRanges::new();
    assert!(ranges.is_empty());
    assert_eq!(ranges.to_string(), "empty");
    for (first, last) in [(30, 32), (10, 10), (20, 21), (12, 12)] {
        ranges.add(r(first, last));
    }
    assert_eq!(ranges.to_string(), "10..10,12..12,20..21,30..32");
    // A range touching two joins them.
    ranges.add(r(11, 11));
    assert_eq!(ranges, set(&[(10, 12), (20, 21), (30, 32)]));
    ranges.add(r(13, 29));
    assert_eq!(ranges, set(&[(10, 32)]));
    ranges.remove(r(15, 15));
    assert_eq!(ranges, set(&[(10, 14), (16, 32)]));
    ranges.remove(r(0, 12));
    ranges.remove(r(30, NRI_MAX));
    assert_eq!(ranges, set(&[(13, 14), (16, 29)]));
    assert!(ranges.contains(16) && !ranges.contains(15));
    assert!(ranges.overlaps(r(0, 13)) && !ranges.overlaps(r(15, 15)));
    ranges.remove(r(0, NRI_MAX));
    assert!(ranges.is_empty());
}

#[test]
fn the_whole_range_of_values_fits() {
    let mut ranges = set(&[(0, 0), (NRI_MAX, NRI_MAX)]);
    ranges.add(r(1, NRI_MAX - 1));
    assert_eq!(ranges.ranges(), [r(0, NRI_MAX)]);
    assert_eq!(
        ranges.limit(u16::MAX, NriBitlen::MAX),
        Some(u16::MAX % 32768)
    );
}

#[test]
fn limit_picks_each_value_in_turn() {
    let ranges = set(&[(10, 10), (20, 21), (30, 32), (300, 400)]);
    let bitlen = NriBitlen::new(8).unwrap();
    let picked: Vec<_> = (0..13).map(|i| ranges.limit(i, bitlen).unwrap()).collect();
    assert_eq!(picked, [10, 20, 21, 30, 31, 32, 10, 20, 21, 30, 31, 32, 10]);
}

/// C ends the process by SIGFPE for these: no value of the list fits in the bit length.
#[test]
fn limit_without_a_value_that_fits_is_none() {
    let bitlen = NriBitlen::new(8).unwrap();
    for ranges in [
        NriRanges::new(),
        set(&[(300, 400)]),
        set(&[(256, 256), (1000, 2000)]),
    ] {
        assert_eq!(ranges.limit(5, bitlen), None);
        assert_eq!(ranges.limit_tmsi(0x1234_5678, bitlen), None);

        let list = OsmoNriRanges::from(ranges);
        let mut nri_v = 5;
        assert_eq!(osmo_nri_v_limit_by_ranges(&mut nri_v, &list, 8), -1);
        assert_eq!(nri_v, 5);
        let mut tmsi = 0x1234_5678;
        assert_eq!(osmo_tmsi_nri_v_limit_by_ranges(&mut tmsi, &list, 8), -1);
        assert_eq!(tmsi, 0x1234_5678);
    }
    // C's int16_t range_max wraps past 15 bits; past 30 its shift overflows.
    let list = OsmoNriRanges::from(set(&[(0, 10)]));
    for nri_bitlen in [16, 30, 31, 32, u32::MAX] {
        let mut nri_v = 5;
        assert_eq!(
            osmo_nri_v_limit_by_ranges(&mut nri_v, &list, nri_bitlen),
            -1
        );
    }
}

#[test]
fn compat_text_has_no_commas() {
    let list = OsmoNriRanges::from(set(&[(23, 42), (123, 142)]));
    assert_eq!(osmo_nri_ranges_to_str_c(&list), "23..42123..142");
    let mut buf = [0xaa; 8];
    assert_eq!(osmo_nri_ranges_to_str_buf(&mut buf, &list), 14);
    assert_eq!(&buf, b"23..421\0");
    assert_eq!(osmo_nri_ranges_to_str_buf(&mut [], &list), 14);
}

#[test]
fn compat_vty_without_arguments_cannot_parse() {
    let mut list = OsmoNriRanges::new();
    let mut message = None;
    assert_eq!(
        osmo_nri_ranges_vty_add(&mut message, None, &mut list, &[], 10),
        -1
    );
    assert_eq!(message, Some("Error: cannot parse NRI range"));
    assert_eq!(
        osmo_nri_ranges_vty_del(&mut message, None, &mut list, &[]),
        -1
    );
    // A NUL ends an argument, as in C.
    assert_eq!(
        osmo_nri_ranges_vty_add(&mut message, None, &mut list, &[Some(b"7\0x")], 10),
        0
    );
    assert_eq!(message, None);
    assert_eq!(list.entries(), [OsmoNriRange { first: 7, last: 7 }]);
}

#[test]
fn errors_describe_themselves() {
    assert_eq!(NriError::TooLarge.to_string(), "NRI value too large");
    assert_eq!(
        NriError::Reversed.to_string(),
        "NRI range ends before it starts"
    );
    assert_eq!(NriError::Syntax.to_string(), "not an NRI value");
}
