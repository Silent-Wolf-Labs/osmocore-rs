# Time accounting

`time_accounting` provides an owned cumulative accumulator with explicit clocks,
configuration and an optional counter sink. The module is always available;
the optional I/O runtime can drive it through a typed accounting adapter.

## Public API and ownership

- `TimeCc` holds activity state, cumulative microseconds, represented reporting
  units and an owned timer handle.
- `TimeCcConfig` holds granularity, rounding threshold, surplus-forgetting policy,
  optional owned TDef storage and an optional sink.
- `CounterSink` receives positive increments capped at `i32::MAX` per report.
  Both `u64` and `rate_ctr::RateCtr` implement it; applications can supply a sink.
- `TimeCcError` preserves timer and TDef failures.

Pair a `TimeCc` with `TimerScheduler<C, TimeCc<...>>`. Construction reads no clock
and registers no timer. Configure `cfg`, call `set_flag` to start or pause
accounting, and call `update` to dispatch the scheduler and surface deferred
callback errors. Callers that dispatch the scheduler directly must consume
`take_callback_error` themselves. Application timers can share the paired
scheduler and context.

There are no global clocks, timer storage or counter groups. Cleanup releases
the timer and resets accounting while retaining configuration and sink totals,
allowing the context to be reused.

## Implementation layout

| File | Responsibility |
| --- | --- |
| [mod.rs](mod.rs) | Public types, scheduler adapter, clock reads, sink reporting and deferred callback errors |
| [state.rs](state.rs) | Scheduler-independent event transitions, reporting effects and optional monotonic deadlines |
| [policy.rs](policy.rs) | Effective configuration and ordered TDef binding resolution |
| [tests.rs](tests.rs) | Focused transition, policy and failure-boundary tests |

## Accounting and failure boundaries

Monotonic time measures elapsed activity; wall time gates scheduler wakeups.
Runtime integration preserves those distinct clock roles. Effective granularity
defaults to one second, rounding threshold defaults to half the granularity,
and zero forgetting retains surplus.

Flag changes report under current configuration before TDef resolution; wakeups
resolve TDefs before accounting. The adapter calls the sink before committing
represented time and the new flag, then resolves scheduling policy. Completed
configuration, reporting and state changes remain visible if a later lookup or
timer operation fails. Sink panics propagate. A missing sink still advances
represented time, so attaching one later does not replay prior reports.

The [type and method Rustdoc](mod.rs) documents individual API contracts. The
[detailed timing contract](../time/README.md#cumulative-accounting) records
rounding, wrapping, forgetting, zero-timestamp startup, clock failures and
deadline edge cases. Compatibility claims are bounded by the recorded 64-bit
Linux ABI and do not imply general C undefined-behavior parity.
Unsigned arithmetic wraps, and relative timer seconds retain the recorded GCC
low-32-bit narrowing. A failed monotonic clock read intentionally supplies
timestamp zero; wall-clock errors remain typed timer failures.

## Validation and related documentation

Repository accounting replays live in
[time_cc_reference_vectors.rs](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/time/time_cc_reference_vectors.rs).
[Consumer tests](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/time/consumers.rs) exercise cumulative totals,
counter history and cleanup. Reference fixtures and harnesses remain outside the
crate package; the [test guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md) describes their scope and
regeneration.

See [time and timing](../time/README.md), [rate counters](../rate_ctr/README.md),
the [runtime guide](../runtime/README.md), and the consolidated
[architecture and compatibility guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#timing-fsms-accounting-and-counters).
