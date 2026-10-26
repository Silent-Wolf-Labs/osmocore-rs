use osmocore_rs::bits::{pack_into_offset, unpack_into_offset};
use std::collections::{BTreeMap, BTreeSet};

const COMMIT: &str = "950430e829a3dc1d162aa241bc0505745c5a7311";
const COUNT: usize = 63_384;
const OFFSETS: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16];
const COUNTS: &[usize] = &[1, 7, 8, 9, 15, 16, 17, 31, 32, 33];
const MODES: &[i32] = &[0, 1, 2, -1];
const FIXTURE: &str = include_str!("../compatibility/vectors/bit_packing_ext_vectors.json");

struct Case {
    name: String,
    pack: bool,
    out: usize,
    input: usize,
    n: usize,
    mode: i32,
    src: Vec<u8>,
    before: Vec<u8>,
}
fn packed(n: usize) -> usize {
    n / 8 + usize::from(!n.is_multiple_of(8))
}
// Preserve the parameter list of the C API or reference record.
#[allow(clippy::too_many_arguments)]
fn case(
    name: String,
    pack: bool,
    out: usize,
    input: usize,
    n: usize,
    mode: i32,
    src: Vec<u8>,
    dst_len: usize,
    init: usize,
) -> Case {
    Case {
        name,
        pack,
        out,
        input,
        n,
        mode,
        src,
        before: (0..dst_len)
            .map(|i| match init {
                0 => 0,
                1 => 255,
                _ => 0xa5 ^ i as u8,
            })
            .collect(),
    }
}
// Frozen input construction only. No converted output is generated here.
// Preserve the parameter list of the C API or reference record.
#[allow(clippy::too_many_arguments)]
fn patterned(
    name: String,
    pack: bool,
    out: usize,
    input: usize,
    n: usize,
    mode: i32,
    pattern: usize,
    extra: usize,
    init: usize,
    tail: usize,
) -> Case {
    let used = if pack { input + n } else { packed(input + n) };
    let mut src = vec![if tail == 0 { 0 } else { 255 }; used + 3];
    for i in 0..n {
        let bit =
            pattern == 1 || (pattern == 2 && i % 2 == 0) || (pattern >= 3 && i == pattern - 3);
        let pos = input + i;
        if pack {
            src[pos] = u8::from(bit);
        } else {
            let bn = if mode != 0 { pos % 8 } else { 7 - pos % 8 };
            if bit {
                src[pos / 8] |= 1 << bn;
            } else {
                src[pos / 8] &= !(1 << bn);
            }
        }
    }
    case(
        name,
        pack,
        out,
        input,
        n,
        mode,
        src,
        (if pack { packed(out + n) } else { out + n }) + extra * 3,
        init,
    )
}
fn expected_cases() -> Vec<Case> {
    let mut cases = Vec::with_capacity(COUNT);
    for byte in 0..=255u8 {
        for ofs in 0..8 {
            for n in 1..=8 - ofs {
                for p in 0..=1 {
                    for mode in 0..=1 {
                        let pack = p == 1;
                        let out = if pack {
                            ofs
                        } else {
                            (usize::from(byte) + n) % 8
                        };
                        let input = if pack { 3 + ofs } else { ofs };
                        let mut src = vec![0xa5; if pack { 11 } else { 1 }];
                        if pack {
                            for i in 0..8 {
                                src[3 + i] = (byte >> if mode != 0 { i } else { 7 - i }) & 1;
                            }
                        } else {
                            src[0] = byte;
                        }
                        let seed = usize::from(byte) + ofs + n + p + mode as usize;
                        cases.push(case(
                            format!("exhaustive_b{byte:02x}_s{ofs}_n{n}_p{p}_m{mode}"),
                            pack,
                            out,
                            input,
                            n,
                            mode,
                            src,
                            (if pack { packed(out + n) } else { out + n }) + seed % 2 * 3,
                            seed % 3,
                        ));
                    }
                }
            }
        }
    }
    for (oi, &out) in OFFSETS.iter().enumerate() {
        for pairing in 0..2 {
            let input = OFFSETS[(oi + pairing) % 12];
            for (ni, &n) in COUNTS.iter().enumerate() {
                for pat in 0..3 + n {
                    for p in 0..=1 {
                        for mode in 0..=1 {
                            let seed = oi + pairing + ni + pat + p + mode as usize;
                            cases.push(patterned(
                                format!("boundary_o{out}_i{input}_n{n}_t{pat}_p{p}_m{mode}"),
                                p == 1,
                                out,
                                input,
                                n,
                                mode,
                                pat,
                                seed % 2,
                                seed % 3,
                                0,
                            ));
                        }
                    }
                }
            }
        }
    }
    for (oi, &out) in OFFSETS.iter().enumerate() {
        for (ii, &input) in OFFSETS.iter().enumerate() {
            for (ni, &n) in COUNTS.iter().enumerate() {
                for p in 0..=1 {
                    for mode in 0..=1 {
                        let seed = oi + ii + ni + p + mode as usize;
                        cases.push(patterned(
                            format!("offsets_o{out}_i{input}_n{n}_p{p}_m{mode}"),
                            p == 1,
                            out,
                            input,
                            n,
                            mode,
                            2,
                            seed % 2,
                            seed % 3,
                            0,
                        ));
                    }
                }
            }
        }
    }
    for value in [2, 0x80, 0xff] {
        for &out in OFFSETS {
            for &mode in MODES {
                for n in [1, 9, 17] {
                    for init in 0..3 {
                        let mut src = vec![0x5a; n + 6];
                        for i in 0..n {
                            src[3 + i] = if i % 2 == 0 { value } else { 0 };
                        }
                        cases.push(case(
                            format!("truthy_v{value:02x}_o{out}_n{n}_m{mode}_d{init}"),
                            true,
                            out,
                            3,
                            n,
                            mode,
                            src,
                            packed(out + n) + 3,
                            init,
                        ));
                    }
                }
            }
        }
    }
    for p in 0..=1 {
        for &mode in MODES {
            for &out in OFFSETS {
                for tail in 0..2 {
                    cases.push(patterned(
                        format!("neighbors_o{out}_p{p}_m{mode}_t{tail}"),
                        p == 1,
                        out,
                        9,
                        17,
                        mode,
                        2,
                        1,
                        2,
                        tail,
                    ));
                }
            }
        }
    }
    for p in 0..=1 {
        for (mi, &mode) in MODES.iter().enumerate() {
            for (oi, out) in [0, 1, 7, 8, 9, 16, 65].into_iter().enumerate() {
                for (ii, input) in [0, 9, 65].into_iter().enumerate() {
                    cases.push(case(
                        format!("zero_o{out}_i{input}_p{p}_m{mode}"),
                        p == 1,
                        out,
                        input,
                        0,
                        mode,
                        vec![255],
                        1,
                        (oi + ii + mi + p) % 3,
                    ));
                }
            }
        }
    }
    assert_eq!(cases.len(), COUNT, "frozen case count");
    cases
}
// The C runner emits flat objects, one per line. Read that fixed format
// strictly, without adding a JSON dependency to this crate's test suite.
fn parse_record(line: &str) -> BTreeMap<String, String> {
    let mut record = BTreeMap::new();
    let mut rest = line.trim().strip_prefix('{').expect("object starts with {");
    rest = rest.strip_suffix('}').expect("object ends with }");

    while !rest.is_empty() {
        rest = rest
            .strip_prefix('"')
            .expect("field name starts with quote");
        let (key, tail) = rest.split_once('"').expect("field name closes");
        rest = tail
            .strip_prefix(':')
            .expect("field name is followed by colon");
        let value;
        let numeric = matches!(
            key,
            "c_unsigned_bits"
                | "out_ofs"
                | "in_ofs"
                | "num_bits"
                | "lsb_mode"
                | "dst_len"
                | "return_code"
        );
        if let Some(string) = rest.strip_prefix('"') {
            assert!(!numeric, "{key} must be an integer, not a string");
            let (string, tail) = string.split_once('"').expect("string value closes");
            assert!(
                !string.contains('\\') && !string.chars().any(char::is_control),
                "fixture strings must be unescaped printable text"
            );
            value = string;
            rest = tail;
        } else {
            assert!(numeric, "{key} must be a string");
            let end = rest.find(',').unwrap_or(rest.len());
            value = &rest[..end];
            let digits = if matches!(key, "lsb_mode" | "return_code") {
                value.strip_prefix('-').unwrap_or(value)
            } else {
                value
            };
            assert!(
                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()),
                "{key} must be an integer"
            );
            assert!(
                digits.len() == 1 || !digits.starts_with('0'),
                "{key} must not have leading zeros"
            );
            rest = &rest[end..];
        }
        assert!(record.insert(key.to_owned(), value.to_owned()).is_none());
        if rest.is_empty() {
            break;
        }
        rest = rest
            .strip_prefix(',')
            .expect("fields are separated by commas");
        assert!(!rest.is_empty(), "object must not have a trailing comma");
    }
    let fields: BTreeSet<&str> = [
        "case",
        "api",
        "libosmocore_commit",
        "c_unsigned_bits",
        "out_ofs",
        "in_ofs",
        "num_bits",
        "lsb_mode",
        "src_hex",
        "dst_len",
        "dst_before_hex",
        "return_code",
        "dst_after_hex",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        record.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        fields
    );
    record
}

