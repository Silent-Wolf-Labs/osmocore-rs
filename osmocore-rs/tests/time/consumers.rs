use osmocore_rs::{
    rate_ctr::RateCtr,
    time::{Clock, ClockId, ClockOverrides, TimeError, TimeSpec, TimeVal, TimeZone},
    time_accounting::TimeCc,
    timer::{TimerError, TimerId, TimerScheduler},
};

struct UnusedSource;
impl Clock for UnusedSource {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        panic!("consumer must use the wall-clock override")
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        panic!("consumer must use the monotonic override")
    }
}
fn clocks(microseconds: i64) -> ClockOverrides<UnusedSource> {
    let mut clock = ClockOverrides::new(UnusedSource);
    clock.gettimeofday_override = true;
    clock.clock_override_enable(ClockId::MONOTONIC, true);
    advance(&mut clock, microseconds);
    clock
}
fn advance(clock: &mut ClockOverrides<UnusedSource>, microseconds: i64) {
    clock.gettimeofday_override_time =
        TimeVal::new(100 + microseconds / 1_000_000, microseconds % 1_000_000);
    *clock
        .clock_override_gettimespec_mut(ClockId::MONOTONIC)
        .unwrap() = TimeSpec::new(microseconds / 1_000_000, (microseconds % 1_000_000) * 1000);
}

#[test]
fn accounting_consumer_counts_active_periods_and_keeps_rate_counter_history() {
    let mut scheduler = TimerScheduler::new(clocks(1_000_000));
    let mut tc = TimeCc::<RateCtr>::new();
    tc.cfg.rate_ctr = Some(RateCtr {
        previous: 9,
        ..RateCtr::default()
    });
    tc.set_flag(&mut scheduler, true).unwrap();
    advance(&mut scheduler.clock, 2_000_000);
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.cfg.rate_ctr.unwrap().current, 1);
    advance(&mut scheduler.clock, 2_200_000);
    tc.set_flag(&mut scheduler, false).unwrap();
    advance(&mut scheduler.clock, 5_000_000);
    // A previously scheduled wakeup drains once while paused, without counting.
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.cfg.rate_ctr.unwrap().current, 1);
    assert_eq!(scheduler.check(), 0);
    tc.set_flag(&mut scheduler, true).unwrap();
    advance(&mut scheduler.clock, 6_000_000);
    assert_eq!(tc.update(&mut scheduler), Ok(1));
    assert_eq!(tc.total_sum, 2_200_000);
    assert_eq!(tc.cfg.rate_ctr.unwrap().current, 2);
    assert_eq!(tc.cfg.rate_ctr.unwrap().previous, 9);
    assert_eq!(tc.cfg.rate_ctr.unwrap().intv, RateCtr::default().intv);
    let timer = tc.timer().unwrap().clone();
    tc.cleanup(&mut scheduler).unwrap();
    assert_eq!(scheduler.pending(&timer), Err(TimerError::InvalidHandle));
    assert_eq!(scheduler.check(), 0);
    assert_eq!(tc.cfg.rate_ctr.unwrap().current, 2);
}

#[derive(Default)]
struct Consumer {
    events: Vec<TimerId>,
    counter: RateCtr,
}
type Scheduler = TimerScheduler<ClockOverrides<UnusedSource>, Consumer>;
fn dispatch(scheduler: &mut Scheduler, id: TimerId, context: &mut Consumer) {
    assert_eq!(scheduler.pending(&id), Ok(false));
    context.events.push(id);
    context.counter.inc();
}

#[test]
fn timer_operation_sequences_match_deadline_order_and_reject_released_handles() {
    let mut scheduler = Scheduler::new(clocks(0));
    let mut consumer = Consumer::default();
    let mut ids: Vec<_> = (0..8).map(|_| scheduler.create(Some(dispatch))).collect();
    let mut active = [None; 8];
    let mut activation_order = 0;
    let mut random = 0x2718_2818u32;
    let mut dispatched = 0;
    for step in 0..1024 {
        random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let slot = (random >> 16) as usize % ids.len();
        match step % 4 {
            0 => {
                let deadline = TimeVal::new(100 + i64::from(random % 17), 0);
                scheduler.add(&ids[slot], deadline).unwrap();
                active[slot] = Some((deadline, activation_order));
                activation_order += 1;
            }
            1 => {
                scheduler.delete(&ids[slot]).unwrap();
                active[slot] = None;
            }
            2 => {
                let old = ids[slot].clone();
                scheduler.release(&old).unwrap();
                ids[slot] = scheduler.create(Some(dispatch));
                active[slot] = None;
                assert_ne!(ids[slot], old);
                assert_eq!(
                    scheduler.add(&old, TimeVal::new(100, 0)),
                    Err(TimerError::InvalidHandle)
                );
            }
            _ => {
                let now = TimeVal::new(100 + (step / 64) as i64, 0);
                scheduler.clock.gettimeofday_override_time = now;
                let mut due: Vec<_> = active
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, &event)| {
                        event
                            .filter(|&(deadline, _)| deadline <= now)
                            .map(|(deadline, order)| (deadline, order, slot))
                    })
                    .collect();
                // Selected timers dispatch in descending deadline/activation order.
                due.sort();
                due.reverse();
                let expected: Vec<_> = due.iter().map(|&(_, _, slot)| ids[slot].clone()).collect();
                consumer.events.clear();
                assert_eq!(
                    scheduler.update(&mut consumer),
                    Ok(i32::from(!due.is_empty()))
                );
                assert_eq!(consumer.events, expected);
                dispatched += due.len() as u64;
                assert_eq!(consumer.counter.current, dispatched);
                for &(_, _, slot) in &due {
                    active[slot] = None;
                }
            }
        }
        assert_eq!(scheduler.check(), active.iter().flatten().count());
        for (slot, id) in ids.iter().enumerate() {
            assert_eq!(scheduler.pending(id), Ok(active[slot].is_some()));
        }
    }
    assert!(dispatched > 0);
}
