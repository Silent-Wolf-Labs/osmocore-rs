# Rate counters and groups

[Library overview](../README.md). Implementation: [arithmetic](counter.rs),
[descriptors/groups/handles](group.rs), [registry/rollups](runtime.rs),
[monotonic driver](driver.rs), [public exports](mod.rs).

## Implemented scope

Stages 5a and 7 cover standalone arithmetic/reset and the selected owned group,
registry and interval-rollup behavior. Use `RateCtr::{add,inc,difference,reset}`
directly; the four redundant C-named forwarding functions were removed.
`RateCtrPerIntv` and interval constants expose second/minute/hour/day last/rate
storage. `RateCtr` implements `time_accounting::CounterSink`; groups and periodic
processing belong to `RateCtrRuntime` and `RateCtrPeriodicDriver`.

Reference: `src/core/rate_ctr.c`, identifier validation in `utils.c`, and
`include/osmocom/core/rate_ctr.h`. Select sources participate in native C evidence
only; their inclusion does not implement a Rust select/timerfd subsystem.

## Counter arithmetic

Signed `i32` additions convert to unsigned 64-bit increments and wrap modulo
2^64, including negative increments and INT_MIN. Add and inc change only
`current`. Difference performs unsigned wrapping subtraction, interprets the
result as the signed two's-complement value observed with the pinned GCC/Linux
compiler, then assigns `previous = current`; interval fields remain unchanged.
High-bit unsigned-to-signed conversion is implementation-defined in C: this
claim is tied to the recorded compiler/ABI, rather than a portable C guarantee.
For example, `(current, previous) = (2^63, 0)` returns INT64_MIN;
`(UINT64_MAX, 0)` returns -1; `(0, UINT64_MAX)` returns 1.
Reset clears current, previous and every last/rate field.

## Owned groups and registry

`RateCtrGroupDesc` owns its prefix, optional description, signed `class_id` and
counter descriptors. `RateCtrRuntime::allocate()` validates and mangles names,
allocates zeroed counters and registers the group newest first. `group()` and
`group_mut()` access owned groups through `RateCtrGroupId`; group methods expose
immutable effective descriptors, counter lookup/access and reset. Runtime
methods provide lifecycle, name/index changes, group lookup, add/inc, iteration
and periodic processing. `unregister()` is an owned convenience corresponding
to an explicit C list unlink, rather than another upstream exported function:
it keeps the group alive but removes it from registry lookup and rollups.

Recorded field types are distinct: C `unsigned int num_ctr`, group `idx` and
counter indices are unsigned 32-bit; C `int class_id`, increments and callback
returns are signed 32-bit. Current/previous, interval last/rate and timer ticks
are unsigned 64-bit; difference returns signed 64-bit. Rust derives `num_ctr`
from owned storage after checking it fits the C-width count. Private slot
positions use `usize` as Rust storage positions, not as C group indices.

A handle combines runtime identity, slot and generation. Changing its group's
index preserves identity; free and slot reuse invalidate the old handle.
Foreign or stale handles return `InvalidHandle`; counter bounds are checked.
Indexes need not remain unique: `set_index()` mirrors the unchecked C update.
Lookup returns the first match in newest-first registry order. Duplicate
allocation checks the original prefix before descriptor mangling. The unused
index calculation reproduces the C ordered unsigned wrapping scan, including
`UINT_MAX`; it is not replaced by a maximum over a set or a hole-filling search.

## Descriptor and name ownership

Direct C observations compare pointer identity and mutate live caller-owned
backing storage after allocation, without ending its lifetime or changing
allocated counter bounds. A valid descriptor is retained by pointer: its
counter descriptor array, names and descriptions remain aliased. Changes to
the original prefix, counter name, descriptions and class ID become visible.
When mangling is needed, C copies the group descriptor and counter descriptor
array. It copies strings containing dots and changes those dots to colons;
unchanged strings and descriptions still alias caller storage. The observations
show copied dotted names remaining fixed while unchanged names/descriptions
reflect mutations. `rate_ctr_group_set_name()` separately copies its input;
mutating caller report-name storage does not change the stored group name.

Rust deliberately owns every descriptor string and fixes the counter storage
shape after allocation. It does not retain caller aliases or expose mutable
metadata/descriptor arrays. This avoids C dangling descriptors, changed array
bounds and externally invalidated identifiers. Changing a caller's descriptor
copy cannot change an allocated Rust group. Optional descriptions support C
null metadata; Rust metadata is UTF-8, whereas C can retain arbitrary byte
strings. Identifier validation accepts the recorded printable 7-bit ASCII set,
with C's explicit reserved characters excluded and only dots mangled to colons.
Embedded NUL strings are rejected instead of being truncated as C strings.
Allocation and string-copy exhaustion use Rust's allocator behavior rather
than a claim of matching C talloc failures.

