# Time, scheduling, timeout definitions and accounting

[Library overview](../README.md). Clock implementation: [raw values](value.rs),
[sources/overrides](clock.rs), [exports](mod.rs). Related root modules are
[timer.rs](../timer.rs), [tdef.rs](../tdef.rs) and [time_accounting/mod.rs](../time_accounting/mod.rs).
They stay separate public concepts; this README documents their integration.

## Implemented scope

Stages 1–4 cover raw time/clock primitives, owned wall-clock scheduling, core
TDef tables and monotonic cumulative accounting with wall-clock wakeups.
Stage 5a adds the concrete `RateCtr` sink; Stage 6 completes FSM/TDef and shared
application-timer integration. Earlier stage exclusions of FSM integration are
superseded by that completed scope; [FSM details](../fsm/README.md) live with FSMs.

Use `time::{TimeVal,TimeSpec,Clock,SystemClock,ClockOverrides}`,
`timer::TimerScheduler`, `tdef::TDefTable` and `time_accounting::{TimeCc,CounterSink}`.
Read clocks and mutate contexts with methods; redundant C forwarding functions
were removed. `compat::timer` retains integer pending status; `compat::tdef`
retains NULL/sentinel/group adapters, C status/name tables and formatting buffers.
The FSM bridge is `FsmRuntime::state_change_with_tdef`.

References in the required local C checkout: `timer_gettimeofday.c`,
`timer_clockgettime.c`, `timer.c`, `tdef.c`, `time_cc.c` under src/core;
public timer/timer_compat/tdef/time_cc headers under include/osmocom/core.
Recorded parity uses 64-bit Linux long/time fields and 32-bit int; 32-bit
ULONG_MAX behavior is outside the current TDef claim.

## Raw arithmetic and clock behavior

Raw signed fields compare seconds, then fraction. Addition carries once and
subtraction borrows once; abnormal fractions are not fully normalized.
For example, timeval `(0,2000000)+(0,0)` becomes `(1,1000000)`, while
`(0,-2000001)-(0,0)` becomes `(-1,-1000001)`. Nanosecond rules are analogous.
Checked arithmetic rejects each signed-overflow intermediate in C evaluation
order; override errors are atomic. Constructors retain negative/noncanonical
fields. Duration conversion requires canonical nonnegative values; TimeVal
conversion truncates sub-microsecond precision.

Wall time and clock-ID readings are independent: scheduling uses gettimeofday,
accounting uses CLOCK_MONOTONIC. Do not silently substitute either clock.
`ClockOverrides` replaces C globals with owned wall state and eight clock slots.
Initial wall storage is {23,424242}, disabled; slots start at zero, disabled.
Disable/re-enable retains values; slots and wall time remain independent. Unknown
IDs have no slot, enable/add are no-ops and reads pass to the injected source.
Overridden wall reads preserve optional timezone output.

`SystemClock` uses small native gettimeofday/clock_gettime FFI boundaries only
on 64-bit Linux. Other targets return `UnsupportedPlatform`; raw values and
injected sources still work. System errno becomes a typed error. Returned time
is owned; no nullable output pointer or shared mutable clock state is exposed.

## Scheduler ownership and dispatch

`TimerScheduler<C,D>` owns clock/storage, and `TimerId` combines owner identity
with slot generation. Activation sequence is independent and changes on every
add/schedule. Callbacks receive scheduler, handle and caller-owned context;
setup changes made by self/peers remain applied. Rc ownership keeps this scheduler
and its handles local to their owning thread. A NULL callback still counts as work.

Absolute add/reactivation and relative schedule preserve raw wall-clock/single-
carry behavior. Delete is idempotent cancellation retaining deadline/callback;
release cancels and invalidates the handle. Selected batches capture generation
and activation identity so canceled/rearmed/released/reused peers are skipped.

Expired batches dispatch in descending deadline order; equal deadlines reverse
insertion order. Timers become inactive before callbacks. Self-rearms/new
immediate timers wait for the next ordinary update. Update returns a 0/1 work
flag, not callback count. Peer cancel/rearm removes it from the selected batch.
Panic guards reset dispatch state while preserving callback mutations and still-
active undispatched peers; panic=abort cannot recover.

`remaining(id,Some(now))` does no read; None reads wall time. It returns raw
single-borrow time/status even for inactive timers: -1 only when seconds are
negative; equal deadlines are zero/status 0 and dispatch when due.
Only prepare refreshes nearest cache; clock/mutation/update changes do not.
Overdue nearest becomes zero; minimum seconds LONG_MAX yields no cache while
remaining active/dispatchable. Milliseconds round with `(microseconds+999)/1000`;
no cache is -1. Unrepresentable i32/intermediate arithmetic is a typed error,
not C narrowing/undefined overflow. Clock/arithmetic failures preserve activation
and cache, although a failed injected read may mutate the clock source.

