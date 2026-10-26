# Library source guide

This crate provides Rust-native telecom and core primitives with compatibility
adapters. The supported scope is documented below. Current stable Rust,
edition 2024, no declared MSRV.

This guide locates source modules, API/reference documentation and validation
evidence. The consolidated [architecture and compatibility guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md)
explains ownership, feature relationships, diagrams, supported families and
exclusions. Durable API details and runnable examples belong in module Rustdoc.

## Functional documentation

| Location | Completed functionality |
| --- | --- |
| [Convolutional coding](conv/README.md) | Checked descriptors, encoding, stateful decoding, portable correlation and dispatch |
| [FSMs](fsm/README.md) | Owned registry/runtime, hierarchy, transitions, termination and timer/TDef integration |
| [I/O runtime](runtime/README.md) | Optional Mio readiness, bounded TCP/UDP, typed commands/drivers and coordinated shutdown |
| [Networking](net/README.md) | Optional IPv4/IPv6 TCP/UDP establishment, local source binding, pre-bind options and bounded connect |
| [GSMTAP](gsmtap/README.md) | GSMTAP v2 Vec/Msgb encoding, UM defaults and generic UDP runtime integration |
| [Message buffers](msgb/README.md) | Owned storage, typed/layer operations, copy/resize, queues, comparisons and formatting |
| [Rate counters](rate_ctr/README.md) | Counter arithmetic, owned groups/registry, interval rollups and monotonic polling |
| [Time and timing](time/README.md) | Raw clocks, scheduling, timeout definitions and detailed accounting edge cases |
| [Time accounting](time_accounting/README.md) | Owned accumulator/configuration, counter sinks, policy boundaries, clock roles and callback errors |
| [Compatibility adapters](compat/README.md) | Always-compiled adapters, functional alternatives and conformance-inspection boundaries |
| [Structured diagnostics](diagnostics/README.md) | Structured targets/events, fields, severity, span lifetime and subscriber example |

The single-file utilities below live in this directory. Each subsystem README
records its completed scope, validation, outstanding work and Rust/C differences.

## Public API and architecture

All functional modules are at the crate root; there is no public `core` namespace
or root type glob. Prefer methods on owned `Msgb`, `FsmRuntime`, `TimerScheduler`
and `RateCtrRuntime`, and on caller-backed `BitVec`. Shared types stay in their
functional modules. Clocks, application context and scheduler ownership are
explicit; handles reject foreign owners and stale lifetimes.

[Compatibility adapters](compat/README.md) export distinct C contracts under `compat::<subsystem>`:
integer statuses, output parameters, nullable/sentinel inputs, signed counts,
terminated buffers, formatting limits and read-only conformance diagnostics.
These are safe Rust adapters, not an exported C ABI. Ordinary slice operations
use direct functional names.

Implementation layout follows responsibility: `net/` separates resolution,
phase errors, socket options/platform handling and establishment; `compat/`
contains one adapter module per subsystem. `timer.rs` remains a single cohesive
scheduler: handles, activation ordering and callback dispatch share its state.
`time_accounting/` separates its public adapter from private event transitions
and policy/TDef resolution. `diagnostics/README.md` owns the public diagnostics reference;
`diagnostics/mod.rs` includes it in Rustdoc and contains crate-private instrumentation helpers.
Test placement follows the [test-location convention](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md).

Migration from the conversion-era API:

- Replace `osmocore_rs::core::<module>` imports with root module imports.
- Use clock, timer, FSM, accounting and counter methods directly.
- Rename `state_chg*` methods to `state_change*`; the timeout-table bridge is
  `FsmRuntime::state_change_with_tdef`.
- Use `hexparse::parse_into`, `bits::pack_into`/`unpack_into` and offset variants,
  `prim::packed_id`/`operation_name`, and `conv::encode`/`decode`.
- Use `compat::<subsystem>` only for the distinct contracts above. Decoder
  history inspection is through `compat::conv::diagnostics`.

The large subsystems use private implementation files; timer, timeout-definition
and cumulative-accounting APIs remain separate concepts, rather than being
nested under the public `time` namespace. Their relationships are documented
in [the timing README](time/README.md).

## Time accounting

