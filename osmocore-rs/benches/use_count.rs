// SPDX-License-Identifier: GPL-3.0-or-later

//! Getting and putting a use on an object with six tokens, a typical maximum.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::base::use_count::UseCount;
use std::hint::black_box;

const TOKENS: [&str; 6] = ["attached", "paging", "sms", "sms-pending", "cm", "lu"];

fn use_count(c: &mut Criterion) {
    let mut uses = UseCount::new();
    for token in TOKENS {
        uses.get(token).unwrap();
    }

    let mut group = c.benchmark_group("use_count");
    group.bench_function("get_put_last_token", |b| {
        b.iter(|| {
            uses.get(black_box(TOKENS[5])).unwrap();
            uses.put(black_box(TOKENS[5])).unwrap()
        })
    });
    group.bench_function("total", |b| b.iter(|| black_box(&uses).total()));
    group.finish();
}

criterion_group!(benches, use_count);
criterion_main!(benches);