All nested updates return `TimerError::NestedUpdate`, preserving ordinary callback
mutation. This is intentional: a C nested update can run a new immediate timer,
but nesting with an outer selected peer can corrupt intrusive batch linkage and
repeat an inactive peer. The bounded C probe exited 77 after 12 callbacks,
including ten repeated peer invocations. Recursive records are diagnostics/
integrity evidence, not a promised general Rust recursive behavior.

## Timeout definitions

`TDefTable` validates a borrowed bounded sentinel table once; entries excludes
its terminator. C adapters distinguish None/null from present-empty/unterminated.
The TDef sentinel uses only ID=0, default=0 and description=None; Some("") is
active. A group terminates only on table=None; Some(empty) remains a group.
Unterminated tables/groups return typed errors and suffix storage stays untouched.

Lookup/set choose the first signed-ID match; reset visits all active duplicates.
Missing get returns a nonnegative fallback unchanged, including unknown units;
negative fallback is legal if the entry exists. Zero/same/CUSTOM conversions
retain values; other supported pairs round up when coarsening and saturate at
u64::MAX when refining. Conversion precedes inclusive range checks; max=0 means
unbounded, and inverted nonzero bounds reject all candidates.

Missing definition and rejected value map to recorded -17/-34 via
`compat::tdef::status`; set_status returns 0/-17/-34 for normal C returns and
keeps typed errors for assertion/undefined cases. Reset failures retain the
valid reset prefix and report failing index/ID/default/bounds. State mapping
accepts exactly 32 entries; ID=0/keep_timer=true remains a mapping.
Unknown unit names own text rather than borrowing reused C scratch. Compatibility
range buffers preserve snprintf-style required length/truncation/NUL. Empty Rust
buffers intentionally avoid C's unconditional backing-byte-zero write at declared
capacity zero. Preferred `range_string` returns owned untruncated text.

## Cumulative accounting

`TimeCc` pairs with `TimerScheduler<C,TimeCc<...>>`; application timers may share
it. Each owned context has one accumulator, mutable owned TDef configuration and
optional CounterSink. Default u64 sinks wrap; `RateCtr` is a concrete sink.
There are no accounting globals or implicit event loops.

The `time_accounting` module separates responsibilities into a public adapter
(`mod.rs`), scheduler-independent event transitions (`state.rs`), and effective
configuration/TDef resolution (`policy.rs`). Transitions mutate the authoritative
public fields directly and return reporting/flag effects. The adapter sends the
increment to the sink before committing represented time and the new flag; it
then resolves scheduling policy and applies an optional monotonic deadline.
Policy is derived from current configuration rather than cached between events.

Observation order is event-specific: changed flags report with the current
configuration before TDef resolution, while wakeups resolve TDefs before
accounting. Startup resolves at initialization and again at deadline evaluation;
wakeups also resolve again after reporting. These boundaries preserve successful
configuration and accounting prefixes when a later lookup or timer operation
fails. An initialized unchanged flag does not refresh TDefs.

- Monotonic time measures true elapsed time; wall time only gates callback wakeups.
- Effective granularity defaults to 1,000,000 microseconds; threshold defaults to
  half and caps at granularity. At granularity 1 the default threshold is zero,
  so even empty delta can increment. Arithmetic/report multiplication wraps u64;
  each reported increment caps at INT_MAX, with recorded i32 timer-delay narrowing.
- Disconnected sinks still advance reported_sum; reconnection has no catchup.
- Forgetting clears sum/reported_sum, retains total/sink, advances last_counted
  only if below now and uses wrapping deadline addition. Disabling forgetting
  retains surplus; direct configuration changes retain sums.
- TDef configuration refreshes at startup, callbacks and scheduling. Granularity
  or nonzero-forget changes reset nonempty sums without reporting; threshold-only
  changes or disabling forgetting do not. Later configuration errors retain
  earlier applied values.
- Unchanged flags only read monotonic time after startup. Delayed callbacks
  count the full elapsed delta once and follow the scheduler's selected batch.
- start_time=0 is the C unstarted sentinel: zero-timestamp startup can repeat
  cleanup/start on the next flag call and reset accumulated state.
