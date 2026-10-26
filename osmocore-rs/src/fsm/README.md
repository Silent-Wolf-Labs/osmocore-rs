# Finite-state machines

[Library overview](../README.md). Implementation: [descriptors/handles](descriptor.rs),
[runtime/registry/timers](runtime.rs), [hierarchy/termination](hierarchy.rs),
[public exports](mod.rs).

## Implemented scope

Stage 6 completes the selected execution, timer/TDef, hierarchy and registry/
identity scope. Use `fsm::FsmRuntime` methods and `state_change*` spellings.
The TDef bridge is `FsmRuntime::state_change_with_tdef`, implemented in this
runtime rather than under `tdef`. It closes the earlier deferred core adapter.
`compat::fsm` contains nullable naming, format/sanitize, termination-cause text
and recorded integer error-status conversion. Trivial forwarding exports were
removed before the first release; shared types remain under `fsm`.

Reference files in the required C checkout are `src/core/fsm.c`, naming/identifier
helpers in `src/core/utils.c`, and `include/osmocom/core/fsm.h`/`utils.h`.
This is selected behavior on the recorded 64-bit Linux ABI, not every upstream
subsystem that uses FSMs.

## Ownership and public APIs

Pair an owned `FsmRuntime<C, D, E>` with
`TimerScheduler<C, FsmRuntime<C, D, E>>`. `C` supplies clocks, `D` is caller-owned
runtime context, and `E` is typed event payload data. Definitions own bounded
state/event tables and function-pointer callbacks. Instances own optional IDs,
names, timers, hierarchy links, and private values with typed `Any` access.
Inspection returns shared references; the runtime exposes no mutable instance
reference across callback execution. Callbacks receive the runtime, scheduler
and checked instance handles, allowing synchronous nested dispatch, transition,
allocation and termination. Instance and definition handles reject foreign
runtimes; instance generations prevent slot reuse from reviving stale handles.

Observable registration and lookup behavior is preserved **within an explicitly
owned runtime/registry**. C process-global registry ownership is intentionally
not reproduced. Preferred methods use the owned runtime; timer operations also receive the
paired scheduler. Distinct nullable/name/format/status adapters live in
`compat::fsm`. Descriptor registration order and instance lookup
order are explicit. Instance ordering follows allocation recency, including
slot reuse; duplicate IDs resolve to the newest matching instance. Unregistration
hides a definition from name lookup while retaining its live instances and
owned descriptor. The local C allocation implementation also permits allocation
after unregistration, despite its comment saying otherwise; Rust follows the
observed implementation. Re-registering an owned new descriptor under that
name is permitted, without discarding the old descriptor's instances.

Runtime methods cover register/unregister/find, alloc/free, child allocation,
parent change/unlink, dispatch, all five state-change forms, term/term_children,
broadcast_children, safe-termination configuration, ID update/format/sanitize,
and event/state/instance/termination names. Variadic printf ID formatting is
replaced by Rust `fmt::Arguments`; no printf format-language parity is claimed.
Names use the C no-address form. State names can be absent; event tables retain
first-match and null-name sentinel behavior. Private data is Rust-owned rather
than a raw talloc pointer. Exact C logging levels/output and source-file/line macros,
native pointer-address names, talloc deallocation contexts and arbitrary talloc
ownership/destructors are outside the conversion. VTY, CTRL, timerfd/select and
statistics integrations remain deferred.

## Dispatch, transitions and timers

Dispatch rejects disallowed events, gives all-state actions precedence only
when an all-state callback exists, and permits absent per-state callbacks.
Terminating instances reject dispatch and transitions with C-compatible
`-EINVAL`; missing-instance dispatch maps to `-ENODEV`, forbidden transitions
to `-EPERM`, and disallowed events to C's generic -1. Typed errors retain these
categories even when numeric values overlap.

