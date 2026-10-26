//! Encoder and length conformance against every committed direct C observation.
use super::conv_fixture::{Fixture, Record, bytes, list, signed, unsigned};
use osmocore_rs::compat::conv::{osmo_conv_get_input_length, osmo_conv_get_output_length};
use osmocore_rs::conv::{ConvCode, ConvCodeSpec, ConvEncoder, ConvError, Termination, encode};
use std::collections::{BTreeMap, BTreeSet};

const VECTORS: &str = include_str!("../compatibility/vectors/conv/conv_vectors.ndjson");

fn pairs(text: &str) -> Vec<[u8; 2]> {
    let data = bytes(text);
    assert_eq!(data.len() % 2, 0);
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|row| [row[0], row[1]])
        .collect()
}

pub(super) fn descriptor(record: &Record) -> ConvCode {
    let optional = |key: &str| {
        let data = bytes(&record[key]);
        if data.is_empty() { None } else { Some(data) }
    };
    ConvCode::new(ConvCodeSpec {
        n: signed(record, "n"),
        k: signed(record, "k"),
        len: signed(record, "len"),
        term: Termination(signed(record, "term")),
        next_output: pairs(&record["next_output"]),
        next_state: pairs(&record["next_state"]),
        next_term_output: optional("next_term_output"),
        next_term_state: optional("next_term_state"),
        puncture: if unsigned(record, "puncture_present") == 0 {
            None
        } else {
            Some(
                list(&record["puncture"], 0, i32::MAX as i64)
                    .into_iter()
                    .map(|value| value as i32)
                    .collect(),
            )
        },
    })
    .unwrap()
}

