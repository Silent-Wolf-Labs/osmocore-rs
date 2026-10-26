use super::fsm_fixture;
use osmocore_rs::{
    fsm::*,
    tdef::{StateTimeout, TDef, TDefUnit},
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};
const TEXT: &str = include_str!("../compatibility/vectors/fsm_timer/fsm_timer_vectors.tsv");
const REFERENCE: &str =
    include_str!("../compatibility/vectors/fsm_timer/fsm_timer_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/fsm_timer/fsm_timer_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/fsm_timer/fsm_timer_diagnostics.json");
const SPEC: fsm_fixture::Evidence = fsm_fixture::Evidence {
    text_hash: 0x521d233f523ba62a,
    reference_hash: 0xcab6c5c42bd14581,
    reference_sha: "f572b2e98589d209217fdd17b117b79f1ad113b499a96898fd6c7c251d3d7fe0",
    generator_hash: 0x15b44fc0a588abf3,
    generator_sha: "2bf067670670ec52567f4c63a8f6fbbb2f06cad399fe49d766788a8ea0ed83d9",
    diagnostics_hash: 0xb3d1523434907a5b,
    records: 9369,
    scenarios: 1080,
    fields: 20,
    probes: 5,
};
#[test]
fn fsm_timer_evidence() {
    fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    fsm_fixture::corruptions(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
}
struct Source;
impl Clock for Source {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Err(TimeError::SystemError(5))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Err(TimeError::SystemError(5))
    }
}
#[derive(Default)]
struct Data {
    scenario: usize,
    mode: usize,
    kind: usize,
    step: usize,
    definition: Option<FsmDefinitionId>,
    trace: Vec<String>,
    input0: u64,
    input1: i64,
    input2: u64,
    input3: i64,
}
type R = FsmRuntime<ClockOverrides<Source>, Data>;
type S = TimerScheduler<ClockOverrides<Source>, R>;
fn record(s: &S, r: &mut R, op: &str, arg: i64, rc: i32) {
    let d = r.data.definition.as_ref().unwrap();
    let found = r.find_by_id(d, "node").unwrap();
    let name = r.find_by_name(d, "Evidence(node)").unwrap();
    let (state, t, active, ds, dus, term) = if let Some(id) = found.as_ref() {
        let i = r.instance(id).unwrap();
        let deadline = s.deadline(i.timer()).unwrap();
        (
            i.state as i32,
            i.timer_number,
            i32::from(s.pending(i.timer()).unwrap()),
            deadline.seconds,
            deadline.microseconds,
            i32::from(i.terminating),
        )
    } else {
        (-1, 0, 0, 0, 0, 0)
    };
    let wall = s.clock.gettimeofday_override_time;
    r.data.trace.push(format!("{}\t{}\t{op}\t{arg}\t{rc}\t{state}\t{t}\t{active}\t{ds}\t{dus}\t{term}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",r.data.scenario,r.data.step,i32::from(found.is_some()),i32::from(name.is_some()),r.instances(d).unwrap().len(),r.data.input0,r.data.input1,r.data.input2,r.data.input3,wall.seconds,wall.microseconds));
    r.data.step += 1;
}
fn status(r: Result<(), FsmError>) -> i32 {
    r.err()
        .map(|e| osmocore_rs::compat::fsm::status(&e))
        .unwrap_or(0)
}
fn leave(s: &mut S, r: &mut R, id: FsmId, next: u32) {
    record(s, r, "leave", next as i64, 0);
    let timer = r.instance(&id).unwrap().timer().clone();
    if r.data.mode == 1 {
        r.set_timer_number(s, &id, 123).unwrap();
        s.schedule(&timer, 9, 456).unwrap();
        record(s, r, "leave-rearm", 0, 0);
    }
    if r.data.mode == 2 {
        s.delete(&timer).unwrap();
        record(s, r, "leave-cancel", 0, 0);
    }
}
fn enter(s: &mut S, r: &mut R, _: FsmId, prev: u32) {
    record(s, r, "enter", prev as i64, 0);
}
fn preterm(s: &mut S, r: &mut R, _: FsmId, cause: TermCause) {
    record(s, r, "preterm", cause as i64, 0);
}
fn cleanup(s: &mut S, r: &mut R, _: FsmId, cause: TermCause) {
    record(s, r, "cleanup", cause as i64, 0);
}
fn timeout(s: &mut S, r: &mut R, id: FsmId) -> i32 {
    let kind = r.data.kind;
    let rc = if kind == 2 {
        1
    } else if kind == 3 {
        -1
    } else if kind == 4 {
        2
    } else {
        0
    };
    let t = r.instance(&id).unwrap().timer_number;
    record(s, r, "timeout", t as i64, rc);
    if kind == 5 {
        let timer = r.instance(&id).unwrap().timer().clone();
        r.set_timer_number(s, &id, 77).unwrap();
        s.schedule(&timer, 1, 0).unwrap();
        record(s, r, "timeout-rearm", 0, 0);
    }
    if kind == 6 {
        r.term(s, &id, TermCause::Regular, &EventData::None)
            .unwrap();
        record(s, r, "timeout-term", 0, 0);
    }
    rc
}
fn definition(permit: bool, kind: usize) -> Fsm<ClockOverrides<Source>, Data, ()> {
    Fsm {
        name: "Evidence".into(),
        states: (0..3)
            .map(|_| FsmState {
                name: Some("state".into()),
                in_event_mask: 0,
                out_state_mask: if permit { 7 } else { 5 },
                action: None,
                onenter: Some(enter),
                onleave: Some(leave),
            })
            .collect(),
        allstate_event_mask: 0,
        allstate_action: None,
        pre_term: Some(preterm),
        cleanup: Some(cleanup),
        timer_cb: if kind == 0 { None } else { Some(timeout) },
        event_names: None,
    }
}
#[test]
fn fsm_timer_and_tdef_complete_c_replay() {
    let rows = fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut all = Vec::new();
    for row in rows.iter().filter(|r| r[2] == "init") {
        let scenario = row[0].parse::<usize>().unwrap();
        let marker = row[3].parse::<usize>().unwrap();
        let input0 = row[14].parse::<u64>().unwrap();
        let input1 = row[15].parse::<i64>().unwrap();
        let input2 = row[16].parse::<u64>().unwrap();
        let input3 = row[17].parse::<i64>().unwrap();
        let is_tdef = marker == 100;
        let active = if is_tdef {
            input3 % 2 != 0
        } else {
            input3 / 2 != 0
        };
        let permit = is_tdef || input3 % 2 != 0;
        let kind = if is_tdef { 0 } else { scenario % 7 };
        let mut r = R::new(Data {
            scenario,
            mode: if is_tdef { 0 } else { marker },
            kind,
            input0,
            input1,
            input2,
            input3,
            ..Data::default()
        });
        let mut clock = ClockOverrides::new(Source);
        clock.gettimeofday_override = true;
        clock.gettimeofday_override_time = TimeVal::new(100, 789);
        let mut s = S::new(clock);
        let d = r.register(definition(permit, kind)).unwrap();
        r.data.definition = Some(d.clone());
        let id = r.alloc(&mut s, &d, Some("node"), ()).unwrap();
        if active {
            r.set_timer_number(&s, &id, 99).unwrap();
            let timer = r.instance(&id).unwrap().timer().clone();
            s.schedule(&timer, 3, 123).unwrap();
        }
        record(&s, &mut r, "init", marker as i64, 0);
        if is_tdef {
            let ids = [0, 23, -23, 99];
            let mut map = [StateTimeout::default(); 32];
            map[1] = StateTimeout {
                id: ids[input2 as usize],
                keep_timer: input3 / 2 != 0,
            };
            let defs = [
                TDef {
                    id: 23,
                    val: input0,
                    unit: TDefUnit(input1 as i32),
                    ..TDef::default()
                },
                TDef {
                    id: -23,
                    val: input0,
                    unit: TDefUnit(input1 as i32),
                    ..TDef::default()
                },
                TDef::default(),
            ];
            let rc = status(r.state_change_with_tdef(&mut s, &id, 1, &map, Some(&defs), 999));
            record(&s, &mut r, "tdef", 0, rc);
        } else {
            let result = match input2 {
                0 => r.state_change(&mut s, &id, 1, input0, input1 as i32),
                1 => r.state_change_ms(&mut s, &id, 1, input0, input1 as i32),
                2 => r.state_change_keep_timer(&mut s, &id, 1),
                3 => r.state_change_keep_or_start_timer(&mut s, &id, 1, input0, input1 as i32),
                4 => r.state_change_keep_or_start_timer_ms(&mut s, &id, 1, input0, input1 as i32),
                _ => unreachable!(),
            };
            record(&s, &mut r, "transition", input2 as i64, status(result));
        }
        s.clock.gettimeofday_override_time = TimeVal::new(120, 789);
        let rc = r.update(&mut s).unwrap();
        record(&s, &mut r, "update", 0, rc);
        if let Some(id) = r.find_by_id(&d, "node").unwrap() {
            r.term(&mut s, &id, TermCause::Request, &EventData::None)
                .unwrap();
            record(&s, &mut r, "term", 1, 0);
        }
        record(&s, &mut r, "removed", 0, 0);
        all.extend(r.data.trace);
    }
    assert_eq!(all, TEXT.lines().skip(1).collect::<Vec<_>>());
}

