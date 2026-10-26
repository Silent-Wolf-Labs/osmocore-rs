# Changelog

## [0.1.0] - 2026-10-22

First release.

### Added

- Rust-native message buffers, primitive metadata, bit/bit-vector helpers,
  Base64/BCD/hexadecimal conversion, CRC and convolutional coding.
- Explicit clocks, checked timer schedulers, timeout definitions and owned FSMs
  with hierarchy, transitions, teardown and timer integration.
- Rate counters, groups, monotonic rollups and cumulative time accounting.
- Optional `tracing` diagnostics for FSMs, timers, runtime/transport, networking
  establishment and GSMTAP preparation, with application-owned subscribers.
- Optional `net` helpers for IPv4/IPv6 resolution, TCP/UDP setup, pre-bind options,
  local binding and caller-bounded completed TCP connection attempts.
- Independent Mio-backed synchronous `runtime` with checked resource handles,
  bounded TCP/UDP output, typed cross-thread commands, deferred callback effects,
  typed component drivers and coordinated shutdown.
- GSMTAP v2 Vec/Msgb encoding and generic UDP runtime integration.
- Safe, always-compiled `compat::<subsystem>` adapters for selected observed C
  contracts. Defaults are empty; `tracing`, `net` and `runtime` select independently.

### Changed

- Runtime initialization and registration errors preserve I/O source errors;
  generic runtime failures support conventional formatting and conditional
  `std::error::Error` propagation without additional state or command bounds.
- The standalone composition/round-trip example requests shutdown on worker
  unwind and bounds the scenario with an independent watchdog.

### Compatibility

- Selected behavior is checked against recorded Osmocom/libosmocore observations,
  with documented Rust safety differences. No C ABI or full upstream parity is
  promised. See [compatibility status](docs/README.md#compatibility-status).
- Pending TCP connect, SCTP/TLS, async runtime integration, native C select/timerfd
  compatibility, exporters, VTY/CTRL, supervision and general GSMTAP parsing remain
  intentionally excluded.
- Licensing is `GPL-3.0-or-later`; see [LICENSE](osmocore-rs/LICENSE).

### Platform

- Initial validated platform: Linux x86_64.
- Rust edition: 2024. Toolchain policy: current stable. MSRV: not declared.
- Broader runtime portability has not been validated; subsystem guides describe
  platform-specific clock and compatibility boundaries.
