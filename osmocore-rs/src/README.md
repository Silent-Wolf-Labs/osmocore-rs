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

### PRBS

[prbs.rs](prbs.rs) generates the ITU-T O.150 PRBS-7, 9, 11 and 15 sequences and
caller-defined ones. `PrbsState::new`, `next_bit` and `fill` replace the C
functions. `fill` takes its length from the slice, so it doesn't return the count
that C does. `next_bits` reads up to 64 bits as one number, first bit most
significant, which C has no equivalent for. The C state field is public and can be seeded directly; Rust always
starts from state 1 and exposes the register read-only.

### GSM 05.02

[gsm0502.rs](gsm0502.rs) provides TDMA frame arithmetic, burst sizes, block starts,
CCCH blocks, frequency hopping and paging groups.

`hop_index` and `hop_arfcn` take a frame number, not C's `gsm_time`, and derive T1,
T2 and T3 from it. Results match C when the `gsm_time` comes from `gsm_fn2gsmtime`.
A caller with only T1, T2 and T3 gets the frame number from `GsmTime::from_parts`.
There is no equivalent of a `gsm_time` whose fields disagree, and that C behavior
isn't recorded for hopping.

Rust rejects inputs where C crashes, reads out of bounds or computes a meaningless
result:

| Input | C | Rust |
| --- | --- | --- |
| Empty mobile allocation | Divides by zero | `HopError::EmptyAllocation` |
| Hopping sequence number above 63 | Can read past RNTABLE | `HopError::InvalidHsn` |
| No block ends on the frame | Logs an error, returns the input | `block_start` returns `None` |
| Frame can't carry CCCH | Returns a negative code | `ccch_block` returns `None` |
| Reserved `CCCH_CONF` code | Wrapped arithmetic | `CcchConf::from_code` returns `None` |
| `BS_AG_BLKS_RES` above 2 on a combined channel | Clamped or wrapped arithmetic | `PagingConfig::new` returns `None` |

`compat::gsm0502` keeps the C results for all but the first two rows, and returns
`None` from the group helpers where C divides by zero. Frames exactly half a
hyperframe apart each compare as later than the other, in both `frame_cmp` and C,
so `frame_cmp` must not be used as a sort order.

### GSM 05.03 xCCH

[gsm0503.rs](gsm0503.rs) encodes a 23-byte L2 frame into four bursts of unpacked
bits, and decodes soft bits back into a frame with bit-error counts. SACCH, SDCCH
and BCCH share this coding. PDTCH, TCH, AMR and RACH aren't implemented.

`xcch_decode` returns `data: None` where C returns -1. `bit_errors` and `total_bits`
are set either way. Slices are fixed-size arrays, so C's unchecked pointer lengths
have no equivalent, and the C option to pass null count pointers isn't kept.
`XCCH_STEALING_FLAG_OFFSETS` gives where the `hl` and `hn` flags sit in a burst, which
C leaves implicit in its burst mapping.

`xcch_decode` follows the C SSE and AVX decoders for -127 to 127, which is the
documented soft-bit range. All three C backends give identical results on the
recorded frames. It also accepts -128 without panicking, but that isn't compared
with C, because the C backends disagree there.

### GSM utilities

[gsm_utils.rs](gsm_utils.rs) provides the numeric helpers from C's `gsm_utils`: bands,
ARFCNs and frequencies, mobile power levels, RxLev, GSM time, reduced frame numbers,
TLLIs, classmark A5 support, RACH parameters and SMS and USSD 7-bit coding.

Results match C, with these differences in the preferred API. `compat::gsm_utils`
keeps C's integer and sentinel results for all but the first two rows.

| Input | C | Rust |
| --- | --- | --- |
| RFN of 42432 or more | Aborts | `rfn_to_frame` returns `None` |
| dBm within 110 of `INT_MAX` | Overflows | `dbm_to_rxlev` returns 63 |
| A band value that names no band | Returns -22 or `"invalid"` | `Band` can't hold one |
| Frame number from the hyperframe on | Keeps it, and `t1` truncates | `GsmTime::from_frame` returns `None` |
| T1, T2 or T3 out of range | Reduces the result, possibly oddly | `GsmTime::from_parts` returns `None` |
| TLLI type with no conversion | `gprs_tmsi2tlli` returns 0 | `local_tlli` and `foreign_tlli` have no such input |

