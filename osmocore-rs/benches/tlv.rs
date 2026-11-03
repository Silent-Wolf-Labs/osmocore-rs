// SPDX-License-Identifier: GPL-3.0-or-later

//! Parsing and encoding a message with one element of each format.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::msgb::Msgb;
use osmocore_rs::tlv::{TlvDef, TlvDefinition, TlvParsed, TlvType, encode, parse, parse_multiple};
use std::hint::black_box;

const MESSAGE: [u8; 25] = [
    0x01, 0x02, 0xaa, 0x03, 0xb1, 0xb2, 0xb3, 0x04, 0x02, 0xc1, 0xc2, 0x05, 0x00, 0x02, 0xd1, 0xd2,
    0x06, 0x82, 0xe1, 0xe2, 0x07, 0x02, 0x11, 0x12, 0xa7,
];

fn definition() -> TlvDefinition {
    let mut definition = TlvDefinition::new();
    for (tag, kind) in [
        (0x01, TlvType::T),
        (0x02, TlvType::Tv),
        (0x04, TlvType::Tlv),
        (0x05, TlvType::Tl16v),
        (0x06, TlvType::Tvlv),
        (0x07, TlvType::VtvlvGan),
        (0x0a, TlvType::SingleTv),
    ] {
        definition.set(tag, TlvDef::new(kind));
    }
    definition.set(0x03, TlvDef::fixed(3));
    definition
}

fn tlv(c: &mut Criterion) {
    let definition = definition();
    let (parsed, count) = parse(&definition, &MESSAGE, [None, None]).unwrap();
    assert_eq!(count, 8);

    let mut group = c.benchmark_group("tlv");
    group.bench_function("parse", |b| {
        b.iter(|| parse(black_box(&definition), black_box(&MESSAGE), [None, None]))
    });
    let mut decoded = [TlvParsed::new(), TlvParsed::new()];
    group.bench_function("parse_two_occurrences", |b| {
        b.iter(|| {
            parse_multiple(
                &mut decoded,
                black_box(&definition),
                black_box(&MESSAGE),
                [None, None],
            )
        })
    });
    let mut msg = Msgb::new(64).unwrap();
    group.bench_function("encode", |b| {
        b.iter(|| {
            msg.reset();
            encode(&mut msg, black_box(&definition), black_box(&parsed))
        })
    });
    group.finish();
}

criterion_group!(benches, tlv);
criterion_main!(benches);
