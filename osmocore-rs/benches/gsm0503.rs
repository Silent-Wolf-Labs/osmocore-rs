// SPDX-License-Identifier: GPL-3.0-or-later

//! xCCH block encoding and decoding.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::coding::gsm0503::{XCCH_L2_BYTES, xcch_decode, xcch_encode};
use std::hint::black_box;

fn xcch(c: &mut Criterion) {
    let frame: [u8; XCCH_L2_BYTES] = std::array::from_fn(|index| (index * 37) as u8);
    let clean = xcch_encode(&frame).map(|bit| if bit == 0 { 127i8 } else { -127 });
    // Every 16th soft bit is wrong, and the frame still decodes.
    let mut noisy = clean;
    for soft in noisy.iter_mut().step_by(16) {
        *soft = -*soft;
    }
    assert_eq!(xcch_decode(&clean).data, Some(frame));
    assert_eq!(xcch_decode(&noisy).data, Some(frame));

    let mut group = c.benchmark_group("gsm0503/xcch");
    group.bench_function("encode", |b| b.iter(|| xcch_encode(black_box(&frame))));
    group.bench_function("decode_clean", |b| {
        b.iter(|| xcch_decode(black_box(&clean)))
    });
    group.bench_function("decode_noisy", |b| {
        b.iter(|| xcch_decode(black_box(&noisy)))
    });
    group.finish();
}

criterion_group!(benches, xcch);
criterion_main!(benches);
