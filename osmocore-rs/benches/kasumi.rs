// SPDX-License-Identifier: GPL-3.0-or-later

//! KASUMI: one block, and KGCORE's keystream for a 1,500-byte LLC frame.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::gsm::kasumi::{Bearer, Kasumi, KgcoreInput, kgcore};
use std::hint::black_box;

const KEY: [u8; 16] = [
    0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xb3, 0x00, 0x95, 0x2c, 0x49, 0x10, 0x48, 0x81, 0xff, 0x48,
];

fn kasumi(c: &mut Criterion) {
    let mut group = c.benchmark_group("kasumi");
    let cipher = Kasumi::new(&KEY);
    group.bench_function("encrypt_block", |b| {
        b.iter(|| cipher.encrypt(black_box(0xea024714ad5c4d84)))
    });
    let input = KgcoreInput {
        ca: 0xff,
        bearer: Bearer::ZERO,
        count: 0x12345678,
        direction: true,
    };
    let mut out = vec![0; 1500];
    group.bench_function("kgcore_1500_bytes", |b| {
        b.iter(|| kgcore(black_box(&KEY), black_box(&input), &mut out))
    });
    group.finish();
}

criterion_group!(benches, kasumi);
criterion_main!(benches);
