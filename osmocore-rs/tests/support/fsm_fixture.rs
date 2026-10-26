//! Shared evidence validation only; no FSM behavior model.
#[derive(Clone, Copy)]
pub struct Evidence {
    pub text_hash: u64,
    pub reference_hash: u64,
    pub generator_hash: u64,
    pub diagnostics_hash: u64,
    pub reference_sha: &'static str,
    pub generator_sha: &'static str,
    pub records: usize,
    pub scenarios: usize,
    pub fields: usize,
    pub probes: usize,
}
pub fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}
pub fn validate(
    e: Evidence,
    text: &str,
    reference: &str,
    generator: &str,
    diagnostics: &str,
) -> Vec<Vec<String>> {
    assert_eq!(fingerprint(reference), e.reference_hash);
    assert_eq!(fingerprint(generator), e.generator_hash);
    assert_eq!(fingerprint(diagnostics), e.diagnostics_hash);
    assert!(diagnostics.contains(e.reference_sha) && diagnostics.contains(e.generator_sha));
    assert_eq!(diagnostics.matches("\"probe\":").count(), e.probes);
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap().split('\t').collect::<Vec<_>>(),
        [
            "meta",
            "1",
            "exact-source",
            "unknown",
            e.reference_sha,
            e.generator_sha,
            "64",
            "64",
            "32"
        ]
    );
    let mut rows = Vec::new();
    let mut scenario = 0;
    let mut step = 0;
    for line in lines {
        let row = line.split('\t').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(row.len(), e.fields);
        let current = row[0].parse::<usize>().unwrap();
        if current != scenario {
            assert_eq!(current, scenario + 1);
            scenario = current;
            step = 0;
            assert_eq!(row[2], "init");
        }
        assert_eq!(row[1].parse::<usize>().unwrap(), step);
        step += 1;
        assert!(
            !row[2].is_empty()
                && row[2]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        );
        for (i, field) in row.iter().enumerate() {
            if i == 2 {
                continue;
            }
            if let Some(hex) = field.strip_prefix("s:") {
                assert!(
                    hex == "-"
                        || (hex.len() % 2 == 0
                            && hex
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                );
            } else {
                let value = field.parse::<i128>().unwrap();
                assert!((i128::from(i64::MIN)..=i128::from(u64::MAX)).contains(&value));
                assert_eq!(value.to_string(), *field);
            }
        }
        rows.push(row);
    }
    assert_eq!(scenario, e.scenarios);
    assert_eq!(rows.len(), e.records);
    assert_eq!(fingerprint(text), e.text_hash);
    rows
}
pub fn corruptions(e: Evidence, t: &str, r: &str, g: &str, d: &str) {
    let reject = |t: &str, r: &str, g: &str, d: &str| {
        assert!(std::panic::catch_unwind(|| validate(e, t, r, g, d)).is_err())
    };
    reject(t, &r.replace("unknown", "claimed"), g, d);
    reject(t, r, &g.replace("schema", "changed"), d);
    reject(t, r, g, &d.replace("probe", "changed"));
    reject(t.trim_end(), r, g, d);
    reject(&t.replacen("1\t0\tinit", "1\t1\tinit", 1), r, g, d);
    reject(&t.replacen("1\t0\tinit", "01\t0\tinit", 1), r, g, d);
    reject(&t.replacen("1\t0\tinit", "1\t0\tother", 1), r, g, d);
    let changed = t.replacen("\t-19\t", "\t-18\t", 1);
    if changed != t {
        reject(&changed, r, g, d);
    }
}
