// SPDX-License-Identifier: GPL-3.0-or-later

//! `TimerScheduler::update` against a fixed clock.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use osmocore_rs::{
    base::time::{Clock, ClockId, TimeError, TimeSpec, TimeVal, TimeZone},
    base::timer::{TimerId, TimerScheduler},
};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

const NOW: i64 = 100;

struct FixedClock;
impl Clock for FixedClock {
    fn gettimeofday(&mut self, _: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        Ok(TimeVal::new(NOW, 0))
    }
    fn clock_gettime(&mut self, _: ClockId) -> Result<TimeSpec, TimeError> {
        Ok(TimeSpec::new(NOW, 0))
    }
}

type Scheduler = TimerScheduler<FixedClock, ()>;

fn expire(_: &mut Scheduler, _: TimerId, _: &mut ()) {}

fn timers(count: usize) -> (Scheduler, Vec<TimerId>) {
    let mut scheduler = Scheduler::new(FixedClock);
    let ids = (0..count).map(|_| scheduler.create(Some(expire))).collect();
    (scheduler, ids)
}

fn update(c: &mut Criterion) {
    let mut group = c.benchmark_group("timer/update");

    // Nothing expires, so this measures the empty scan and the clock read.
    let (mut idle, ids) = timers(256);
    for id in &ids {
        idle.add(id, TimeVal::new(NOW + 1_000, 0)).unwrap();
    }
    group.bench_function("idle_256_pending", |b| {
        b.iter(|| idle.update(black_box(&mut ())).unwrap())
    });

    // One scheduler lives across iterations, so a future reused buffer is
    // visible; a fresh scheduler per iteration would start with an empty one.
    // Rearming is excluded, which adds a constant `Instant` cost to each sample.
    for count in [1usize, 16, 256] {
        let (mut scheduler, ids) = timers(count);
        let deadline = TimeVal::new(NOW, 0);
        group.throughput(Throughput::Elements(count as u64));
        group.bench_function(BenchmarkId::new("expire", count), |b| {
            b.iter_custom(|iterations| {
                let mut total = Duration::ZERO;
                for _ in 0..iterations {
                    for id in &ids {
                        scheduler.add(id, deadline).unwrap();
                    }
                    let start = Instant::now();
                    let work = scheduler.update(black_box(&mut ())).unwrap();
                    total += start.elapsed();
                    assert_eq!(work, 1);
                }
                total
            })
        });
    }
    group.finish();
}

criterion_group!(benches, update);
criterion_main!(benches);
