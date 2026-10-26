# Message buffers

[Library overview](../README.md). Implementation: [buffer/layers](buffer.rs),
[queues](queue.rs), [formatting](format.rs), [public types](mod.rs).

## Implemented scope

All four planned message-buffer stages are complete: fixed storage primitives,
typed and layer helpers, copy/area resize, and queues/comparison/formatting.
`Msgb` owns zero-initialized fixed-capacity storage (maximum u16::MAX);
capacity includes headroom. Use its methods for normal Rust code.
`compat::msgb` retains backing-offset queries, wrapping lengths, optional-input
comparisons, counted queues and C buffer/static-limit formatting contracts.

`data`/`data_mut` expose active bytes; backing access exposes all storage.
L1–L4 are checked optional offsets, including one-past-end and unused storage.
The invariant checks marker ordering, not containment in active data. Control
storage is five u64 words matching the recorded unsigned-long ABI.

## Buffer, layer, copy and resize semantics

Untyped reserve/put/get/push/pull/trim do not erase backing bytes. Reserve requires
an empty buffer; trim can shrink or extend without initialization and checks
capacity remaining after data. Reset clears headroom/data, L2–L4 and control
words while preserving L1 and every backing byte. Compatibility headlen retains
C's wrapping u32 `len - data_len`, not ordinary headroom.

Typed u8/u16/u32 writes/reads use big-endian bytes and preserve unrelated storage,
including removed bytes. Layer access returns optional offsets; SMS aliases L4.
Layer lengths retain unsigned wrapping subtraction for markers beyond tail;
missing markers are typed errors. Pull-to-L2 clears L1; pull-to-L3 clears L1/L2;
both require a marker in [data, tail] and preserve state on error.

L3 trim computes `(L3 - data) + signed_requested_length`. A negative request is
valid if its total is nonnegative; even markers before data or beyond tail can
participate. Missing layers, signed overflow, negative totals and out-of-storage
tails are errors without mutation. Tag/length prepend stores the tag and prior
length narrowed to eight bits, including wrap above 255; insufficient headroom
writes neither byte.

Copy/copy_resize preserves active bytes and data/tail/layer offsets. Contrary
to the broad C comment, only active bytes are copied: unused headroom/tailroom
in the new allocation is zero, and control words are cleared. Copies are
independent; new capacity must contain tail and preserved layers. Source remains
unchanged on failure.

Area resize accepts an absolute offset and old/new signed sizes, moves the suffix
with memmove semantics, and requires the old area inside active data. Markers
at/after old area end move by the size delta; markers inside the removed area
stay fixed, even if now beyond tail or out of order. Growth does not initialize
extension bytes; shrinkage does not erase unused suffix storage. Control words
are retained. Rust validates every relocation and negative/overflowing size
before changing bytes or metadata.

## Queues, comparisons and formatting

`MsgbQueue` is `VecDeque<Msgb>` with owning FIFO semantics. Counted compatibility
operations use a separate wrapping u32; empty dequeue and queue free leave it
unchanged. Moving a message transfers ownership; queue clear drops it. There
are no intrusive links or duplicate ownership of a queued message.

Equality helpers compare active or selected layer bytes, including headroom
markers. Optional compatibility inputs preserve NULL-versus-present-empty
behavior. Unset/beyond-tail layers return checked errors instead of C's unsafe
pointer subtraction and potentially enormous wrapped comparison lengths.

`Msgb::hexdump` produces a complete owned dump without C output-size limits.
Compatibility caller-buffer dumps preserve truncation, NUL and untouched suffix
bytes. Static dumps have C's 4100-byte buffer; allocated dumps use `length*3+100`;
both retain C's 1365 input-byte limit per segment. Layer annotations preserve
markers in headroom, behind earlier markers or beyond tail. Unset/empty layers
format as `[]`; beyond-tail layer dumps are errors. Unsafe short annotation
writes are bounds-checked in Rust and excluded from successful C observations.

Formatted append uses Rust formatting/UTF-8, not printf/locale syntax. It needs
an extra NUL outside active data; embedded NUL counts toward message length.
On insufficient tailroom, offsets/layers/control metadata stay unchanged but
available unused tail bytes receive truncated output plus NUL. No tailroom
means no write. This is the explicit exception to atomic mutation errors.

