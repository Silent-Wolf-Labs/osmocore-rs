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
| [Core utilities](base/README.md) | The `base` group, C's `src/core`: bits, encodings, CRCs, messages, time, FSMs, counters and small utilities |
| [GSM utilities](gsm/README.md) | The `gsm` group, C's `src/gsm`: TDMA frames, bands and power, A5, GSM authentication, 7-bit coding and TLV |
| [Channel coding](coding/README.md) | The `coding` group, C's `src/coding`: GSM 05.03 xCCH |
| [Convolutional coding](base/conv/README.md) | Checked descriptors, encoding, stateful decoding, portable correlation and dispatch |
| [FSMs](base/fsm/README.md) | Owned registry/runtime, hierarchy, transitions, termination and timer/TDef integration |
| [I/O runtime](runtime/README.md) | Optional Mio readiness, bounded TCP/UDP, typed commands/drivers and coordinated shutdown |
| [Networking](net/README.md) | Optional IPv4/IPv6 TCP/UDP establishment, local source binding, pre-bind options and bounded connect |
| [GSMTAP](base/gsmtap/README.md) | GSMTAP v2 Vec/Msgb encoding, UM defaults and generic UDP runtime integration |
| [Message buffers](base/msgb/README.md) | Owned storage, typed/layer operations, copy/resize, queues, comparisons and formatting |
| [Rate counters](base/rate_ctr/README.md) | Counter arithmetic, owned groups/registry, interval rollups and monotonic polling |
| [Time and timing](base/time/README.md) | Raw clocks, scheduling, timeout definitions and detailed accounting edge cases |
| [Time accounting](base/time_accounting/README.md) | Owned accumulator/configuration, counter sinks, policy boundaries, clock roles and callback errors |
| [Compatibility adapters](compat/README.md) | Always-compiled adapters, functional alternatives and conformance-inspection boundaries |
| [Structured diagnostics](diagnostics/README.md) | Structured targets/events, fields, severity, span lifetime and subscriber example |

Modules without a folder of their own are described in their group's README. Each
subsystem README records its completed scope, validation, outstanding work and Rust/C
differences.

## Public API and architecture

Functional modules are grouped as `base`, `gsm` and `coding`, following libosmocore's
`src/core`, `src/gsm` and `src/coding`; `runtime`, `net`, `diagnostics` and `compat` stay at
the crate root. There is no root type glob. Prefer methods on owned `Msgb`, `FsmRuntime`, `TimerScheduler`
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
contains one adapter module per subsystem, at its root rather than in groups.
`base/bitvec/`, `gsm/gsm_utils/` and `gsm/tlv/` split large modules into private files
without changing their paths. `base/timer.rs` remains a single cohesive
scheduler: handles, activation ordering and callback dispatch share its state.
`time_accounting/` separates its public adapter from private event transitions
and policy/TDef resolution. `diagnostics/README.md` owns the public diagnostics reference;
`diagnostics/mod.rs` includes it in Rustdoc and contains crate-private instrumentation helpers.
Test placement follows the [test-location convention](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/osmocore-rs/tests/README.md).

Migration from 0.1:

- Import modules from their group: `osmocore_rs::bits` is now `osmocore_rs::base::bits`,
  `osmocore_rs::tlv` is `osmocore_rs::gsm::tlv` and `osmocore_rs::gsm0503` is
  `osmocore_rs::coding::gsm0503`. Item names are unchanged.
- `compat::<subsystem>`, `runtime`, `net` and `diagnostics` keep their paths.

The large subsystems use private implementation files; timer, timeout-definition
and cumulative-accounting APIs remain separate concepts, rather than being
nested under the public `time` namespace. Their relationships are documented
in [the timing README](base/time/README.md).

## Time accounting

[Time accounting](base/time_accounting/README.md) owns one cumulative accumulator, mutable
configuration and an optional counter sink. Pair it with an explicit
`TimerScheduler<C, TimeCc<...>>`; `set_flag` changes activity and `update`
dispatches callbacks while surfacing deferred accounting errors. Callers that
dispatch the scheduler directly must consume `take_callback_error` themselves.
Elapsed accounting uses monotonic time; wall time gates scheduler wakeups.
The optional runtime's accounting driver preserves those separate clock roles.