#[test]
fn fsm_timer_errors_preserve_completed_mutation_prefix() {
    let mut r = R::new(Data::default());
    let d = r.register(definition(true, 1)).unwrap();
    r.data.definition = Some(d.clone());
    let mut clock = ClockOverrides::new(Source);
    clock.gettimeofday_override = true;
    clock.gettimeofday_override_time = TimeVal::new(100, 0);
    let mut s = S::new(clock);
    let id = r.alloc(&mut s, &d, Some("node"), ()).unwrap();
    r.state_change(&mut s, &id, 0, 10, 99).unwrap();
    let timer = r.instance(&id).unwrap().timer().clone();
    let initial = s.deadline(&timer).unwrap();
    let mut map = [StateTimeout::default(); 32];
    map[1] = StateTimeout {
        id: 23,
        keep_timer: false,
    };
    assert_eq!(
        r.state_change_with_tdef(&mut s, &id, 1, &map, None, -1),
        Err(FsmError::TDef(
            osmocore_rs::tdef::TDefError::NegativeFallback {
                id: 23,
                fallback: -1
            }
        ))
    );
    assert_eq!(
        r.state_change_with_tdef(&mut s, &id, 1, &map, None, i64::MAX),
        Err(FsmError::TimeoutOverflow)
    );
    assert_eq!(
        (
            r.instance(&id).unwrap().state,
            r.instance(&id).unwrap().timer_number
        ),
        (0, 99)
    );
    assert_eq!(s.deadline(&timer).unwrap(), initial);
    assert!(s.pending(&timer).unwrap());
    let defs = [
        TDef {
            id: 23,
            val: 1,
            unit: TDefUnit(99),
            ..TDef::default()
        },
        TDef::default(),
    ];
    assert!(matches!(
        r.state_change_with_tdef(&mut s, &id, 1, &map, Some(&defs), 0),
        Err(FsmError::TDef(_))
    ));
    assert!(s.pending(&timer).unwrap());
    s.clock.gettimeofday_override = false;
    assert!(matches!(
        r.state_change_ms(&mut s, &id, 1, 7, 23),
        Err(FsmError::Timer(_))
    ));
    assert_eq!(
        (
            r.instance(&id).unwrap().state,
            r.instance(&id).unwrap().timer_number
        ),
        (1, 23)
    );
    assert!(!s.pending(&timer).unwrap());
    assert_eq!(s.deadline(&timer).unwrap(), initial);
    assert_eq!(
        r.data.trace.last().unwrap().split('\t').nth(2),
        Some("leave"),
        "onenter must not run after a Rust clock failure"
    );
}

