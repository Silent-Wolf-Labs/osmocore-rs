# Compatibility adapters

`compat::<subsystem>` exposes safe Rust adapters for selected observed C
contracts. Use the functional modules' owned types and methods for new code;
choose an adapter when integer statuses, sentinel inputs, signed lengths,
terminated buffers, mutation order or conformance inspection are needed.

This namespace is always compiled. There is no `compat` Cargo feature, and
`net`, `runtime` and `tracing` do not control adapter availability. Adapters
share the functional implementations and owned types; they require no upstream
C library and export no C ABI.

## Adapter modules

| Module | Distinct contract | Functional implementation |
| --- | --- | --- |
| [a5.rs](a5.rs) | Cipher numbers as integers, -95 and -22 results, optional key and output directions | [a5](../gsm/a5.rs) |
| [auth.rs](auth.rs) | C's subscriber and vector structs, integer algorithms and results, SQN changes on a failed resync, names for unknown values | [auth](../gsm/auth/mod.rs), [milenage](../gsm/milenage.rs) |
| [base64.rs](base64.rs) | Integer statuses, output lengths, termination and nullable size probes | [base64](../base/base64.rs) |
| [bcd.rs](bcd.rs) | Signed lengths, terminated inputs/outputs and buffer mutation | [bcd](../base/bcd.rs) |
| [bitvec.rs](bitvec.rs) | Cursor, buffer, sentinel and partial-progress operations | [bitvec](../base/bitvec/mod.rs) |
| [conv.rs](conv.rs) | Signed lengths, low-level encode/scan contracts and read-only decoder inspection | [convolutional coding](../base/conv/README.md) |
| [crc.rs](crc.rs) | Signed bit counts, check/set statuses and generated CRC table metadata | [crc](../base/crc.rs) |
| [fsm.rs](fsm.rs) | Integer error mapping, names and identifier-formatting adapters | [FSMs](../base/fsm/README.md) |
| [gsm0502.rs](gsm0502.rs) | Integer comparison, input-returning remap, negative CCCH codes and wrapping paging values | [gsm0502](../gsm/gsm0502.rs) |
| [gsm0503.rs](gsm0503.rs) | Integer status and output parameters for xCCH coding | [gsm0503](../coding/gsm0503.rs) |
| [gsm_utils.rs](gsm_utils.rs) | Integer band and type values, -22 and 0xffff error results, `gsm_time` fields that can disagree, bounded and NUL-terminated 7-bit buffers, wrapped header lengths | [gsm_utils](../gsm/gsm_utils/mod.rs) |
| [sockaddr_str.rs](sockaddr_str.rs) | C's struct with any family, errno results, writes made before a failure, glibc's address text, Linux socket address layouts, comparison that falls back to text | [sockaddr_str](../base/sockaddr_str.rs) |
| [strrb.rs](strrb.rs) | C's size with one unused slot, 16-bit sizes, slot checks, indexes past the newest string, text up to a NUL | [strrb](../base/strrb.rs) |
| [use_count.rs](use_count.rs) | Integer results, a callback after each change, optional counters, bounded text buffers | [use_count](../base/use_count.rs) |
| [tlv.rs](tlv.rs) | Integer status, partly written parse outputs, truncated and out-of-range lengths, C's merge and copy results | [tlv](../gsm/tlv/mod.rs) |
| [gsmtap.rs](gsmtap.rs) | Positional packet constructors returning owned wire bytes | [GSMTAP](../base/gsmtap/README.md) |
| [msgb.rs](msgb.rs) | Buffer/layer operations, comparisons, queues and bounded formatting | [message buffers](../base/msgb/README.md) |
| [prim.rs](prim.rs) | Initialization, event-map sentinel and lookup contracts | [primitive metadata](../base/prim.rs) |
| [tdef.rs](tdef.rs) | Sentinel tables/groups, fallback lookup, status mapping and range buffers | [timeout definitions](../base/tdef.rs) |
| [timer.rs](timer.rs) | Optional-handle pending query | [timer scheduler](../base/timer.rs) |

[mod.rs](mod.rs) declares the public namespace and links to this guide.
Subsystem files re-export the relevant functional operations or provide
small adapters. Clocks, accounting and rate counters use their functional types
and methods directly; matching an upstream family name does not require an
additional compatibility module.

## Safety and evidence boundaries

An `osmo_*` name identifies a selected contract, not complete upstream API
coverage or an identical Rust signature. Each operation defines whether errors
preserve the destination or retain a successful mutation prefix. Rust bounds
checks, checked handle ownership and typed errors still apply. There is no
pointer/layout, allocator-failure or undefined-behavior parity guarantee.

`compat::conv::diagnostics` borrows read-only decoder state for conformance
replay. It is separate from the optional event/span instrumentation documented
by the [diagnostics module](../diagnostics/README.md).

Repository integration tests replay frozen C observations and supplement them
with independent Rust safety and consumer checks. Tests are grouped by
functional subsystem under `tests/`; the adapters do not have a parallel
implementation or a separate fixture oracle. The
[public API tests](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/public_api.rs) also check the public interface
boundaries. Ordinary package builds need no C checkout, harness or evidence
fixtures.

The [source guide](../README.md#converted-c-api-mapping) maps selected C APIs to
Rust families. The consolidated
[compatibility status](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#compatibility-status) records
supported families and exclusions, and the [test guide](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md)
documents evidence and regeneration procedures.
