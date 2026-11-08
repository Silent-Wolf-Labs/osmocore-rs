//! Where the stealing flags sit in a burst.

use osmocore_rs::gsm0503::{
    XCCH_BURST_BITS, XCCH_BURST_DATA_BITS, XCCH_STEALING_FLAG_OFFSETS, xcch_burst_map,
    xcch_burst_unmap, xcch_encode,
};

#[test]
fn burst_map_writes_the_flags_at_their_offsets() {
    let mut burst = [0u8; XCCH_BURST_BITS];
    xcch_burst_map(&[0; XCCH_BURST_DATA_BITS], &mut burst, Some(1), Some(1));
    let ones: Vec<usize> = (0..XCCH_BURST_BITS).filter(|&i| burst[i] == 1).collect();
    assert_eq!(ones, XCCH_STEALING_FLAG_OFFSETS);
}

#[test]
fn burst_unmap_reads_the_flags_from_their_offsets() {
    let mut burst = [0i8; XCCH_BURST_BITS];
    burst[XCCH_STEALING_FLAG_OFFSETS[0]] = 5;
    burst[XCCH_STEALING_FLAG_OFFSETS[1]] = -5;
    let (data, hl, hn) = xcch_burst_unmap(&burst);
    assert_eq!((hl, hn), (5, -5));
    assert!(data.iter().all(|&bit| bit == 0));
}

#[test]
fn encoded_bursts_set_both_flags() {
    let bursts = xcch_encode(&[0; 23]);
    for burst in bursts.chunks(XCCH_BURST_BITS) {
        assert_eq!(
            XCCH_STEALING_FLAG_OFFSETS.map(|offset| burst[offset]),
            [1, 1]
        );
    }
}
