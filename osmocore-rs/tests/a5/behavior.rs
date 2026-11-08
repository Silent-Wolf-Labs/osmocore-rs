//! Results the C recording can't hold: inputs where C crashes or overflows, and the
//! ciphers this crate doesn't implement.

use osmocore_rs::compat::a5 as compat;
use osmocore_rs::gsm::a5::*;

const KEY: [u8; KEY_BYTES] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

#[test]
fn compat_a5_0_needs_no_key_and_a5_1_and_a5_2_do() {
    let mut dl = [2; STREAM_BITS];
    assert_eq!(compat::osmo_a5(0, None, 0, Some(&mut dl), None), 0);
    assert_eq!(dl, [0; STREAM_BITS]);

    // C crashes here.
    let mut dl = [2; STREAM_BITS];
    assert_eq!(compat::osmo_a5(1, None, 0, Some(&mut dl), None), -22);
    assert_eq!(compat::osmo_a5(2, None, 0, Some(&mut dl), None), -22);
    assert_eq!(dl, [2; STREAM_BITS]);
}

#[test]
fn compat_reports_a5_3_and_a5_4_as_unsupported() {
    // C generates these with KASUMI, which isn't ported.
    let mut dl = [2; STREAM_BITS];
    assert_eq!(compat::osmo_a5(3, Some(&KEY), 0, Some(&mut dl), None), -95);
    assert_eq!(compat::osmo_a5(4, Some(&KEY), 0, Some(&mut dl), None), -95);
    assert_eq!(dl, [2; STREAM_BITS]);
}

#[test]
fn compat_writes_only_the_directions_given() {
    let stream = a5_1(&KEY, 123456);
    let mut ul = [2; STREAM_BITS];
    assert_eq!(
        compat::osmo_a5(1, Some(&KEY), 123456, None, Some(&mut ul)),
        0
    );
    assert_eq!(ul, stream.uplink);
    assert_eq!(compat::osmo_a5(1, Some(&KEY), 123456, None, None), 0);
}

#[test]
fn frame_count_uses_unsigned_32_bit_arithmetic() {
    // A shift overflows an int in C from the first frame here on.
    let frame = 1326u32 << 20;
    let low = (frame % 51) << 5 | (frame % 26);
    assert_eq!(frame_count(frame), 1 << 31 | low);
    // T1 no longer fits, and its high bit is dropped.
    let frame = 1326u32 << 21;
    let low = (frame % 51) << 5 | (frame % 26);
    assert_eq!(frame_count(frame), low);
    a5_1(&KEY, u32::MAX);
    a5_2(&KEY, u32::MAX);
}

#[test]
fn frames_with_the_same_low_count_bits_share_a_stream() {
    // T1 is a multiple of 2048 and T2 and T3 are zero, so the 22 bits a cipher loads
    // are the same as for frame 0.
    let frame = 1326 * 2048 * 26 * 51;
    assert_ne!(frame_count(frame), 0);
    assert_eq!(a5_1(&KEY, frame), a5_1(&KEY, 0));
    assert_eq!(a5_2(&KEY, frame), a5_2(&KEY, 0));
}

#[test]
fn a_stream_ciphers_and_deciphers_a_burst() {
    let burst: [u8; STREAM_BITS] = std::array::from_fn(|index| (index % 3 == 0) as u8);
    let stream = a5_2(&KEY, 5000);
    let cipher = |bits: [u8; STREAM_BITS]| -> [u8; STREAM_BITS] {
        std::array::from_fn(|index| bits[index] ^ stream.downlink[index])
    };
    assert_ne!(cipher(burst), burst);
    assert_eq!(cipher(cipher(burst)), burst);
}
