use super::rate_ctr_fixture;
use osmocore_rs::{rate_ctr::*, time_accounting::CounterSink};
const TEXT: &str = include_str!("../compatibility/vectors/rate_ctr/rate_ctr_vectors.tsv");
const REFERENCE: &str = include_str!("../compatibility/vectors/rate_ctr/rate_ctr_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/rate_ctr/rate_ctr_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/rate_ctr/rate_ctr_diagnostics.json");
#[test]
fn rate_ctr_evidence_integrity() {
    rate_ctr_fixture::checks::integrity(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
#[test]
fn rate_ctr_evidence_corruptions() {
    rate_ctr_fixture::checks::corruptions(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
fn counter(state: [u64; 10]) -> RateCtr {
    RateCtr {
        current: state[0],
        previous: state[1],
        intv: std::array::from_fn(|i| RateCtrPerIntv {
            last: state[2 + 2 * i],
            rate: state[3 + 2 * i],
        }),
    }
}
#[test]
fn rate_ctr_complete_c_replay() {
    let rows = rate_ctr_fixture::validate(TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut ctr = RateCtr::default();
    for row in rows {
        if row.step == 0 {
            ctr = counter(row.before);
        }
        assert_eq!(ctr, counter(row.before));
        let mut methods = ctr;
        let mut sink = ctr;
        let result = match row.op.as_str() {
            "add" => {
                ctr.add(row.increment);
                methods.add(row.increment);
                CounterSink::add(&mut sink, row.increment);
                assert_eq!(sink, counter(row.after));
                0
            }
            "inc" => {
                ctr.inc();
                methods.inc();
                0
            }
            "difference" => {
                let result = ctr.difference();
                assert_eq!(methods.difference(), result);
                result
            }
            "reset" => {
                ctr.reset();
                methods.reset();
                0
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result, row.result,
            "scenario {} step {}",
            row.scenario, row.step
        );
        assert_eq!(
            ctr,
            counter(row.after),
            "scenario {} step {}",
            row.scenario,
            row.step
        );
        assert_eq!(methods, ctr);
    }
}