#[test]
fn fsm_callback_error_is_consumed_by_update_or_raw_dispatch() {
    fn invalid_timeout(s: &mut S, r: &mut R, id: FsmId) -> i32 {
        r.free(s, &id).unwrap();
        1
    }
    let mut r = R::new(Data::default());
    let mut f = definition(true, 1);
    f.timer_cb = Some(invalid_timeout);
    let d = r.register(f).unwrap();
    r.data.definition = Some(d.clone());
    let mut c = ClockOverrides::new(Source);
    c.gettimeofday_override = true;
    c.gettimeofday_override_time = TimeVal::new(100, 0);
    let mut s = S::new(c);
    let id = r.alloc(&mut s, &d, Some("node"), ()).unwrap();
    r.state_change_ms(&mut s, &id, 1, 1, 1).unwrap();
    s.clock.gettimeofday_override_time = TimeVal::new(100, 1000);
    assert_eq!(r.update(&mut s), Err(FsmError::InvalidHandle));
    assert!(r.take_callback_error().is_none());
    assert_eq!(s.check(), 0);
    let id = r.alloc(&mut s, &d, Some("node"), ()).unwrap();
    r.state_change_ms(&mut s, &id, 1, 1, 1).unwrap();
    s.clock.gettimeofday_override_time = TimeVal::new(100, 2000);
    assert_eq!(s.update(&mut r), Ok(1));
    assert_eq!(r.take_callback_error(), Some(FsmError::InvalidHandle));
    assert!(r.take_callback_error().is_none());
}