## Iteration and mutation

Group callbacks receive mutable runtime access and a stable handle. Counter
callbacks additionally receive the unsigned counter index; descriptors and
counter references can be retrieved for the duration of an individual access.
Nested lookup/iteration, current index/name changes, counter reset, freeing or
unregistering the next group, and allocating a new group are covered by direct
C evidence. Allocation inserts at the head and does not enter the current
traversal. Counters visit descriptor order. Both iterator methods preserve a
positive final callback return and stop immediately on a negative return.

The C group loop is not a safe iterator. Freeing the current group and then
continuing yields an ASan use-after-free. Unlinking the current node with
`llist_del()` dereferences list poison; `llist_del_init()` produces a bounded
nontermination probe. Freeing the current group followed by a negative callback
return is defined and observed because traversal returns before touching it.
Rust permits that early return. Attempting to continue after current-group
free/unregister returns `CurrentGroupRemoved`, retaining the completed callback
mutations. Free/reallocation of the same slot still invalidates the old current
handle. Counter iteration likewise reports removal before continuing. Callback
panics propagate through ordinary Rust unwinding; no C panic-parity claim is
made. Raw null descriptors and out-of-bounds counter helpers remain isolated C
failure diagnostics and are excluded by Rust ownership/bounds checks.

## Tick processing and monotonic driver

The runtime owns a wrapping unsigned tick phase independently of group
lifetimes and counter resets. Each expiration increments ticks first, updates
every registered counter's second interval, then updates minute/hour/day pairs
on divisibility by 60/3600/86400. Rates are wrapping current-minus-last deltas,
not values divided by the interval length. Tick wrap to zero expires all four
intervals. Difference calls update `previous`; rollups never touch it.

`process_expirations()` processes every missed expiration separately, matching
the C callback. Later catch-up ticks can overwrite a second rate with zero.
A zero count returns `ZeroExpirations` without mutation; the C do/while count
underflows and is recorded as a bounded timeout. Groups allocated mid-period
join the existing global phase and start with zero interval storage.

`RateCtrPeriodicDriver<C>` provides explicit one-second monotonic polling using
the existing injectable `Clock`. `start()` is idempotent, `update()` delivers
accumulated expirations, and `stop()` affects only the driver, preserving runtime
phase/counters. The first deadline is one second after start. No wall-clock
reads, background threads, general scheduler changes or async runtime are
required. Applications may instead supply positive expiration counts from their
own event loop. Invalid times, clock failures, backwards monotonic readings and
deadline overflow return typed errors before rollup/driver-state mutation.

Native C observations characterize one-second `CLOCK_MONOTONIC`/nonblocking
setup and repeated initialization. Setup failures propagate; a scheduling
failure still returns zero and leaves the fd registered, so a later init skips
rescheduling. These quirks are evidence about C, not the contract of Rust's
portable driver or an implementation of native `rate_ctr_init()`.

## Testing and verification

Standalone evidence under `tests/compatibility/vectors/rate_ctr/` contains a TSV
metadata row and 1,700 calls across 100 scenarios. Every call records increment,
signed return, current/previous and all eight interval words before/after.
The 10-by-10 initial matrix spans zero/small values, INT_MAX/INT_MAX+1,
INT64_MAX and adjacent unsigned boundaries through UINT64_MAX. Repeated
differences, negative/zero/positive increments, INT_MIN, wrap, reset and retained
interval fields are covered. Four isolated NULL-pointer sanitizer probes remain
diagnostics; Rust references exclude these calls.

The standalone runner compiles unmodified local rate-counter code and real
inline increment. LTO/section garbage collection removes unused dependencies
without API stubs. It pins transitive project/platform includes, compiler/target
and source/generator hashes, clears search overrides and records unknown revision.
Successful ASan+UBSan audits, two golden and diagnostic generations and shared
artifact comparisons passed. Leak checking was disabled under ptrace.

`tests/rate_ctr/rate_ctr_reference_vectors.rs` replays primitive methods/sinks;
`rate_ctr_group_reference_vectors.rs` replays group/registry/rollup behavior.
A separate `TimeCc<RateCtr>` replay covers all 5,947 existing accounting records
across 469 scenarios, matching current while preserving previous/interval fields,
including disconnected sinks. Shared readers independently validate canonical
encoding, coverage/order/state continuity, all outputs, provenance and corruption.

