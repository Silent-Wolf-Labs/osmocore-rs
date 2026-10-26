# Synchronous I/O runtime

Enable `runtime` for a Linux-first, single-threaded Mio reactor. It is independent
of `net` and `tracing`; default features remain empty. It consumes established
standard TCP streams/listeners and UDP sockets. All transport I/O then uses Mio
sources. Do not keep using aliases or socket clones after transferring ownership.
Pending TCP connect is deferred; establish before registration or transfer a
completed socket from an application worker. Optional `net` helpers compose when
both features are selected, and retain their synchronous connect timeout.

```rust
use osmocore_rs::runtime::*;
use std::{rc::Rc, time::Duration};
struct App(Rc<()>); // Application state need not be Send, Sync or Debug.
impl Application for App {
    type Command = (); // Only commands require Send + 'static.
    type Error = ();
    fn on_event(&mut self, _: &mut RuntimeContext<'_>, _: RuntimeEvent<'_>)
        -> Result<(), ()> { Ok(()) }
    fn on_command(&mut self, cx: &mut RuntimeContext<'_>, _: ())
        -> Result<(), ()> { cx.request_shutdown(ShutdownMode::Immediate).unwrap(); Ok(()) }
}
let mut runtime = IoRuntime::new(RuntimeConfig::default()).unwrap();
let receipt = runtime.control_handle().try_submit(()).ok().unwrap().receipt;
let mut app = App(Rc::new(()));
runtime.run(&mut app, &mut DriverSet::default()).unwrap();
assert_eq!(receipt.recv().unwrap(), CommandStatus::Processed);
```

## Ownership and deferred effects

`IoRuntime<C>` owns I/O and mailbox plumbing, never the application or its FSMs,
schedulers, accounting or counters. Both runtime and driver set remain confined
to the runtime thread, including when `C: Send`. `ControlHandle<C>` can be cloned
across threads; commands need neither Sync nor Debug. Multiple runtimes coexist.
Handles carry an opaque runtime identity, slot and generation. Private Mio tokens
are monotonically allocated per registration lifetime and never recycled. Stale,
foreign and wrong-kind keys are rejected; exhausted identities never wrap.

`RuntimeContext` contains a validated logical snapshot and bounded effect outbox.
It cannot access live resources or perform socket I/O. Registration, close,
pause/resume, send and shutdown effects apply in FIFO order after successful
callback return. Deferred registration returns a `RegistrationId`; the usable
handle arrives in `Registered`. Immediate registration outside callbacks returns
a handle directly. Registration failures close consumed sockets and retain their
phase and original I/O error.

A successful send reserves `WriteId`, full backing bytes and message credit.
It is accepted work, not evidence of an OS write. Reservations include all sends
in the callback; rejected sends return the original owned value. Close-then-send
rejects; send-then-close yields a cancelled write. Later OS failures do not roll
back earlier effects or application state. External shutdown is checked after
callbacks and can cancel ordinary effects before application. Callback errors
and panics cancel uncommitted effects and close pending-registration sockets.
Panic cleanup closes admission and I/O, then resumes unwinding. No application
state rollback, panic hook or recursive failure callback is installed.

## Bounds and output

TCP delivers borrowed chunks; framing and retained copies belong to the app.
UDP preserves boundaries, source addresses and empty datagrams. Scratch storage
receives one byte beyond `max_datagram_size`, reporting/discarding oversize input
on the validated Linux target. Platform truncation errors surface as receive
errors; non-Linux behavior is not validated.

TCP FIFO entries retain Msgb plus offset across short writes and WouldBlock.
Their full backing capacity remains charged until storage is released; `queue_usage()` reports pending wire bytes separately as the sum of remaining
entry lengths. Empty TCP messages complete
without I/O; WriteZero fails the connection. UDP sends whole packets, with an
explicit destination for unconnected sockets and no destination for connected
ones. Short sends fail that packet. UDP send errors are packet-local; receive
errors and TCP errors close the resource. No partial message is retried on another
connection. Completion means local kernel acceptance, not peer delivery.

Limits bound resources, effect count, normal notifications, per-resource/global
retained bytes and messages, scratch space, and per-turn work. Completion events
retain message credits until dispatched, bounding completion bookkeeping even
for empty messages. Normal notifications have a separate fixed capacity; effects
reserve conservative space for their outcomes. Callback errors and terminal
shutdown reports include accepted write outcomes and undelivered registration
results. Handles in a terminal report are already invalid. Drop is abortive and
does not deliver callbacks or return a report.

The mailbox bounds command count only. Arbitrary Box/Arc/String allocations inside
opaque commands are application memory policy; `size_of::<C>()` cannot bound
them. No runtime output-byte quota claims to cover command payload heaps.

## Defaults and cooperative scheduling

| Configuration | Default |
| --- | ---: |
| Resources / poll event capacity | 1,024 / 256 |
| Maximum poll interval | 250 ms |
| Command capacity / command quantum | 64 / 32 |
| Effects per callback / separate shutdown hook allowance | 64 / 16 |
| Normal notification capacity / dispatch quantum | 256 / 128 |
| I/O calls / bytes per direction per resource per step | 16 / 65,536 |
| Accepts per listener per step | 16 |
| Maximum incoming UDP payload | 65,507 |
| Per-resource backing bytes / messages | 262,144 / 128 |
| Global backing bytes / messages | 4,194,304 / 2,048 |
| Driver invocation quantum | 32 |

