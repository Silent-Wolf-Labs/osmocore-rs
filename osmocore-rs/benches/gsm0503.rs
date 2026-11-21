// SPDX-License-Identifier: GPL-3.0-or-later

//! xCCH block coding, and the GSM 05.03 convolutional codes on their own.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::base::conv;
use osmocore_rs::coding::gsm0503::{Gsm0503Code, XCCH_L2_BYTES, xcch_decode, xcch_encode};
use std::hint::black_box;

fn xcch(c: &mut Criterion) {
    let frame: [u8; XCCH_L2_BYTES] = std::array::from_fn(|index| (index * 37) as u8);
    let clean = xcch_encode(&frame).map(|bit| if bit == 0 { 127i8 } else { -127 });
    // Every 16th soft bit is wrong, and the frame still decodes.
    let mut noisy = clean;
    for soft in noisy.iter_mut().step_by(16) {
        *soft = -*soft;
    }
    assert_eq!(xcch_decode(&clean).data, Some(frame));
    assert_eq!(xcch_decode(&noisy).data, Some(frame));

    let mut group = c.benchmark_group("gsm0503/xcch");
    group.bench_function("encode", |b| b.iter(|| xcch_encode(black_box(&frame))));
    group.bench_function("decode_clean", |b| {
        b.iter(|| xcch_decode(black_box(&clean)))
    });
    group.bench_function("decode_noisy", |b| {
        b.iter(|| xcch_decode(black_box(&noisy)))
    });
    group.finish();
}

/// One code of each kind the channels add: punctured, recursive and punctured,
/// N > 4 on the generic decoder, and tail-biting.
fn codes(c: &mut Criterion) {
    for (name, code) in [
        ("cs3", Gsm0503Code::Cs3),
        ("tch_afs_12_2", Gsm0503Code::TchAfs12_2),
        ("tch_f24", Gsm0503Code::TchF24),
        ("mcs1_dl_hdr", Gsm0503Code::Mcs1DlHdr),
    ] {
        let code = code.code();
        let input: Vec<u8> = (0..code.spec().len)
            .map(|bit| ((bit * 7 % 5) & 1) as u8)
            .collect();
        let mut coded = vec![0; code.output_capacity().unwrap()];
        conv::encode(code, &input, &mut coded).unwrap();
        // Every 16th soft bit is wrong, and the frame still decodes.
        let noisy: Vec<i8> = coded
            .iter()
            .enumerate()
            .map(|(index, &bit)| {
                let soft = if bit == 0 { 127 } else { -127 };
                if index % 16 == 0 { -soft } else { soft }
            })
            .collect();
        let mut decoded = vec![0; input.len()];
        conv::decode(code, &noisy, &mut decoded).unwrap();
        assert_eq!(decoded, input, "{name}");

        let mut group = c.benchmark_group(format!("gsm0503/codes/{name}"));
        group.bench_function("encode", |b| {
            b.iter(|| conv::encode(code, black_box(&input), &mut coded))
        });
        group.bench_function("decode_noisy", |b| {
            b.iter(|| conv::decode(code, black_box(&noisy), &mut decoded))
        });
        group.finish();
    }
}

criterion_group!(benches, xcch, codes);
criterion_main!(benches);
