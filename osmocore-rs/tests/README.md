# Integration and conformance tests

## Compressed fixtures

Fixtures of 5 MB or more are stored gzip-compressed as `*.gz`. Tests read them
through `support/gzip_fixture.rs`, and fingerprints and hashes apply to the
decompressed text, so compression doesn't change the evidence. To review a changed
fixture as text, run `git config diff.gz.textconv "gzip -dc"` once in each clone.

## Test-location convention

- Keep unit tests that need private implementation access beside the code in
  `src/`, using `#[cfg(test)]` modules (or a local `tests.rs` for larger suites).
- Put public API contract tests spanning subsystems in `tests/public_api.rs`.
  Import `osmocore_rs` as an external consumer; these tests must not rely on
  crate-private access.
- Put subsystem public API, conformance and consumer tests in
  `tests/<subsystem>/`, registered by that directory's `main.rs`.
- Keep shared fixture readers and capture helpers in `tests/support/`, and
  frozen C observations in `tests/compatibility/vectors/`.
- Put tests requiring process isolation under their owning subsystem, such as
  `tests/net/diagnostics/main.rs` and `tests/runtime/diagnostics/main.rs`.
  Register these explicitly in `Cargo.toml` as separate test executables; do not
  also register them as modules in the subsystem's `main.rs`. Scoped tracing
  subscribers need this isolation from uninstrumented test threads.
- Keep runnable documentation examples in Rustdoc. In particular,
  `src/diagnostics/README.md` owns the diagnostics reference and runnable subscriber
  example, included as module-level Rustdoc by `src/diagnostics/mod.rs`; conceptual architecture and compatibility guidance belongs in `docs/README.md`.

Choose the location by the boundary being tested before adding tests. After
moving tests, compare `cargo test --all-features -- --list` output to verify
that every test remains registered exactly once.

Cargo discovers `main.rs` in each functional directory as an integration test
entry point. They register the replay and safety test modules in their
respective directories. These tests call the crate's public APIs and compare their
results with committed observations from the C `libosmocore` implementation.
The JSON files in `compatibility/vectors/` are the expected C results, including buffer
contents where those are part of the API contract.

| Cargo target | Responsibility |
| --- | --- |
| `public_api` | Cross-subsystem public API contracts and compatibility distinctions |
| `utilities` | Base64, BCD, bit packing, bitvec, hex parsing, PRBS, primitive metadata and CRC |
| `gsm0502` | GSM 05.02 frame arithmetic, block mapping, hopping and paging groups |
| `gsm0503` | GSM 05.03 xCCH code tables, interleaving, burst mapping, encoding and decoding |
| `a5` | A5/0, A5/1 and A5/2 cipher streams and frame counts |
| `comp128` | COMP128 v1, v2 and v3 and XOR-2G SRES and Kc |
| `gsm_utils` | Bands, power, ARFCNs, GSM time, reduced frame numbers, TLLIs, classmarks, RACH parameters and 7-bit coding |
| `sockaddr_str` | IP addresses as text: family, conversions both ways, C's errors and comparison |
| `strrb` | Ring of recent strings, its length limit and C's slot and index results |
| `use_count` | Use counters, entry order and reuse, callbacks and text |
| `tlv` | TLV parsing, encoding, `Msgb` methods, shifting and protocol validation |
| `conv` | Convolutional reference validation, encoding, decoding and correlation |
| `fsm` | FSM core, timers/TDef integration, trees, registry and shared scheduling |
| `msgb` | Message-buffer primitives, layers, resizing and adapters |
| `rate_ctr` | Standalone counters and counter groups |
| `time` | Time/clock helpers, timers, TDef and accounting with concrete counter sinks |
| `timer_diagnostics` | Isolated timer event-count/filter/context assertions in `time/diagnostics/` |
| `gsmtap` | GSMTAP constructor parity, direct Msgb encoding and generic UDP/runtime integration |
| `gsmtap_diagnostics` | Isolated preparation trace/privacy assertions in `gsmtap/diagnostics/` |
| `runtime` | Feature-gated I/O lifecycle, typed commands/drivers, FIFO/backpressure, fairness and acceptance service |
| `runtime_diagnostics` | Isolated runtime tracing/privacy/span assertions in `runtime/diagnostics/` |
| `net` | Optional real TCP/UDP loopback setup, option/error/cleanup contracts and normalized C socket observations |
| `net_diagnostics` | Optional network target/field/span assertions in `net/diagnostics/`, in a separate executable |

Shared independent readers are stored in `support/`, with no standalone target
or blanket support module. Each entry point selectively declares its required
readers through `#[path = "../support/…"]`. Local fixture declarations in replay
modules use the same path prefix. Crate-root `bitvec_fixture` and `msgb_fixture`
aliases preserve the dependent readers' imports; nested fixture module names
preserve validator test identities. Keep each reader registered exactly once in
its consuming target, and run `cargo test -- --list` with the crate manifest to
verify discovery and test multiplicity after registration changes. For example,
`cargo test --manifest-path osmocore-rs/Cargo.toml --test time` runs the time
integration target from the repository root.

The C reference harnesses live in the sibling `gsmtap-rs/reference/` project.
They generate the fixtures; ordinary `cargo test` runs only Rust code and reads
the committed JSON, so it does not require a C library or the harness project.

Focused Rust unit tests live in `#[cfg(test)]` modules beside the implementation
in `src/`. Those check Rust behavior directly. The tests here establish
compatibility with the C reference at the public API boundary.

## Current validation

