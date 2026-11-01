// SPDX-License-Identifier: GPL-3.0-or-later

//! Generic unpacked-bit CRC and byte-oriented CRC-16 throughput.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use osmocore_rs::crc::{Crc64Code, crc16, crc16_ccitt};
use std::hint::black_box;

/// GSM 05.03 Fire code: g(D) = (D^23 + 1)(D^17 + D^3 + 1), inverted parity.
const FIRE: Crc64Code = Crc64Code {
    bits: 40,
    poly: 0x00_0482_0009,
    init: 0,
    remainder: 0xff_ffff_ffff,
};

fn generic(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc/generic_fire40");
    for bits in [184usize, 1024] {
        let input: Vec<u8> = (0..bits).map(|i| ((i * 7) >> 2) as u8 & 1).collect();
        group.throughput(Throughput::Elements(bits as u64));
        group.bench_function(format!("{bits}_bits"), |b| {
            b.iter(|| FIRE.compute_bits(black_box(&input)).unwrap())
        });
    }
    group.finish();
}

fn bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc/crc16");
    for size in [64usize, 1500] {
        let input: Vec<u8> = (0..size).map(|i| (i * 31) as u8).collect();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(format!("crc16_{size}_bytes"), |b| {
            b.iter(|| crc16(0, black_box(&input)))
        });
        group.bench_function(format!("ccitt_{size}_bytes"), |b| {
            b.iter(|| crc16_ccitt(0xffff, black_box(&input)))
        });
    }
    group.finish();
}

criterion_group!(benches, generic, bytes);
criterion_main!(benches);
