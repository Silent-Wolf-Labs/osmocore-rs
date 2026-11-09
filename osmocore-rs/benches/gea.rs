// SPDX-License-Identifier: GPL-3.0-or-later

//! GEA3 and GEA4 keystreams for a 1,500-byte LLC frame.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::gsm::gea::{Direction, gea3, gea4};
use std::hint::black_box;

const KC: [u8; 8] = [0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00];
const KEY: [u8; 16] = [
    0xd3, 0xc5, 0xd5, 0x92, 0x32, 0x7f, 0xb1, 0x1c, 0x40, 0x35, 0xc6, 0x68, 0x0a, 0xf8, 0xc6, 0xd1,
];

fn gea(c: &mut Criterion) {
    let mut group = c.benchmark_group("gea");
    let mut out = vec![0; 1500];
    group.bench_function("gea3_1500_bytes", |b| {
        b.iter(|| {
            gea3(
                black_box(&KC),
                black_box(0x8e9421a3),
                Direction::Downlink,
                &mut out,
            )
        })
    });
    group.bench_function("gea4_1500_bytes", |b| {
        b.iter(|| {
            gea4(
                black_box(&KEY),
                black_box(0x8e9421a3),
                Direction::Downlink,
                &mut out,
            )
        })
    });
    group.finish();
}

criterion_group!(benches, gea);
criterion_main!(benches);