Some C results look surprising, and both APIs keep them:

- **Uplink ARFCNs:** `freq10_to_arfcn` sets `ARFCN_UPLINK` on an uplink result, but
  `arfcn_to_freq10` returns nothing for an ARFCN with that flag, so the two don't
  round-trip until the flag is cleared.
- **The last field of a formatted `GsmTime`:** this is the frame number's low byte
  modulo 52, not the frame number modulo 52, so the two differ from frame 256 on.
- **Reduced frame numbers:** an RFN over half the modulus from the current frame's
  RFN counts as the previous cycle, even when it's the larger of the two.

A local TLLI of `0xffffffff` converts to the P-TMSI `0xffffffff`, the same value C
uses for an error. `tlli_to_tmsi` tells them apart, and `compat::gsm_utils::gprs_tlli2tmsi`
doesn't. Text after a NUL in `Band::parse` is ignored, because C would never see it.

#### 7-bit coding

`encode_7bit`, `encode_7bit_ussd`, `decode_7bit`, `decode_7bit_ussd` and
`decode_7bit_with_header` convert between bytes and packed septets, and `encode_septets`,
`pack_septets` and `septets_to_octets` expose the steps. Text is bytes, not UTF-8: the
alphabet maps Latin-1 values. Results match C, with these differences in the preferred
API. The last column gives what `compat::gsm_utils` does.

| Input | C | Preferred API | `compat::gsm_utils` |
| --- | --- | --- | --- |
| Byte 0xff | Reads past its table | No septet, packs as 0x7f | Same as preferred |
| NUL in the text | Ends the text | A byte with no septet | Ends the text |
| Padding above 7 | Shifts by a negative amount | `pack_septets` returns `None` | Returns -22 |
| Too little result space for `gsm_septet_encode` or `gsm_septet_pack` | Overruns | Returns a `Vec` | Returns -22 |
| Input shorter than the septets | Reads past it | `None` | Missing octets read as zero |
| Header longer than the message | The septet count wraps in 8 bits, and it reads on | `None` | C's result when the input covers the reads |
| Empty decode output | Aborts | No output limit | Returns 0 |
| More than 255 septets to decode | Can't be passed | Decodes them | Takes `u8`, like C |

Some C results look surprising, and both APIs keep them. Most come from C keeping the
byte-to-septet map and the extension characters in one table, so each overwrites part of
the other:

- **Accented letters don't round-trip:** `Ä`, `Ö`, `Ñ`, `Ü`, `ä`, `ö`, `ñ` and `ü` encode
  to the right septets, but those decode to 0xbb, 0xae, 0xbd, 0x93, 0xa7, 0xbf, 0xa8 and
  0xbc. `ø` decodes to 0x89, and `¡` encodes correctly but decodes to `|`.
- **`§` and `¿`:** these encode as `ä` and `ö`, and septets 0x5f and 0x60 decode to 0xff.
- **Greek letters:** septets 0x10 and 0x12 to 0x1a decode to 0xff, and no byte encodes to
  them.
- **Escapes:** an escape before a septet that isn't an extension character gives the
  value at that table position. Escape and 0x65, which is `€` in the standard, gives `{`.
  An escape as the last septet decodes to 0xff.
- **Form feed:** this encodes to an escape and 0x7f, which decodes to 0xff.
- **Bytes with no septet:** these pack as 0x7f, which is `à`.
- **Limits:** a limit can cut between an escape and its character.
- **USSD:** `decode_7bit_ussd` drops the last byte whenever the top seven bits of the
  last octet hold `<CR>`, even if those bits aren't a whole septet. `encode_7bit_ussd`
  adds a second `<CR>` only if that leaves an octet free, though it would fit without.

### String ringbuffers

[strrb.rs](strrb.rs) keeps recent log strings, forgetting the oldest when full, like C's
`osmo_strrb`. Results match C, with these differences in the preferred API:

