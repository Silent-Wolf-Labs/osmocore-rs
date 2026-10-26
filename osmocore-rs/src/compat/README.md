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
| [base64.rs](base64.rs) | Integer statuses, output lengths, termination and nullable size probes | [base64](../base64.rs) |
| [bcd.rs](bcd.rs) | Signed lengths, terminated inputs/outputs and buffer mutation | [bcd](../bcd.rs) |
| [bitvec.rs](bitvec.rs) | Cursor, buffer, sentinel and partial-progress operations | [bitvec](../bitvec.rs) |
| [conv.rs](conv.rs) | Signed lengths, low-level encode/scan contracts and read-only decoder inspection | [convolutional coding](../conv/README.md) |
| [crc.rs](crc.rs) | Signed bit counts, check/set statuses and generated CRC table metadata | [crc](../crc.rs) |
| [fsm.rs](fsm.rs) | Integer error mapping, names and identifier-formatting adapters | [FSMs](../fsm/README.md) |
| [gsmtap.rs](gsmtap.rs) | Positional packet constructors returning owned wire bytes | [GSMTAP](../gsmtap/README.md) |
| [msgb.rs](msgb.rs) | Buffer/layer operations, comparisons, queues and bounded formatting | [message buffers](../msgb/README.md) |
| [prim.rs](prim.rs) | Initialization, event-map sentinel and lookup contracts | [primitive metadata](../prim.rs) |
| [tdef.rs](tdef.rs) | Sentinel tables/groups, fallback lookup, status mapping and range buffers | [timeout definitions](../tdef.rs) |
| [timer.rs](timer.rs) | Optional-handle pending query | [timer scheduler](../timer.rs) |

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
