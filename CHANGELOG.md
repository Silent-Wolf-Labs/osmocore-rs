# Changelog

## [Unreleased]

### Fixed

- `conv::decode` and `osmo_conv_decode_acc` no longer return `ArithmeticOverflow`
  for long unpunctured K=5 and K=7 frames at or near full scale. Path metrics now
  saturate like the C SSE and AVX backends. Affected frames include clean +/-127
  xCCH blocks.

### Changed

- `TimerId`, `FsmId`, `FsmDefinitionId` and `RateCtrGroupId` are now `Copy`, `Send`
  and `Sync`, and `TimerScheduler` and `RateCtrRuntime` can move between threads.
  Existing code still compiles, but `.clone()` on these handles now triggers
  Clippy's `clone_on_copy`. `FsmRuntime` and `IoRuntime` stay on one thread.
- `conv::decode` is faster for K=5 and K=7 codes, and so is `xcch_decode`, because
  the compiler now vectorizes the correlation decoder's main loop. Results are
  unchanged. See `docs/benchmarks.md`.
- `TimerScheduler::update` is faster when nothing is due and for small batches,
  because it returns early and reuses its batch buffer, which stays allocated at
  the largest batch size so far. Very large batches are about 2% slower. See
  `docs/benchmarks.md`.
- The 10 largest test fixtures are stored gzip-compressed. Their decompressed
  content and fingerprints are unchanged.

### Added

- `xcch_demo` example, which sends a BCCH System Information block through a noisy
  channel and out as GSMTAP to the gsmtap-rs workbench or Wireshark.
- `Msgb::put_slice` and `Msgb::push_slice`, which append and prepend bytes in one call.
- `gsmtap::GSMTAP_UDP_PORT` and the `gsmtap::message_type` and `gsmtap::channel`
  constants, checked against the C header.
- `bits::to_soft` and `bits::from_soft`, for C's `osmo_ubit2sbit` and `osmo_sbit2ubit`,
  checked against the C library.
- `PrbsState::next_bits`, which reads up to 64 bits as one number.
- `gsm0503::XCCH_STEALING_FLAG_OFFSETS`, where the stealing flags sit in a burst.
- `strrb` module with a ring of recent log strings, checked against the C library.
- `sockaddr_str` module with IP addresses kept as text with a port, checked against the C library.
- `use_count` module with named use counters for one object, checked against the C
  library.
- `a5` module with the A5/0, A5/1 and A5/2 cipher streams, checked against the C
  library.
- `gsm_utils` module with GSM bands, ARFCN and frequency conversion, mobile power
  levels, RxLev, a `GsmTime` type, reduced frame numbers, TLLIs, classmark A5 support,
  RACH parameters and SMS and USSD 7-bit coding with user data header support,
  checked against the C library.
- `tlv` module for parsing, encoding and validating tag-length-value messages, with
  values borrowed from the input and an encoding method on `Msgb` for each format,
  checked against the C library.
- `gsm0503` module with SACCH, SDCCH and BCCH block encoding and soft decoding,
  including bit-error counts, checked against the C library.
- `gsm0502` module with TDMA frame arithmetic, burst sizes, block starts, CCCH
  blocks, frequency hopping and paging groups, checked against the C library.
- `prbs` module with the ITU-T O.150 PRBS-7, 9, 11 and 15 sequences and
  caller-defined ones, checked against the C generator.
- Criterion benchmarks for CRC, convolutional coding, bit packing, message buffers,
  timers and FSMs, with tracked results in [docs/benchmarks.md](docs/benchmarks.md).

## [0.1.0] - 2026-10-26

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
