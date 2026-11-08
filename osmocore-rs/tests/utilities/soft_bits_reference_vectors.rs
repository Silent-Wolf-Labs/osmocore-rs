use crate::soft_bits_fixture;
use osmocore_rs::base::bits::{from_soft, to_soft};

const FIXTURE: &str = include_str!("../compatibility/vectors/bits/soft_bits.ndjson");

#[test]
fn fixture_is_well_formed() {
    soft_bits_fixture::check_corruptions(FIXTURE);
}

#[test]
fn to_soft_matches_c_for_every_byte() {
    for conversion in soft_bits_fixture::validate(FIXTURE).ubit2sbit {
        let input = u8::try_from(conversion.input).unwrap();
        assert_eq!(i16::from(to_soft(input)), conversion.output, "{input}");
    }
}

#[test]
fn from_soft_matches_c_for_every_soft_bit() {
    for conversion in soft_bits_fixture::validate(FIXTURE).sbit2ubit {
        let input = i8::try_from(conversion.input).unwrap();
        assert_eq!(i16::from(from_soft(input)), conversion.output, "{input}");
    }
}
