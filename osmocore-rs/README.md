# osmocore-rs

Rust-native telecom and core primitives with owned buffers, explicit clocks and
schedulers, checked handles and typed errors. Current stable Rust, edition 2024;
no declared MSRV. Initial validated platform: Linux x86_64.

Repository: [Silent-Wolf-Labs/osmocore-rs](https://github.com/Silent-Wolf-Labs/osmocore-rs).
The Cargo package is `osmocore-rs`; Rust imports use `osmocore_rs`.

## What it provides

- Message buffers and inter-layer primitive metadata.
- Bit/bitvec helpers, Base64, BCD and hex parsing.
- CRC and convolutional coding.
- Timers, timeout definitions and cumulative time accounting.
- Finite-state machines and rate counters.
- GSMTAP v2 packet construction.

## Compatibility

Behavior is defined by thousands of recorded outputs from direct calls into
Osmocom/libosmocore. The C source was consulted to understand that behavior and
decide what carries over to Rust; the implementation is idiomatic Rust verified
against those observations, not translated C. See the [development approach](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#development-approach).
Each subsystem documents its supported contracts, buffer mutations and
deliberate Rust safety differences.

## Usage

Functional modules live at the crate root:

- `base64`, `bcd`, `hexparse`, `bits`, `bitvec`, `crc`, `conv`: encoding and buffers.
- `msgb`, `prim`, `gsmtap`: message storage, primitive metadata, GSMTAP Vec/Msgb encoding and optional generic UDP runtime integration.
- `time`, `timer`, `tdef`, `time_accounting`: clocks, scheduling, timeout definitions and accounting.
- `fsm`, `rate_ctr`: owned state-machine and counter-group runtimes.

Use methods on owned types such as `Msgb`, `FsmRuntime`, `TimerScheduler` and
`RateCtrRuntime`. Clocks and scheduler ownership are explicit; runtime handles
are checked for identity and lifetime. `BitVec` borrows caller-owned storage.

```rust
use osmocore_rs::{base64, msgb::Msgb};

let encoded = base64::encode(b"telecom").unwrap();
assert_eq!(base64::decode(encoded.as_bytes()).unwrap(), b"telecom");

let mut message = Msgb::with_headroom(16, 4).unwrap();
message.put_u16(0x1234).unwrap();
assert_eq!(message.data(), &[0x12, 0x34]);
```

Distinct C contracts live under `compat::<subsystem>`: integer statuses, output
parameters, signed lengths, sentinel tables, terminated buffers and conformance
inspection. These are safe Rust adapters, not a C ABI. Ordinary Rust functions
such as `hexparse::parse_into`, `bits::pack_into` and `conv::encode` use direct
names; forwarding aliases to existing methods have been removed.

Observed behavior is preserved, including partial writes on some errors,
raw numeric values and callback order. Buffer and compatibility documentation
specifies each operation's mutation rules. Owned Base64/BCD outputs omit C
terminators; BCD string inputs reject embedded NUL.

The package includes Rust source, source unit tests, examples and the GPLv3
text. Large evidence files and repository
integration tests are excluded. Package builds need neither C nor a sibling
project. GSMTAP support constructs packets; it does not provide a general parser. Native select/timerfd and additional upstream conversions are outside this scope.

## License

This project is licensed under the GNU General Public License, version 3 or
(at your option) any later version (`GPL-3.0-or-later`). See [LICENSE](LICENSE)
for the complete GPLv3 text. Copyright for the original Rust contributions is
held by Josh Shilts.

## Functional documentation

[Library source overview](src/README.md) documents utilities, API ownership,
C-source mapping, validation and package boundaries. Detailed subsystem guides:
[convolutional coding](src/conv/README.md), [FSMs](src/fsm/README.md),
[GSMTAP](src/gsmtap/README.md), [message buffers](src/msgb/README.md),
[rate counters](src/rate_ctr/README.md), and [time/timing](src/time/README.md).
Each records completed work, tests, remaining gaps and Rust/C differences.

## Structured diagnostics

The optional `tracing` feature instruments FSM execution, timer scheduling,
runtime lifecycle/I/O, network establishment and GSMTAP datagram preparation. Defaults remain empty. Stable targets
are `osmocore_rs::fsm`, `osmocore_rs::timer`, `osmocore_rs::net`,
`osmocore_rs::runtime` and `osmocore_rs::gsmtap`; applications own the subscriber,
filters, formatting and output. No logger is installed by the library. See the
[diagnostics contract and application example](src/diagnostics/README.md) for levels,
fields, context propagation and deferred logging capabilities.

## Networking establishment

Enable `net` for explicit IPv4/IPv6 resolution, TCP listeners, bounded TCP
connect (including local source binding), and bound/connected UDP sockets.
Pre-bind options cover reuse, IPv6-only mode and DSCP; returned standard-library
sockets can be blocking or nonblocking. TCP establishment itself is synchronous
with a caller-supplied timeout. `net` and `tracing` are independent, optional
features with empty defaults; together they emit `osmocore_rs::net` diagnostics.
See the [networking contract](src/net/README.md) for errors, examples, C capability
mapping, platform limits and deferred functionality. Select `runtime` separately for the optional I/O reactor.

## Synchronous I/O runtime

Enable `runtime` for `IoRuntime<C>`, typed cross-thread commands, callback-deferred
socket/output effects, bounded TCP/UDP queues, typed component drivers and
coordinated drain. Application state remains single-threaded and application-owned.
The feature is independent of `net` and `tracing`; defaults remain empty.
See [runtime contracts and limits](src/runtime/README.md).
Run the finite acceptance service with:

```sh
cargo run --no-default-features --features runtime --example runtime_service
```

Linux x86_64 is the initial validated target. Pending TCP connect remains deferred.

Default features are empty (`default = []`). `tracing`, `net` and `runtime`
are independently selectable, including every combination. `compat::<subsystem>`
adapters are always compiled and are not a Cargo feature.
