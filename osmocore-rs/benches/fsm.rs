// SPDX-License-Identifier: GPL-3.0-or-later

//! FSM event dispatch and state transitions with no-op callbacks.

use criterion::{Criterion, criterion_group, criterion_main};
use osmocore_rs::{
    base::fsm::{EventData, Fsm, FsmId, FsmRuntime, FsmState},
    base::time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    base::timer::TimerScheduler,
};
use std::hint::black_box;

struct FixedClock;
impl Clock for FixedClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Ok(TimeVal::new(100, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Ok(TimeSpec::new(100, 0))
    }
}

type Runtime = FsmRuntime<FixedClock, ()>;
type Scheduler = TimerScheduler<FixedClock, Runtime>;

const EVENT: u32 = 0;

fn action(_: &mut Scheduler, _: &mut Runtime, _: FsmId, _: u32, _: &EventData<'_, ()>) {}

/// Two states that accept `EVENT` and may transition to each other.
fn machine() -> (Scheduler, Runtime, FsmId) {
    let mut scheduler = Scheduler::new(FixedClock);
    let mut runtime = Runtime::new(());
    let state = |out_state: u32| FsmState {
        name: None,
        in_event_mask: 1 << EVENT,
        out_state_mask: 1 << out_state,
        action: Some(action as _),
        onenter: None,
        onleave: None,
    };
    let definition = runtime
        .register(Fsm {
            name: "bench".to_owned(),
            states: vec![state(1), state(0)],
            allstate_event_mask: 0,
            allstate_action: None,
            pre_term: None,
            cleanup: None,
            timer_cb: None,
            event_names: None,
        })
        .unwrap();
    let id = runtime
        .alloc(&mut scheduler, &definition, None, ())
        .unwrap();
    (scheduler, runtime, id)
}

fn fsm(c: &mut Criterion) {
    let mut group = c.benchmark_group("fsm");

    let (mut scheduler, mut runtime, id) = machine();
    group.bench_function("dispatch", |b| {
        b.iter(|| {
            runtime
                .dispatch(
                    &mut scheduler,
                    Some(&id),
                    black_box(EVENT),
                    &EventData::None,
                )
                .unwrap()
        })
    });

    let (mut scheduler, mut runtime, id) = machine();
    let mut target = 1;
    group.bench_function("state_change", |b| {
        b.iter(|| {
            runtime
                .state_change(&mut scheduler, &id, black_box(target), 0, 0)
                .unwrap();
            target = 1 - target;
        })
    });
    group.finish();
}

criterion_group!(benches, fsm);
criterion_main!(benches);