All eight combinations of the independent `tracing`, `net` and `runtime` features
are supported. Run `cargo test` with each selected combination for current counts.
The [diagnostics guide](../../docs/README.md#features-and-structured-diagnostics) explains the
single-dispatcher callsite cache issue and timer test isolation.

Focused evidence:

- GSMTAP Vec/Msgb bytes replay all seven frozen C cases. Owned encoding and
  runtime integration tests check capacity boundaries, connected/unconnected
  sends, empty payloads and rejection ownership. Isolated preparation diagnostics
  check fields, filtering, context and endpoint/payload privacy.
- Networking tests on Linux include six normalized direct-C observations,
  reader corruption validation, IPv4/IPv6 loopback, options, source binding,
  timeout/error phases and cleanup. Its isolated diagnostics target checks fields,
  levels, application context and endpoint privacy.
- FSM and isolated timer diagnostics tests check typed fields,
  filtering, activation context, hierarchy, deferred errors, cancellation/release,
  callback re-arming and panic payload propagation. No formatter text is asserted.
- Runtime tests in `runtime/boundaries.rs` check every setting, quota, budget and
  rejection, each callback and driver failure, and connection resets. The remaining
  uncovered runtime code needs OS failures or an active `tracing` subscriber.
- Runtime tests combine scripted output/poll/wake failures with loopback,
  mixed clocks, cooperative fairness and acceptance subprocess failure paths.
  [Focused behavioral tests](../../docs/README.md#acceptance-evidence)
  separately establish accounting, catch-up, partial output and drain timeout.

C harnesses and recorded observations are the compatibility evidence.

## Preferred API and model checks

Frozen evidence also checks preferred bitvec and message-buffer methods,
CRC whole-slice functions and descriptor methods, primitive construction and
ordinary event lookup, bounded timeout tables/range helpers, and owned GSMTAP
encoding. Base64 replays 21 applicable frozen cases through its preferred buffer
or owned APIs; BCD replays 39 cases within the owned API's domain. The original
C adapters still cover distinct contracts. Rust Base64 encoding omits a NUL and
empty decoding reports zero; owned BCD does not truncate or accept embedded NUL;
ordinary primitive maps exclude C's terminating entry; complete Rust hexdumps
are compared only where C's segment/output limits do not truncate them. These
are explicit domain mappings, not new C observations.

Deterministic Rust-only checks include 16,875 three-operation message-buffer
sequences against an independent head/live/tail partition model, 2,560 bitvec
steps against a boolean model, repeated copy/resize/reset ownership checks,
complete hexdumps beyond C's segment limit, Base64 round trips across all byte
values and CRC streaming at every split against bitwise polynomial division.
Consumer tests compose bit fields, CRC, message storage and GSMTAP; borrow
primitive messages and enqueue independent copies; account active periods into
`RateCtr`; and check 1,024 timer operations against a deadline-order model with
released-handle rejection. The models preserve partial bit writes, reverse
expired-timer dispatch and the final non-counting wakeup after accounting pauses.

## Coverage

Coverage isn't part of CI. To measure lines and branches locally, from
`osmocore-rs/`, with [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)
and the `llvm-tools` component installed on a nightly toolchain:

```bash
cargo +nightly llvm-cov --branch --all-features --locked --html
```

Branch coverage needs nightly. Reported totals count each generic instantiation
separately, so a branch can look missed in one instantiation and be covered in
another. Coverage shows what ran, not what the tests assert.

## Bit-vector primitives

`utilities/bitvec_reference_vectors.rs` compares the bit-vector primitives with all 5,944
direct C calls in `compatibility/vectors/bitvec_vectors.json`. Cases cover all
four bit values, every bit position across zero to three bytes, byte boundaries,
three initial patterns, out-of-range positions/cursors (including `UINT_MAX`),
empty writes, partial bulk writes, cursor advancement, zeroing, and capacity
helpers. Every case records and checks the entire four-byte backing array,
including suffix bytes outside the vector's declared length.

The shared `support/bitvec_fixture.rs` validator freezes case order, API arguments,
initial bytes, JSON schema/types, count, unsigned-int width, and pinned source,
header, and upstream test hashes. `gsmtap-rs` uses an identical copy for fixture
integrity checks. Expected outcomes come only from C. Tailroom comparisons now
match C's 32-bit wraparound directly, without a special exception. The separate
checked helper rejects invalid cursors. Unit tests additionally cover
`usize::MAX` positions and empty Rust slices.

From the repository root, check fresh observations without rewriting fixtures:

```bash
../gsmtap-rs/reference/bitvec/run_bitvec_vector.sh > /tmp/bitvec.fresh.json
cmp /tmp/bitvec.fresh.json osmocore-rs/tests/compatibility/vectors/bitvec_vectors.json
cmp /tmp/bitvec.fresh.json ../gsmtap-rs/tests/vectors/bitvec/bitvec_vectors.json
cmp osmocore-rs/tests/support/bitvec_fixture.rs ../gsmtap-rs/tests/support/bitvec_fixture.rs
```

Review intentional fixture changes before copying the regenerated C output
into both locations. Normal tests consume these committed observations.

## Bit-vector integers, fill/padding, and bytes

`utilities/bitvec_buffers_reference_vectors.rs` compares eight additional APIs with
38,837 observations in `compatibility/vectors/bitvec_buffers_vectors.json.gz`.
Coverage includes zero/full/oversized widths, partial reads/writes, L/H mode,
32-bit signed numeric results, `set_uint`'s count narrowing (256 becomes zero),
wrapping fill and inclusive padding counts, all byte alignments, and complete
state after capacity errors. Each call records the eight-byte backing array
and a six-byte transfer array, including unchanged neighbors.

The shared `support/bitvec_buffers_fixture.rs` validator freezes input arguments,
case order/count, types, provenance, and initialized buffers. It uses
`support/bitvec_fixture.rs` for strict record parsing; both copies must match their
counterparts in `gsmtap-rs/tests/support/`. Corruption tests cover malformed
fixtures and a validly typed incorrect outcome.

The C input manifest excludes undefined integer shifts and scalar reads beyond
the declared vector length. Rust tests separately check those slice and shift
errors, and arithmetic outside the C argument domain. Widths exceeding 32 in
`get_uint` remain valid when all bits needing a larger shift are zero, or when
C encounters a capacity error before such a shift. Expected outcomes are
observations of the local C build, not a Rust model.

```bash
../gsmtap-rs/reference/bitvec/run_bitvec_vector.sh --buffers > /tmp/bitvec_buffers.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/bitvec_buffers_vectors.json.gz | cmp /tmp/bitvec_buffers.fresh.json -
gzip -dc ../gsmtap-rs/tests/vectors/bitvec/bitvec_buffers_vectors.json.gz | cmp /tmp/bitvec_buffers.fresh.json -
cmp osmocore-rs/tests/support/bitvec_buffers_fixture.rs ../gsmtap-rs/tests/support/bitvec_buffers_fixture.rs
```

## Bit-vector indexed operations and helpers

`utilities/bitvec_operations_reference_vectors.rs` compares eight more APIs with
38,643 observations in `compatibility/vectors/bitvec_operations_vectors.json.gz`.
The shared `support/bitvec_operations_fixture.rs` validator freezes inputs,
order/count, numeric/string types, source provenance, initial backing bytes,
array arguments, return fields, and index/cursor state constraints. C field
reads also record errno, mapped to explicit Rust overflow errors; the Rust API
does not set POSIX errno. Every case compares the entire eight-byte backing
array, including storage beyond the declared vector length.

Field cases include zero-width cursor repositioning, independent initial
cursor/index, exact/short capacity, 64-bit results, byte-count narrowing and
index wrapping. Search cases preserve C's zero/-1 sentinels and L/H behavior.
Run-length cases cover every in-range cursor, partial limits, invalid cursors,
wrapping return counts and defined early returns with oversized limits. Shifts
capture C's retained tail bytes and skipped penultimate byte. Array cases cover
empty inputs, dry-run wrapping estimates, ignored setter errors and partial
writes. Frozen C inputs exclude undefined reads/shifts; Rust unit tests cover
those hazards separately.

From the repository root:

```bash
../gsmtap-rs/reference/bitvec/run_bitvec_vector.sh --operations > /tmp/bitvec_operations.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/bitvec_operations_vectors.json.gz | cmp /tmp/bitvec_operations.fresh.json -
gzip -dc ../gsmtap-rs/tests/vectors/bitvec/bitvec_operations_vectors.json.gz | cmp /tmp/bitvec_operations.fresh.json -
cmp osmocore-rs/tests/support/bitvec_operations_fixture.rs ../gsmtap-rs/tests/support/bitvec_operations_fixture.rs
```

The shared strict parser `bitvec_fixture.rs` must also remain identical between
repositories. Regenerated C observations require review before replacing both
fixture copies. Normal Rust tests read committed observations only.

## Bit-vector buffer and text adapters

`utilities/bitvec_adapters_reference_vectors.rs` compares pack/unpack, hex loading,
and formatting with 6,757 observations in
`compatibility/vectors/bitvec_adapters_vectors.json.gz`. The shared
`support/bitvec_adapters_fixture.rs` validator freezes inputs/order/count, types,
source provenance, and all backing, source and transfer buffers. In addition
to bitvec hashes, the runner pins the `utils.c`, `utils.h`, and upstream utils
test hashes verified against the same revision, because unhex calls hexparse.

Copy cases cover zero through eight bytes, exact and oversized transfer slices,
and arbitrary cursors. Hex cases cover malformed digits, odd nibbles, embedded
NULs, whitespace, case, capacity errors, and every possible single input byte.
Formatting covers every valid cursor through eight bytes over eight byte
patterns, exact/oversized outputs, leading byte separators, partial bytes, and
NUL terminators. Comparisons include entire eight-byte backing and eighty-byte
transfer arrays, preserving guards and unmodified suffixes.

Rust safety tests separately cover short copy/format buffers, invalid cursors,
empty slices and source strings ending at the Rust slice boundary. Formatting
validates the actual `cur_bit + ceil(cur_bit / 8) + 1` output size, including
spaces omitted in C's comment, before writing. No unsafe C calls are used to
generate those error expectations.

From the repository root:

```bash
../gsmtap-rs/reference/bitvec/run_bitvec_vector.sh --adapters > /tmp/bitvec_adapters.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/bitvec_adapters_vectors.json.gz | cmp /tmp/bitvec_adapters.fresh.json -
gzip -dc ../gsmtap-rs/tests/vectors/bitvec/bitvec_adapters_vectors.json.gz | cmp /tmp/bitvec_adapters.fresh.json -
cmp osmocore-rs/tests/support/bitvec_adapters_fixture.rs ../gsmtap-rs/tests/support/bitvec_adapters_fixture.rs
```

Keep the shared parser identical as well, and review C-generated changes before
replacing both copies. Normal Rust tests consume committed observations only.

## BCD vectors

`bcd_reference_vectors.rs` checks all four public BCD conversions against
`compatibility/vectors/bcd_conversion_vectors.json`. The fixture records C source bytes,
including string terminators, nibble offsets, flags, initialized destination
bytes, return values, and the complete final destination. The runner checks
the local C source, header, and C test file hashes before generating it.

From the repository root, regenerate the fixture with:

```bash
../gsmtap-rs/reference/bcd/run_bcd_conversion_vector.sh > osmocore-rs/tests/compatibility/vectors/bcd_conversion_vectors.json
```

Review any fixture diff before accepting a regeneration. The normal Rust test
suite reads the committed fixture and does not need the sibling project.

## Bit-packing vectors

`bit_packing_reference_vectors.rs` compares both public bit-conversion APIs
against every C observation in `compatibility/vectors/bit_packing_vectors.json`. The fixture
records all 9,016 calls, complete source and destination buffers, return
values, and pinned C provenance, including C's zero-bit unpacking behavior.
The runner verifies hashes of `bits.c` and `bits.h` before generating output.
The conformance test requires the frozen case names and order, exact fields,
and integer/string types; unexpected or malformed records fail rather than
being skipped.
The Rust API reports safe slice-capacity errors when a call lacks backing
storage; those cases are covered by unit tests rather than unsafe C calls.
The unit tests also cover empty zero-bit packing, zero-bit unpacking errors,
maximum `usize` counts, and raw nonbinary input shifting.

Regenerate the fixture from the repository root with:

```bash
../gsmtap-rs/reference/bits/run_bit_packing_vector.sh > osmocore-rs/tests/compatibility/vectors/bit_packing_vectors.json
cmp ../gsmtap-rs/tests/vectors/bits/bit_packing_vectors.json osmocore-rs/tests/compatibility/vectors/bit_packing_vectors.json
```

The test suite consumes the committed fixture and needs no C build.

## Extended bit-conversion vectors

`bit_packing_ext_reference_vectors.rs` compares `osmo_ubit2pbit_ext()` and
`osmo_pbit2ubit_ext()` against all 63,384 observations in
`compatibility/vectors/bit_packing_ext_vectors.json.gz`. The copied fixture must match the
testing project's `tests/vectors/bits/bit_packing_ext_vectors.json.gz` byte for
byte. It uses the same pinned C revision and `bits.c`/`bits.h` hashes as basic
packing and records a 32-bit C unsigned-int ABI.

The conformance test validates the exact schema and numeric/string types,
frozen names/order/count and input construction, full source and initialized
destination bytes, API and mode arguments, provenance, and C integer width.
Return counts and every destination byte are compared to direct C observations;
no expected output is generated by the Rust input manifest. Negative and other
nonzero C modes map to Rust's `lsb_mode = true`. Malformed records and normalized
zero-count returns are rejected.

Coverage includes independent offsets, byte crossings, MSB/LSB ordering,
nonbinary truthy input, source neighbors, partial-byte destination preservation,
and zero-count calls. Rust unit tests separately check empty/undersized slices,
offset-addition overflow, error precedence, and unchanged destinations on
failure. Zero-count tests accept arbitrary offsets without accessing storage.

From this repository root, verify fresh observations without rewriting either
fixture:

```bash
set -e
../gsmtap-rs/reference/bits/run_bit_packing_ext_vector.sh > /tmp/bit_packing_ext.fresh.json
gzip -dc ../gsmtap-rs/tests/vectors/bits/bit_packing_ext_vectors.json.gz | cmp /tmp/bit_packing_ext.fresh.json -
gzip -dc osmocore-rs/tests/compatibility/vectors/bit_packing_ext_vectors.json.gz | cmp /tmp/bit_packing_ext.fresh.json -
```

For an intentional regeneration, review the fixture diff before accepting it.
Ordinary Rust tests consume the copied fixture and do not require C or gsmtap-rs.

## Soft-bit vectors

`compatibility/vectors/bits/soft_bits.ndjson` records C's `osmo_ubit2sbit` on every byte
value and `osmo_sbit2ubit` on every soft bit from -128 to 127.
`utilities/soft_bits_reference_vectors.rs` checks `bits::to_soft` and `bits::from_soft`
against each. Regenerate it from `gsmtap-rs` with
`reference/bits/run_soft_bits_vector.sh`.

## GSMTAP constants

`compatibility/vectors/gsmtap_constants/gsmtap_constants.ndjson` records the version, UDP
port, message types and channel types in C's `gsmtap.h`, by name.
`gsmtap/constants_reference_vectors.rs` checks that `gsmtap`'s constants match them, one
for one and in order. Regenerate it from `gsmtap-rs` with
`reference/gsmtap/run_gsmtap_constants_vector.sh`.

## Message-buffer storage

The dedicated `compatibility/vectors/msgb_vectors.json.gz` contains 3,695 direct C
operation records, with full backing bytes before/after, data/tail offsets,
length/capacity, layer markers, control words, destination/channel presence,
query results, and normalized panic/error outcomes. The fixture is identical to
the sibling `gsmtap-rs/tests/vectors/msgb/msgb_vectors.json.gz`.
The shared `support/msgb_fixture.rs` validator is identical to the
sibling's `tests/support/msgb_fixture.rs`. Tests freeze schema, inputs,
order, provenance, ABI, state consistency, sequence continuity, and corruption
rejection. Normal Cargo tests consume committed JSON without building C.

The sibling's `reference/msgb/msgb_vector.c` calls the actual APIs, including inline primitives.
`reference/msgb/run_msgb_vector.sh` pins msgb source/header/upstream tests and panic
source/header, rebuilds the required local core library, and selects it at link
and runtime. All five files were compared byte-for-byte with upstream revision
`950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-08. The runner contains
the verified SHA-256 hashes. It fails on missing/mismatched source or unsupported
ABI instead of falling back. The ABI is 8-bit bytes, 32-bit unsigned int,
64-bit unsigned long, and 64-bit pointers.

Coverage includes all offsets/lengths for capacities 0, 1, and 2; selected
zero/half/full offsets and lengths for capacities 8 and 16; zero/exact/excess
arguments; nonzero headroom; trim extension/shrinkage; guarded panic cases;
all optional-layer combinations; reset preservation; reuse sequences; and
65,535-byte capacity. Every before/after state directly queries data, length,
headlen, headroom, tailroom, and invariant. The operation-record count excludes
setup calls and these queries. Head is always zero; pointers are offsets or
null (`-1`). Destination/channel fields record presence, never addresses.
A controlled panic handler uses setjmp/longjmp as in upstream tests.
Void/allocation/invariant observations use return sentinel `-1`; `panic`
distinguishes aborts. Allocation snapshots record actual C zero initialization.

No unsafe C trim with headroom, nonempty reserve, invalid offsets, or NULL
access is used to generate evidence. Rust-only safety tests live in the
conversion repository. That crate owns fixed-capacity storage up to 65,535,
returns checked errors and pointer offsets, and preserves C's wrapping headlen
and reset behavior (L1 and bytes survive; L2–L4 and control words clear).
It checks trim against remaining storage after data and reserve against an
empty message. Fixed `u64` control words match the recorded ABI. Raw
channel/destination pointers and talloc contexts are excluded. Rust allocation
failure follows standard allocator behavior rather than C's NULL return.

This fixture covers data/length, headlen/headroom/tailroom, reserve, put/get,
push/pull, trim, reset, invariant, and basic construction/drop equivalents.

Regenerate and verify from the conversion repository root:

```bash
../gsmtap-rs/reference/msgb/run_msgb_vector.sh > /tmp/msgb_vectors.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/msgb_vectors.json.gz | cmp /tmp/msgb_vectors.fresh.json -
cmp ../gsmtap-rs/tests/vectors/msgb/msgb_vectors.json.gz osmocore-rs/tests/compatibility/vectors/msgb_vectors.json.gz
cmp ../gsmtap-rs/tests/support/msgb_fixture.rs osmocore-rs/tests/support/msgb_fixture.rs
cargo fmt --manifest-path osmocore-rs/Cargo.toml --check
cargo test --manifest-path osmocore-rs/Cargo.toml
```

`msgb/msgb_reference_vectors.rs` compares Rust bytes, offsets, queries, metadata,
and returns against all records. It also tests Rust-only safety errors. C
raw destination/channel presence is checked by the shared validator; those
pointers have no Rust counterpart. Ordinary Cargo tests need no C build.

## Message-buffer typed and layer helpers

`core::msgb` covers 25 more C entry points: `msgb_put_u8/u16/u32`,
`msgb_push_u8/u16/u32`, `msgb_get_u8/u16/u32`, `msgb_pull_u8/u16/u32`,
`msgb_l1/l2/l3/l4`, `msgb_sms`, `msgb_l1len/l2len/l3len/l4len`,
`msgb_pull_to_l2`, `msgb_pull_to_l3`, `msgb_l3trim`, and `msgb_push_tl`.
The slash notation expands the related names, e.g. `msgb_put_u16`.
All typed multi-byte operations use big-endian order. Writes return `Result<()>`;
reads return the corresponding unsigned value. Bytes outside written spans
remain unchanged, including bytes removed by typed get/pull operations.

Layer access returns `Option<usize>` backing offsets; SMS aliases L4. Layer
lengths return `Result<u32>`, preserving C's unsigned wrapping subtraction when
a marker is beyond tail, even outside active data. Missing markers return
`MsgbError::MissingLayer` instead of C assertion panics. Pull-to-L2 clears L1;
pull-to-L3 clears L1/L2. Both require a marker in `[data, tail]`, inclusive,
and preserve all state on errors (`MissingLayer` or `LayerOutsideMessage`).

L3 trimming sets total length to `(L3 - data) + l3len`, using a signed
calculation. Negative requested L3 lengths are accepted when the resulting
total is nonnegative. Markers before data or beyond tail may participate in
this defined calculation. Rust rejects missing L3, totals outside signed
32-bit range (`ArithmeticOverflow`), negative totals (`NegativeLength`), and
tails beyond backing storage (`NotEnoughTailroom`), without mutation. C
narrowing outside the signed-int range and NULL pointer subtraction are not
portable fixture cases.

Tag/length prepending adds two bytes: the tag and the prior message length
narrowed to eight bits, including wrap for lengths above 255. It returns the
new data offset. Headroom errors do not write either byte.

A separate 7,572-record C fixture, `msgb_layers_vectors.json.gz`, captures complete
before/after storage and metadata, queries, return values, and panic outcomes.
It covers all 256 u8 values for all four typed operations; u16/u32 boundary
values and high-bit results; exact/insufficient room and message lengths;
layer markers in headroom, active data, and tailroom or unset; wrapping layer
lengths; successful and failed pull-to-layer; signed L3 trim boundaries;
tag and length wrapping; and a 14-step mixed sequence. It shares schema helpers
with the storage fixture.

Successful C u16/u32 pushes use aligned destinations: the C wrappers cast to
typed pointers before byte stores. Rust uses byte slices and supports unaligned
pushes; that extension has separate Rust-only tests. The C fixture excludes
unsafe NULL subtraction, out-of-storage trim, and implementation-defined signed
narrowing. Both projects validate exact schema/provenance/input order, untouched
bytes, typed byte order, query consistency, metadata changes, and corrupt records.
The conversion crate compares Rust results against C and tests safety differences.

The runner's `--layers` mode additionally pins `bits.h`, `utils.h`, the endian
header template, its `Makefile.am` generation rule, and generated `bit16gen.h`
and `bit32gen.h`. The template/rule/bits/utils files were compared byte-for-byte
with upstream revision `950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-08.
The generated headers were reproduced from that template using the verified
`XX` replacement rule and compared byte-for-byte. Their SHA-256 hashes are
pinned in the runner and recorded in each observation. Headers/library still
come exclusively from the required local C checkout.

`msgb/msgb_layers_reference_vectors.rs` registers conformance, corruption,
missing/out-of-message-layer, signed-overflow, storage-bound, and unaligned-push
tests. `support/msgb_layers_fixture.rs` and `support/msgb_fixture.rs` are identical to
the corresponding sibling `tests/support/` files. Ordinary Cargo tests need no C.

From the conversion repository root:

```bash
../gsmtap-rs/reference/msgb/run_msgb_vector.sh --layers > /tmp/msgb_layers.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/msgb_layers_vectors.json.gz | cmp /tmp/msgb_layers.fresh.json -
cmp ../gsmtap-rs/tests/vectors/msgb/msgb_layers_vectors.json.gz osmocore-rs/tests/compatibility/vectors/msgb_layers_vectors.json.gz
cmp ../gsmtap-rs/tests/support/msgb_layers_fixture.rs osmocore-rs/tests/support/msgb_layers_fixture.rs
cargo fmt --manifest-path osmocore-rs/Cargo.toml --check
cargo test --manifest-path osmocore-rs/Cargo.toml
```

## Message-buffer copy and area resize

`core::msgb` adds `msgb_copy(&Msgb) -> Result<Msgb, MsgbError>`,
`msgb_copy_resize(&Msgb, capacity: usize) -> Result<Msgb, MsgbError>`, and
`msgb_resize_area(&mut Msgb, area: usize, old_size: i32, new_size: i32)` returning
`Result<(), MsgbError>`. The two copy functions also represent the buffer
behavior of C's `msgb_copy_c` and `msgb_copy_resize_c`; Rust ownership replaces
context/name arguments rather than reproducing talloc context semantics.

Copies preserve active bytes, data/tail offsets, and optional L1–L4 offsets.
The local C implementation copies only active bytes despite its broader
comment: headroom and unused tailroom bytes in the new allocation are zero.
Control words and raw destination/channel metadata are cleared in C; Rust
clears its control words and has no raw pointer metadata. Copy storage is owned
independently of the source. New capacity includes headroom and must hold the
current tail; Rust also requires preserved layer offsets within that capacity.
Failed copies leave the source unchanged. Rust allocation failure retains the
standard allocator policy rather than C's NULL allocation-failure return.

Area resize accepts an absolute backing offset, requires the old area fully
inside `[data, tail]`, and moves its suffix using memmove semantics. Layer
markers at or after `area + old_size` move by `new_size - old_size`, including
markers in unused storage. Markers inside the removed area remain unchanged,
even if they subsequently lie beyond tail or violate layer ordering. Control
words and C destination/channel metadata are preserved. Zero-size insertion,
whole-message removal, overlap in both directions, and no-op resize are covered.

Growth does not initialize the extension; shrinkage does not erase the unused
suffix. Observations of initialized buffers retain all bytes outside the moved
suffix's destination. This does not promise generally zero-filled extension
contents. Rust checks active storage bounds and every relocated layer before
mutation, rejects negative sizes, and leaves all state unchanged on errors.
A marker that would move beyond backing storage produces `OffsetOutOfRange`;
an old area outside active data produces `AreaOutsideMessage`.

The separate `msgb_resize_vectors.json.gz` holds 8,888 direct C records: 1,131
copy calls across all four wrappers and 7,757 area-resize calls. Successful
copies record both unchanged source state and the complete new buffer, with
pointer identity reduced to a distinct-storage flag. NULL copy results are
explicit (`copy_present=0`), with absent-state sentinels. Resize calls record
before/after bytes, offsets, all layer/control metadata, queries, return codes,
and guarded panics. Inputs/order/schema/provenance and corruption rejection
are validated identically in both projects. All three msgb fixtures total
20,155 operation records; the first two fixtures remain byte-for-byte unchanged.

The C harness excludes pointer arithmetic beyond either allocation, NULL
relational comparisons during mutating resize, and growth that would exploit
C trim's incomplete headroom check. Negative-size panic cases keep C's
pre-check pointers within backing storage. Rust-only tests cover unset layers,
unsafe shrink-copy layer offsets, unsafe growth/relocation, extreme arguments,
copy independence, and preservation of unwritten bytes and removed-area markers.

`reference/msgb/run_msgb_vector.sh --resize` pins the existing msgb/panic reference
plus local logging source/header and the assertion header. Logging is initialized
without targets so legitimate copy-failure logging does not cause an unrelated
panic. The logging files were verified byte-for-byte against upstream revision
`950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-09; the assertion header
was already verified on 2026-10-08. The fixture ABI additionally records 32-bit
signed int. The runner builds, links, and loads only the required local checkout.

`msgb/msgb_resize_reference_vectors.rs` registers conformance, corruption,
copy independence, checked-error, and nullable-layer/byte-preservation tests.
`support/msgb_resize_fixture.rs` is identical to the sibling
`gsmtap-rs/tests/support/msgb_resize_fixture.rs`. It shares schema/state helpers
with `msgb_fixture.rs`; ordinary Cargo tests need no C build.

From the conversion repository root:

```bash
../gsmtap-rs/reference/msgb/run_msgb_vector.sh --resize > /tmp/msgb_resize.fresh.json
gzip -dc osmocore-rs/tests/compatibility/vectors/msgb_resize_vectors.json.gz | cmp /tmp/msgb_resize.fresh.json -
cmp ../gsmtap-rs/tests/vectors/msgb/msgb_resize_vectors.json.gz osmocore-rs/tests/compatibility/vectors/msgb_resize_vectors.json.gz
cmp ../gsmtap-rs/tests/support/msgb_resize_fixture.rs osmocore-rs/tests/support/msgb_resize_fixture.rs
cargo fmt --manifest-path osmocore-rs/Cargo.toml --check
cargo test --manifest-path osmocore-rs/Cargo.toml
```

## Message-buffer remaining adapters

Rust queues
own their messages in `MsgbQueue` (`VecDeque<Msgb>`): `msgb_enqueue`,
`msgb_dequeue`, `msgb_queue_free`, `msgb_enqueue_count`, and
`msgb_dequeue_count` preserve FIFO order. Counted operations use a separate
caller-owned `u32` with wrapping arithmetic; an empty dequeue and queue free
leave that counter unchanged. Moving a message transfers ownership, and
queue free drops queued buffers. Rust does not expose intrusive links or allow
the same owned message to be enqueued twice.

`msgb_eq_data` / `msgb_eq_l1_data` through `msgb_eq_l4_data` compare slices;
`msgb_eq` / `msgb_eq_l1` through `msgb_eq_l4` compare messages. They return
`Result<bool, MsgbError>`. `msgb_eq_level` selects levels 0–4, and
`msgb_eq_optional` returns false for absent message/data inputs, matching
C NULL comparisons. Empty slices remain present inputs. Headroom layer bytes
are comparable. Unset or beyond-tail layer comparisons return checked errors;
C's pointer subtraction and potentially enormous wrapped lengths are excluded.

`msgb_hexdump_buf(&mut [u8], &Msgb)` returns a success boolean in place of C's
buffer pointer or `"ERROR"`, preserving defined truncation, NUL writes, and
untouched suffix bytes. `msgb_hexdump` returns an owned `String` with C's
4100-byte output limit. `msgb_hexdump_owned` models `msgb_hexdump_c` using
`length * 3 + 100` bytes without talloc. These retain the underlying
`osmo_hexdump` limit of 1365 input bytes **per segment** and C's layer
annotations, including markers in headroom, behind an earlier marker, or
beyond tail. `msgb_hexdump_l1` through `msgb_hexdump_l4` return `"[]"` for
unset/empty layers and errors for beyond-tail markers. Rust safely checks
short annotation writes where C fails to check `snprintf` truncation; those
unsafe C buffer sizes are excluded from observations. Out-of-allocation layer
pointers cannot be represented by Rust, so C's corresponding annotation has
no callable Rust case.

`msgb_printf(&mut Msgb, format_args!(...))` uses Rust formatting;
`msgb_append_str` appends already formatted UTF-8 bytes. The byte-storage
contract matches C: a spare NUL byte is required, that NUL is outside active
data, and embedded NUL bytes count toward message length. On insufficient
space, it returns `NotEnoughTailroom` (C `-EINVAL`), preserves offsets/layers/
control metadata, and writes truncated output plus NUL into available unused
tail bytes. With no tailroom, no bytes change. This is the documented
exception to the other mutating APIs' unchanged-on-error rule. Rust format
syntax, formatting errors, and allocation failures are not C variadic/locale
formatting compatibility claims.

The explicit allocation and context inventory is:

| C entry points / adapters | Rust equivalent or exclusion |
| --- | --- |
| `msgb_alloc`, `msgb_alloc_c` | `Msgb::new`; zero-initialized owned storage, maximum `u16::MAX`. Names, parent contexts, talloc reporting, and C NULL-on-allocation-failure behavior are excluded; Rust allocation follows its allocator policy. |
| `msgb_alloc_headroom`, `msgb_alloc_headroom_c` | `Msgb::with_headroom`; capacity includes headroom. Excess headroom is an error instead of C assertion/panic. Context/name arguments are excluded. |
| `msgb_free`, `msgb_queue_free` | Rust drop and explicit queue clear/drop. No shared talloc tree lifetime or intrusive linkage. |
| `msgb_copy`, `msgb_copy_c`, `msgb_copy_resize`, `msgb_copy_resize_c` | Owned copies preserve the tested buffer semantics. Context/name variants share the Rust owned entry points. |
| `msgb_hexdump_c` | `msgb_hexdump_owned`; owned string replaces talloc output/context. |
| `msgb_set_talloc_ctx`, `msgb_talloc_ctx_init` | Excluded. Rust has no global talloc root or pool; no process-global allocation state is introduced. |
| `_msgb_eq` source-location/print arguments; `msgb_eq_data_print`, `msgb_eq_l1_data_print` through `msgb_eq_l4_data_print` | Comparison behavior is represented by the checked equality helpers. Fatal logging, source locations, and print side effects are excluded. |
| C destination/channel pointers and list links | Excluded from owned `Msgb`; control words remain fixed 64-bit values matching the fixture ABI. |

The separate `msgb_adapters_vectors.json.gz` contains **4,055 direct C records**:
450 slice comparisons (including NULL inputs), 270 message comparisons,
2,942 caller-buffer dumps, 62 static and 62 allocated dumps, 152 layer dumps,
90 formatted appends, and 27 queue operations. Cases cover zero capacity/empty data,
nonzero headroom, full buffers, exact dump size thresholds, reordered/unset
layers, 1364/1365/1366-byte dump limits, 3000-byte segmented output, FIFO
sequences, and unsigned counter underflow/overflow.

The shared `msgb_adapters_fixture.rs` validates strict schema, frozen input
coverage, ABI/source provenance, complete backing and metadata snapshots,
queue state/counters, equality outcomes, append writes, and dump destination
integrity. A frozen FNV-1a fingerprint of the direct C file additionally
rejects all changes to observed output bytes; it is an integrity check, not a
cryptographic authenticity claim. Corruption tests cover missing/duplicate/
unknown fields, provenance, ABI, outcomes, backing bytes, dump output,
counters, and missing records. Rust conformance checks every record; separate
safety tests exercise unsafe C annotation sizes and invalid layer spans.
Ordinary Cargo tests require no C installation.

`reference/msgb/run_msgb_vector.sh --adapters` uses only the required local C
checkout. It additionally pins `src/core/utils.c`, `utils.h`, and `linuxlist.h`;
new source/list dependencies were verified byte-for-byte against revision
`950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-09. The queue harness
uses GNU C11 because the upstream list macros use `typeof`. Its libtalloc
link supplies the existing C library's allocation dependency. Fixtures and
validators are shared byte-for-byte between repositories.

This is not a claim of talloc, logging, raw-pointer, or C printf ABI parity.

Regenerate these adapters from the testing repository:

```bash
./reference/msgb/run_msgb_vector.sh --adapters > /tmp/msgb_adapters.fresh.json
gzip -dc tests/vectors/msgb/msgb_adapters_vectors.json.gz | cmp /tmp/msgb_adapters.fresh.json -
cmp tests/vectors/msgb/msgb_adapters_vectors.json.gz ../libosmocore-rs/osmocore-rs/tests/compatibility/vectors/msgb_adapters_vectors.json.gz
cmp tests/support/msgb_adapters_fixture.rs ../libosmocore-rs/osmocore-rs/tests/support/msgb_adapters_fixture.rs
```

`./scripts/generate-reference-vectors.sh` includes all four msgb runners.
When intentionally revising C cases, review every changed observation and
update the frozen input manifest/count/fingerprint from direct C output.

## Primitive observations

`prim_vectors.json` contains **6,865 direct C records**: 2,112 initialization
and reinitialization records, 4,752 event-map records, and one callback record.
The initialization matrix combines six SAPs (including all three namespace
bases), eight primitive IDs, eleven operation values, both message attachment
states, and two initialization phases. Phase two complements every numeric
field and toggles attachment. This covers zero/max values, packed-ID overflow,
low-byte operation collisions, all four known names, and unknown-name fallback.
Event cases cover empty sentinel maps, exact and single-field mismatches,
duplicate first matches, sentinel-before-match, zero events, and `UINT32_MAX-1`.
Full raw operations must match even when packed low bytes coincide.

Every init/event record snapshots all 16 backing bytes, data/tail/length,
all four layer offsets, and all five control words. Attachment is checked by
identity. The callback record observes header/payload/context mutation and a
negative return through the C callback typedef. Unsafe pointers, unterminated C
maps, and raw embedded-header layout are not C fixture inputs; Rust separately
tests bounded maps and stable unknown-name text.

The shared validator checks exact schema, numeric canonicalization/ranges,
ordered input coverage, map entries, ABI, source hashes, and a frozen FNV-1a
fingerprint of the complete C output. This fingerprint protects fixture integrity,
not cryptographic authenticity. Corruption tests reject missing/duplicate/unknown
fields, altered provenance/ABI/inputs/maps/output/association/names, missing or
extra records, and malformed framing. Ordinary Cargo tests require no C build.

`reference/prim/run_prim_vector.sh` verifies SHA-256 hashes for `prim.c`, `prim.h`,
`utils.c`, `utils.h`, `msgb.c`, and `msgb.h`, verified byte-for-byte against
revision `950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-09. It builds,
links, and selects runtime libraries only from the required local C checkout.
It requires the observed ABI: 32-bit unsigned int/operation enum and 64-bit
unsigned long/pointers. It fails on source or ABI mismatch.

Regenerate from the testing repository:

```bash
./reference/prim/run_prim_vector.sh > /tmp/prim.fresh.json
cmp /tmp/prim.fresh.json tests/vectors/prim/prim_vectors.json
cmp tests/vectors/prim/prim_vectors.json ../libosmocore-rs/osmocore-rs/tests/compatibility/vectors/prim_vectors.json
cmp tests/support/prim_fixture.rs ../libosmocore-rs/osmocore-rs/tests/support/prim_fixture.rs
```

`./scripts/generate-reference-vectors.sh` includes the prim runner. Fixtures and
validators are shared byte-for-byte. Review all regeneration diffs; update the
frozen coverage/fingerprint only from reviewed direct C observations.

## Core CRC observations

The CRC conversion covers byte-oriented CRC-16/CCITT and generic parameterized
8/16/32/64-bit CRCs. GSM parity/channel definitions and convolutional coding
are outside its scope.

`crc_byte_vectors.json` contains **3,472 records**: all 512 table entries,
2,560 single-byte calls covering every byte and five initial states, and 400
buffer scenarios. Each buffer scenario directly calls whole-buffer, first-chunk,
and continuation APIs, records all 260 input bytes before/after, and includes
an untouched suffix. The zero-length buffer calls use a null C input pointer.

`crc_generic_vectors.json.gz` contains **22,848 records**. The frozen matrix covers
every configured width in each storage type, four neutral descriptors, four
input patterns (including nonbinary bytes), lengths `INT_MIN`, -1, 0, 1, 7,
and 32, and exact/oversized CRC destinations. Each record directly observes
compute, set, matching check, a single-bit mismatch at every position, and a
nonbinary CRC byte at every position. Complete input and destination bytes are
recorded before/after, with canaries preserving destination suffixes.

The local C template's positive-input full-width 32/64-bit mask invokes undefined
shifts. Those calls are excluded from golden fixtures. Their zero/negative-length
calls are defined and included. Full-width CRC-8/16 is defined on the pinned ABI
through integer promotion and is included for all lengths. The `--audit` mode
compiles the exact local CRC sources with UBSan, verifies the whole golden domain
is clean, and requires diagnostics for full-width 32/64 positive-input shifts,
zero width, and `INT_MIN` width subtraction overflow. Diagnostics are not golden
CRC results and the C source is never patched.

Rust accepts widths 1..=storage width, defines full-width 32/64 masks safely,
and rejects all other widths before arithmetic/access. Nonpositive input lengths
return init XOR remainder; check/set still need the configured CRC slice. Input
bytes use only their low bit; supplied CRC bytes are compared exactly against
0/1. High descriptor field bits and high final-XOR bits are preserved. Bounds
errors leave destinations unchanged. Full-width positive-input Rust behavior
is tested with hand-traced single-bit cases and independent u128 polynomial
division, outside C fixtures.

The strict shared validator checks exact unsigned `u64` values (including
`u64::MAX`) and signed `i32` lengths, schema/canonical numbers, provenance,
ordered coverage, unchanged inputs, full output state/suffixes, and a frozen
FNV-1a fingerprint of all direct C output. The fingerprint checks integrity,
not cryptographic authenticity. Corruption tests cover fields, framing,
revision/hashes/ABI, numeric overflow, coverage, observed results, mismatch
records and output bytes; separate parser tests exercise validation independently
of the fingerprint. Rust conformance compares every observation.

`reference/crc/run_crc_vector.sh` pins 16 local source/header/template/build-rule
and generated-file hashes. Nongenerated files were verified byte-for-byte at
revision `950430e829a3dc1d162aa241bc0505745c5a7311` on 2026-10-09. All eight
generated CRC files match the verified templates' `XX` substitutions. Build,
link and runtime selection use only the required local C checkout; mismatches
fail. The observed ABI requires 32-bit `int` and 8-bit bytes.

Regenerate and audit from the testing repository:

```bash
./reference/crc/run_crc_vector.sh byte > /tmp/crc-byte.fresh.json
./reference/crc/run_crc_vector.sh generic > /tmp/crc-generic.fresh.json
./reference/crc/run_crc_vector.sh --audit
cmp /tmp/crc-byte.fresh.json tests/vectors/crc/crc_byte_vectors.json
gzip -dc tests/vectors/crc/crc_generic_vectors.json.gz | cmp /tmp/crc-generic.fresh.json -
cmp tests/vectors/crc/crc_byte_vectors.json ../libosmocore-rs/osmocore-rs/tests/compatibility/vectors/crc_byte_vectors.json
cmp tests/vectors/crc/crc_generic_vectors.json.gz ../libosmocore-rs/osmocore-rs/tests/compatibility/vectors/crc_generic_vectors.json.gz
cmp tests/support/crc_fixture.rs ../libosmocore-rs/osmocore-rs/tests/support/crc_fixture.rs
```

The full generator includes every CRC fixture. Fixtures and validators are shared
byte-for-byte; ordinary Cargo tests require no C build. Review every generation
diff, including existing fixtures. Update fingerprints only from reviewed direct
C evidence.

## PRBS observations

`prbs_vectors.ndjson` records every `osmo_prbs_*` call in 14 cases: two full
periods of each standard sequence, single-bit steps, mixed and zero-length calls,
and caller-defined descriptors. `utilities/prbs_reference_vectors.rs` replays each
call and compares the result, the complete destination with its guard bytes and
the generator state. Regenerate it from `gsmtap-rs` with
`reference/prbs/run_prbs_vector.sh`.

## GSM 05.02 observations

`gsm0502_vectors.ndjson` holds 6,376 records of direct `libosmogsm` calls: the
header constants, frame arithmetic at edge and pseudo-random values, block starts
including all 258 frames in upstream's `gsm0502_test.ok`, CCCH blocks, hopping rows
that reach every RNTABLE entry, and paging groups for every value of the three-bit
fields. `gsm0502/reference_vectors.rs` replays them against both the preferred and
`compat::gsm0502` APIs. Regenerate it from `gsmtap-rs` with
`reference/gsm0502/run_gsm0502_vector.sh`.

The fixture has no C result for an empty allocation, a hopping sequence number
above 63 or a zero group divisor, because those calls crash or are undefined. Those
Rust errors have direct tests instead.

## gsm_utils observations

`gsm_utils/gsm_utils_numeric_vectors.ndjson` records direct `libosmogsm` calls: bands
and power for valid and invalid band values, every ARFCN under each flag combination,
every frequency in the band table, GSM time from frame numbers and from fields that
disagree, reduced frame numbers around the wrap threshold, TLLIs, classmarks and RACH
parameters. The harness also replays upstream's RACH output and its checks on power
classes, GSM time and reduced frame numbers. `gsm_utils/reference_vectors.rs` replays
the records against both the preferred and `compat::gsm_utils` APIs. Regenerate it
from `gsmtap-rs` with `reference/gsm_utils/run_gsm_utils_numeric_vector.sh`.

The fixture has no C result for an RFN of 42432 or more, which aborts, or a dBm within
110 of `INT_MAX`, which overflows. `gsm_utils/behavior.rs` tests those Rust results
directly.

`gsm_utils/gsm_utils_7bit_vectors.ndjson` records direct calls to the 7-bit functions:
octet lengths, the septet of every byte from 0x01 to 0xfe, packing with each padding,
SMS and USSD encoding under a range of output sizes, every septet alone and after an
escape, and random input with and without a user data header. The harness replays
upstream's `sms_test.ok` lines for these functions and its checks on them, and the
fixture marks the records made from upstream's data. `gsm_utils/reference_vectors_7bit.rs`
replays the records against both APIs. Regenerate it from `gsmtap-rs` with
`reference/gsm_utils/run_gsm_utils_7bit_vector.sh`.

The fixture has no C result for byte 0xff, which reads past C's table, padding above 7,
which shifts by a negative amount, an empty decode output, which aborts, or input
shorter than the septets. `gsm_utils/behavior_7bit.rs` tests those Rust results directly,
and messages over 255 septets, which C's decoders can't take.

## sockaddr_str observations

`sockaddr_str/sockaddr_str_vectors.ndjson` records direct `libosmocore` calls in sections:
the family of each text; every conversion from a stored address, including families
that don't match the text; conversions into an address over a destination holding junk,
so writes made before a failure show; and comparisons. The texts cover valid, nearly valid
and invalid addresses of both families, and lengths around the 46-byte field. The harness
replays upstream's test and checks its output matches `sockaddr_str_test.ok`.
`sockaddr_str/reference_vectors.rs` replays the records against both APIs. Regenerate it
from `gsmtap-rs` with `reference/sockaddr_str/run_sockaddr_str_vector.sh`.

C's results come from glibc's `inet_pton` and `inet_ntop`, and the recording is from a
little-endian host, where IPv4 addresses compare by their byte-reversed value.
`sockaddr_str/behavior.rs` tests what the recording can't hold: NUL bytes in the text,
missing addresses, and where the preferred API differs from C.

## strrb observations

`strrb/strrb_vectors.ndjson` records direct `libosmocore` calls as a stream of events on
named ringbuffers: each string added with its result, then the start, end, element count,
slot checks and the result at indexes past the newest string and near the top of the
`unsigned int` range. The harness also makes the calls and checks of upstream's test.
`strrb/reference_vectors.rs` replays the events against both APIs. Regenerate it from
`gsmtap-rs` with `reference/strrb/run_strrb_vector.sh`.

The fixture has no C result for adding to a ringbuffer of size 0, which crashes.
`strrb/behavior.rs` tests that Rust result directly, and what the preferred API adds.

## use_count observations

`use_count/use_count_vectors.ndjson` records direct `libosmocore` calls as a stream of
events on named objects: each change with its result, each callback it runs, static
entries, freed entries and text at a range of sizes, with the total, text and every entry
after each change. Scripted objects cover entry order and reuse, NULL and empty tokens
and the limits of a count, and others make random calls under each callback behaviour.
The harness also runs the sequence of upstream's test and compares its use count lines
with `use_count_test.err`. `use_count/reference_vectors.rs` replays the events against
`compat::use_count`, running the same callbacks, and those without callbacks against the
preferred API. Regenerate it from `gsmtap-rs` with
`reference/use_count/run_use_count_vector.sh`.

The fixture has no C result for `INT32_MIN` as a change to a negative count or as a count
after a negative total, where a negation overflows, or for freeing a static entry, which C
forbids. `use_count/behavior.rs` tests those Rust results directly.

## A5 observations

`a5/a5_vectors.ndjson` records direct `libosmogsm` calls: frame counts, both directions
of A5/0, A5/1 and A5/2 with fixed and random keys and frame numbers and with each key and
frame count bit alone, and `osmo_a5` results for cipher numbers with no algorithm. The
harness also replays upstream's A5/0, A5/1 and A5/2 output. `a5/reference_vectors.rs`
replays the records against both APIs. Regenerate it from `gsmtap-rs` with
`reference/a5/run_a5_vector.sh`.

The fixture has no C result for frame numbers from 1326 x 2^20 on, where a shift
overflows an `int`, or for A5/1 and A5/2 with no key, which crashes. `a5/behavior.rs`
tests those Rust results directly, and that `compat::a5` reports A5/3 and A5/4 as
unsupported.

## COMP128 observations

`comp128/comp128_vectors.ndjson` records direct `libosmogsm` calls:
- SRES and Kc from COMP128 v1, v2 and v3 for upstream's 2,048 v2 and v3 vectors, and for a
  sweep of fixed patterns, single Ki and RAND bits and random pairs;
- SRES and Kc from XOR-2G through `osmo_auth_gen_vec2` over the same sweep.

The harness checks each COMP128 result against `osmo_auth_gen_vec2`, and replays upstream's
`comp128_test` and `xor2g_test` output. `comp128/reference_vectors.rs` replays the records
against `comp128v1`, `comp128v2`, `comp128v3`, `xor_2g` and `GsmAlgorithm::vector`.
Regenerate it from `gsmtap-rs` with `reference/comp128/run_comp128_vector.sh`.

The fixture has no result for a UMTS subscriber, which the Rust API can't express.

## GSM 05.03 xCCH observations

`gsm0503/xcch_vectors.ndjson` records direct `libosmocoding` calls: the code and
parity descriptors, interleaving, burst mapping, encoded frames and decodes. Decodes
cover the three frames in upstream's `coding_test.ok`,
clean frames at several amplitudes, erased bursts, bit flips, additive noise, random
erasures, altered stealing flags and input with no signal, so they include both
corrected frames and parity failures. `gsm0503/reference_vectors.rs` replays them
against the preferred and `compat::gsm0503` APIs, comparing frames and bit-error
counts. Regenerate it from `gsmtap-rs` with `reference/gsm0503/run_xcch_vector.sh`.

The fixture has no -128 soft bits, because that value is outside the documented
range and the C backends disagree on it. A direct test checks that Rust accepts it.

## TLV observations

`tlv/tlv_vectors.ndjson` records direct `libosmogsm` calls: `tlv_parse_one` for each
format at its length boundaries, including the outputs C has written by a failure;
`tlv_parse2` with repeated tags, leading LV tags and truncated or random input;
`tlv_encode_one`, `tlv_encode` and `tlv_encode_ordered`; each `msgb_*` TLV helper;
the gross length helpers; the `osmo_shift_*` functions; `osmo_tlvp_merge`,
`osmo_tlvp_copy`, `tlv_def_patch` and the accessors; and `osmo_tlv_prot_*` validation
and naming. `tlv/reference_vectors.rs` replays them against the preferred and
`compat::tlv` APIs. Values are compared by their offset in the input as well as their
contents. `tlv/behavior.rs` covers what C can't record, such as a message with no
room, which aborts in C. Regenerate the fixture from `gsmtap-rs` with
`reference/tlv/run_tlv_vector.sh`.

## Convolutional descriptors, encoder, decoders and evidence

`compatibility/vectors/conv/conv_vectors.ndjson.gz` contains 38,560 direct C
records: four backend metadata records, 1,354 descriptors and 37,202
operations covering all 17 selected core convolutional exports. Its byte-identical
sibling is `gsmtap-rs/tests/vectors/conv/conv_vectors.ndjson.gz`. The shared
`support/conv_fixture.rs` reader and `conv/conv_reference_vectors.rs` tests
validate evidence integrity and backend comparisons only. Separate
`conv/conv_encoder.rs` tests replay all 15,529 frozen length/encoder operations against
the Rust APIs, including return values, full buffers, suffixes, state and both
cursors across split calls and reuse. `conv/conv_decoder.rs` replays all 7,261
frozen generic stateful lifecycle observations across 332 cases: 6,597 callable
API observations and 664 deinitializations represented by Rust drops. All match
the frozen evidence. `conv/conv_correlation.rs` independently replays 7,300
`osmo_conv_decode_acc` observations (1,843 each linked/SSE/AVX, 1,771 scalar) and
7,112 public `osmo_conv_decode` observations (1,796 each linked/SSE/AVX, 1,724
scalar), totaling 14,412 complete output-buffer, status/metric and unchanged-input comparisons. All match
the frozen evidence, preserving backend identity. Snapshots describe semantic
state, never struct memory, pointers or uninitialized history.

Replay covers 11,058 signed length returns and 4,471 encoder observations,
including 460 state loads (four with nonbinary bytes) and 368 distinct reusable
sessions across 1,851 cases. Backend identity is retained: only linked-dispatch
records these seven APIs, while the exact-source backend streams remain separate
decoder evidence, not extra encoder conformance observations.

Coverage includes full-scale unpunctured frames whose path metrics saturate. Scalar
C omits their -128 input, which is outside the documented `sbit_t` range and which
its wrapping metrics decode differently. Coverage also includes exhaustive small inputs; rates/constraint lengths beyond the
optimized domain and byte-width boundaries; flush/truncation/tail-biting/raw
termination values; explicit recursive tables; puncture cursor continuation;
helper-length/write-count divergence; state packing; reset/rewind/reuse;
clean/erased/noisy/tie/extreme soft inputs; normalization boundaries; and
independent synthetic length-97 K=7 rate-2 cases. Generic squared-error
metrics and one-shot
correlation status returns are retained independently.

Rust-only safety tests cover descriptor dimensions/table sizes/targets, paired
termination tables, bounded ordered punctures, hard-bit validation, short state
loading/tail-biting input, insufficient output, checked arithmetic and failure
atomicity. Additional source-derived boundaries (`K=1`, wide symbol shifts,
high output-symbol bits) are tested independently, not claimed as additional
frozen C observations. Internal encoder unit tests exercise C-domain counter
and absolute-position overflow without allocating enormous buffers.

The decoder adds 15 tests: 11 unit safety tests, three integration safety tests
and one frozen replay. Safety coverage includes atomic state/operation and
short-slice errors, missing/unreachable history, no survivors, checked arithmetic
and byte-mask/predecessor boundaries. Reset/rewind invalidate old history;
successful traceback preserves output suffixes. Use decoder
`scan_input_capacity` and `flush_input_capacity` for soft-byte sizing.

Use `ConvCode::output_capacity`, `ConvEncoder::raw_output_capacity` and
`flush_output_capacity` to size buffers, not the signed compatibility output
length helper. That helper may validly be negative or differ from actual writes.
Use `ConvCode::decode_input_capacity() -> Result<usize, ConvError>` for one-shot
soft-byte sizing: it counts encountered punctures and does not impose the
encoder's short tail-biting state-loading limit. Both one-shot decoders accept
`(&ConvCode, &[i8], &mut [u8])` and return `Result<i32, ConvError>`.
Correlation returns `Ok(0)`; generic dispatch returns accumulated error bounded
by `MAX_AE`. Public `K=5/7 && N<=4` dispatch includes `N=1` rejection without
fallback; direct correlation requires `N=2..=4`, `K=5/7` and positive lengths.
Nonsystematic recursion maps to `UnsupportedRecursiveCode`; negative traceback
statuses map to `NoSurvivor`. Focused Rust-only tests cover dimensions, slices,
recursion, short correlation tail-biting, checked sizing/metric arithmetic and
atomic output. Correlation path metrics saturate at the `i16` limits like the C
SSE and AVX backends. All `i8` inputs, including `-128`, are supported; output
suffixes remain untouched.
Run these with `cargo test --manifest-path osmocore-rs/Cargo.toml conv` from the
repository root. The sibling utility validates the fixture's integrity.

The required local C tree has no `.git`: upstream Git revision is unknown.
The convolutional manifests pin local content hashes; embedded version text is
descriptive only. The runner verifies those hashes, transitive includes and
the existing local library, building isolated backends in temporary directories
without changing the source snapshot. Available scalar/SSE/AVX observations
agree for this bounded coverage; linked runtime identifies AVX. NEON is not
executed and is outside claimed backend parity. Review has approved the observed
scalar/SSE/AVX tie behavior as the CPU-independent portable Rust correlation
contract, implemented without SIMD selection. Linked dispatcher
observations are pinned-local-binary evidence, not proof of source build lineage;
exact-source backend observations remain separate.

This evidence doesn't claim NEON parity, linked-library instrumentation,
arbitrary-descriptor parity or injected allocation-failure evidence.

From the sibling testing repository:

```bash
./reference/conv/run_conv_vector.sh --verify
./reference/conv/run_conv_vector.sh --audit
./reference/conv/run_conv_vector.sh all > /tmp/conv.fresh.ndjson
gzip -dc tests/vectors/conv/conv_vectors.ndjson.gz | cmp /tmp/conv.fresh.ndjson -
gzip -dc ../libosmocore-rs/osmocore-rs/tests/compatibility/vectors/conv/conv_vectors.ndjson.gz | cmp /tmp/conv.fresh.ndjson -
cmp tests/support/conv_fixture.rs ../libosmocore-rs/osmocore-rs/tests/support/conv_fixture.rs
```

The full generator includes convolutional evidence and preserves the previous
convolutional fixture on runner failure. The isolated golden domain is audited
with ASan/UBSan; linked library instrumentation and NEON execution are not
claimed. Invalid-shift/sizing probes are diagnostics only. Review all fixture
diffs and preserve the shared manifests/readers byte-for-byte. See
[the compatibility document](../src/base/conv/README.md) for safety
exclusions and source/backend findings.

## Time evidence

`tests/time/time_reference_vectors.rs` replays 20,828 direct C time/clock/helper
observations. `time_fixture.rs` is shared byte-for-byte with gsmtap-rs and validates
all 20,881 records, including 52 timer traces. Both projects
reject schema, provenance, coverage and output corruption. Ordinary tests require
no C build. Fixtures and manifests live in `compatibility/vectors/time/`.

The harness includes abnormal fractional fields, direct override-add, aliasing,
clock independence/retention, callback mutation, safe nested updates, and
prepare/nearest during dispatch. A bounded pathological peer-nested update is
kept outside golden data; nested updates are a documented intentional exception
for the implemented Rust scheduler and its deliberate reentrancy policy. See [time compatibility](../src/base/time/README.md)
for exact scope and targeted verification/regeneration commands.

## Standalone rate counters

`rate_ctr/rate_ctr_reference_vectors.rs` replays all 1,700 direct-C calls from
`compatibility/vectors/rate_ctr/`, comparing complete initialized/final state
through wrappers, methods and the addition sink. A shared independent reader
validates fixtures, provenance and four separate null diagnostics.
`time/time_cc_reference_vectors.rs` also replays every frozen time_cc scenario
using the concrete owned `RateCtr` sink. Regenerate only this family with
`../gsmtap-rs/reference/rate_ctr/run_rate_ctr_vector.sh all` and `--diagnostics`, following
the sibling reference README; verify/audit and compare two generations before
accepting either. See [compatibility](../src/base/rate_ctr/README.md)
for the exact scope and ABI boundary.

## FSM execution and TDef/timer integration

`fsm/fsm_*_reference_vectors.rs` replays 14,852 direct-C records from the
`fsm_core`, `fsm_timer`, `fsm_tree`, `fsm_registry` and `fsm_shared` fixture
directories. The shared independent `support/fsm_fixture.rs` validates shape,
chronology, canonical fields, frozen observations, manifests and separate unsafe
probes. Additional Rust safety tests cover stale/foreign handles, clock-failure
mutation prefixes, callback error consumption, hierarchy cycles and unsafe C
traversals. Each dedicated sibling runner supports `--verify`, `--audit`, `all`
and `--diagnostics`; follow its reference README for targeted regeneration,
repeat comparisons and shared-file checks. See
[FSM compatibility](../src/base/fsm/README.md).

## Public API ownership

Replay imports functional modules from their groups for shared types and ordinary functions,
and `compat::<subsystem>` for distinct C contracts. Trivial runtime, scheduler,
clock and counter adapters are method calls, and recorded operation names keep
their C spelling. Decoder histories are read-only through
`compat::conv::diagnostics`.
These integration tests and fixtures are repository-only and excluded from the
`.crate`; source unit tests and documentation examples also run from the package.

## Functional documentation

Conversion status, ownership differences and remaining gaps live in the
[source overview](../src/README.md) and its subsystem READMEs. This test guide
retains the repository-only fixture/provenance validation and regeneration
procedures. C API names identify frozen observations even where preferred Rust
functions have been renamed or forwarding wrappers replaced by method calls.

## Runtime/lifecycle validation

The runtime tests use scripted transport, poll and wake boundaries alongside real
IPv4/IPv6 ephemeral-port loopback. Socket permission failures are failures, never
skips. `runtime_service` is an end-to-end composition and round-trip acceptance
example: bounded framing, FSM timer-driven TCP/UDP responses, typed status
commands, write completions and clean shutdown. It composes accounting/rate
drivers without asserting their timed effects or pending-output drain.
[Focused behavioral tests](../../docs/README.md#acceptance-evidence)
prove those guarantees separately. `runtime,net` adds helper composition without
making runtime depend on net. The runtime diagnostics executable is isolated to
avoid scoped-subscriber callsite-cache interference. Timer diagnostics now use
the same isolation. All eight feature combinations run independently in CI.