- Deadlines use `Option<u64>` internally. A calculated UINT64_MAX is mapped
  to `None` for compatibility; absence means no schedule and no cancellation
  of an existing timer. False/no-forget can leave one final callback. Cleanup releases timer,
  resets accounting, retains configuration/sink and permits reuse.

Monotonic read failure intentionally means timestamp zero, retaining C's
possible running-delta wrap. Wall-clock failures return typed timer errors;
completed accounting/report prefixes stay applied and failed rearm stays inactive.
Callback errors are consumed by `TimeCc::update`; raw scheduler callers must take
the stored callback error themselves. Foreign handles, TDef assertions/invalid
conversions and bounded-adapter errors are typed failures. Sink panics propagate
through scheduler guards without undoing sink/application mutation.

## Testing and verification

Repository fixtures are `time/time_vectors.ndjson`, `tdef/` and `time_cc/` under
`tests/compatibility/vectors/`; replay files are the corresponding
`tests/time/*_reference_vectors.rs` with shared strict readers.

| Evidence | Verified coverage |
| --- | --- |
| Time/timers | 20,933 records: provenance, 20,736 arithmetic, 44 clocks, 48 seconds helpers, 72 traces and 32 timer APIs |
| Rust time replay | 20,828 time/clock/helper observations |
| Rust timer replay | 57 nonrecursive trace records plus 32 API observations; 15 recursive records validate integrity and rejection policy |
| TDef | 2,175 records and 95 isolated failure probes |
| time_cc | 5,948 records: metadata and 5,947 observations across 469 scenarios, plus 5 isolated failures |

Arithmetic tests span positive/negative/noncanonical fractions, carry/borrow,
aliases and independent overrides. Scheduler tests cover batch ordering,
callback replacement/cancel/rearm/new timers, cache persistence, remaining
rounding, equal deadlines and LONG_MAX. TDef tests cover signed/zero/duplicate/
hidden IDs, sentinel boundaries, all units, saturation/ranges and reset prefix.
Accounting's 448-scenario matrix combines 8 granularities, 7 thresholds,
4 forget periods and 2 sink states; edge traces add clock jumps/faults, cleanup
reuse, zero startup, config/sink mutation, caps/wrap/UINT64_MAX and raw time limits.
Shared application timers change TDef before accounting/FSM callbacks, and a
RateCtr sink replay checks all existing accounting observations.

The time runner pins 13 local files and 94 platform headers; accounting pins
30 local files and 128 headers. Reference/source/generator manifests freeze
compiler flags, transitive includes and unmodified dependencies, recording
unknown revision rather than the enclosing Rust revision. Accounting's reference
manifest digest is 782c22f7112724bac91f6132bdf2e21ed679b09af57b70be950419078d98a2d7.
Temporary exact-source builds never substitute system libosmocore. Shared
fingerprints/schema/coverage/provenance/corruption checks are independent from
Rust behavior replay.

Historical combined validation passed both Rust suites, formatting/whitespace,
all exact-source verifications, ASan+UBSan successful-domain audits, two golden
runs per family and two TDef/accounting diagnostic runs matching shared evidence.
Overflow/NULL/assertion/division diagnostics never generate expected time results;
leak checking was disabled under ptrace. Current full-suite/package results are
in the library overview. Ordinary Rust tests need no C build. Sibling runners
are `run_time_vector.sh`, `run_tdef_vector.sh` and `run_time_cc_vector.sh` under
reference; the repository test guide retains complete regeneration commands.

## Remaining work and fundamental differences

The selected stack and FSM/TDef integration are complete. VTY commands, generic
select/timerfd/file-descriptor registration, stats reporters, background/async
runtimes and additional protocol timer stacks remain outside this scope.
Other native clock targets and 32-bit ABI parity require independent validation.

Owned injectable clocks, explicit scheduler/context ownership, checked handles,
bounded tables and typed errors replace C globals, intrusive pointer lifetimes,
assertions and undefined arithmetic. Raw single-carry/borrow, wall/monotonic
separation, selected-batch mutation, partial configuration/reset/reporting and
recorded narrowing/wrap remain behavioral contracts. Nested update rejection,
empty formatting-buffer preservation and checked unsafe values are intentional
Rust differences, not missing conversion work.

## Structured diagnostics

Enable `tracing` for `osmocore_rs::timer` arm, cancel, release and expiry events,
rejected mutation/update diagnostics and the existing update panic boundary.
Activation context connects later expiry/callback execution to its scheduling
span. No additional clock reads or callback catches are introduced. Defaults
remain empty; applications own subscribers, filtering and output. See the
[shared diagnostics contract](../diagnostics/README.md) for fields and context lifetime.
