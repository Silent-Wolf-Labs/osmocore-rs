// SPDX-License-Identifier: GPL-3.0-or-later

//! Message-buffer allocation and cursor operations.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use osmocore_rs::msgb::Msgb;
use std::hint::black_box;

const HEADROOM: usize = 64;

fn msgb(c: &mut Criterion) {
    let mut group = c.benchmark_group("msgb");
    for size in [64usize, 1500] {
        let capacity = size + HEADROOM;
        let mut msg = Msgb::with_headroom(capacity, HEADROOM).unwrap();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(format!("alloc_{size}"), |b| {
            b.iter(|| Msgb::with_headroom(black_box(capacity), HEADROOM).unwrap())
        });
        group.bench_function(format!("put_pull_{size}"), |b| {
            b.iter(|| {
                msg.put(black_box(size)).unwrap();
                msg.pull(black_box(size)).unwrap();
                msg.reset();
                msg.reserve(HEADROOM).unwrap();
            })
        });
    }
    group.throughput(Throughput::Bytes(4));
    let mut msg = Msgb::with_headroom(64 + HEADROOM, HEADROOM).unwrap();
    group.bench_function("put_pull_u32", |b| {
        b.iter(|| {
            msg.put_u32(black_box(0x0102_0304)).unwrap();
            let value = msg.pull_u32().unwrap();
            msg.reset();
            msg.reserve(HEADROOM).unwrap();
            value
        })
    });
    group.finish();
}

criterion_group!(benches, msgb);
criterion_main!(benches);