`DriverSet::new(limit)` bounds the static driver collection (default 32). It
provides typed projections into independently owned component/context pairs.
`TimerDriver`, `FsmDriver`, `AccountingDriver` and `RateDriver` call the existing
update boundaries, including FSM/accounting deferred-error wrappers. No dynamic
driver handles exist. Optional failures disable their entry and are preserved
for `DriverSet::take_failures`; critical failures stop with their original value.
`prepare` must only refresh scheduling state; all runtime effects use `drive`.
Visited enabled drivers recheck through their authoritative update operation even
with idle/future hints; hints control polling, not dispatch eligibility. This also
surfaces deferred wrapper errors and backward monotonic-clock errors promptly.
Custom drivers must decide actual due work within `drive`.

Every adapter computes relative waits from its own clock. Scheduler prepare is
refreshed before blocking. Sentinel/noncanonical waits recheck at the finite
poll cap without normalization or zero-wait spinning. Accounting retains its
monotonic accounting/wall-clock wakeup split; rate rollups retain their stored
phase and missed-interval catch-up. Real clock/arithmetic failures propagate.
`next_deadline()` only exposes the rate driver's existing stored value.

Commands, drivers, resources and notifications receive bounded quanta; resource
and driver starting positions rotate. Software readiness is deduplicated per
resource/direction and retained on budget exhaustion, cleared on WouldBlock.
Resuming reads schedules an attempt without waiting for a new edge. Interest-free
resources deregister and get a fresh token upon resumption. Listeners suspend
at the resource cap and resume when a slot closes. Accepted streams start read
paused so the Accepted handler can configure application state then resume them.
Mio provides their nonblocking mode; nodelay is explicitly applied.

Datagrams are indivisible and may exceed a byte quantum. Timer selected batches
and rate catch-up remain indivisible legacy calls. Callbacks must not block.
`max_wait` limits poll waiting, not total step duration. There is no preemption,
hard latency bound or hard real-time claim.

## Control, shutdown and diagnostics

Submission is serialized with an out-of-band shutdown latch under a short-held
mutex. Shutdown bypasses a full mailbox. Enqueue precedes the shared Mio wake;
coalesced wakeups are safe because commands are checked every turn. Wake failure
after enqueue returns the receipt and wake error, with finite-poll fallback.
Receipts report Processed, Rejected or Failed, never remote transport success;
dropped/slow receipt receivers cannot block runtime progress.

Running transitions to Draining then Stopped. Shutdown closes admission/intake,
rejects queued commands, invokes one hook with a bounded final-output allowance,
and quiesces automatic drivers. The hook has its own effect-count allowance and
reserved notification space; existing hard output byte/message limits still
apply to its final output. The hook chooses final FSM/accounting/counter
policy. Accepted output drains until empty or a monotonic deadline. Later requests
may shorten the deadline or escalate to immediate stop. Timeout, immediate stop
and fatal poll failures cancel remaining output with partial-write offsets in
the terminal report. Controls reject after stop. The deadline bounds runtime
waiting; it cannot preempt application callbacks or whole driver batches.

Optional tracing uses `osmocore_rs::runtime` and `osmocore_rs::net`. Trace covers
readiness, callback/effect work, transport progress and driver checks; debug covers
registration, interests, completion and lifecycle; warn covers saturation
transitions, recoverable failures and drain timeout; error covers fatal runtime
boundaries. Fields include slot/generation, queue charges, byte progress and
driver labels. Command and write callbacks retain the submitting span. Payloads,
commands, raw descriptors and arbitrary application errors are never logged.
Applications install subscribers and select filters/output.

## Validation and deferred scope

Initial validated platform: Linux x86_64. Rust edition: 2024. Toolchain policy:
current stable. MSRV: not declared. Mio is exactly 1.2.4, defaults disabled,
with `os-poll` and `net`; runtime-only normal/build dependencies are Mio and libc,
without socket2, log or a subscriber. No MSRV or other-platform claim is added.
The runtime uses portable API concepts but inherits existing SystemClock platform
bounds. Tests combine scripted output/poll/wake boundaries with ephemeral-port
IPv4/IPv6 sockets, mixed clocks, failure cleanup and the finite
`examples/runtime_service.rs` acceptance service. Integration tests also compose
the separately enabled net helpers.

Async/Tokio APIs, pending connect, TLS/SCTP, reconnection, protocol framing,
signals, workers, supervision, exporters and C select/osmo_fd compatibility remain
application policy or deferred work.

The finite acceptance service demonstrates composition and TCP/UDP round trips.
Focused tests separately establish accounting totals, rate catch-up, partial
output and drain timeout; the service does not assert those timed effects or
pending-output drain. Worker unwind requests shutdown, and an independent
10-second process watchdog bounds the standalone scenario.
