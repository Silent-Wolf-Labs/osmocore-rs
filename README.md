# osmocore-rs

Rust-native telecom and core primitives with owned buffers, explicit clocks,
checked handles and typed errors. The project provides selected libosmocore
contracts through Rust APIs with explicit ownership and documented safety rules.
Behavior is defined by recorded libosmocore outputs and implemented as idiomatic
Rust rather than translated C; see the [development approach](docs/README.md#development-approach).

The crate lives in [osmocore-rs/](osmocore-rs). Its Cargo package is
`osmocore-rs`; Rust imports use `osmocore_rs`. The current release is `0.1.0`.

## Capabilities

Message buffers and primitive metadata; bit/bit-vector, Base64/BCD/hexadecimal,
CRC and convolutional coding; timers, FSMs, time accounting and rate counters;
GSMTAP Vec/Msgb encoding and generic UDP runtime integration. Distinct observed
C contracts use always-compiled `compat::<subsystem>` adapters.

| Feature | Capability |
| --- | --- |
| `tracing` | Structured diagnostics with application-owned subscribers, filters and output |
| `net` | TCP/UDP establishment, explicit resolution, local binding and pre-bind options |
| `runtime` | Mio readiness, bounded output, typed commands/drivers and coordinated shutdown |

Default features are empty. All three features select independently.

## Usage

Add the crate from crates.io, selecting optional features as needed:

```toml
[dependencies]
osmocore-rs = "0.1"
# osmocore-rs = { version = "0.1", features = ["net", "runtime", "tracing"] }
```

```rust
use osmocore_rs::{base64, msgb::Msgb};

let encoded = base64::encode(b"telecom").unwrap();
assert_eq!(base64::decode(encoded.as_bytes()).unwrap(), b"telecom");
let mut message = Msgb::with_headroom(16, 4).unwrap();
message.put_u16(0x1234).unwrap();
assert_eq!(message.data(), &[0x12, 0x34]);
```

See the [crate README](osmocore-rs/README.md) for feature-specific usage and the
[source overview](osmocore-rs/src/README.md) for API and subsystem guides.

## Support and documentation

Initial validated platform: Linux x86_64. Rust edition: 2024. Toolchain policy:
current stable. MSRV: not declared. Compatibility is limited to documented APIs
and recorded inputs; the crate provides no C ABI or full libosmocore parity.

- [Architecture and compatibility](docs/README.md): design, diagrams and supported boundaries.
- [Changelog](CHANGELOG.md): unreleased user-facing changes.
- [Test guide](osmocore-rs/tests/README.md): fixture evidence and developer checks.
- [Repository](https://github.com/Silent-Wolf-Labs/osmocore-rs).

## License

`GPL-3.0-or-later`. See [LICENSE](osmocore-rs/LICENSE). Copyright for original
Rust contributions is held by Josh Shilts.
