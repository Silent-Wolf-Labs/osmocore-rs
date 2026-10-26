# Structured diagnostics

Enable the Cargo feature `tracing` to observe FSMs, timers, runtime lifecycle
and transport operations. With `net` also enabled, the [networking layer](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/src/net/README.md) adds
its own structured establishment target. With `runtime`, GSMTAP adds trace-level
datagram preparation metadata. Default features are empty. Without this feature, the library has no
logging dependencies, diagnostic fields are not evaluated, and no span context
is stored. The feature adds no public types, payload bounds, logger handles or
async runtime requirement.

The library **does not install a subscriber**, choose output destinations or
formatting, or manage process-wide filters. Applications own those choices.
`tracing-subscriber` is a development dependency used by tests and this example;
applications can select any compatible subscriber.

## Targets and events

Targets and the operation/field names below are the filtering and structured
observation contract. Human-readable messages and source locations are not.
Fields whose context is unavailable (for example, a foreign or released handle)
are omitted. Numeric events/states need no application `Debug` implementation.
Instance-private values, application instance IDs/names, event payloads, buffers
and pointer addresses are never emitted. `fsm_name` is the registered descriptor
name; applications should choose a nonsensitive descriptor name.

| Target | Level | `operation` | Important fields |
| --- | --- | --- | --- |
| `osmocore_rs::gsmtap` | trace | `prepare_datagram` | `message_type`, `arfcn`, `timeslot`, `channel_type`, `frame_number`, `payload_len`, `explicit_destination` (boolean) |
| `osmocore_rs::fsm` | trace | `dispatch` | `fsm_name`, `instance_slot`, `instance_generation`, `event`, `current_state` |
| `osmocore_rs::fsm` | debug | `alloc`, `retire` | `fsm_name`, `instance_slot`, `instance_generation`, `current_state` |
| `osmocore_rs::fsm` | debug | `state_commit`, `state_change` | `fsm_name`, `instance_slot`, `instance_generation`, `old_state`, `new_state` |
| `osmocore_rs::fsm` | debug | `change_parent` | `fsm_name`, instance identity, optional `parent_slot`, `parent_generation` |
| `osmocore_rs::fsm` | debug | `unlink_parent` | `fsm_name`, instance identity, `current_state` |
| `osmocore_rs::fsm` | debug | `termination_requested` | `fsm_name`, instance identity, numeric `cause` matching `TermCause` |
| `osmocore_rs::fsm` | debug | `termination_completed` | `fsm_name`, instance identity, entry `current_state` |
| `osmocore_rs::timer` | debug | `arm` | `timer_slot`, `timer_generation`, `deadline_seconds`, `deadline_microseconds` |
| `osmocore_rs::timer` | debug | `cancel` | timer identity, `was_pending` |
| `osmocore_rs::timer` | debug | `release` | timer identity before invalidation |
| `osmocore_rs::timer` | debug | `expire` | timer identity, deadline fields, `now_seconds`, `now_microseconds` |

The GSMTAP target requires both `runtime` and `tracing`. Its preparation event
is emitted only after successful `GsmtapMessage::into_datagram` encoding; it does
not mean enqueue acceptance, transmission or peer delivery. Plain Vec/Msgb
encoding and failed preparation emit no GSMTAP event. `explicit_destination`
records only whether a destination was supplied; addresses and payload bytes
are omitted. Socket/destination compatibility is checked by the generic runtime.
Preparation inherits the current application span and is filtered at trace
because it can occur for every datagram.

Slot/generation pairs are scoped to their owning runtime or scheduler, not
process-wide IDs. Use application spans to distinguish multiple owners. A timer
keeps its handle identity across reactivation; no new public timer ID is added.
Deadlines and current time retain raw `TimeVal` components without normalization
or extra clock reads. `cancel` includes successful idempotent cancellation of an
inactive timer. `expire` occurs after an activation is selected, validated and
deactivated for dispatch; a timer without a callback still expires.

`state_commit` observes the actual state assignment. Existing scheduling errors
can occur after that assignment; they do not roll the state back. `state_change`
observes successful completion after scheduling and `onenter`. Its old/new fields
refer to this operation's transition; nested callbacks may already have changed
state again or retired the instance. It does not assert the instance's final state.
`retire` means the instance is no longer observable; storage reclamation can be
deferred during cascades. Repeated termination of an already-terminating instance
remains a successful no-op and emits no new request/completion events.

