# Convolutional coding

[Library overview](../README.md). Implementation: [descriptors](mod.rs),
[encoder](encoder.rs), [generic decoder](decoder.rs), [correlation](correlation.rs).

## Implemented scope

All 17 selected upstream core convolutional exports have behavior represented:
two length helpers, five encoder operations, generic decoder lifecycle/scan/
traceback and one-shot dispatch, and explicit correlation decoding. Rust drop
replaces deinitialization. This covers checked descriptors, reusable and one-shot
encoding, generic stateful squared-error decoding and portable scalar correlation.

Use `ConvCode::new(ConvCodeSpec)`, `ConvEncoder`/`ConvDecoder` methods and
`conv::encode`/`decode`. Signed C length helpers, signed raw encode/scan counts,
explicit correlation dispatch and read-only decoder snapshots live under
`compat::conv`. Redundant init/reset/flush forwarding functions were removed.
Descriptor validation is immutable; `spec()` returns shared inspection only.

## Descriptor and encoder contracts

`Termination(i32)` retains FLUSH, TRUNCATION, TAIL_BITING and unknown numeric
values. Unknown values follow the nonflush/nontail one-shot path. Owned
punctures are strictly increasing nonnegative absolute positions without a C
sentinel; absent and present-empty lists remain distinct. Endpoint/beyond-end
punctures are accepted. Tables own byte outputs/transition targets and paired
optional termination tables.

Encoder bounds are N=1..=32, K=1..=31 and nonnegative frame length. Tables have
exactly 2^(K-1) rows and in-range byte transition targets. High output-byte bits
are retained; selected bits are MSB-first, with leading zeros for N>8. Descriptor
bounds are not decoder support promises or evidence at every dimension.

The signed output-length helper subtracts every puncture, including positions
beyond the stream, so defined negative results are valid. Do not use it to size
allocations. `output_capacity`, raw and flush capacity methods independently
count only encountered punctures and check arithmetic before mutation.

State loading packs K-1 input bytes into byte state without walking transitions
or moving cursors; K=10 can lose the oldest bit by defined byte narrowing.
Raw/one-shot hard inputs must be exactly 0/1. One-shot tail-biting loads the
last K-1 data bits and rejects short input rather than cyclically padding.
Low-level flush always performs K-1 transitions, even for other termination
policies, and repeated flushes are supported. State/cursors persist across raw
chunks. Compatibility nonpositive counts are no-ops; extra input is ignored.
Invalid bits, state, capacities or arithmetic preserve reusable state/output.

## Generic decoder contracts

`ConvDecoder::new(&code, len, start)` resolves positive length overrides and
otherwise uses descriptor length. N=1..=31 is supported; N>8 narrows C's byte
mask to zero, so nonerased soft samples are scored against +127. N=32 is rejected
for generic decoding despite encoder support. K follows the descriptor domain,
with byte predecessors rather than invented wide-state traceback. History
length and total cells must fit signed C INT_MAX and host sizes; fallible
reservations report `AllocationFailed`.

Negative starts enable all states. Starts below state_count enable one;
start==state_count safely disables all survivors, matching C's equality loop.
Larger starts are errors. Nonnegative traceback end selectors instead must be
strictly below state_count. Reset/rewind invalidate active history; rewind
subtracts the minimum error while retaining normalized path metrics.

All signed-byte soft samples, including -128, are accepted; zero is an erasure.
Branch cost is `(soft - expected)^2 >> 9`, with expected +127/-127. Strict
improvement retains the first survivor on ties. Stored/returned u32 metrics
still obey signed C INT_MAX and the separate MAX_AE=0x00ffffff survivor cutoff.
A metric is not BER or a hard-bit error count. Scan/flush return consumed soft
bytes; traceback returns the selected accumulated error and preserves suffixes.

