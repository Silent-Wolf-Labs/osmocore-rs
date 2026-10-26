use super::fsm_fixture;
use osmocore_rs::compat::tdef::osmo_tdef_set;
use osmocore_rs::{
    fsm::*,
    tdef::{StateTimeout, TDef, TDefUnit},
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    timer::TimerScheduler,
};
const TEXT: &str = include_str!("../compatibility/vectors/fsm_shared/fsm_shared_vectors.tsv");
const REFERENCE: &str =
    include_str!("../compatibility/vectors/fsm_shared/fsm_shared_provenance.json");
const GENERATOR: &str =
    include_str!("../compatibility/vectors/fsm_shared/fsm_shared_generator_provenance.json");
const DIAGNOSTICS: &str =
    include_str!("../compatibility/vectors/fsm_shared/fsm_shared_diagnostics.json");
const SPEC: fsm_fixture::Evidence = fsm_fixture::Evidence {
    text_hash: 0x796534f49a258c65,
    reference_hash: 0xcab6c5c42bd14581,
    reference_sha: "f572b2e98589d209217fdd17b117b79f1ad113b499a96898fd6c7c251d3d7fe0",
    generator_hash: 0x0a7265b3ecc40fb8,
    generator_sha: "9873b06cccd1fe1c179c5d71900963579986b8779d3ed44cc606d1a678568c57",
    diagnostics_hash: 0x85d765ec279ed564,
    records: 443,
    scenarios: 21,
    fields: 43,
    probes: 0,
};
#[test]
fn fsm_shared_evidence() {
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
    step: usize,
    mode: usize,
    definition: Option<FsmDefinitionId>,
    trace: Vec<String>,
}
type R = FsmRuntime<ClockOverrides<Source>, Data, i64>;
type S = TimerScheduler<ClockOverrides<Source>, R>;
const IDS: [&str; 4] = ["p", "a", "b", "g"];
fn get(r: &R, n: usize) -> Option<FsmId> {
    r.find_by_id(r.data.definition.as_ref().unwrap(), IDS[n])
        .unwrap()
}
fn number(r: &R, id: Option<&FsmId>) -> i32 {
    match id {
        None => -1,
        Some(id) => (0..4)
            .find(|&n| get(r, n).as_ref() == Some(id))
            .map(|n| n as i32)
            .unwrap_or(-2),
    }
}
fn record(s: &S, r: &mut R, op: &str, target: i32, arg: i64, rc: i32) {
    let mut line = format!(
        "{}\t{}\t{op}\t{target}\t{arg}\t{rc}",
        r.data.scenario, r.data.step
    );
    for n in 0..4 {
        if let Some(id) = get(r, n) {
            let i = r.instance(&id).unwrap();
            let childbits = i.children().iter().fold(0u32, |bits, c| {
                let n = number(r, Some(c));
                if n >= 0 { bits | (1 << n) } else { bits }
            });
            let time = s.deadline(i.timer()).unwrap();
            line.push_str(&format!(
                "\t{}\t{}\t{}\t1\t{}\t{}\t{}\t{}\t{}",
                i.state,
                i.timer_number,
                i32::from(i.terminating),
                number(r, i.parent()),
                childbits,
                i32::from(s.pending(i.timer()).unwrap()),
                time.seconds,
                time.microseconds
            ));
        } else {
            line.push_str("\t-1\t0\t0\t0\t-1\t0\t0\t0\t0");
        }
    }
    let order = r
        .instances(r.data.definition.as_ref().unwrap())
        .unwrap()
        .iter()
        .fold(0, |order, id| order * 5 + number(r, Some(id)) + 1);
    line.push_str(&format!("\t{order}"));
    r.data.trace.push(line);
    r.data.step += 1;
}
fn action(s: &mut S, r: &mut R, id: FsmId, event: u32, data: &EventData<'_, i64>) {
    let payload = match data {
        EventData::User(d) => **d as i32,
        _ => 0,
    };
    record(s, r, "action", number(r, Some(&id)), event as i64, payload);
}
fn enter(s: &mut S, r: &mut R, id: FsmId, prev: u32) {
    record(s, r, "enter", number(r, Some(&id)), prev as i64, 0);
}
fn leave(s: &mut S, r: &mut R, id: FsmId, next: u32) {
    record(s, r, "leave", number(r, Some(&id)), next as i64, 0);
}
fn preterm(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    record(s, r, "preterm", number(r, Some(&id)), cause as i64, 0);
}
fn cleanup(s: &mut S, r: &mut R, id: FsmId, cause: TermCause) {
    record(s, r, "cleanup", number(r, Some(&id)), cause as i64, 0);
}
fn timeout(s: &mut S, r: &mut R, id: FsmId) -> i32 {
    let n = number(r, Some(&id));
    let mode = r.data.mode;
    let rc = if mode == 0 { 1 } else { 0 };
    let t = r.instance(&id).unwrap().timer_number;
    record(s, r, "timeout", n, t as i64, rc);
    let peer = get(r, if n == 1 { 0 } else { 1 });
    if mode == 1
        && let Some(peer) = peer.as_ref()
    {
        let timer = r.instance(peer).unwrap().timer().clone();
        s.delete(&timer).unwrap();
        record(s, r, "cancel-peer", n, 0, 0);
    }
    if mode == 2
        && let Some(peer) = peer.as_ref()
    {
        r.term(s, peer, TermCause::Request, &EventData::None)
            .unwrap();
        record(s, r, "term-peer", n, 0, 0);
    }
    if mode == 3 && get(r, 3).is_none() {
        let d = r.data.definition.clone().unwrap();
        let g = r.alloc(s, &d, Some("g"), ()).unwrap();
        r.set_timer_number(s, &g, 44).unwrap();
        let timer = r.instance(&g).unwrap().timer().clone();
        s.schedule(&timer, 0, 0).unwrap();
        record(s, r, "create-peer", n, 0, 0);
    }
    if mode == 4 {
        let timer = r.instance(&id).unwrap().timer().clone();
        s.schedule(&timer, 0, 0).unwrap();
        record(s, r, "rearm", n, 0, 0);
    }
    if mode == 5
        && let Some(peer) = peer.as_ref()
    {
        r.dispatch(s, Some(peer), 2, &EventData::User(&55)).unwrap();
        record(s, r, "dispatch-peer", n, 0, 0);
    }
    rc
}
fn app(s: &mut S, _: osmocore_rs::timer::TimerId, r: &mut R) {
    record(s, r, "app", -1, 0, 0);
    if r.data.mode == 6
        && let Some(id) = get(r, 1)
    {
        let mut defs = [
            TDef {
                id: 23,
                val: 1,
                unit: TDefUnit::MS,
                ..TDef::default()
            },
            TDef::default(),
        ];
        let mut map = [StateTimeout::default(); 32];
        map[1] = StateTimeout {
            id: 23,
            keep_timer: false,
        };
        osmo_tdef_set(Some(&mut defs), 23, 50, TDefUnit::MS).unwrap();
        record(s, r, "app-tset", -1, 50, 0);
        r.state_change_with_tdef(s, &id, 1, &map, Some(&defs), 0)
            .unwrap();
        record(s, r, "app-tdef", -1, 0, 0);
    }
}
fn definition() -> Fsm<ClockOverrides<Source>, Data, i64> {
    Fsm {
        name: "Tree".into(),
        states: ["alive", "wait"]
            .iter()
            .map(|n| FsmState {
                name: Some((*n).into()),
                in_event_mask: 4,
                out_state_mask: 3,
                action: Some(action),
                onenter: Some(enter),
                onleave: Some(leave),
            })
            .collect(),
        allstate_event_mask: 0,
        allstate_action: None,
        pre_term: Some(preterm),
        cleanup: Some(cleanup),
        timer_cb: Some(timeout),
        event_names: None,
    }
}
#[test]
fn fsm_multiple_timers_application_mutation_and_tdef_c_replay() {
    let rows = fsm_fixture::validate(SPEC, TEXT, REFERENCE, GENERATOR, DIAGNOSTICS);
    let mut all = Vec::new();
    for row in rows.iter().filter(|r| r[2] == "init") {
        let scenario = row[0].parse().unwrap();
        let variant = row[3].parse::<usize>().unwrap();
        let mode = row[4].parse().unwrap();
        let mut r = R::new(Data {
            scenario,
            mode,
            ..Data::default()
        });
        let d = r.register(definition()).unwrap();
        r.data.definition = Some(d.clone());
        let mut c = ClockOverrides::new(Source);
        c.gettimeofday_override = true;
        c.gettimeofday_override_time = TimeVal::new(100, 0);
        let mut s = S::new(c);
        for (n, name) in IDS.iter().enumerate().take(3) {
            let id = r.alloc(&mut s, &d, Some(name), ()).unwrap();
            r.set_timer_number(&s, &id, 10 + n as i32).unwrap();
            let sec = if n == 0 {
                if variant == 1 { 0 } else { 1 }
            } else if n == 1 {
                if variant == 2 { 2 } else { 1 }
            } else {
                2
            };
            let timer = r.instance(&id).unwrap().timer().clone();
            s.schedule(&timer, sec, 0).unwrap();
        }
        let application = s.create(Some(app));
        s.schedule(&application, 1, 0).unwrap();
        record(&s, &mut r, "init", variant as i32, mode as i64, 0);
        for update in 0..3 {
            s.clock.gettimeofday_override_time.seconds = if update == 2 { 102 } else { 101 };
            let rc = r.update(&mut s).unwrap();
            let now = s.clock.gettimeofday_override_time.seconds;
            record(&s, &mut r, "update", update, now, rc);
        }
        s.release(&application).unwrap();
        for n in 0..4 {
            if let Some(id) = get(&r, n) {
                r.term(&mut s, &id, TermCause::Request, &EventData::None)
                    .unwrap();
                record(&s, &mut r, "drain", n as i32, 0, 0);
            }
        }
        record(&s, &mut r, "removed", 0, 0, 0);
        assert_eq!(s.check(), 0);
        all.extend(r.data.trace);
    }
    let expected = TEXT.lines().skip(1).collect::<Vec<_>>();
    assert_eq!(all.len(), expected.len());
    for (a, b) in all.iter().zip(expected) {
        assert_eq!(a, b);
    }
}
