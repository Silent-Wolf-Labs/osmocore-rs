use super::time_fixture::{self, number};
use osmocore_rs::time::*;

const TEXT: &str = include_str!("../compatibility/vectors/time/time_vectors.ndjson");
const REFERENCE: &str = include_str!("../compatibility/vectors/time/time_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/time/time_generator_provenance.json");

#[test]
fn time_evidence_integrity() {
    time_fixture::checks::integrity(TEXT, REFERENCE, GENERATOR);
}
#[test]
fn time_evidence_corruptions() {
    time_fixture::checks::corruptions(TEXT, REFERENCE, GENERATOR);
}

#[derive(Default)]
struct UnusedSource {
    calls: usize,
}
impl Clock for UnusedSource {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        panic!("golden wall read must be overridden")
    }
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
        assert_eq!(id, ClockId(i32::MAX));
        self.calls += 1;
        Err(TimeError::SystemError(22))
    }
}

#[test]
fn clocks_and_raw_arithmetic_match_direct_c() {
    let rows = time_fixture::validate(TEXT, REFERENCE, GENERATOR);
    let mut clocks = ClockOverrides::new(UnusedSource::default());
    let mut compared = 0;
    for row in &rows {
        let kind = row["type"].as_str();
        if matches!(kind, "meta" | "timer" | "timer_api") {
            continue;
        }
        let op = row["op"].as_str();
        let a = (number(row, "as"), number(row, "af"));
        let b = (number(row, "bs"), number(row, "bf"));
        let expected = (number(row, "rs"), number(row, "rf"));
        match kind {
            "timeval" => {
                let a = TimeVal::new(a.0, a.1);
                let b = TimeVal::new(b.0, b.1);
                match op {
                    "add" | "alias_add" | "sub" | "alias_sub" => {
                        let result = if op.ends_with("add") {
                            a.checked_add(b)
                        } else {
                            a.checked_sub(b)
                        }
                        .unwrap();
                        assert_eq!((result.seconds, result.microseconds), expected, "{row:?}");
                    }
                    "cmp" => assert_eq!(a.cmp(&b) as i64, number(row, "ret")),
                    "isset" => assert_eq!(i64::from(a.is_set()), number(row, "ret")),
                    "clear" => {
                        let mut result = a;
                        result.clear();
                        assert_eq!((result.seconds, result.microseconds), expected);
                    }
                    "override_add" => {
                        clocks.gettimeofday_override_time = a;
                        (clocks)
                            .gettimeofday_override_add(b.seconds, b.microseconds)
                            .unwrap();
                        let result = clocks.gettimeofday_override_time;
                        assert_eq!((result.seconds, result.microseconds), expected);
                    }
                    _ => unreachable!(),
                }
            }
            "timespec" => {
                let a = TimeSpec::new(a.0, a.1);
                let b = TimeSpec::new(b.0, b.1);
                match op {
                    "add" | "alias_add" | "sub" | "alias_sub" => {
                        let result = if op.ends_with("add") {
                            a.checked_add(b)
                        } else {
                            a.checked_sub(b)
                        }
                        .unwrap();
                        assert_eq!((result.seconds, result.nanoseconds), expected, "{row:?}");
                    }
                    "cmp" => assert_eq!(a.cmp(&b) as i64, number(row, "ret")),
                    "isset" => assert_eq!(i64::from(a.is_set()), number(row, "ret")),
                    "clear" => {
                        let mut result = a;
                        result.clear();
                        assert_eq!((result.seconds, result.nanoseconds), expected);
                    }
                    "override_add" => {
                        *(clocks)
                            .clock_override_gettimespec_mut(ClockId::MONOTONIC)
                            .unwrap() = a;
                        (clocks)
                            .clock_override_add(ClockId::MONOTONIC, b.seconds, b.nanoseconds)
                            .unwrap();
                        let result = clocks
                            .clock_override_gettimespec(ClockId::MONOTONIC)
                            .unwrap();
                        assert_eq!((result.seconds, result.nanoseconds), expected);
                    }
                    _ => unreachable!(),
                }
            }
            "wall" => {
                let result = match op {
                    "initial" => {
                        assert_eq!(i64::from(clocks.gettimeofday_override), number(row, "ret"));
                        clocks.gettimeofday_override_time
                    }
                    "read" => {
                        clocks.gettimeofday_override = true;
                        let mut tz = TimeZone {
                            minutes_west: 73,
                            dst_time: 74,
                        };
                        let result = clocks.gettimeofday(Some(&mut tz)).unwrap();
                        assert_eq!((i64::from(tz.minutes_west), i64::from(tz.dst_time)), b);
                        assert_eq!(number(row, "ret"), 0);
                        result
                    }
                    "retained" => {
                        clocks.gettimeofday_override_add(a.0, a.1).unwrap();
                        clocks.gettimeofday_override = false;
                        clocks.gettimeofday_override = true;
                        assert_eq!(number(row, "ret"), 0);
                        clocks.gettimeofday(None).unwrap()
                    }
                    _ => unreachable!(),
                };
                assert_eq!((result.seconds, result.microseconds), expected);
            }
            "clock" => {
                let id = ClockId(i32::try_from(number(row, "id")).unwrap());
                if op == "unsupported" {
                    clocks.clock_override_enable(id, true);
                    clocks.clock_override_add(id, 1, 2).unwrap();
                    assert!(clocks.clock_override_gettimespec_mut(id).is_none());
                    assert_eq!(number(row, "bf"), 0);
                    assert_eq!(number(row, "ret"), -1);
                    assert_eq!(
                        clocks.clock_gettime(id),
                        Err(TimeError::SystemError(b.0 as i32))
                    );
                } else {
                    let result = match op {
                        "initial" => *clocks.clock_override_gettimespec(id).unwrap(),
                        "read" => {
                            clocks.clock_override_enable(id, true);
                            *clocks.clock_override_gettimespec_mut(id).unwrap() =
                                TimeSpec::new(a.0, a.1);
                            clocks.clock_gettime(id).unwrap()
                        }
                        "disabled_slot" => {
                            clocks.clock_override_add(id, a.0, a.1).unwrap();
                            clocks.clock_override_enable(id, false);
                            *clocks.clock_override_gettimespec(id).unwrap()
                        }
                        "retained" => {
                            clocks.clock_override_enable(id, true);
                            clocks.clock_gettime(id).unwrap()
                        }
                        "independent" => clocks.clock_gettime(id).unwrap(),
                        _ => unreachable!(),
                    };
                    assert_eq!((result.seconds, result.nanoseconds), expected, "{row:?}");
                    assert_eq!(number(row, "ret"), 0);
                }
            }
            "seconds" => {
                let result = match op {
                    "sec2min" => minutes_component(a.0),
                    "sec2hrs" => hours_component(a.0),
                    "sec2day" => days_component(a.0),
                    _ => unreachable!(),
                };
                assert_eq!(result, expected.0);
            }
            _ => unreachable!(),
        }
        compared += 1;
    }
    assert_eq!(compared, 20_828);
    assert_eq!(clocks.source.calls, 1);
}