Flush end-state selection uses zero. Tail-biting prefers a closing path, then
the lowest-error path. Missing/unreachable history, no survivors, invalid
operations and arithmetic are typed errors; failures leave decoder and output
unchanged. `compat::conv::diagnostics` exposes initialized errors, optional
scratch and active `Option<u8>` predecessors, not pointers/padding/uninitialized
storage. Reset/rewind never make inactive retained history valid again.

## Correlation and one-shot dispatch

`decode` sends K=5/7 with N<=4 to correlation, including N=1 which correlation
rejects without generic fallback. Correlation supports N=2..=4 and positive
length, returns zero on success, and follows recorded scalar/SSE/AVX equality
selection independently of CPU. Other codes use fixed-zero-start generic
one-shot policy and return its accumulated metric. Neither convention is BER.

Correlation uses reversed state bits, scalar >= predecessor selection (even
predecessor on equality), and normalization interval `32767/(N*127)-K`, including
row zero. Nontail initialization boosts state zero by 127*N*K; tail-biting starts
all sums equal, scans twice retaining sums, overwrites paths and restarts row
normalization. Traceback uses first strict maximum closing path, or first strict
maximum fallback. Flush forces zero and removes K-1 tail symbols. Punctures are
erasures at absolute positions.

Termination-output presence activates recursive decoding regardless of mode.
The lowest systematic bit with zero zero-input output across states is selected;
nonsystematic recursion returns `UnsupportedRecursiveCode` (C -EPROTO).
Correlation does not walk next-state tables, but descriptor validation still
checks them. Full i8 input is accepted; selected i16 narrowing/normalization
outside range reports `ArithmeticOverflow`. Negative traceback returns become
`NoSurvivor`, not successful negative metrics. Output is atomic on failure.
`decode_input_capacity` is independent of signed length helpers and the encoder's
short-tail-biting loading restriction; capacity does not promise backend support.

## Testing and verification

Repository evidence: `tests/compatibility/vectors/conv/conv_vectors.ndjson`;
replays: `tests/conv/conv_encoder.rs`, `conv_decoder.rs`, `conv_correlation.rs`.
The strict shared reader validates descriptors, schema, full buffers, semantic
state, operations, backend identity, provenance and frozen coverage; corruption
checks are separate from algorithm comparison.

| Evidence | Frozen coverage |
| --- | --- |
| Full stream | 36,256 records: 4 metadata, 1,066 descriptors, 35,186 operations, 1,707 case IDs |
| Length/encoding replay | 14,521 observations |
| Generic lifecycle | 332 cases; 7,261 observations (6,597 calls and 664 drops) |
| Direct correlation | 6,796 observations: 1,699 per linked/scalar/SSE/AVX stream |
| Public one-shot dispatch | 6,608 observations: 1,652 per stream |

Coverage includes N=1/2/3/4/5/8/9, K=2/3/5/7/9/10, termination 0/1/2/42,
small exhaustive hard patterns, larger independent soft patterns, erasures/ties/
extremes, recursion, puncture boundaries, normalization intervals, split scans,
reuse, reset/rewind, no survivors and selected traceback states. Two independently
selected synthetic rate-2 K=7 length-97 flush cases exercise unpunctured and
every-third-punctured operation. Historical GMR-1/WiMAX source-extracted vectors
were removed from active evidence on 2026-10-13. Separate Rust tests cover atomic
checked safety errors
rather than treating unsafe C histories/overflow as golden output.

The required local C snapshot has no Git metadata; revision is unknown. The
provenance pins 33 source/header/template/build/version files and the linked
library (34 entries), with descriptive embedded version 1.14.1.1-95043.
The linked dispatcher is pinned-local-binary evidence, not proof of build lineage
to those sources. Isolated scalar/SSE/AVX observations compile unchanged exact
sources outside that checkout. No installed library or alternate checkout is used.