## Testing and verification

Repository replays are the four `tests/msgb/msgb*_reference_vectors.rs` files;
fixtures are under `tests/compatibility/vectors/`. They compare full backing
storage, data/tail/layer/control state, queries, returns and guarded panics.

| Stage | Fixture | Direct C operation records |
| --- | --- | ---: |
| Storage | `msgb_vectors.json` | 3,695 |
| Typed/layer | `msgb_layers_vectors.json` | 7,572 |
| Copy/resize | `msgb_resize_vectors.json` | 8,888: 1,131 copies, 7,757 resizes |
| Remaining adapters | `msgb_adapters_vectors.json` | 4,055 |
| Total | Four completed stages | 24,210 |

Coverage includes all 256 u8 values; u16/u32 high-bit boundaries; exact/short
room; every layer position class; signed L3 totals; tag/length wrapping; overlap,
zero insertion and whole-message removal; independent copies; NULL/present-empty
comparisons; FIFO and count wrap; dump thresholds at 1364/1365/1366 bytes and
segmented large output; and formatted failure writes. Adapter coverage is 450
slice comparisons, 270 message comparisons, 2,942 buffer dumps, 62 static and
62 allocated dumps, 152 layer dumps, 90 appends and 27 queue operations.

Shared readers freeze schema, inputs/order, ABI/source provenance, full snapshots
and observations; corruption tests cover missing/duplicate/unknown fields,
metadata, byte output, queues and counts. Fingerprints check integrity, not
cryptographic authenticity. Rust-only safety tests cover unaligned typed pushes,
unsafe trim/copy/relocation, out-of-storage layers and small annotation buffers.
The owned API tests also check partial mutation and full dump behavior.

The sibling `reference/msgb/run_msgb_vector.sh` uses the required local source,
headers and linked/runtime library. Targeted modes are `--layers`, `--resize`
and `--adapters`; the base mode covers storage. Pinned dependencies include
panic/assertion, logging without targets, utils/list headers and generated
endian headers plus their template/rule. Relevant sources were verified against
revision 950430e829a3dc1d162aa241bc0505745c5a7311 on 2026-10-08/09; generated
headers were reproduced and compared. The queue harness uses GNU C11 for
upstream typeof list macros, with libtalloc supplying C allocation dependency.
Both repositories ran replay/integrity/corruption checks and shared-artifact
comparisons. The repository test guide preserves complete regeneration commands;
normal Cargo tests use committed fixtures without C.

## Remaining work and fundamental differences

The selected buffer scope is finished. Talloc parent contexts/names/pools,
global talloc roots, intrusive linkage, raw destination/channel pointers,
logging/source-location/fatal-print side effects, C printf ABI/locale and C
NULL-on-allocation-failure semantics are deliberately outside the Rust surface.
These require a separately scoped decision, not completion of an unfinished stage.

| C interface | Rust ownership or exclusion |
| --- | --- |
| alloc/alloc_c, alloc_headroom variants | `Msgb::new`/`with_headroom`, standard allocator policy and checked capacity |
| free/queue_free | Drop and owning queue clear; no shared talloc tree |
| copy/copy_c/resize-copy variants | Owned independent copies without context/name parameters |
| hexdump_c | Compatibility owned dump without talloc context; preferred complete `Msgb::hexdump` |
| set_talloc_ctx/talloc_ctx_init | Excluded; no process-global allocation root |
| eq print/source-location variants | Checked equality without fatal logging/print side effects |
| destination/channel pointers/list links | Excluded; fixed-width control words retained |

Pointers become offsets/slices, NULL becomes Option where its distinction matters,
and panic/unsafe storage operations become `MsgbError`. Rust trim validates the
actual remaining storage, not C's incomplete total-capacity check. Typed pushes
work on unaligned byte storage, while successful C observations use aligned
locations. Rust never reproduces unsafe NULL subtraction, out-of-allocation
pointers or implementation-defined out-of-range signed narrowing as golden data.
