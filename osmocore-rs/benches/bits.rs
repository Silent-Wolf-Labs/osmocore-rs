// SPDX-License-Identifier: GPL-3.0-or-later

//! Bit packing and unpacking at GSM burst-relevant sizes.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use osmocore_rs::base::bits::{pack_into, pack_into_offset, unpack_into, unpack_into_offset};
use std::hint::black_box;

/// Bit offset that defeats any byte-aligned fast path.
const OFFSET: usize = 3;

fn bits(c: &mut Criterion) {
    let mut group = c.benchmark_group("bits");
    for count in [184usize, 456] {
        let unpacked: Vec<u8> = (0..count + OFFSET)
            .map(|i| ((i * 7) >> 2) as u8 & 1)
            .collect();
        let mut packed = vec![0u8; (count + OFFSET).div_ceil(8)];
        pack_into(&mut packed, &unpacked, count + OFFSET).unwrap();
        let mut packed_out = vec![0u8; packed.len()];
        let mut unpacked_out = vec![0u8; unpacked.len()];

        group.throughput(Throughput::Elements(count as u64));
        group.bench_function(format!("pack_{count}"), |b| {
            b.iter(|| pack_into(&mut packed_out, black_box(&unpacked), count).unwrap())
        });
        group.bench_function(format!("unpack_{count}"), |b| {
            b.iter(|| unpack_into(&mut unpacked_out, black_box(&packed), count).unwrap())
        });
        group.bench_function(format!("pack_offset_{count}"), |b| {
            b.iter(|| {
                pack_into_offset(
                    &mut packed_out,
                    OFFSET,
                    black_box(&unpacked),
                    0,
                    count,
                    false,
                )
                .unwrap()
            })
        });
        group.bench_function(format!("unpack_offset_{count}"), |b| {
            b.iter(|| {
                unpack_into_offset(
                    &mut unpacked_out,
                    0,
                    black_box(&packed),
                    OFFSET,
                    count,
                    false,
                )
                .unwrap()
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bits);
criterion_main!(benches);