Historical final-stage verification passed source/generator verification, isolated
ASan+UBSan audits, two matching 36,254-record generations and both Rust suites.
Successful scalar/SSE/AVX outputs/statuses agreed over frozen cases, including
-128/ties/recursion/tail-biting. The actual linked metrics pointer identified AVX.
This bounded agreement is not arbitrary-descriptor/backend equivalence. NEON
was not executed and its inspected strict tie rule differs; no NEON parity is
claimed. The existing linked binary was not sanitizer-instrumented; isolated
audits do not establish its sanitizer status. Leak checking was disabled under
ptrace. See the library overview for current full-suite validation.

From the sibling harness repository, targeted checks use
`reference/conv/run_conv_vector.sh --verify` and `--audit`; `all` emits fresh NDJSON
for comparison with both committed copies. Use a temporary build/output directory,
compare two independent outputs, and preserve manifests/reader fingerprints.
The repository test guide contains complete commands. Do not run an all-fixture
rewriting generator merely to check documentation or module moves.

## Remaining work and fundamental differences

The selected core scope is complete. GSM channel coding/interleaving,
AMR/DTX, protocol parity definitions and generated GSM/GMR/WiMax catalogues,
Rust SIMD selection and NEON parity require separate scope and evidence.
Allocation failure is checked where fallible reservation is used but was not
injected as C golden evidence. Unsafe empty/short history, invalid shifts,
sizing/metric overflow and unsafe wide-state paths remain excluded.

Owned checked descriptors, bounded punctures, explicit lifetimes/drop and typed
errors replace raw tables, negative-sentinel scanning, nullable storage and C
undefined behavior. Defined byte narrowing and unknown termination fall-through
are preserved; they are not normalized into a different coding algorithm.

## Synthetic evidence replacement (2026-10-13)

The replacement cases are `synthetic-rate2-k7-l97-unpunctured` and
`synthetic-rate2-k7-l97-punctured`, with six soft modes each. Their parameters
were independently selected for this test using the existing local synthetic
rule, not copied from another upstream or standard vector and not chosen to
recreate the removed protocol tables. For N=2, K=7 the local rule
`(1 << (K-1)) | 1 | (j % 2 ? 2 : 4)` gives masks `0x45, 0x43`.

For state `s=0..63` and input `b=0..1`, `register=(s<<1)|b`,
`next_state=register&63`, and
`output=(parity(register&0x45)<<1)|parity(register&0x43)`. There are no explicit
recursive termination tables; flush uses six zero-input steps. The unpunctured
case has a null puncture list. The punctured case omits raw positions
`0,3,...,204` in the 206-symbol stream, ending its C list with `-1`.

Both paired cases start a uint32 LCG at the new seed `0x8f32d6a1`; before each
bit, `seed=(1664525*seed+1013904223) mod 2^32`, then `input=seed>>31`, for 97 bits.
Their shared hard input SHA-256 is
`65c593a6bed7222d2c04961409d7953ab0cef862c4df92eb4bebbd9941f878ef`. The new input
and output tables were checked against the removed cases; neither input matches
an old vector or its equal-length prefix, and the output tables differ.

Sibling producers are `reference/conv/conv_vector.c`, `reference/conv/run_conv_vector.sh`,
`reference/conv/conv_provenance.json`, and `reference/conv/conv_generator_provenance.json`.
The existing mathematical helper generates descriptors and inputs only. Direct
local C calls produce encoding/decoding results, metrics, history, returns and
buffer observations. Rust never supplies expected encode/decode outputs.
Consumers are the three `tests/conv/conv_{encoder,decoder,correlation}.rs` replays,
`tests/conv/conv_reference_vectors.rs` and `tests/support/conv_fixture.rs`, plus
sibling `tests/conv_reference_vectors.rs` and `tests/support/conv_fixture.rs`.
Both repositories retain identical NDJSON and provenance-manifest bytes.

Two full focused generations matched byte-for-byte across linked dispatch and
isolated scalar/SSE/AVX. All unrelated descriptor/operation records are unchanged;
only the two replacement families and producer metadata changed. The existing
recursive/systematic and tail-biting cases remain. Isolated sanitizer audits
passed, including the separate excluded UB diagnostics; linked-library
instrumentation and NEON remain outside the claim.
