//! Implementation-independent reader for complete direct-C counter observations.
#[derive(Debug)]
pub struct Record {
    pub scenario: usize,
    pub step: usize,
    pub op: String,
    pub increment: i32,
    pub result: i64,
    /// current, previous, then last/rate for second, minute, hour, day.
    pub before: [u64; 10],
    pub after: [u64; 10],
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
const TEXT_HASH: u64 = 0x04eee8893d02480c;
const REFERENCE_HASH: u64 = 0xc13de6d46e859cfd;
const GENERATOR_HASH: u64 = 0x434be14b6d318ef3;
const DIAGNOSTICS_HASH: u64 = 0x95d9c6678d8ab839;
const INPUT_HASH: u64 = 0x99fb7448c0b91e80;
const REFERENCE_SHA: &str = "151fa6367553e99b81fbe0fe4543ed747336c0b380b299963ae775b3fb2a5fc5";
const GENERATOR_SHA: &str = "0629517cd95662f7b41c166cad152c059b44143d6940d50b0c66659f367bbbd5";
fn number<T: std::str::FromStr + ToString>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    let value = s.parse::<T>().unwrap();
    assert_eq!(value.to_string(), s, "noncanonical number");
    value
}
pub fn validate_structure(text: &str) -> Vec<Record> {
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap().split('\t').collect::<Vec<_>>(),
        [
            "meta",
            "1",
            "exact-source",
            "unknown",
            REFERENCE_SHA,
            GENERATOR_SHA,
            "64",
            "64",
            "32"
        ]
    );
    let mut rows = Vec::<Record>::new();
    let mut inputs = String::new();
    for (index, line) in lines.enumerate() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 25);
        let scenario = number::<usize>(fields[0]);
        let step = number::<usize>(fields[1]);
        assert_eq!(scenario, index / 17 + 1);
        assert_eq!(step, index % 17);
        let (op, increment) = match step {
            0 | 1 | 10 | 11 | 13 | 15 => ("difference", 0),
            2..=8 => ("add", [0, 1, -1, 2, -2, i32::MIN, i32::MAX][step - 2]),
            9 => ("inc", 0),
            12 | 16 => ("reset", 0),
            14 => ("add", -1),
            _ => unreachable!(),
        };
        assert_eq!(fields[2], op);
        assert_eq!(number::<i32>(fields[3]), increment);
        let result = number::<i64>(fields[4]);
        let before = std::array::from_fn(|i| number::<u64>(fields[5 + i]));
        let after = std::array::from_fn(|i| number::<u64>(fields[15 + i]));
        if step > 0 {
            assert_eq!(rows.last().unwrap().after, before, "state discontinuity");
        }
        // Freeze inputs separately without predicting operation outputs.
        for field in &fields[..4] {
            inputs.push_str(field);
            inputs.push('\n');
        }
        if step == 0 {
            for field in &fields[5..15] {
                inputs.push_str(field);
                inputs.push('\n');
            }
        }
        rows.push(Record {
            scenario,
            step,
            op: op.to_owned(),
            increment,
            result,
            before,
            after,
        });
    }
    assert_eq!(rows.len(), 1700);
    assert_eq!(fingerprint(&inputs), INPUT_HASH);
    rows
}
pub fn validate(text: &str, reference: &str, generator: &str, diagnostics: &str) -> Vec<Record> {
    assert_eq!(fingerprint(reference), REFERENCE_HASH);
    assert_eq!(fingerprint(generator), GENERATOR_HASH);
    assert_eq!(fingerprint(diagnostics), DIAGNOSTICS_HASH);
    assert!(diagnostics.contains(REFERENCE_SHA) && diagnostics.contains(GENERATOR_SHA));
    assert_eq!(diagnostics.matches("\"probe\":").count(), 4);
    let rows = validate_structure(text);
    assert_eq!(fingerprint(text), TEXT_HASH);
    rows
}
#[cfg(test)]
pub mod checks {
    use super::*;
    pub fn integrity(t: &str, r: &str, g: &str, d: &str) {
        assert_eq!(validate(t, r, g, d).len(), 1700);
    }
    pub fn corruptions(t: &str, r: &str, g: &str, d: &str) {
        let reject = |t: &str, r: &str, g: &str, d: &str| {
            assert!(std::panic::catch_unwind(|| validate(t, r, g, d)).is_err());
        };
        reject(t, &r.replace("unknown", "claimed"), g, d);
        reject(t, r, &g.replace("schema", "changed"), d);
        reject(t, r, g, &d.replace("null-pointer", "other"));
        reject(t.trim_end(), r, g, d);
        for replacement in ["1\t1\tdifference", "1\t0\tother", "01\t0\tdifference"] {
            reject(&t.replacen("1\t0\tdifference", replacement, 1), r, g, d);
        }
        let changed = t.replacen("1\t0\tdifference\t0\t0", "1\t0\tdifference\t0\t1", 1);
        assert_ne!(t, changed);
        validate_structure(&changed); // Output hash catches valid but altered observations.
        reject(&changed, r, g, d);
        reject(
            &t.replacen("\t9223372036854775808", "\t18446744073709551616", 1),
            r,
            g,
            d,
        );
        let mut lines = t.lines().collect::<Vec<_>>();
        lines.swap(1, 2);
        reject(&(lines.join("\n") + "\n"), r, g, d);
    }
}