Rejected execution/mutation operations emit `warn` with `operation`,
`error_category` and available identity/context. FSM boundaries cover dispatch,
state changes (including TDef preparation), timer-number changes, parent changes,
free, child termination/broadcast and termination. Read-only inspection and
registration/identifier administration are not instrumented. Low-level handle
validation does not emit its own warnings. An error propagating through several
execution boundaries may produce a warning at each boundary, with its respective
context. FSM `current_state` on rejection is the state at operation entry;
`state_commit` events explain any subsequent mutations.

FSM categories are `invalid_handle`, `invalid_definition`, `invalid_descriptor`,
`invalid_identifier`, `duplicate_name`, `not_registered`, `invalid_state`,
`invalid_event`, `terminating`, `transition_not_permitted`, `event_not_permitted`,
`hierarchy_requires_termination`, `parent_cycle`, `stuck_child`, `tdef`,
`timeout_overflow`, `generation_exhausted`, and the underlying timer categories.
A category is emitted only if the instrumented path returns that error.

Timer rejection boundaries cover callback setup, arm/schedule, cancel, release
and update. Categories are `invalid_handle`, `nested_update`,
`generation_exhausted`, and `time`. FSM timeout delivery also warns with
`callback_error` when storing an existing termination failure for later retrieval,
or `invalid_handle` if its instance is missing.

Existing timer update and FSM termination panic boundaries emit `error` with
`error_category = "callback_panic"` before resuming the original panic. No panic
payload is logged and no new catch boundary is introduced. Other callback panics
continue to unwind normally. Normal lifecycle traffic uses `debug`; frequent
execution entry/spans use `trace`. Returned recoverable errors use `warn`, not
`error`. Filtering and severity interpretation remain application decisions.

## Context and application configuration

Trace-level `fsm_execution`, `timer_operation` and `timer_callback` spans carry
`operation` and available identity; FSM spans also carry descriptor name and
entry state. Nested synchronous calls inherit the active application context.
Each successful arm captures its active span; later expiry/callback execution
re-enters that context, including when the scheduling call has returned. Filtered
internal spans leave the outer application span available for capture.

Each re-arm replaces the activation context. Cancel/release clears it. Expiry
detaches the old activation before invoking the callback, so callback re-arming
cannot be overwritten afterward. Stored spans retain subscriber context until
that activation is replaced, cancelled or dispatched; nested scheduling can retain
its causal ancestry. No global context registry is created, and the library installs no
subscriber. Context propagation assumes the application uses the same subscriber
when scheduling and dispatching; it does not migrate contexts between independent
subscribers. Callback spans observe scheduling context, while update-level events
(such as a propagated panic) observe the active update context.

An application can depend on `osmocore-rs` with `features = ["tracing"]` and on
`tracing-subscriber` with `default-features = false` and
`features = ["std", "fmt", "env-filter"]`, then configure its subscriber:

```rust
# #[cfg(feature = "tracing")]
# fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
use osmocore_rs::{time::ClockOverrides, timer::TimerScheduler};

// Application startup, never library initialization.
tracing_subscriber::fmt()
    .with_env_filter("info,osmocore_rs::fsm=debug,osmocore_rs::timer=debug")
    .try_init()?;

let mut scheduler = TimerScheduler::<_, ()>::new(ClockOverrides::default());
scheduler.clock.gettimeofday_override = true;
let timer = scheduler.create(None);
{
    let request = tracing::info_span!("request", request_id = 42u64);
    let _entered = request.enter();
    scheduler.schedule(&timer, 0, 0)?;
}
scheduler.update(&mut ())?; // Expiry retains the request's context.
# Ok(())
# }
# #[cfg(not(feature = "tracing"))]
# fn main() {}
```

## Scope and deferred logging capabilities

Diagnostics do not change FSM/timer behavior. Callback order, scheduling/clock reads, checked
handles, panic propagation and protocol fixtures retain their existing contracts.
The bounded networking-establishment subset is available through `net`; its [diagnostics contract](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/src/net/README.md) adds target `osmocore_rs::net`
when both features are enabled. The independent `runtime` feature adds
`osmocore_rs::runtime` plus transport events under `osmocore_rs::net` when tracing
is enabled. Its [contract](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/src/runtime/README.md) describes queue/lifecycle fields
and command/write span propagation.

Deferred logging capability: specialized sinks, category metadata query APIs,
VTY logging configuration and remote logging targets. C numeric categories,
logging macros, global target lists and exact C formatting are not reproduced.
