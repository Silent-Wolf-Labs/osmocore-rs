//! Results the C recording can't hold: lengths past C's 16-bit bit count, and the
//! guarantees the preferred API adds.

use osmocore_rs::compat::kasumi as compat;
use osmocore_rs::gsm::kasumi::*;

const KEY: [u8; KEY_BYTES] = [
    0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00, 0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00,
];

const INPUT: KgcoreInput = KgcoreInput {
    ca: 0x0f,
    bearer: Bearer::ZERO,
    count: 0x24f20f,
    direction: false,
};

#[test]
fn a_shorter_keystream_is_a_prefix_of_a_longer_one() {
    let mut long = vec![0; 9000];
    kgcore(&KEY, &INPUT, &mut long);
    for len in [0, 1, 7, 8, 9, 63, 64, 65, 1523] {
        let mut short = vec![0; len];
        kgcore(&KEY, &INPUT, &mut short);
        assert_eq!(short, long[..len]);
    }
}

#[test]
fn keystreams_can_be_longer_than_c_allows() {
    // C counts the length in a 16-bit number of bits, so stops at 8,192 bytes.
    let mut long = vec![0; 8192 + 64];
    kgcore(&KEY, &INPUT, &mut long);
    let mut c = vec![0; 8192];
    compat::_kasumi_kgcore(INPUT.ca, 0, INPUT.count, 0, &KEY, &mut c, u16::MAX);
    assert_eq!(c[..], long[..8192]);
    assert_ne!(long[8192..], [0; 64]);
}

#[test]
fn compat_kgcore_leaves_the_rest_of_the_output_alone() {
    let mut out = [0xaa; 8];
    compat::_kasumi_kgcore(INPUT.ca, 0, INPUT.count, 0, &KEY, &mut out, 9);
    assert_eq!(out[2..], [0xaa; 6]);
    assert_eq!(out[0], 0x88);
}

#[test]
#[should_panic(expected = "shorter than 9 bits")]
fn compat_kgcore_panics_where_c_writes_past_the_output() {
    compat::_kasumi_kgcore(INPUT.ca, 0, INPUT.count, 0, &KEY, &mut [0; 1], 9);
}

#[test]
fn the_direction_and_bearer_change_the_keystream() {
    let mut streams = Vec::new();
    for (bearer, direction) in [(0, false), (0, true), (1, false), (31, true)] {
        let input = KgcoreInput {
            bearer: Bearer::new(bearer).unwrap(),
            direction,
            ..INPUT
        };
        let mut out = [0; 16];
        kgcore(&KEY, &input, &mut out);
        streams.push(out);
    }
    streams.sort_unstable();
    streams.dedup();
    assert_eq!(streams.len(), 4);
}
