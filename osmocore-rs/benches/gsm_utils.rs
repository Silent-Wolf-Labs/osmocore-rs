// SPDX-License-Identifier: GPL-3.0-or-later

//! Encoding and decoding an SMS of nearly full length in the 7-bit alphabet.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::gsm_utils::{decode_7bit, encode_7bit};
use std::hint::black_box;

const MAX_OCTETS: usize = 140;

fn gsm_utils(c: &mut Criterion) {
    let text: Vec<u8> = b"this is a test message [with escapes]. "
        .iter()
        .copied()
        .cycle()
        .take(150)
        .collect();
    let packed = encode_7bit(&text, MAX_OCTETS);
    assert_eq!(packed.septets, 157);
    assert_eq!(decode_7bit(&packed.octets, packed.septets).unwrap(), text);

    let mut group = c.benchmark_group("gsm_utils");
    group.bench_function("encode_7bit", |b| {
        b.iter(|| encode_7bit(black_box(&text), MAX_OCTETS))
    });
    group.bench_function("decode_7bit", |b| {
        b.iter(|| decode_7bit(black_box(&packed.octets), packed.septets))
    });
    group.finish();
}

criterion_group!(benches, gsm_utils);
criterion_main!(benches);
