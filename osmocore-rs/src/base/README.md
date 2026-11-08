# Core utilities

`base` holds the counterpart of libosmocore's `src/core`. [Library source guide](../README.md).

| Module | Covers |
| --- | --- |
| [bits](bits.rs), [bitvec](bitvec/mod.rs) | Packed, unpacked and soft bits; bit vectors with a cursor |
| [prbs](prbs.rs) | Pseudo-random bit sequences |
| [base64](base64.rs), [bcd](bcd.rs), [hexparse](hexparse.rs) | Text and digit encodings |
| [crc](crc.rs) | CRC codes |
| [conv](conv/README.md) | Convolutional coding |
| [msgb](msgb/README.md) | Message buffers |
| [prim](prim.rs) | Primitive metadata |
| [gsmtap](gsmtap/README.md) | GSMTAP encoding |
| [time](time/README.md), [timer](timer.rs), [tdef](tdef.rs) | Clocks, timers and timeout definitions |
| [time_accounting](time_accounting/README.md) | Cumulative time accounting |
| [fsm](fsm/README.md) | Finite state machines |
| [rate_ctr](rate_ctr/README.md) | Rate counters |
| [use_count](use_count.rs) | Named use counters |
| [strrb](strrb.rs) | Ring of recent strings |
| [sockaddr_str](sockaddr_str.rs) | IP addresses kept as text |

The modules with their own folder have their own README. The others are described below.

## PRBS

[prbs.rs](prbs.rs) generates the ITU-T O.150 PRBS-7, 9, 11 and 15 sequences and
caller-defined ones. `PrbsState::new`, `next_bit` and `fill` replace the C
functions. `fill` takes its length from the slice, so it doesn't return the count
that C does. `next_bits` reads up to 64 bits as one number, first bit most
significant, which C has no equivalent for. The C state field is public and can be seeded directly; Rust always
starts from state 1 and exposes the register read-only.

## Base64, BCD and hexadecimal parsing

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

## Packed bits and cursor-based bit vectors

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

[bitvec.rs](bitvec/mod.rs) represents all 30 nonallocation APIs in the local header
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

## CRCs

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

## Primitive metadata

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

## Use counts

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

## String ringbuffers

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

## IP addresses as text

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