#[test]
fn replay_every_c_length_and_encoder_observation() {
    let fixture = Fixture::read(VECTORS);
    let codes: BTreeMap<_, _> = fixture
        .records
        .iter()
        .filter(|record| record["type"] == "code")
        .map(|record| {
            (
                (record["backend"].as_str(), record["id"].as_str()),
                descriptor(record),
            )
        })
        .collect();
    assert_eq!(codes.len(), 1066);
    assert_eq!(
        codes
            .keys()
            .map(|&(backend, _)| backend)
            .collect::<BTreeSet<_>>(),
        ["linked-dispatch", "scalar", "sse", "avx"]
            .into_iter()
            .collect()
    );

    let mut encoders = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut backends = BTreeMap::new();
    let mut cases = BTreeSet::new();
    let mut nonbinary_loads = 0;
    for record in fixture
        .records
        .iter()
        .filter(|record| record["type"] == "op")
    {
        let api = record["api"].as_str();
        if !matches!(
            api,
            "get_input_length"
                | "get_output_length"
                | "encode"
                | "encode_init"
                | "encode_load_state"
                | "encode_raw"
                | "encode_flush"
        ) {
            continue;
        }
        let backend = record["backend"].as_str();
        let id = record["code"].as_str();
        let key = (backend, id, record["case"].as_str());
        let code = &codes[&(backend, id)];
        let context = format!("{key:?}, seq {}, {api}", record["seq"]);
        *counts.entry(api).or_insert(0usize) += 1;
        *backends.entry(backend).or_insert(0usize) += 1;
        cases.insert(key);

        match api {
            "get_input_length" | "get_output_length" => {
                let arg = signed(record, "length_arg");
                let actual = if api == "get_input_length" {
                    osmo_conv_get_input_length(code, arg)
                } else {
                    osmo_conv_get_output_length(code, arg)
                };
                assert_eq!(actual, Ok(signed(record, "ret")), "{context}");
            }
            "encode_init" => {
                if let Some(encoder) = encoders.get_mut(&key) {
                    ConvEncoder::reset(encoder);
                } else {
                    encoders.insert(key, ConvEncoder::new(code));
                }
            }
            _ => {
                let input = bytes(&record["input_before"]);
                let unchanged = input.clone();
                let mut output = bytes(&record["output_before"]);
                let expected = signed(record, "ret") as usize;
                let actual = match api {
                    "encode" => {
                        // One-shot calls must not touch a retained low-level session.
                        let retained = encoders.get(&key).cloned();
                        assert_eq!(code.output_capacity(), Ok(expected), "{context}");
                        let ret = encode(code, &input, &mut output);
                        assert_eq!(encoders.get(&key), retained.as_ref(), "{context}");
                        ret
                    }
                    "encode_load_state" => {
                        nonbinary_loads += usize::from(input.iter().any(|&byte| byte > 1));
                        encoders
                            .get_mut(&key)
                            .unwrap()
                            .load_state(&input)
                            .map(|()| 0)
                    }
                    "encode_raw" => {
                        assert_eq!(input.len(), unsigned(record, "n") as usize, "{context}");
                        let encoder = encoders.get_mut(&key).unwrap();
                        assert_eq!(
                            encoder.raw_output_capacity(input.len()),
                            Ok(expected),
                            "{context}"
                        );
                        encoder.encode_raw(&input, &mut output)
                    }
                    "encode_flush" => {
                        let encoder = encoders.get_mut(&key).unwrap();
                        assert_eq!(encoder.flush_output_capacity(), Ok(expected), "{context}");
                        encoder.flush(&mut output)
                    }
                    _ => unreachable!(),
                };
                assert_eq!(actual, Ok(expected), "{context}");
                assert_eq!(input, unchanged, "{context}: input mutated");
                assert_eq!(input, bytes(&record["input_after"]), "{context}");
                assert_eq!(
                    output,
                    bytes(&record["output_after"]),
                    "{context}: full output/suffix"
                );
            }
        }
        if record.contains_key("state") {
            let encoder = &encoders[&key];
            assert_eq!(
                u32::from(encoder.state()),
                unsigned(record, "state"),
                "{context}"
            );
            assert_eq!(
                encoder.input_index(),
                unsigned(record, "i_idx") as usize,
                "{context}"
            );
            assert_eq!(
                encoder.puncture_index(),
                unsigned(record, "p_idx") as usize,
                "{context}"
            );
        }
    }
    assert_eq!(
        counts,
        [
            ("get_input_length", 5097),
            ("get_output_length", 5097),
            ("encode", 1699),
            ("encode_init", 728),
            ("encode_load_state", 460),
            ("encode_raw", 720),
            ("encode_flush", 720),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(counts.values().sum::<usize>(), 14521);
    // All backend streams were inspected; only linked-dispatch records these APIs.
    assert_eq!(backends, [("linked-dispatch", 14521)].into_iter().collect());
    assert_eq!(cases.len(), 1707);
    assert_eq!(encoders.len(), 368);
    assert_eq!(nonbinary_loads, 4);
}

fn simple_spec(n: i32, k: i32, len: i32, term: Termination) -> ConvCodeSpec {
    let rows = 1usize << (k - 1);
    ConvCodeSpec {
        n,
        k,
        len,
        term,
        next_output: vec![[0, 1]; rows],
        next_state: (0..rows)
            .map(|state| {
                [
                    ((state << 1) & (rows - 1)) as u8,
                    (((state << 1) | 1) & (rows - 1)) as u8,
                ]
            })
            .collect(),
        next_term_output: None,
        next_term_state: None,
        puncture: None,
    }
}

fn basic_spec() -> ConvCodeSpec {
    simple_spec(2, 3, 4, Termination::FLUSH)
}

fn reject_spec(change: impl FnOnce(&mut ConvCodeSpec), error: ConvError) {
    let mut spec = basic_spec();
    change(&mut spec);
    assert_eq!(ConvCode::new(spec), Err(error));
}

#[test]
fn malformed_descriptors_are_rejected_without_large_allocations() {
    for n in [i32::MIN, -1, 0, 33, i32::MAX] {
        reject_spec(|spec| spec.n = n, ConvError::InvalidRate);
    }
    for k in [i32::MIN, -1, 0, 32, i32::MAX] {
        reject_spec(|spec| spec.k = k, ConvError::InvalidConstraintLength);
    }
    for len in [i32::MIN, -1] {
        reject_spec(|spec| spec.len = len, ConvError::InvalidFrameLength);
    }
    for length in [0, 3, 5] {
        reject_spec(
            |spec| spec.next_output.resize(length, [0, 0]),
            ConvError::InvalidTableLength,
        );
        reject_spec(
            |spec| spec.next_state.resize(length, [0, 0]),
            ConvError::InvalidTableLength,
        );
    }
    // K=31 is in the domain, but a tiny table must fail before any state allocation.
    reject_spec(|spec| spec.k = 31, ConvError::InvalidTableLength);
    for row in 0..4 {
        for bit in 0..2 {
            for target in [4, 255] {
                reject_spec(
                    |spec| spec.next_state[row][bit] = target,
                    ConvError::InvalidTransition,
                );
            }
        }
    }
    reject_spec(
        |spec| spec.next_term_output = Some(vec![0; 4]),
        ConvError::InvalidTerminationTables,
    );
    reject_spec(
        |spec| spec.next_term_state = Some(vec![0; 4]),
        ConvError::InvalidTerminationTables,
    );
    for length in [0, 3, 5] {
        reject_spec(
            |spec| {
                spec.next_term_output = Some(vec![0; length]);
                spec.next_term_state = Some(vec![0; 4]);
            },
            ConvError::InvalidTableLength,
        );
        reject_spec(
            |spec| {
                spec.next_term_output = Some(vec![0; 4]);
                spec.next_term_state = Some(vec![0; length]);
            },
            ConvError::InvalidTableLength,
        );
    }
    for row in 0..4 {
        reject_spec(
            |spec| {
                spec.next_term_output = Some(vec![0; 4]);
                let mut states = vec![0; 4];
                states[row] = 4;
                spec.next_term_state = Some(states);
            },
            ConvError::InvalidTransition,
        );
    }
    for puncture in [vec![-1], vec![0, -1], vec![1, 1], vec![2, 1], vec![0, 2, 2]] {
        reject_spec(
            |spec| spec.puncture = Some(puncture),
            ConvError::InvalidPuncture,
        );
    }
}

#[test]
fn descriptor_ownership_empty_punctures_and_open_termination_values() {
    for term in [
        Termination::FLUSH,
        Termination::TRUNCATION,
        Termination::TAIL_BITING,
        Termination(42),
        Termination(-1),
    ] {
        let mut spec = basic_spec();
        spec.term = term;
        let code = ConvCode::new(spec.clone()).unwrap();
        assert_eq!(code.spec(), &spec);
        assert_eq!(code.clone(), code);
        let mut empty = spec.clone();
        empty.puncture = Some(Vec::new());
        let empty_code = ConvCode::new(empty.clone()).unwrap();
        assert_eq!(empty_code.spec(), &empty);
        assert_ne!(empty_code, code);
        let input = [1, 0, 1, 1];
        let mut null_output = [0xa5; 16];
        let mut empty_output = null_output;
        assert_eq!(
            encode(&code, &input, &mut null_output),
            encode(&empty_code, &input, &mut empty_output)
        );
        assert_eq!(null_output, empty_output);
        assert_eq!(
            code.output_capacity(),
            Ok(if term == Termination::FLUSH { 12 } else { 8 })
        );
    }
    assert_eq!(Termination::FLUSH.0, 0);
    assert_eq!(Termination::TRUNCATION.0, 1);
    assert_eq!(Termination::TAIL_BITING.0, 2);
}

#[test]
fn all_invalid_encoding_bits_are_rejected_before_any_mutation() {
    let mut spec = basic_spec();
    spec.puncture = Some(vec![0, 3, 8, 14]);
    let code = ConvCode::new(spec).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    encoder.encode_raw(&[1, 0], &mut [0xa5; 4]).unwrap();
    let saved = encoder.clone();
    for invalid in 2..=255u8 {
        for position in 0..4 {
            let mut input = [1, 0, 1, 0];
            input[position] = invalid;
            let unchanged = input;
            let mut output = [0xa5; 16];
            assert_eq!(
                encoder.encode_raw(&input, &mut output),
                Err(ConvError::InvalidBit)
            );
            assert_eq!(encoder, saved);
            assert_eq!(output, [0xa5; 16]);
            assert_eq!(input, unchanged);
            assert_eq!(
                encode(&code, &input, &mut output),
                Err(ConvError::InvalidBit)
            );
            assert_eq!(output, [0xa5; 16]);
            assert_eq!(input, unchanged);
        }
    }
}

#[test]
fn short_raw_flush_and_one_shot_destinations_are_atomic() {
    let mut spec = basic_spec();
    spec.puncture = Some(vec![0, 3, 8, 11, 15]);
    let code = ConvCode::new(spec).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    encoder.encode_raw(&[1], &mut [0xa5; 2]).unwrap();
    let saved = encoder.clone();
    let raw_capacity = encoder.raw_output_capacity(3).unwrap();
    for size in 0..raw_capacity {
        let mut output = vec![0xa5; size];
        assert_eq!(
            encoder.encode_raw(&[0, 1, 0], &mut output),
            Err(ConvError::OutputTooShort)
        );
        assert_eq!(encoder, saved);
        assert_eq!(output, vec![0xa5; size]);
    }
    let flush_capacity = encoder.flush_output_capacity().unwrap();
    for size in 0..flush_capacity {
        let mut output = vec![0xa5; size];
        assert_eq!(encoder.flush(&mut output), Err(ConvError::OutputTooShort));
        assert_eq!(encoder, saved);
        assert_eq!(output, vec![0xa5; size]);
    }
    let one_shot_capacity = code.output_capacity().unwrap();
    for size in 0..one_shot_capacity {
        let mut output = vec![0xa5; size];
        assert_eq!(
            encode(&code, &[1, 0, 1, 0], &mut output),
            Err(ConvError::OutputTooShort)
        );
        assert_eq!(output, vec![0xa5; size]);
    }
    let mut exact = vec![0xa5; raw_capacity];
    assert_eq!(encoder.encode_raw(&[0, 1, 0], &mut exact), Ok(raw_capacity));
    let capacity = encoder.flush_output_capacity().unwrap();
    assert_eq!(encoder.flush(&mut vec![0xa5; capacity]), Ok(capacity));
    assert_eq!(
        encode(&code, &[1, 0, 1, 0], &mut vec![0xa5; one_shot_capacity]),
        Ok(one_shot_capacity)
    );
}

#[test]
fn short_inputs_and_tail_biting_underflow_leave_buffers_and_state_unchanged() {
    let code = ConvCode::new(basic_spec()).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    encoder.encode_raw(&[1, 1], &mut [0xa5; 4]).unwrap();
    let saved = encoder.clone();
    for input in [&[][..], &[1][..]] {
        assert_eq!(encoder.load_state(input), Err(ConvError::InputTooShort));
        assert_eq!(encoder, saved);
    }
    for size in 0..4 {
        let input = vec![1; size];
        let mut output = [0xa5; 16];
        assert_eq!(
            encode(&code, &input, &mut output),
            Err(ConvError::InputTooShort)
        );
        assert_eq!(input, vec![1; size]);
        assert_eq!(output, [0xa5; 16]);
    }
    for len in 0..2 {
        let tail = ConvCode::new(simple_spec(2, 3, len, Termination::TAIL_BITING)).unwrap();
        let mut output = [0xa5; 16];
        assert_eq!(
            encode(&tail, &vec![1; len as usize], &mut output),
            Err(ConvError::TailBitingTooShort)
        );
        assert_eq!(output, [0xa5; 16]);
    }
    let tail = ConvCode::new(simple_spec(2, 3, 2, Termination::TAIL_BITING)).unwrap();
    let mut output = [0xa5; 8];
    assert_eq!(encode(&tail, &[1, 0], &mut output), Ok(4));
    assert_eq!(output, [0, 1, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5]);
}

#[test]
fn load_state_packs_arbitrary_bytes_and_narrows_k10_without_resetting_indices() {
    let mut spec = simple_spec(1, 10, 9, Termination::TRUNCATION);
    spec.puncture = Some(vec![0, 4, 10]);
    let code = ConvCode::new(spec).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    encoder
        .encode_raw(&[1, 0, 1, 1, 0], &mut [0xa5; 5])
        .unwrap();
    let indices = (encoder.input_index(), encoder.puncture_index());
    for (input, expected) in [
        ([1, 0, 0, 0, 0, 0, 0, 0, 0], 0),
        ([0xff; 9], 255),
        ([0x80, 1, 0, 1, 0, 1, 0, 1, 0], 170),
        ([1; 9], 255),
    ] {
        let unchanged = input;
        assert_eq!(encoder.load_state(&input), Ok(()));
        assert_eq!(encoder.state(), expected);
        assert_eq!((encoder.input_index(), encoder.puncture_index()), indices);
        assert_eq!(input, unchanged);
    }
    let mut extra = vec![0; 9];
    extra.extend_from_slice(&[255, 128]);
    assert_eq!(encoder.load_state(&extra), Ok(()));
    assert_eq!(encoder.state(), 0);
    assert_eq!((encoder.input_index(), encoder.puncture_index()), indices);
}

#[test]
fn repeated_flush_load_and_reset_keep_the_correct_session_indices() {
    let mut spec = basic_spec();
    spec.puncture = Some(vec![0, 3, 8, 10, 14]);
    // Low-level flush is explicit even when the descriptor says truncation.
    spec.term = Termination::TRUNCATION;
    let code = ConvCode::new(spec).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    let initial = encoder.clone();
    assert_eq!(encoder.encode_raw(&[1, 1], &mut [0xa5; 4]), Ok(2));
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (3, 2, 2)
    );
    let mut output = [0xa5; 8];
    assert_eq!(encoder.flush(&mut output), Ok(4));
    assert_eq!(output, [0, 0, 0, 0, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (0, 4, 2)
    );
    output.fill(0xa5);
    assert_eq!(encoder.flush(&mut output), Ok(2));
    assert_eq!(output, [0, 0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (0, 6, 4)
    );
    assert_eq!(encoder.load_state(&[1, 1]), Ok(()));
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (3, 6, 4)
    );
    output.fill(0xa5);
    assert_eq!(encoder.encode_raw(&[1, 0], &mut output), Ok(3));
    assert_eq!(output, [0, 1, 0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (2, 8, 5)
    );
    encoder.reset();
    assert_eq!(encoder, initial);
    encoder.reset();
    assert_eq!(encoder, initial);
    assert_eq!(encoder.encode_raw(&[], &mut output), Ok(0));
    assert_eq!(encoder, initial);
    assert_eq!(output, [0, 1, 0, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
}

#[test]
fn signed_compatibility_lengths_do_not_determine_actual_buffer_sizes() {
    let mut spec = simple_spec(1, 2, 0, Termination::TRUNCATION);
    spec.puncture = Some(vec![0, 1, 2, 100, i32::MAX]);
    let code = ConvCode::new(spec).unwrap();
    for arg in [i32::MIN, -1, 0] {
        assert_eq!(osmo_conv_get_input_length(&code, arg), Ok(0));
        assert_eq!(osmo_conv_get_output_length(&code, arg), Ok(-5));
    }
    assert_eq!(osmo_conv_get_input_length(&code, 1), Ok(1));
    assert_eq!(osmo_conv_get_output_length(&code, 1), Ok(-4));
    assert_eq!(osmo_conv_get_output_length(&code, 8), Ok(3));
    assert_eq!(code.output_capacity(), Ok(0));
    let mut output = [0xa5; 4];
    assert_eq!(encode(&code, &[], &mut output), Ok(0));
    assert_eq!(output, [0xa5; 4]);
    let mut encoder = ConvEncoder::new(&code);
    assert_eq!(encoder.raw_output_capacity(3), Ok(0));
    assert_eq!(encoder.encode_raw(&[1, 1, 1], &mut []), Ok(0));
    assert_eq!(
        (
            encoder.state(),
            encoder.input_index(),
            encoder.puncture_index()
        ),
        (1, 3, 3)
    );
    assert_eq!(encoder.raw_output_capacity(2), Ok(2));
    assert_eq!(encoder.encode_raw(&[1, 0], &mut output), Ok(2));
    assert_eq!(output, [1, 0, 0xa5, 0xa5]);
}

#[test]
fn arithmetic_limits_are_checked_without_allocating_frame_buffers() {
    let code = ConvCode::new(simple_spec(32, 2, i32::MAX, Termination::FLUSH)).unwrap();
    assert_eq!(osmo_conv_get_input_length(&code, 0), Ok(i32::MAX));
    assert_eq!(
        osmo_conv_get_output_length(&code, 0),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(
        osmo_conv_get_output_length(&code, i32::MAX),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(code.output_capacity(), Err(ConvError::ArithmeticOverflow));
    let encoder = ConvEncoder::new(&code);
    let saved = encoder.clone();
    assert_eq!(
        encoder.raw_output_capacity(usize::MAX),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(encoder, saved);
    assert_eq!(
        encoder.raw_output_capacity(usize::MAX / 32 + 1),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(encoder, saved);
    let mut output = [0xa5; 4];
    assert_eq!(
        encode(&code, &[], &mut output),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(output, [0xa5; 4]);
    // Addition of termination bits alone can overflow the signed C-length domain.
    let code = ConvCode::new(simple_spec(1, 2, i32::MAX, Termination::FLUSH)).unwrap();
    assert_eq!(
        osmo_conv_get_output_length(&code, 0),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(code.output_capacity(), Err(ConvError::ArithmeticOverflow));
    let code = ConvCode::new(simple_spec(1, 1, i32::MAX, Termination::TRUNCATION)).unwrap();
    assert_eq!(osmo_conv_get_output_length(&code, i32::MAX), Ok(i32::MAX));
    assert_eq!(code.output_capacity(), Ok(i32::MAX as usize));

    // Signed compatibility checks intermediate multiplication; actual capacity
    // subtracts encountered punctures before checking the final write count.
    let mut spec = simple_spec(2, 1, i32::MAX / 2 + 1, Termination::TRUNCATION);
    spec.puncture = Some(vec![0, 1]);
    let code = ConvCode::new(spec).unwrap();
    assert_eq!(
        osmo_conv_get_output_length(&code, 0),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(code.output_capacity(), Ok(i32::MAX as usize - 1));

    let code = ConvCode::new(simple_spec(1, 1, 0, Termination::TRUNCATION)).unwrap();
    let mut encoder = ConvEncoder::new(&code);
    assert_eq!(
        encoder.raw_output_capacity(i32::MAX as usize),
        Ok(i32::MAX as usize)
    );
    encoder.encode_raw(&[1], &mut [0xa5]).unwrap();
    let saved = encoder.clone();
    assert_eq!(
        encoder.raw_output_capacity(i32::MAX as usize),
        Err(ConvError::ArithmeticOverflow)
    );
    assert_eq!(encoder, saved);
}

#[test]
fn one_shot_ignores_unused_input_but_raw_consumes_the_entire_slice() {
    let code = ConvCode::new(simple_spec(1, 2, 2, Termination::TRUNCATION)).unwrap();
    let input = [1, 0, 255, 128];
    let mut output = [0xa5; 6];
    assert_eq!(encode(&code, &input, &mut output), Ok(2));
    assert_eq!(output, [1, 0, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(input, [1, 0, 255, 128]);
    let mut encoder = ConvEncoder::new(&code);
    let saved = encoder.clone();
    assert_eq!(
        encoder.encode_raw(&input, &mut output),
        Err(ConvError::InvalidBit)
    );
    assert_eq!(encoder, saved);
    assert_eq!(output, [1, 0, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(encoder.encode_raw(&[1, 0, 1, 1], &mut output), Ok(4));
    assert_eq!(output, [1, 0, 1, 1, 0xa5, 0xa5]);
    assert_eq!(encoder.input_index(), 4);
}

#[test]
fn byte_symbol_high_bits_are_accepted_and_only_the_selected_rate_is_emitted() {
    let mut spec = simple_spec(1, 2, 2, Termination::FLUSH);
    spec.next_output = vec![[0xfe, 0xff]; 2];
    spec.next_term_output = Some(vec![0xff; 2]);
    spec.next_term_state = Some(vec![0; 2]);
    let code = ConvCode::new(spec).unwrap();
    let mut output = [0xa5; 7];
    assert_eq!(encode(&code, &[0, 1], &mut output), Ok(3));
    assert_eq!(output, [0, 1, 1, 0xa5, 0xa5, 0xa5, 0xa5]);
    let mut encoder = ConvEncoder::new(&code);
    assert_eq!(encoder.encode_raw(&[1], &mut output), Ok(1));
    output.fill(0xa5);
    assert_eq!(encoder.flush(&mut output), Ok(1));
    assert_eq!(output, [1, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(encoder.state(), 0);
}

#[test]
fn n32_symbols_have_twenty_four_leading_zero_bits_and_k1_has_no_flush_bits() {
    let mut spec = simple_spec(32, 1, 2, Termination::FLUSH);
    spec.next_output = vec![[0x81, 0xff]];
    let code = ConvCode::new(spec).unwrap();
    let mut output = [0xa5; 68];
    assert_eq!(encode(&code, &[0, 1], &mut output), Ok(64));
    let mut expected = [0xa5; 68];
    expected[..64].fill(0);
    expected[24..32].copy_from_slice(&[1, 0, 0, 0, 0, 0, 0, 1]);
    expected[56..64].fill(1);
    assert_eq!(output, expected);
    let mut encoder = ConvEncoder::new(&code);
    assert_eq!(encoder.load_state(&[]), Ok(()));
    assert_eq!(encoder.encode_raw(&[0, 1], &mut output), Ok(64));
    let saved = encoder.clone();
    assert_eq!(encoder.flush_output_capacity(), Ok(0));
    assert_eq!(encoder.flush(&mut []), Ok(0));
    assert_eq!(encoder, saved);
    assert_eq!(encoder.flush(&mut output), Ok(0));
    assert_eq!(encoder, saved);
    assert_eq!(output, expected);
}