fn parse_fixture(fixture: &str) -> Vec<BTreeMap<String, String>> {
    let body = fixture
        .trim()
        .strip_prefix('[')
        .expect("fixture starts with [")
        .strip_suffix(']')
        .expect("fixture ends with ]");
    let lines: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let object = if index + 1 == lines.len() {
                *line
            } else {
                line.strip_suffix(',')
                    .expect("array entries are separated by commas")
            };
            parse_record(object)
        })
        .collect()
}

fn required<'a>(record: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    record
        .get(key)
        .unwrap_or_else(|| panic!("fixture is missing {key}"))
}

fn decode_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex must have complete bytes");
    assert!(
        hex.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

fn unsigned(record: &BTreeMap<String, String>, key: &str) -> usize {
    required(record, key)
        .parse()
        .unwrap_or_else(|_| panic!("{key} must fit usize"))
}

fn compare_record(record: &BTreeMap<String, String>, expected: &Case) {
    let name = required(record, "case");
    assert_eq!(name, expected.name, "frozen case name/order");
    assert_eq!(
        required(record, "libosmocore_commit"),
        COMMIT,
        "{name} provenance"
    );
    assert_eq!(
        unsigned(record, "c_unsigned_bits"),
        32,
        "supported C unsigned ABI"
    );
    let out = unsigned(record, "out_ofs");
    let input = unsigned(record, "in_ofs");
    let n = unsigned(record, "num_bits");
    for value in [out, input, n] {
        assert!(u32::try_from(value).is_ok(), "{name} C unsigned argument");
    }
    let mode: i32 = required(record, "lsb_mode").parse().expect("C int mode");
    assert_eq!(
        (out, input, n, mode),
        (expected.out, expected.input, expected.n, expected.mode),
        "{name} frozen arguments"
    );
    let src = decode_hex(required(record, "src_hex"));
    let mut dst = decode_hex(required(record, "dst_before_hex"));
    assert_eq!(src, expected.src, "{name} full source");
    assert_eq!(dst, expected.before, "{name} full initial destination");
    assert_eq!(
        dst.len(),
        unsigned(record, "dst_len"),
        "{name} backing length"
    );
    let api = required(record, "api");
    assert_eq!(
        api,
        if expected.pack {
            "osmo_ubit2pbit_ext"
        } else {
            "osmo_pbit2ubit_ext"
        },
        "{name} API"
    );
    let result = if expected.pack {
        pack_into_offset(&mut dst, out, &src, input, n, mode != 0)
    } else {
        unpack_into_offset(&mut dst, out, &src, input, n, mode != 0)
    }
    .unwrap_or_else(|error| panic!("{name} safe C fixture call failed: {error:?}"));
    let c_return: i32 = required(record, "return_code")
        .parse()
        .expect("C int return");
    assert_eq!(
        result,
        usize::try_from(c_return).expect("nonnegative fixture return"),
        "{name} return count"
    );
    let after = decode_hex(required(record, "dst_after_hex"));
    assert_eq!(after.len(), dst.len(), "{name} complete destination size");
    assert_eq!(dst, after, "{name} complete destination");
}

#[test]
fn extended_bit_conversions_match_all_c_reference_vectors() {
    let records = parse_fixture(FIXTURE);
    let cases = expected_cases();
    assert_eq!(records.len(), COUNT, "complete frozen fixture");
    for (record, expected) in records.iter().zip(&cases) {
        compare_record(record, expected);
    }
}

#[test]
fn extended_fixture_rejects_corrupt_metadata_and_observations() {
    let cases = expected_cases();
    let first = FIXTURE.lines().nth(1).unwrap().strip_suffix(',').unwrap();
    for text in [
        first.replace("\"out_ofs\":1", "\"out_ofs\":\"1\""),
        first.replace("\"lsb_mode\":0", "\"lsb_mode\":0.0"),
        first.replace("\"num_bits\":1", "\"num_bits\":-1"),
        first.replace("\"dst_len\":5,", ""),
        first.replacen('{', "{\"extra\":\"value\",", 1),
        "null".to_owned(),
    ] {
        assert!(std::panic::catch_unwind(|| parse_fixture(&format!("[\n{text}\n]"))).is_err());
    }
    let record = parse_record(first);
    for (key, value) in [
        ("case", "replaced_case"),
        ("api", "osmo_pbit2ubit"),
        ("c_unsigned_bits", "64"),
        ("libosmocore_commit", "unverified"),
        ("return_code", "0"),
        ("dst_after_hex", "00"),
    ] {
        let mut changed = record.clone();
        changed.insert(key.to_owned(), value.to_owned());
        assert!(
            std::panic::catch_unwind(|| compare_record(&changed, &cases[0])).is_err(),
            "accepted corrupt {key}"
        );
    }
    let index = cases
        .iter()
        .position(|case| case.pack && case.n == 0 && case.out == 0)
        .unwrap();
    let line = FIXTURE
        .lines()
        .nth(index + 1)
        .unwrap()
        .trim_end_matches(',');
    let mut zero = parse_record(line);
    zero.insert("return_code".to_owned(), "0".to_owned());
    assert!(
        std::panic::catch_unwind(|| compare_record(&zero, &cases[index])).is_err(),
        "normalized C underflow return"
    );
}
