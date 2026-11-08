// SPDX-License-Identifier: GPL-3.0-or-later

//! Convolutional encode and decode for the two constraint lengths `decode`
//! routes through the correlation decoder, plus the reusable `ConvDecoder`.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use osmocore_rs::base::conv::{ConvCode, ConvCodeSpec, ConvDecoder, Termination, decode, encode};
use std::hint::black_box;

const INPUT_BITS: usize = 224;

/// Build flushed, non-recursive code tables from generator polynomials, where
/// bit `i` of a generator is the coefficient of `D^i`.
fn code(k: u32, generators: &[u32]) -> ConvCode {
    let states = 1usize << (k - 1);
    let output = |state: usize, bit: usize| {
        generators.iter().fold(0u8, |symbol, generator| {
            let parity = (((state << 1) | bit) as u32 & generator).count_ones() as u8 & 1;
            (symbol << 1) | parity
        })
    };
    ConvCode::new(ConvCodeSpec {
        n: generators.len() as i32,
        k: k as i32,
        len: INPUT_BITS as i32,
        term: Termination::FLUSH,
        next_output: (0..states)
            .map(|state| [output(state, 0), output(state, 1)])
            .collect(),
        next_state: (0..states)
            .map(|state| {
                let shifted = state << 1;
                [
                    (shifted & (states - 1)) as u8,
                    ((shifted | 1) & (states - 1)) as u8,
                ]
            })
            .collect(),
        next_term_output: None,
        next_term_state: None,
        puncture: None,
    })
    .unwrap()
}

/// Deterministic soft-bit noise, so every run decodes the same input.
fn noisy(clean: &[i8]) -> Vec<i8> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    clean
        .iter()
        .map(|&soft| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let noise = (state % 201) as i32 - 100;
            (i32::from(soft) + noise).clamp(-127, 127) as i8
        })
        .collect()
}

fn bench_code(c: &mut Criterion, name: &str, code: &ConvCode) {
    let input: Vec<u8> = (0..INPUT_BITS).map(|i| ((i * 7) >> 2) as u8 & 1).collect();
    let mut encoded = vec![0u8; code.output_capacity().unwrap()];
    encode(code, &input, &mut encoded).unwrap();
    let clean: Vec<i8> = encoded
        .iter()
        .map(|&bit| if bit == 0 { 127 } else { -127 })
        .collect();
    let noisy = noisy(&clean);
    let mut decoded = vec![0u8; INPUT_BITS];

    // A benchmark of a decoder that fails to recover its input measures nothing.
    for soft in [&clean, &noisy] {
        decode(code, soft, &mut decoded).unwrap();
        assert_eq!(decoded, input, "{name} round trip");
    }

    let mut group = c.benchmark_group(format!("conv/{name}"));
    group.throughput(Throughput::Elements(INPUT_BITS as u64));
    group.bench_function("encode", |b| {
        b.iter(|| encode(code, black_box(&input), &mut encoded).unwrap())
    });
    for (label, soft) in [("clean", &clean), ("noisy", &noisy)] {
        group.bench_function(format!("decode_{label}"), |b| {
            b.iter(|| decode(code, black_box(soft), &mut decoded).unwrap())
        });
    }
    let mut decoder = ConvDecoder::new(code, 0, 0).unwrap();
    group.bench_function("decoder_reuse_noisy", |b| {
        b.iter(|| {
            decoder.reset(0).unwrap();
            let consumed = decoder.scan(black_box(&noisy), INPUT_BITS).unwrap();
            decoder.flush(&noisy[consumed..]).unwrap();
            decoder.get_output(&mut decoded, true, -1).unwrap()
        })
    });
    group.finish();
}

fn conv(c: &mut Criterion) {
    // GSM 05.03 xCCH: G0 = 1 + D^3 + D^4, G1 = 1 + D + D^3 + D^4.
    bench_code(c, "k5_n2", &code(5, &[0b11001, 0b11011]));
    // GSM 05.03 half-rate speech generators G4, G5 and G6.
    bench_code(c, "k7_n3", &code(7, &[0b1101101, 0b1010011, 0b1011111]));
}

criterion_group!(benches, conv);
criterion_main!(benches);