Transition ordering is observed, not inferred from comments: validate the old
state's transition mask, cancel the old timer for replacement forms, call
onleave while the old state is still visible, assign the new state, decide
whether the timer must be started/replaced, assign its timer number, schedule
if nonzero, and call onenter last. A forbidden transition leaves an active
timer, its deadline/number, and state untouched. An onleave callback may rearm
or cancel the timer; keep/start decisions inspect its resulting pending state.
Nested transitions preserve the outer operation's captured old state.

Seconds-to-milliseconds multiplication wraps as unsigned 64-bit arithmetic.
Scheduling narrows seconds to the pinned compiler's signed C-int result; the
local implementation has no explicit clamp despite its API comments. Keep
forms preserve a running timer's deadline and number. With no running timer,
keep-only sets the supplied internal zero number and does not arm a timer.
Timer callbacks terminate only when returning exactly 1; absent callbacks
terminate automatically. Other return values, callback rearming, and callback
self-termination with a non-1 return are preserved. Timeout notification data
contains the timer number captured before calling the timeout callback, even
if that callback changes the instance's number.

The TDef adapter preserves absent mapping, T=0/keep-only, signed IDs,
first-match lookup, seconds-based missing-definition fallback, MS/US/S/M
rounding, CUSTOM-as-seconds behavior, zero values, saturating unit conversion
and the adapter's subsequent unsigned wrapping multiplication. Conversion or
fallback errors happen before state-transition mutations. Rust reports invalid
units, missing terminators, negative missing-definition fallbacks and signed
fallback-multiplication overflow as typed errors. Wall-clock failures during
scheduling preserve the completed cancellation/onleave/state/number prefix and
skip onenter. Existing scheduler batch ordering and typed nested-update rejection
remain unchanged. Internal timeout errors are consumed by runtime `update()`;
a caller using raw scheduler dispatch must consume `take_callback_error()`.

## Lifetime, hierarchy and termination

Standalone and hierarchical termination have dedicated C evidence. Parent/child
ordering, changes/unlinking, broadcast and teardown are replayed against C,
including callbacks that remove the current child.

Termination first marks the instance terminating. During pre_term and cleanup,
lookup still finds it and callbacks can inspect it; repeated term is ignored.
Pre_term precedes child teardown. Children are terminated with PARENT cause and
null event data; that cause suppresses parent notification. The child is removed
from its parent's child list before cleanup, while its parent pointer remains
available for inspection. C captures the notification event before pre_term,
but fetches the parent again after cleanup: a pre_term reparent can therefore
notify the new parent with the original event.

With legacy ordering, registry removal precedes parent notification. With safe
ordering, the child remains observable during notification and is removed
afterward. Storage reclamation is separate from lookup visibility: slots become
invisible tombstones during a safe termination cascade and are reclaimed only
when the cascade unwinds. Removed instances can remain readable through handles
inside those callbacks, while lookup excludes them. Panic unwinding restores
termination depth and sweeps already removed slots; it does not undo callback
mutations or complete interrupted cleanup. No raw instance borrow survives a
nested operation.

The C hierarchy audit demonstrated use-after-free for legacy cleanup terminating
its talloc parent, raw freeing of a linked child, and parent termination during
child teardown/broadcast iteration. These calls are separate diagnostics, never
golden observations. Rust checks handles after nested calls and returns errors
for stale traversal, rejects raw free of linked instances or instances with
children, and rejects parent cycles. It does not reproduce arbitrary talloc
parentage or the unsafe C memory accesses. Parent notification ignores ordinary
dispatch rejection as C does; stale handles are never dereferenced. C's stuck-child
loop guard becomes a typed error. Raw free during termination is rejected.

State/event indices outside the 32-bit mask domain are errors. Rust uses unsigned
shifts, safely supporting bit 31, while the observed C signed shifts at 31 and
32 have separate UBSan failures. Empty/oversized state descriptors are rejected
before execution. Rust strings reject embedded NUL and invalid identifiers;
non-UTF8 C strings are outside this string API. C's null-ID lookup failures are
separate diagnostics; Rust ID lookup skips absent IDs safely and accepts a
non-null query. Sanitization replaces C's explicit illegal-character set,
rather than every invalid byte; formatting/update rejection preserves the old
identity. Arbitrary callback mutation of C struct fields/descriptors and unsafe
cached-sibling list manipulations are outside the recorded replay domain.