Configuration and reporting have event-specific observation boundaries; completed
accounting and sink effects remain visible after a later fallible lookup or
timer operation. Cleanup releases the timer and resets accounting while keeping
configuration and sink totals. The [accounting guide](base/time_accounting/README.md)
documents ownership and entry points; the [timing contract](base/time/README.md#cumulative-accounting)
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

Modules are grouped like libosmocore's sources. Each group README lists its modules
and describes the ones without a folder of their own:

- [Core utilities](base/README.md): `base`, for C's `src/core`.
- [GSM utilities](gsm/README.md): `gsm`, for C's `src/gsm`.
- [Channel coding](coding/README.md): `coding`, for C's `src/coding`.

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
observations. Subsystem READMEs hold the direct-C audits and regeneration
instructions.

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
| `bits` | `osmo_ubit2pbit`, `osmo_pbit2ubit`, `osmo_ubit2pbit_ext`, `osmo_pbit2ubit_ext`, `osmo_ubit2sbit` and `osmo_sbit2ubit` as `to_soft` and `from_soft` on one bit, for use with `map` | `src/core/bits.c` | `include/osmocom/core/bits.h` |
| `bitvec` (primitives) | `bitvec_get_bit_pos`, `bitvec_get_bit_pos_high`, `bitvec_set_bit_pos`, `bitvec_set_bit`, `bitvec_get_bit_high`, `bitvec_set_bits`, `bitvec_zero`, `bitvec_used_bytes`, `bitvec_tailroom_bits`, `bit_value_to_char` | `src/core/bitvec.c` (capacity helpers are inline in the header) | `include/osmocom/core/bitvec.h` |
| `bitvec` (integer and buffer operations) | `bitvec_set_u64`, `bitvec_set_uint`, `bitvec_get_uint`, `bitvec_get_int16_msb`, `bitvec_fill`, `bitvec_spare_padding`, `bitvec_get_bytes`, `bitvec_set_bytes` | `src/core/bitvec.c` | `include/osmocom/core/bitvec.h` |
| `bitvec` (indexed operations and helpers) | `bitvec_read_field`, `bitvec_write_field`, `bitvec_get_nth_set_bit`, `bitvec_find_bit_pos`, `bitvec_rl`, `bitvec_rl_curbit`, `bitvec_shiftl`, `bitvec_add_array` | `src/core/bitvec.c` | `include/osmocom/core/bitvec.h` |
| `bitvec` (buffer and text adapters) | `bitvec_pack`, `bitvec_unpack`, `bitvec_unhex`, `bitvec_to_string_r` | `src/core/bitvec.c`; hex loading calls `osmo_hexparse` in `src/core/utils.c` | `include/osmocom/core/bitvec.h` |
| `msgb` | `msgb_put_u8`, `msgb_put_u16`, `msgb_put_u32`, `msgb_push_u8`, `msgb_push_u16`, `msgb_push_u32`, `msgb_get_u8`, `msgb_get_u16`, `msgb_get_u32`, `msgb_pull_u8`, `msgb_pull_u16`, `msgb_pull_u32`, `msgb_l1`, `msgb_l2`, `msgb_l3`, `msgb_l4`, `msgb_sms`, `msgb_l1len`, `msgb_l2len`, `msgb_l3len`, `msgb_l4len`, `msgb_pull_to_l2`, `msgb_pull_to_l3`, `msgb_l3trim`, `msgb_push_tl` | Inline/macro operations in `msgb.h`; endian operations in generated headers | `include/osmocom/core/msgb.h`, `bit16gen.h`, `bit32gen.h` |
| `hexparse` | `osmo_hexparse` | `src/core/utils.c` | `include/osmocom/core/utils.h` |
| `gsm0503` | `gsm0503_xcch_encode`, `gsm0503_xcch_decode`, `gsm0503_xcch_interleave`, `gsm0503_xcch_deinterleave`, `gsm0503_xcch_burst_map`, `gsm0503_xcch_burst_unmap`, `gsm0503_xcch`, `gsm0503_fire_crc40`; xCCH only | `src/coding/gsm0503_coding.c`, `gsm0503_interleaving.c`, `gsm0503_mapping.c`, `gsm0503_parity.c`, generated `src/gsm/gsm0503_conv.c` | `include/osmocom/coding/gsm0503_*.h`, `include/osmocom/gsm/gsm0503.h` |
| `gsm0502` | `GSM_TDMA_FN_SUM`, `GSM_TDMA_FN_SUB`, `GSM_TDMA_FN_DIFF`, `GSM_TDMA_FN_INC`, `GSM_TDMA_FN_DEC`, `gsm0502_fncmp`, `gsm0502_fn_remap`, `gsm0502_fn2ccch_block`, `gsm0502_hop_seq_gen` (frame-number input, no `gsm_time`), `gsm0502_get_n_pag_blocks`, `gsm0502_get_ccch_group`, `gsm0502_get_paging_group`, `gsm0502_calc_paging_group`, `gsm48_number_of_paging_subchannels`, `rsl_ccch_conf_to_bs_cc_chans`, `GSM_NBITS_*` | `src/gsm/gsm0502.c`, `gsm48.c`, `rsl.c` | `include/osmocom/gsm/gsm0502.h` |
| `sockaddr_str` | `osmo_ip_str_type`, `osmo_sockaddr_str_is_set`, `osmo_sockaddr_str_is_nonzero`, `osmo_sockaddr_str_cmp`, `osmo_sockaddr_str_from_str`, `osmo_sockaddr_str_from_str2`, `osmo_sockaddr_str_from_in_addr`, `osmo_sockaddr_str_from_in6_addr`, `osmo_sockaddr_str_from_32`, `osmo_sockaddr_str_from_32h`, `osmo_sockaddr_str_from_sockaddr_in`, `osmo_sockaddr_str_from_sockaddr_in6`, `osmo_sockaddr_str_from_sockaddr`, `osmo_sockaddr_str_to_in_addr`, `osmo_sockaddr_str_to_in6_addr`, `osmo_sockaddr_str_to_32`, `osmo_sockaddr_str_to_32h`, `osmo_sockaddr_str_to_sockaddr_in`, `osmo_sockaddr_str_to_sockaddr_in6`, `osmo_sockaddr_str_to_sockaddr`, `OSMO_SOCKADDR_STR_FMT` with `OSMO_SOCKADDR_STR_FMT_ARGS`; see below for what's left out | `src/core/sockaddr_str.c` | `include/osmocom/core/sockaddr_str.h` |
| `strrb` | `osmo_strrb_create`, `osmo_strrb_is_empty`, `osmo_strrb_get_nth`, `_osmo_strrb_is_bufindex_valid`, `osmo_strrb_elements`, `osmo_strrb_add` | `src/core/strrb.c` | `include/osmocom/core/strrb.h` |
| `use_count` | `_osmo_use_count_get_put` and `osmo_use_count_get_put`, `osmo_use_count_total`, `osmo_use_count_by`, `osmo_use_count_to_str_buf`, `osmo_use_count_name_buf`, `osmo_use_count_to_str_c`, `osmo_use_count_make_static_entries`, `osmo_use_count_free`; see below for what's left out | `src/core/use_count.c` | `include/osmocom/core/use_count.h` |
| `a5` | `osmo_a5` for A5/0, A5/1 and A5/2, `osmo_a5_fn_count`; see below for what's left out | `src/gsm/a5.c` | `include/osmocom/gsm/a5.h` |
| `comp128` | `comp128v1`, `comp128v2`, `comp128v3` | `src/gsm/comp128.c`, `src/gsm/comp128v23.c` | `include/osmocom/gsm/comp128.h`, `include/osmocom/gsm/comp128v23.h` |
| `auth` | `osmo_auth_gen_vec2` for a GSM subscriber with COMP128v1, COMP128v2, COMP128v3 or XOR-2G, as `GsmAlgorithm::vector`; see below for what's left out | `src/gsm/auth_core.c`, `auth_comp128v1.c`, `auth_comp128v23.c`, `auth_xor_2g.c` | `include/osmocom/crypt/auth.h` |
| `gsm_utils` | `gsm_band_name`, `gsm_band_parse`, `ms_class_gmsk_dbm`, `ms_pwr_ctl_lvl`, `ms_pwr_dbm`, `rxlev2dbm`, `dbm2rxlev`, `gsm_arfcn2band_rc`, `gsm_arfcn2freq10`, `gsm_freq102arfcn`, `gsm_fn2gsmtime`, `gsm_gsmtime2fn`, `gsm_fn_as_gsmtime_str`, `osmo_dump_gsmtime`, `gsm_rfn2fn`, `gsm_fn2rfn`, `gprs_tlli_type`, `gprs_tlli2tmsi`, `gprs_tmsi2tlli`, `ms_cm2_a5n_support`, `ms_cm3_a5n_support`, `rach_max_trans_val2raw`, `rach_max_trans_raw2val`, `rach_tx_integer_raw2val`, `gsm_get_octet_len`, `gsm_septet_encode`, `gsm_septet_pack`, `gsm_7bit_encode_n`, `gsm_7bit_encode_n_ussd`, `gsm_7bit_decode_n`, `gsm_7bit_decode_n_ussd`, `gsm_7bit_decode_n_hdr`; see below for what's left out | `src/gsm/gsm_utils.c` | `include/osmocom/gsm/gsm_utils.h` |
| `tlv` | `tlv_parse_one`, `tlv_parse`, `tlv_parse2`, `tlv_def_patch`, `tlv_encode_one`, `tlv_encode`, `tlv_encode_ordered`, `osmo_tlvp_merge`, `osmo_tlvp_copy`, `osmo_shift_v_fixed`, `osmo_match_shift_tv_fixed`, `osmo_shift_tlv`, `osmo_match_shift_tlv`, `osmo_shift_lv`, `osmo_tlv_prot_msg_name`, `osmo_tlv_prot_ie_name`, `osmo_tlv_prot_validate_tp`, `osmo_tlv_prot_parse`, `osmo_tlv_prot_msgt_flags`, `tlvp_val8`, `tlvp_val16be`, `tlvp_val32be`, `tlvp_val16_unal`, `tlvp_val32_unal`, `TLVP_*`, `*_GROSS_LEN`, `msgb_*` TLV helpers; not `tlv_dump` or the raw-pointer `*_put` writers | `src/gsm/tlv_parser.c` | `include/osmocom/gsm/tlv.h` |
| `prbs` | `osmo_prbs7`, `osmo_prbs9`, `osmo_prbs11`, `osmo_prbs15`, `osmo_prbs_state_init`, `osmo_prbs_get_ubit`, `osmo_prbs_get_ubits` | `src/core/prbs.c` | `include/osmocom/core/prbs.h` |
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
| `gsmtap` | `gsmtap_makemsg_ex`, `gsmtap_makemsg`; `GSMTAP_VERSION`, `GSMTAP_UDP_PORT`, `GSMTAP_TYPE_*` as `message_type`, `GSMTAP_CHANNEL_*` as `channel` | `src/core/gsmtap_util.c` | `include/osmocom/core/gsmtap_util.h`, `gsmtap.h` |

### C APIs left out of `sockaddr_str`

These are in `sockaddr_str.h` and have no Rust equivalent. Each can be added if a consumer
needs it.

| C API | Why it's left out |
| --- | --- |
| `osmo_sockaddr_str_from_osa`, `osmo_sockaddr_str_to_osa` | Take C's `struct osmo_sockaddr` from `socket.h`, which isn't ported. It's a union of the same socket addresses, so `from_sockaddr` and `to_sockaddr` give the same results |
| `osmo_sockaddr_str_from_32n`, `osmo_sockaddr_str_to_32n` | Deprecated, misnamed aliases of `from_32h` and `to_32h` |
| `OSMO_SOCKADDR_STR_FMT_ARGS_NOT_NULL` | The same text as `OSMO_SOCKADDR_STR_FMT_ARGS` for an address that isn't NULL |

### C APIs left out of `use_count`

These are in `use_count.h` and have no direct Rust equivalent. Each can be added if a
consumer needs it.

| C API | Why it's left out |
| --- | --- |
| `osmo_use_count_find`, `struct osmo_use_count_entry` | Return or expose list pointers. `UseCount::entries`, `count_of` and `UseChange::entry` cover the same uses |
| `osmo_use_count_cb_t`'s `file` and `line`, `struct osmo_use_count`'s `talloc_object` | Only for logging from C macros and for talloc. A Rust caller can capture what it needs in its callback |

### C APIs left out of `a5`

These are in `a5.h`, `a5.c` or the files they call, and have no Rust equivalent. Each can
be added if a consumer needs it.

| C API | Why it's left out |
| --- | --- |
| `osmo_a5` for A5/3 and A5/4, `_a5_3`, `_a5_4` | Need KASUMI |
| `_kasumi`, `_kasumi_key_expand`, `_kasumi_kgcore`, `osmo_c4` | Only A5/3 and A5/4 use them here |
| `osmo_a5_1`, `osmo_a5_2` | Deprecated wrappers for `osmo_a5` |
| `_a5_1`, `_a5_2` | Exported only for `osmo_a5` and not in a public header |

### C APIs left out of `comp128` and `auth`

These are in `comp128.h`, `comp128v23.h` or `auth.h`. The UMTS algorithms and the rest of
the auth core are planned; the others can be added if a consumer needs them.

| C API | Why it's left out |
| --- | --- |
| `comp128` | Deprecated wrapper for `comp128v1` |
| `osmo_auth_gen_vec2` for UMTS subscribers, `osmo_auth_gen_vec_auts2`, `osmo_auth_c2`, `osmo_auth_c3`, `osmo_auth_alg_name`, `osmo_auth_alg_parse`, `osmo_sub_auth_type_names` | Come with Milenage and XOR-3G |
| `osmo_auth_gen_vec`, `osmo_auth_gen_vec_auts` | Deprecated forms taking `osmo_sub_auth_data` |
| `osmo_auth_register`, `osmo_auth_load`, `osmo_auth_supported` | C selects algorithms through registered plugins. Rust has no plugins, and every algorithm in `GsmAlgorithm` is always there |

### C APIs left out of `gsm_utils`

These are in `gsm_utils.h` or `gsm_utils.c` but have no Rust equivalent. Each can be
added if a consumer needs it.

| C API | Why it's left out |
| --- | --- |
| `gsm_7bit_decode`, `gsm_7bit_decode_ussd`, `gsm_7bit_encode`, `gsm_7bit_encode_ussd`, `gsm_7bit_encode_oct` | Deprecated, and take no output size. The `_n` functions replace them |
| `gsm_septets2octets` | Deprecated, and limited to 255 septets. `gsm_septet_pack` replaces it |
| `gsm_arfcn2band` | Deprecated, and aborts on an invalid ARFCN. `gsm_arfcn2band_rc` replaces it |
| `osmo_dump_gsmtime_buf`, `osmo_dump_gsmtime_c` | Format like `osmo_dump_gsmtime` into a raw buffer or talloc context |
| `osmo_rat_type_name`, `gsm_chan_t_name`, `enum osmo_rat_type`, `enum gsm_chan_t`, `enum gsm_phys_chan_config` | No API here uses these types |
| `osmo_get_rand_id` | Reads OS randomness, so recordings can't pin its results |
| `gsm0858_rsl_ul_meas_enc`, `bitvec_add_range1024` | Declared in other headers and belong with RSL and system information encoding |
