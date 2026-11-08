// SPDX-License-Identifier: GPL-3.0-or-later

//! Generating one frame's cipher stream.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::gsm::a5::{a5_1, a5_2};
use std::hint::black_box;

const KEY: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

fn a5(c: &mut Criterion) {
    let mut group = c.benchmark_group("a5");
    group.bench_function("a5_1", |b| {
        b.iter(|| a5_1(black_box(&KEY), black_box(123456)))
    });
    group.bench_function("a5_2", |b| {
        b.iter(|| a5_2(black_box(&KEY), black_box(123456)))
    });
    group.finish();
}

criterion_group!(benches, a5);
criterion_main!(benches);