## Testing and verification

| Evidence family | Successful records | Scenarios | Separate probes |
| --- | ---: | ---: | ---: |
| fsm_core (6a) | 1,506 | 64 | 7 |
| fsm_timer (6b) | 9,369 | 1,080 | 5 |
| fsm_tree (6c) | 2,010 | 134 | 5 |
| fsm_registry (6d) | 1,524 | 127 | 2 |
| fsm_shared (integration) | 443 | 21 | 0 |

The 14,852 records freeze callback-time state, timer number/pending/deadline,
terminating flags, ID/name lookup visibility, registry removal, hierarchy links,
list order, notification data and operation results. Shared-scheduler evidence
covers multiple FSM timers, peer cancellation/termination, callback-created
timers, zero-delay rearming, dispatch from timeout callbacks, and TDef updates
from an application timer. Registry evidence covers all ASCII input bytes 1–127,
validation/sanitization, rejected-update preservation, duplicate lookup, slot
reuse, nullable state names, unregistration and allocation afterward.

The five direct-call runners compile required unmodified local C sources and
real dependencies, with no observed-API stubs. They verify 39 project files,
149 platform dependencies including libtalloc, transitive includes, compiler
identity and generator hashes. Runtime library search uses the checked libtalloc
directory; compiler/library search overrides are cleared. Revision is `unknown`
for the local snapshot without Git metadata. Logging infrastructure has process
lifetime, with no targets. ASan+UBSan successful-domain audits pass; leak checking
is disabled under ptrace. Unsafe probes are bounded isolated subprocesses.

The five sibling runners directly compile unchanged local C sources and real
dependencies with no observed-API stubs. Each evidence increment passed successful
ASan+UBSan audits and bounded isolated unsafe probes, then two independent golden
and diagnostic generations matching the frozen artifacts. The sibling validates
schema, fingerprint, provenance and corruption rejection; this crate replays
behavior. Prior time/TDef/accounting/counter observations were retained.

Repository tests are `tests/fsm/fsm_{core,timer,tree,registry,shared}_reference_vectors.rs`;
fixtures are under `tests/compatibility/vectors/fsm_*`. Exact generation/compare
commands live in the repository test guide and sibling reference README. Targeted
runners use `--verify`/`--audit`; source/generator mismatches fail closed. Ordinary
Cargo tests require no C build. The library overview records the current complete
suite and independent-package validation, distinct from the historical stage runs.

## Remaining work and fundamental differences

The selected FSM scope and shared timer/TDef integration are complete.
Global C registry/logging hooks, VTY/CTRL, statistics integrations and native
select/timerfd APIs are separate remaining families. Arbitrary talloc ownership/
destructors, pointer-address naming, exact C logging levels/source locations/output,
non-UTF8 C strings and unsafe direct mutation of C descriptor/instance fields
are outside the Rust contract.

Rust owns definitions/private data and uses generation-checked handles rather
than exposing mutable instance pointers across nested callbacks. C undefined
hierarchy/traversal/mask-shift calls are diagnosed separately and become typed
Rust errors, not golden outputs. Both safe and legacy observable termination
orders are supported with safe storage reclamation. Rust formatting replaces
printf syntax without claiming variadic/locale compatibility. Callback panics
propagate after restoring traversal guards; they do not roll back application
mutations or finish interrupted cleanup.

## Structured diagnostics

Enable `tracing` for `osmocore_rs::fsm` events and execution spans covering
dispatch, committed/completed transitions, instance retirement, parent changes,
timeouts and termination. Rejections and stalled child termination use structured
categories; existing termination panic boundaries report before resuming unwind.
Payloads/private data are omitted and need no `Debug` bound. See the
[shared diagnostics contract](../diagnostics/README.md), including partial transitions
and application ownership of subscribers/filters/output.