The sibling `reference/rate_ctr/rate_ctr_group_vector.c` includes the unmodified local
`src/core/rate_ctr.c` to access static ticks and the actual callback. It calls
real allocation/registry/iteration/reset functions, supplies initialized seed
state, and delivers expiration counts through a pipe to the callback's actual
read. OS-boundary wrappers inject timerfd creation/scheduling failures; the
observed native helpers and select path are real local sources. Native probes
run in separate processes to isolate initialization globals, and a bounded real
one-second timerfd/select smoke check confirms dispatch. No rollup algorithm
or observed API is replaced in the harness.

The closed runner pins 47 local source/header files, 227 platform headers or
linked library files, compiler identity, exact compilation/audit/link flags and
preprocessor defines. The full macro dump is generator-pinned and compared on
verification. Stats-TCP, stat-item, socket and io_uring sources are build
requirements of the real select path; linking them does not extend the Rust
conversion claim. Successful-domain source paths are ASan+UBSan instrumented;
external system libraries are content-pinned but not sanitizer-instrumented.
Leak detection is disabled under ptrace, as in the earlier runners. The
reference revision remains `unknown`.

The dedicated `rate_ctr_group/` fixture contains 1,915 records in 608 scenarios,
with before/after tick, complete registry order, effective group/counter
metadata, all ten counter words, return values and callback traces. All 1,911
selected-behavior records are replayed in Rust. Four ownership records are
validated separately as C aliasing/copy observations. Native initialization
has its own three-row TSV; seven isolated failure/nontermination probes have
separate JSON. Full prefix and counter-name byte matrices cover bytes 1–255.
Zero counters, invalid descriptors, duplicate/wrapped indices, mangled-name
collisions, iteration mutations, boundaries and 86,401-expiration catch-up are
included. Each scenario explicitly resets the registry and tick input; native
and unsafe probes use independent processes.

The shared independent reader checks canonical encoding, record shape,
chronology, state continuity, frozen inputs/coverage, full artifact fingerprints,
provenance and diagnostics. Both repositories validate corruptions; only this
repository replays Rust behavior. Fixtures, reader and manifests are shared
byte-for-byte, and ordinary Rust tests need no C build. See the sibling reference README and repository test guide
for targeted verification/regeneration commands. No Stage 5a observations,
vendored snapshot or C reference files are changed.

Stage 7 validation passed source verification, ASan+UBSan successful-domain
and isolated-diagnostic checks, two successful/diagnostic/native generations,
shared-artifact comparisons and both Rust suites. The native one-second smoke
check confirmed real timerfd/select dispatch; it is characterization of C, not a
Rust native-driver claim. The library overview records current full-suite results.
Normal Rust tests need neither C nor the sibling project. Targeted runners are
`reference/rate_ctr/run_rate_ctr_vector.sh` and `run_rate_ctr_group_vector.sh` in the
sibling; full regeneration commands remain in its reference README/test guide.

## Remaining work and fundamental differences

The selected arithmetic/group/rollup scope is complete. Native
`rate_ctr_init`, osmo_fd/select/timerfd, stat_item, separate legacy counter.h,
stats reporters/TCP, VTY and logging APIs remain separately scoped gaps.

C uses global registry/tick state and retained descriptor pointers. Rust owns
strings/counter shapes inside explicit runtimes and preserves identity with
owner/slot/generation handles; cloned handles do not keep groups alive. Mutable
C metadata aliasing is recorded but deliberately not reproduced. Printable ASCII
identifier rules/dot mangling and UTF-8 metadata have explicit string boundaries;
embedded NUL is rejected instead of truncating. Rust allocation follows its
allocator policy rather than C talloc failure/NULL semantics.

Defined wrap, duplicate selection, traversal order and callback mutation are
preserved. Unsafe continuing traversal after current removal becomes
`CurrentGroupRemoved`; negative early return after removal remains valid. The
portable monotonic driver has explicit start/update/stop and no global event loop,
background thread or async dependency. Recorded 64-bit long/unsigned long,
32-bit int and compiler-specific unsigned-to-signed interpretation are parity
boundaries, not a portable C or all-platform claim.

The optional I/O runtime composes the existing periodic driver through a typed
`RateDriver` projection. `next_deadline()` reads the stored monotonic deadline
without changing start phase or missed-interval catch-up.
