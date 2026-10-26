//! Stateful generic decoder conformance; no correlation or one-shot dispatch.
use super::conv_encoder::descriptor;
use super::conv_fixture::{Fixture, bytes, list, signed, unsigned};
use osmocore_rs::compat::conv::osmo_conv_decode_scan;
use osmocore_rs::conv::*;
use std::collections::{BTreeMap, BTreeSet};

const VECTORS: &str = include_str!("../compatibility/vectors/conv/conv_vectors.ndjson");

#[test]
fn replay_every_c_generic_decoder_observation() {
    let fixture = Fixture::read(VECTORS);
    let codes: BTreeMap<_, _> = fixture
        .records
        .iter()
        .filter(|r| r["type"] == "code")
        .map(|r| ((r["backend"].as_str(), r["id"].as_str()), descriptor(r)))
        .collect();
    let mut decoders = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut cases = BTreeSet::new();
    let mut failures = 0;
    for r in fixture.records.iter().filter(|r| r["type"] == "op") {
        let api = r["api"].as_str();
        if !matches!(
            api,
            "decode_init"
                | "decode_reset"
                | "decode_rewind"
                | "decode_scan"
                | "decode_flush"
                | "decode_get_best_end_state"
                | "decode_get_output"
                | "decode_deinit"
        ) {
            continue;
        }
        let backend = r["backend"].as_str();
        assert_eq!(backend, "linked-dispatch");
        let key = (backend, r["code"].as_str(), r["case"].as_str());
        let code = &codes[&(backend, r["code"].as_str())];
        let context = format!("{key:?}, seq {}, {api}", r["seq"]);
        *counts.entry(api).or_insert(0usize) += 1;
        cases.insert(key);
        match api {
            "decode_init" => {
                let decoder =
                    ConvDecoder::new(code, signed(r, "length_arg"), signed(r, "start_state"))
                        .unwrap_or_else(|e| panic!("{context}: {e:?}"));
                assert!(decoders.insert(key, decoder).is_none(), "{context}");
            }
            "decode_deinit" => {
                // Owned storage is released by Drop, not a pointer-clearing API.
                drop(decoders.remove(&key).unwrap());
                continue;
            }
            _ => {
                let decoder = decoders.get_mut(&key).unwrap();
                match api {
                    "decode_reset" => {
                        assert_eq!(decoder.reset(signed(r, "start_state")), Ok(()), "{context}")
                    }
                    "decode_rewind" => {
                        assert_eq!(decoder.rewind(), Ok(()), "{context}")
                    }
                    "decode_get_best_end_state" => {
                        let actual = decoder.best_end_state();
                        let expected = signed(r, "ret");
                        assert_eq!(
                            actual,
                            if expected < 0 {
                                Err(ConvError::NoSurvivor)
                            } else {
                                Ok(expected as usize)
                            },
                            "{context}"
                        );
                    }
                    "decode_scan" | "decode_flush" => {
                        let input: Vec<_> = bytes(&r["input_before"])
                            .into_iter()
                            .map(|b| b as i8)
                            .collect();
                        let before = input.clone();
                        let actual = if api == "decode_scan" {
                            osmo_conv_decode_scan(decoder, &input, signed(r, "n"))
                        } else {
                            decoder.flush(&input)
                        };
                        assert_eq!(actual, Ok(signed(r, "ret") as usize), "{context}");
                        assert_eq!(input, before, "{context}");
                        assert_eq!(
                            input.iter().map(|&b| b as u8).collect::<Vec<_>>(),
                            bytes(&r["input_after"]),
                            "{context}"
                        );
                    }
                    "decode_get_output" => {
                        let before = decoder.clone();
                        let mut output = bytes(&r["output_before"]);
                        let actual = decoder.get_output(
                            &mut output,
                            unsigned(r, "has_flush") != 0,
                            signed(r, "end_state"),
                        );
                        let expected = signed(r, "ret");
                        failures += usize::from(expected < 0);
                        assert_eq!(
                            actual,
                            if expected < 0 {
                                Err(ConvError::NoSurvivor)
                            } else {
                                Ok(expected as u32)
                            },
                            "{context}"
                        );
                        assert_eq!(*decoder, before, "{context}");
                        assert_eq!(
                            output,
                            bytes(&r["output_after"]),
                            "{context}: full destination"
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }
        let decoder = &decoders[&key];
        assert_eq!(
            decoder.frame_length(),
            unsigned(r, "len") as usize,
            "{context}"
        );
        assert_eq!(
            decoder.state_count(),
            unsigned(r, "n_states") as usize,
            "{context}"
        );
        assert_eq!(
            osmocore_rs::compat::conv::diagnostics(decoder).output_index,
            unsigned(r, "o_idx") as usize,
            "{context}"
        );
        assert_eq!(
            osmocore_rs::compat::conv::diagnostics(decoder).puncture_index,
            unsigned(r, "p_idx") as usize,
            "{context}"
        );
        assert_eq!(
            osmocore_rs::compat::conv::diagnostics(decoder)
                .accumulated_errors
                .iter()
                .map(|&x| i64::from(x))
                .collect::<Vec<_>>(),
            list(&r["ae"], 0, 0xffffff),
            "{context}"
        );
        // Scratch is diagnostic only, but its initialized values can be matched.
        assert_eq!(
            osmocore_rs::compat::conv::diagnostics(decoder)
                .next_accumulated_errors
                .map(|ae| ae.iter().map(|&x| i64::from(x)).collect::<Vec<_>>()),
            if r["ae_next_valid"] == "1" {
                Some(list(&r["ae_next"], 0, 0xffffff))
            } else {
                None
            },
            "{context}"
        );
        assert_eq!(
            osmocore_rs::compat::conv::diagnostics(decoder)
                .predecessor_history
                .iter()
                .map(|x| x.map_or(-1, i64::from))
                .collect::<Vec<_>>(),
            list(&r["history"], -1, 255),
            "{context}"
        );
    }
    assert!(decoders.is_empty());
    assert_eq!(
        counts,
        [
            ("decode_init", 664),
            ("decode_reset", 1357),
            ("decode_rewind", 332),
            ("decode_scan", 1328),
            ("decode_flush", 228),
            ("decode_get_best_end_state", 797),
            ("decode_get_output", 1891),
            ("decode_deinit", 664)
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(counts.values().sum::<usize>(), 7261);
    assert_eq!(cases.len(), 332);
    assert!(failures > 0, "no-survivor output cases must be replayed");
}

fn code(term: Termination, puncture: Option<Vec<i32>>) -> ConvCode {
    ConvCode::new(ConvCodeSpec {
        n: 2,
        k: 2,
        len: 3,
        term,
        next_output: vec![[0, 3], [3, 0]],
        next_state: vec![[0, 1], [0, 1]],
        next_term_output: None,
        next_term_state: None,
        puncture,
    })
    .unwrap()
}

#[test]
fn invalid_start_and_end_states_are_atomic() {
    let code = code(Termination::TRUNCATION, None);
    assert_eq!(ConvDecoder::new(&code, 0, 3), Err(ConvError::InvalidState));
    let mut decoder = ConvDecoder::new(&code, -1, -7).unwrap();
    decoder.scan(&[127; 6], 3).unwrap();
    let before = decoder.clone();
    assert_eq!(decoder.reset(3), Err(ConvError::InvalidState));
    assert_eq!(decoder, before);
    let mut output = [0xa5; 8];
    assert_eq!(
        decoder.get_output(&mut output, false, 2),
        Err(ConvError::InvalidState)
    );
    assert_eq!(decoder, before);
    assert_eq!(output, [0xa5; 8]);
}

#[test]
fn short_input_output_and_repeated_operations_are_atomic() {
    let code = code(Termination::FLUSH, Some(vec![0, 6]));
    let mut decoder = ConvDecoder::new(&code, 0, 0).unwrap();
    let before = decoder.clone();
    assert_eq!(decoder.scan(&[127; 4], 3), Err(ConvError::InputTooShort));
    assert_eq!(decoder, before);
    assert_eq!(
        decoder.get_output(&mut [0xa5; 3], true, 0),
        Err(ConvError::InvalidOperation)
    );
    assert_eq!(decoder, before);
    assert_eq!(decoder.scan_input_capacity(3), Ok(5));
    decoder.scan(&[127; 5], 3).unwrap();
    let before = decoder.clone();
    assert_eq!(decoder.flush(&[]), Err(ConvError::InputTooShort));
    assert_eq!(decoder, before);
    assert_eq!(decoder.flush_input_capacity(), Ok(1));
    decoder.flush(&[127]).unwrap();
    let before = decoder.clone();
    let mut output = [0xa5; 2];
    assert_eq!(
        decoder.get_output(&mut output, true, -1),
        Err(ConvError::OutputTooShort)
    );
    assert_eq!(output, [0xa5; 2]);
    assert_eq!(decoder, before);
    assert_eq!(decoder.flush(&[127; 2]), Err(ConvError::InvalidOperation));
    assert_eq!(decoder.scan(&[127; 2], 1), Err(ConvError::InvalidOperation));
    assert_eq!(decoder, before);
    decoder.reset(0).unwrap();
    decoder.scan(&[127; 5], 3).unwrap();
    decoder.flush(&[127]).unwrap();
    assert_eq!(decoder, before);
}

#[test]
fn tail_biting_requires_initialized_history_and_zero_scan_is_a_noop() {
    let code = code(Termination::TAIL_BITING, None);
    let mut decoder = ConvDecoder::new(&code, 0, -1).unwrap();
    let before = decoder.clone();
    assert_eq!(decoder.best_end_state(), Err(ConvError::MissingHistory));
    let mut output = [0xa5; 5];
    assert_eq!(
        decoder.get_output(&mut output, false, -1),
        Err(ConvError::MissingHistory)
    );
    assert_eq!(output, [0xa5; 5]);
    assert_eq!(osmo_conv_decode_scan(&mut decoder, &[], -4), Ok(0));
    assert_eq!(decoder, before);
    decoder.scan(&[0; 6], 3).unwrap();
    decoder.rewind().unwrap();
    assert_eq!(decoder.best_end_state(), Err(ConvError::MissingHistory));
    decoder.scan(&[0; 6], 3).unwrap();
    assert_eq!(decoder.get_output(&mut output, false, -1), Ok(0));
    assert_eq!(&output[3..], &[0xa5; 2]);
}