[Time accounting](time_accounting/README.md) owns one cumulative accumulator, mutable
configuration and an optional counter sink. Pair it with an explicit
`TimerScheduler<C, TimeCc<...>>`; `set_flag` changes activity and `update`
dispatches callbacks while surfacing deferred accounting errors. Callers that
dispatch the scheduler directly must consume `take_callback_error` themselves.
Elapsed accounting uses monotonic time; wall time gates scheduler wakeups.
The optional runtime's accounting driver preserves those separate clock roles.

Configuration and reporting have event-specific observation boundaries; completed
accounting and sink effects remain visible after a later fallible lookup or
timer operation. Cleanup releases the timer and resets accounting while keeping
configuration and sink totals. The [accounting guide](time_accounting/README.md)
documents ownership and entry points; the [timing contract](time/README.md#cumulative-accounting)
details rounding, wrapping, forgetting, configuration refresh and failure prefixes.

## Networking establishment

The optional `net` feature exposes `net::{resolve, listen_tcp, connect_tcp,
bind_udp}` and socket/TCP option types. It returns owned standard-library sockets
and retains phase-specific I/O errors. See the [networking contract](net/README.md)
for pre-bind settings, explicit TCP timeouts, feature composition, source mapping
and tests. The independent [runtime](runtime/README.md) adds readiness and
lifecycle integration; pending TCP connections remain deferred. With `net` disabled there is no socket2 dependency.

## Structured diagnostics

The optional `tracing` feature emits structured FSM/timer/runtime/net/GSMTAP diagnostics without
installing a subscriber or configuring application output/filters. Defaults
remain empty. See the [diagnostics contract](diagnostics/README.md) for stable targets,
fields, levels, execution context and an application configuration example.

## Implemented utilities

### Base64, BCD and hexadecimal parsing

[base64.rs](base64.rs) provides typed `encode_into`/`decode_into` results and owned
`encode`/`decode` conveniences. Rust encoding omits the trailing C NUL; empty or
whitespace-only decoding reports zero bytes. Capacity and invalid-input errors
preserve destination bytes. The recorded whitespace and incomplete-quartet
rules remain: incomplete quartets can be accepted without producing a partial
quartet. Compatibility adapters retain integer status, mutable `olen`, nullable
decode size probes and C NUL/output-length mutation behavior.

[bcd.rs](bcd.rs) covers nibble/character conversion and low-nibble-first BCD.
`nibble_to_char` emits uppercase A–F, checked `char_to_nibble` rejects invalid
characters, and owned `encode`/`decode` avoid C string terminators/truncation.
Owned encoding rejects embedded NUL and pads a final started byte with F.
`compat::bcd` retains invalid-character-to-zero conversion, C-string termination,
explicit nibble ranges, used-byte counts, truncated NUL-terminated output and
partial writes on invalid digits. A disallowed decoded nibble can be written
before the compatibility adapter reports its error; untouched nibbles stay intact.

[hexparse.rs](hexparse.rs) provides `parse_into` over byte slices with typed errors.
It preserves the recorded parsing and caller-buffer mutation contract. Source
NUL and malformed/odd digits are covered by the frozen C observations; capacity
and unsafe slice boundaries are checked in Rust.

### Packed bits and cursor-based bit vectors

[bits.rs](bits.rs) covers basic and offset packed/unpacked conversion, including
selectable LSB ordering. Basic zero-bit packing performs no access; basic
zero-bit unpacking still writes the first source bit and returns one when both
slices are nonempty. Empty slices then return a capacity error. Raw basic input
bytes preserve C shifting rather than being silently restricted to 0/1.
Extended packing treats nonzero unpacked bytes as one, preserves surrounding
bits, and checks offset overflow before capacities. Zero-width offset operations
perform no accesses; packed offset zero retains the observed 32-bit unsigned
return quirk of 536870912. These behaviors did not change with the Rust renames.

[bitvec.rs](bitvec.rs) represents all 30 nonallocation APIs in the local header
with borrowed mutable backing and `BitValue::{Zero, One, L, H}`. L/H compares
against the repeating 0x2b padding pattern. Positional access preserves the
cursor; sequential operations advance it on each successful bit. Integer,
fill and field operations preserve completed writes on failure; byte transfers
validate before changing buffers. Field write errors preserve the external
index; successful zero-width reads can reposition the cursor.

`BitVec::remaining_bits` is checked; compatibility tailroom wraps at the recorded
32-bit C width, including invalid cursors. Integer narrowing, signed successful
read values, padding/run-length wrapping, unchanged shift tail bytes and C's
skipped penultimate shift byte are preserved. Compatibility array writes ignore
setter errors and return zero; dry runs return wrapping size estimates. Buffer
pack/unpack copies all backing bytes without moving the cursor. Hex loading
clears storage and preserves partial parsing on error; only success changes the
cursor. C string formatting includes spaces and NUL and requires
`cur_bit + ceil(cur_bit / 8) + 1` bytes.

Rust checks invalid storage/shift cases, excludes invalid bit-value enum values,
and reports field overflow with typed errors instead of setting `errno`.
Caller ownership replaces talloc allocation/free.

### Primitive metadata

[prim.rs](prim.rs) covers raw operation/SAP constants, headers, packed IDs, names,
event maps and typed-context callbacks. Unknown 32-bit values remain
representable. Packed IDs use the operation's low byte and retain unsigned
shift/truncation. Names have stable `Cow` storage, including C's lowercase
unknown hexadecimal text. `PrimHeader` borrows an optional message exclusively
and stores metadata outside its bytes; initialization preserves the message.

`event_for_primitive` scans an ordinary slice and returns `Option<u32>`.
`compat::prim` retains first-match sentinel termination and the no-event value.
Typed callbacks retain header/message/context mutation and signed returns without
raw `void *` or a C dispatcher. Rust `Default` is a convenience, not a C default.

### CRCs

[crc.rs](crc.rs) covers byte CRC-16/CCITT with caller-supplied state and
`Crc8Code`/`Crc16Code`/`Crc32Code`/`Crc64Code` bit compute/check/set methods.
Tables use reflected polynomials 0xa001 and 0x8408; every entry is checked
against C. Byte APIs add no final XOR or reversal.

Generic input contributes `byte & 1`; supplied CRC bytes are compared exactly
against 0/1 and writes are MSB-first. High raw descriptor bits and unmasked final
XOR remain observable. Methods consume whole slices and return boolean matches;
`compat::crc` retains signed explicit lengths and integer check statuses.
Nonpositive compatibility lengths consume no input and compute init XOR
remainder; check/set still validate the complete CRC slice. Width/capacity errors
are atomic. Positive-input full-width CRC-32/64 is a safe Rust extension tested
by independent polynomial division, not undefined C-shift output. Full-width
8/16 and nonpositive-length full-width 32/64 have direct-C coverage.

## Testing and evidence

Recorded compatibility observations come exclusively from direct calls into the required ignored
in-repository `libosmocore/` checkout. The sibling `gsmtap-rs/reference/` hosts
harnesses and validates fixture integrity/provenance; it is not a Rust dependency
or an alternative behavior oracle. Never substitute a system C installation.

Older fixtures pin revision `950430e829a3dc1d162aa241bc0505745c5a7311`. Later
fixtures pin exact source/header/compiler/generator hashes with upstream revision
`unknown` because the local snapshot lacks Git metadata. The parent Rust Git
revision is not C provenance. Convolutional linked-binary observations have
additional limitations described in their README. Fingerprints detect changes
but do not establish cryptographic authenticity.

Repository integration entry points are
`tests/{utilities,conv,fsm,msgb,rate_ctr,time,gsmtap,net,runtime}/main.rs`, plus
`tests/public_api.rs` for cross-subsystem public API contracts. Isolated
diagnostics executables live under their owning subsystem and are registered
explicitly in `Cargo.toml`.
Shared independent readers live under `tests/support/` and are registered only
by the targets that use them.
The [repository test guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md) contains exact
regeneration/compare commands and strict validator coverage. Tests replay complete
return values, buffers, cursors, metadata and callback traces, not only success.

| Utility evidence | Coverage |
| --- | --- |
| Base64, BCD, hex parsing | Frozen C buffer inputs/returns/complete destinations; malformed input, terminators, offsets and capacity boundaries |
| Basic bit packing | 9,016 calls, complete source/destination buffers |
| Offset bit conversion | 63,384 calls, offsets, ordering, zero widths and untouched surrounding storage |
| Bit-vector primitives | 5,944 calls |
| Bit-vector integers/fill/bytes | 38,837 calls, all eight alignments and partial progress |
| Bit-vector fields/search/run/shift/arrays | 38,643 calls |
| Bit-vector buffer/text adapters | 6,757 calls |
| Primitive metadata | 6,865 calls, message preservation and callback/context mutation |
| Byte/generic CRC | 26,320 calls plus sanitizer checks and independent full-width safety tests |

The [runtime architecture](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#runtime-and-lifecycle) describes composition
and focused behavioral evidence. Preferred-API frozen replays,
independent operation models and cross-module consumer tests supplement direct-C
observations. Coverage measurements and their historical scope are recorded in
the [repository test guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md).
Subsystem READMEs retain the earlier direct-C audits and regeneration instructions.

From the repository root:

```bash
cargo fmt --manifest-path osmocore-rs/Cargo.toml --check
cargo clippy --manifest-path osmocore-rs/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path osmocore-rs/Cargo.toml
RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path osmocore-rs/Cargo.toml --no-deps
git diff --check
```

These commands check the default feature set. The [test guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md#current-validation)
describes the independent `net`, `runtime` and `tracing` combinations; build
Rustdoc with `--all-features` to include the optional networking and runtime APIs.

## Package boundary and independent validation

The package includes the manifest, Cargo lockfile/metadata, crate README,
LICENSE, `src/**` and `examples/**`, including these functional READMEs,
module Rustdoc, source unit tests and the feature-gated runtime service example.
Integration tests, large C evidence, reference harnesses
and the upstream C checkout remain outside the package. Ordinary package builds
need no C checkout, sibling project, evidence fixtures or repository configuration.

Independent package validation unpacks the actual `.crate` and runs
`cargo check`, `cargo test --lib` and `cargo doc --no-deps` without repository
fixtures or C tooling. The crate's project license is `GPL-3.0-or-later`;
see [LICENSE](../LICENSE).

## Remaining work and fundamental boundaries

The supported Rust surface is documented above. The project license is
`GPL-3.0-or-later`; see [LICENSE](../LICENSE).
Further protocol-specific coding/CRC/primitive catalogues, other C utilities, bit
compression and allocation/global-runtime APIs need separate scope and evidence.
There is no C ABI, pointer/layout promise, implicit global clock/runtime,
allocator-failure parity or guarantee for undefined C calls. Other platforms and
32-bit ABI behavior require separate validation where the subsystem README
limits parity to recorded 64-bit Linux behavior.

## Converted C API mapping

C source/header paths below are relative to the required ignored `libosmocore/`
checkout. C names identify observed behavior, not necessarily same-named Rust
exports. Each functional README above specifies preferred API ownership.
The table maps each Rust family to the C APIs whose recorded behavior it
reproduces and the upstream files consulted to understand that behavior. It is a
behavior map, not a record of translated code; see the [development approach](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#development-approach).

C-named adapters with distinct contracts live under `compat::<subsystem>`;
the family names below also cover preferred methods in the functional modules.

| Rust module | Converted C APIs | C source | C header |
| --- | --- | --- | --- |
| `base64` | `osmo_base64_encode`, `osmo_base64_decode` | `src/core/base64.c` | `include/osmocom/core/base64.h` |
| `bcd` | `osmo_bcd2char`, `osmo_char2bcd`, `osmo_bcd2str`, `osmo_str2bcd` | `src/core/utils.c` | `include/osmocom/core/utils.h` |
| `bits` | `osmo_ubit2pbit`, `osmo_pbit2ubit`, `osmo_ubit2pbit_ext`, `osmo_pbit2ubit_ext` | `src/core/bits.c` | `include/osmocom/core/bits.h` |
| `bitvec` (primitives) | `bitvec_get_bit_pos`, `bitvec_get_bit_pos_high`, `bitvec_set_bit_pos`, `bitvec_set_bit`, `bitvec_get_bit_high`, `bitvec_set_bits`, `bitvec_zero`, `bitvec_used_bytes`, `bitvec_tailroom_bits`, `bit_value_to_char` | `src/core/bitvec.c` (capacity helpers are inline in the header) | `include/osmocom/core/bitvec.h` |
| `bitvec` (integer and buffer operations) | `bitvec_set_u64`, `bitvec_set_uint`, `bitvec_get_uint`, `bitvec_get_int16_msb`, `bitvec_fill`, `bitvec_spare_padding`, `bitvec_get_bytes`, `bitvec_set_bytes` | `src/core/bitvec.c` | `include/osmocom/core/bitvec.h` |
| `bitvec` (indexed operations and helpers) | `bitvec_read_field`, `bitvec_write_field`, `bitvec_get_nth_set_bit`, `bitvec_find_bit_pos`, `bitvec_rl`, `bitvec_rl_curbit`, `bitvec_shiftl`, `bitvec_add_array` | `src/core/bitvec.c` | `include/osmocom/core/bitvec.h` |
| `bitvec` (buffer and text adapters) | `bitvec_pack`, `bitvec_unpack`, `bitvec_unhex`, `bitvec_to_string_r` | `src/core/bitvec.c`; hex loading calls `osmo_hexparse` in `src/core/utils.c` | `include/osmocom/core/bitvec.h` |
| `msgb` | `msgb_put_u8`, `msgb_put_u16`, `msgb_put_u32`, `msgb_push_u8`, `msgb_push_u16`, `msgb_push_u32`, `msgb_get_u8`, `msgb_get_u16`, `msgb_get_u32`, `msgb_pull_u8`, `msgb_pull_u16`, `msgb_pull_u32`, `msgb_l1`, `msgb_l2`, `msgb_l3`, `msgb_l4`, `msgb_sms`, `msgb_l1len`, `msgb_l2len`, `msgb_l3len`, `msgb_l4len`, `msgb_pull_to_l2`, `msgb_pull_to_l3`, `msgb_l3trim`, `msgb_push_tl` | Inline/macro operations in `msgb.h`; endian operations in generated headers | `include/osmocom/core/msgb.h`, `bit16gen.h`, `bit32gen.h` |
| `hexparse` | `osmo_hexparse` | `src/core/utils.c` | `include/osmocom/core/utils.h` |
| `msgb` | `msgb_data`, `msgb_length`, `msgb_headlen`, `msgb_headroom`, `msgb_tailroom`, `msgb_reserve`, `msgb_put`, `msgb_get`, `msgb_push`, `msgb_pull`, `msgb_trim`, `msgb_reset`, `msgb_test_invariant`; basic allocation/free equivalents via `Msgb::new`, `Msgb::with_headroom`, and drop | `src/core/msgb.c` (most primitives are inline in the header) | `include/osmocom/core/msgb.h` |
| `msgb` | Queue/count/free helpers; all slice/message equality helpers; dump/buffer/layer helpers; `msgb_hexdump_c` via `msgb_hexdump_owned`; `msgb_printf` via Rust formatting and `msgb_append_str`. Allocation/context/logging inventory in the message-buffer README. | `src/core/msgb.c`, `src/core/utils.c`; inline queue, comparison, and layer dump helpers | `include/osmocom/core/msgb.h`, `utils.h`, `linuxlist.h` |
| `msgb`| `msgb_copy`, `msgb_copy_resize`, `msgb_resize_area`; `msgb_copy_c` / `msgb_copy_resize_c` buffer behavior represented by owned Rust copies without context/name arguments | `src/core/msgb.c` | `include/osmocom/core/msgb.h` |
| `crc` (byte) | `osmo_crc16`, `osmo_crc16_byte`, `osmo_crc16_table`, `osmo_crc16_ccitt`, `osmo_crc16_ccitt_byte`, `osmo_crc16_ccitt_table` | `src/core/crc16.c` | `include/osmocom/core/crc16.h` |
| `crc` (generic) | `osmo_crc{8,16,32,64}gen_code` and each width's `compute_bits`, `check_bits`, `set_bits` | `src/core/crcXXgen.c.tpl` and generated `crc{8,16,32,64}gen.c` | `include/osmocom/core/crcgen.h`, `crcXXgen.h.tpl` and generated headers |
| `conv` (descriptor/encoder) | `osmo_conv_code` via checked `ConvCode`, `osmo_conv_get_input_length`, `osmo_conv_get_output_length`, `osmo_conv_encode_init`, `osmo_conv_encode_load_state`, `osmo_conv_encode_raw`, `osmo_conv_encode_flush`, `osmo_conv_encode`; separate operation-derived capacity helpers | `src/core/conv.c` | `include/osmocom/core/conv.h` |
| `conv` (generic stateful decoder) | `osmo_conv_decode_init` via `ConvDecoder::new`, `osmo_conv_decode_reset`, `osmo_conv_decode_rewind`, `osmo_conv_decode_scan`, `osmo_conv_decode_flush`, `osmo_conv_decode_get_best_end_state`, `osmo_conv_decode_get_output`; `osmo_conv_decode_deinit` via `Drop` | `src/core/conv.c` | `include/osmocom/core/conv.h` |
| `conv` (portable correlation/public dispatch) | `osmo_conv_decode_acc`, `osmo_conv_decode`; `ConvCode::decode_input_capacity` for actual soft-byte sizing | `src/core/conv.c`, `src/core/conv_acc.c`, `src/core/conv_acc_generic.c` (scalar metrics; SSE/AVX evidence is separate) | `include/osmocom/core/conv.h`; former `src/core/conv_acc.h` entry is stale/absent, with no implementation provenance |
| `prim` | `osmo_prim_hdr`, `osmo_prim_init`, operation constants/name table and `osmo_prim_operation_name`, SAP base constants, `OSMO_PRIM`, `OSMO_PRIM_HDR`, `osmo_prim_event_map`, `OSMO_NO_EVENT`, `osmo_event_for_prim`; typed Rust callback equivalent | `src/core/prim.c`; name fallback in `src/core/utils.c` | `include/osmocom/core/prim.h`, `utils.h` |
| `timer` | `osmo_timer_setup`, `osmo_timer_add`, `osmo_timer_schedule`, `osmo_timer_del`, `osmo_timer_pending`, `osmo_timer_remaining`, `osmo_timers_prepare`, `osmo_timers_nearest`, `osmo_timers_nearest_ms`, `osmo_timers_update`, `osmo_timers_check`; owned scheduler/handles | `src/core/timer.c` | `include/osmocom/core/timer.h`, `timer_compat.h` |
| `fsm` | Owned registration/lookup, instance allocation/free, dispatch, all five state changes, timeout callbacks, termination/hierarchy/broadcast, ID update/format/sanitize and name helpers; preferred methods take explicit scheduler; distinct C adapters live under `compat::fsm` | `src/core/fsm.c`; naming/identifier helpers in `src/core/utils.c` | `include/osmocom/core/fsm.h`, `utils.h` |
| `tdef` | `osmo_tdef_unit_name`, unit-name table, `osmo_tdefs_reset`, `osmo_tdef_get`, `osmo_tdef_get_entry`, `osmo_tdef_set`, `osmo_tdef_val_in_range`, `osmo_tdef_range_str_buf`, `osmo_tdef_get_state_timeout`, `osmo_tdef_fsm_inst_state_chg` via `FsmRuntime::state_change_with_tdef`, sentinel/group iteration | `src/core/tdef.c` | `include/osmocom/core/tdef.h` |
| `rate_ctr` | `rate_ctr_add`, inline `rate_ctr_inc`, `rate_ctr_difference`, `rate_ctr_reset`; owned current/previous and four interval pairs; `CounterSink` implementation | `src/core/rate_ctr.c` | `include/osmocom/core/rate_ctr.h` |
| `rate_ctr` | Owned equivalents of `rate_ctr_group_alloc/free/set_name/get_ctr`, inline index/add/inc helpers, `rate_ctr_get_group_by_name_idx`, `rate_ctr_get_by_name`, `rate_ctr_for_each_counter/group`, `rate_ctr_group_reset`; static interval engine replay and portable monotonic driver; native `rate_ctr_init`/select/timerfd characterized only | `src/core/rate_ctr.c`, `utils.c` (identifier validation), `select.c` (native evidence only) | `include/osmocom/core/rate_ctr.h`; `select.h` for evidence only |
| `time_accounting` | `osmo_time_cc_init`, `osmo_time_cc_set_flag`, `osmo_time_cc_cleanup` via owned `TimeCc` methods; optional `CounterSink`, mutable configuration and explicit scheduler dispatch | `src/core/time_cc.c`, `src/core/rate_ctr.c` (`rate_ctr_add` sink only) | `include/osmocom/core/time_cc.h`, `rate_ctr.h` |
| `time` | `osmo_gettimeofday`, `osmo_clock_gettime`, override globals via `ClockOverrides`, `osmo_gettimeofday_override_add`, `osmo_clock_override_enable`, `osmo_clock_override_add`, `osmo_clock_override_gettimespec`; raw time arithmetic/presence/clear/comparison and seconds helpers | `src/core/timer_gettimeofday.c`, `src/core/timer_clockgettime.c` | `include/osmocom/core/timer.h`, `timer_compat.h` |
| `gsmtap` | `gsmtap_makemsg_ex`, `gsmtap_makemsg` | `src/core/gsmtap_util.c` | `include/osmocom/core/gsmtap_util.h` |