| Input | C | `StringRing` | `compat::strrb` |
| --- | --- | --- | --- |
| Size or capacity | Holds one string fewer than its size, which is cut to 16 bits | `new` takes the number of strings to hold, with no 16-bit limit | Same as C |
| An index past the newest string, once wrapped | Can return an older string | `get` returns `None` | Same as C |
| NUL in the text | Ends the text | An ordinary byte | Ends the text |
| Adding with no slots | Crashes | Keeps nothing | Returns -22 |

Strings are bytes, because the limit of 239 can cut through a UTF-8 character. A size of 1
holds nothing in C, and `add` still returns 0 or 1. Space is taken as strings arrive, not
all at once at 240 bytes a slot as in C. `osmo_strrb_create` takes no talloc context, and
every other function in `strrb.h` is ported. The log target built on it in C's
`loggingrb.c` isn't, because this crate has no port of C's logging.

### IP addresses as text

[sockaddr_str.rs](sockaddr_str.rs) keeps an IP address as text with a port, like C's
`osmo_sockaddr_str`, and converts it to and from standard-library addresses. Results match
C on glibc, with these differences in the preferred API:

| Input | C | `SocketAddrText` | `compat::sockaddr_str` |
| --- | --- | --- | --- |
| Text with neither `:` nor `.` | Kept with `AF_UNSPEC`, returning -22 | `new` returns `NoFamily` | Same as C |
| Empty text, or text over 45 bytes | Kept, cut to 45 bytes, returning -5 or -28 | `new` returns `Empty` or `TooLong` | Same as C |
| A family that doesn't match the text | Can be stored | Can't be: the family comes from the text | Same as C |
| NUL in the text | Ends the text | An ordinary character | Ends the text |
| Converting to the other family | -97 for IPv4, -22 for IPv6 | `WrongFamily` | Same as C |
| Comparing | `osmo_sockaddr_str_cmp` compares addresses, or the text when one doesn't parse. It isn't a total order, and IPv4 order depends on the host's byte order | `==` compares the text and port, with no ordering. Compare `to_socket_addr()` results instead | Same as C, returning -1, 0 or 1 |

Both APIs parse text with the standard library, which accepts the same text as glibc's
`inet_pton` on every recorded string. They write addresses as glibc's `inet_ntop` does,
which differs from Rust's `Ipv6Addr` display in one case: an address whose first 96 bits are
zero, with a nonzero seventh group, ends in dotted IPv4 (`::1.2.3.4`, where Rust writes
`::102:304`). C on another libc can write or accept different text. IPv6 flow labels and
scopes are dropped, as in C.

`compat`'s socket address types have Linux's layout and hold ports in network byte order,
as in C. NULL pointers are only accepted where C gives them a meaning: a NULL text, and a
missing address when comparing or formatting.

### Use counts

[use_count.rs](use_count.rs) counts an object's uses by token, like C's `osmo_use_count`.
`UseCount::change`, `get` and `put` return what changed, and the caller acts on it, where
C runs a callback. `compat::use_count::osmo_use_count_get_put` takes that callback.
Counts, totals, entry order and text match C, with these differences:

| C | Rust |
| --- | --- |
| Tokens are pointers the caller keeps valid, and a callback can set one to NULL | `UseCount` owns copies, so tokens can't become invalid and neither API can clear one |
| Entries are talloc children of the object | Entries live in a `Vec` owned by `UseCount`, with no talloc parent |
| A callback gets the entry, the old count, and a source file and line | `UseChange` gives the entry's position and both counts, with no file or line |
| Static entries avoid allocation and can't be freed | `add_idle_entries` adds the same idle entries, which can be removed, and a new token's text is still allocated |
| `INT32_MIN` as a change to a negative count, or as a count after a negative total, overflows | `change` returns `CountOutOfRange`, and `total` stops at the limit |
| `osmo_use_count_to_str_buf` or `osmo_use_count_find` with no object crashes | Both need counters; the rest of `compat::use_count` takes an `Option`, as C allows NULL |

Some C results look surprising, and both APIs keep them:

- **Text of an empty object:** this reads `0 ()` until an entry or a call to add idle entries
  exists, even if that adds none, and `0 (-)` from then on, even if every entry is removed.
- **Reuse:** a new token takes over the first idle entry, whatever its token was, and an
  entry leaving zero moves to the end of the list.
- **Idle `None` entries:** a `None` token finds the first entry with no token. After one
  moves to the end with a count, `count_of(None)` can find another that's idle and return
  0, so a later `get(None)` adds a second counted entry. `UseChange::entry` always gives
  the entry that changed.
- **The total:** it stops at the limits of `i32` as it adds, so it can depend on list order.
- **Errors:** a count that would leave the range of `i32` stays as it was, but a callback's
  error undoes nothing. The callback has to correct the count itself.

### A5 ciphers

[a5.rs](a5.rs) generates one frame's A5/0, A5/1 or A5/2 stream, as 114 unpacked bits for
each direction. Results match C, and `frame_count` gives C's count. A5/1 and A5/2 are
publicly broken, so use them to interoperate with GSM networks, not to protect data.

| Input | C | Rust |
| --- | --- | --- |
| A5/3 or A5/4 | Generates a stream | Not implemented. `compat::a5::osmo_a5` returns -95 |
| A5/1 or A5/2 with no key | Crashes | `a5_1` and `a5_2` require a key. `compat::a5::osmo_a5` returns -22 |
| Frame number from 1326 x 2^20 on | A shift overflows an `int` | Uses unsigned 32-bit arithmetic |

The ciphers load only the low 22 bits of the count, as in C, so a frame number past the
hyperframe can give the same stream as a smaller one.

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
`to_soft` and `from_soft` convert one bit to and from a soft bit, as C's
`osmo_ubit2sbit` and `osmo_sbit2ubit` do for a buffer: 0 gives 127 and any other byte
-127, and only a negative soft bit gives 1.

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


### TLV

[tlv.rs](tlv.rs) parses messages described by a `TlvDefinition` into a `TlvParsed`,
whose values borrow from the input. `encode` and the `Msgb` methods such as `put_tlv`
write elements back. `shift_*` functions take elements from the front of a slice, and
`TlvProtocol` checks mandatory elements and minimum lengths per message type.

Results match C, with these differences in the preferred API. `compat::tlv` keeps
C's behavior for each of them unless noted:

- **Lengths that don't fit:** `tlv_encode_one` truncates a length to its field. Rust
  returns `ValueTooLong`.
- **Lengths from `0x8000`:** C writes Tvlv and GAN lengths and GAN tags from `0x8000`
  whose high bit its parser reads as the other form. Rust rejects them, so whatever
  it writes parses back.
- **No room:** `msgb_put` aborts in C. Rust returns an error and leaves the message
  unchanged. `encode` also undoes earlier elements, while `compat::tlv::tlv_encode`
  keeps them.
- **`osmo_tlvp_merge`:** C copies each value but leaves its length at zero. Rust's
  `TlvParsed::merge` keeps the length, and `compat::tlv::osmo_tlvp_merge` returns C's
  empty result.
- **`osmo_tlvp_copy`:** C copies no values, so every tag of the copy is absent with
  only its length set. Clone a `TlvParsed` instead. `compat::tlv::osmo_tlvp_copy`
  returns those lengths.
- **Reading past the input:** C reads one byte past an exhausted buffer for an LV tag,
  and past a short value in `tlvp_val16_unal` and `tlv_encode_one`. Rust returns an
  error or `None`, and an LV tag on an exhausted input gives C's status.
- **Logging:** C logs validation problems. Rust returns the first as an error naming
  its tag.

Rust keeps one C behavior that looks accidental: when both LV tags name the same tag,
the second replaces the first in the first `TlvParsed` rather than going in the next.
A `SingleTv` entry also takes priority over a tag's own entry, and `encode` shifts the
tag into the upper half, so a `0xN0` tag loses its bits as it does in C.

C's raw-pointer writers such as `tlv_put` have no Rust equivalent, because the `Msgb`
methods write the same bytes. `msgb_vtvlv_gan_put` returns a wrong pointer in C, and
Rust returns nothing there.
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
