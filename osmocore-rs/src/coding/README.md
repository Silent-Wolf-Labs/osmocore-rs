# Channel coding

`coding` holds the counterpart of libosmocore's `src/coding`. [Library source guide](../README.md).

## GSM 05.03

[gsm0503](gsm0503/mod.rs) keeps one public path, `coding::gsm0503`. Its private files
follow libosmocore's split: [coding.rs](gsm0503/coding.rs) for the channels,
[codes.rs](gsm0503/codes.rs) for the convolutional codes,
[interleaving.rs](gsm0503/interleaving.rs), [mapping.rs](gsm0503/mapping.rs),
[parity.rs](gsm0503/parity.rs) and [tables.rs](gsm0503/tables.rs) for the puncturing
patterns and speech bit tables, and [tch_fr.rs](gsm0503/tch_fr.rs) for TCH/F. xCCH
(SACCH, SDCCH and BCCH), TCH/F speech (FR and EFR) and FACCH/F have channel coding so
far. TCH/H, CSD, RACH, SCH, PDTCH and AMR aren't implemented.

### xCCH

`xcch_encode` encodes a 23-byte L2 frame into four bursts of unpacked bits, and
`xcch_decode` decodes soft bits back into a frame with bit-error counts.

`xcch_decode` returns `data: None` where C returns -1. `bit_errors` and `total_bits`
are set either way. Slices are fixed-size arrays, so C's unchecked pointer lengths
have no equivalent, and the C option to pass null count pointers isn't kept.
`XCCH_STEALING_FLAG_OFFSETS` gives where the `hl` and `hn` flags sit in a burst, which
C leaves implicit in its burst mapping.

### TCH/F speech and FACCH/F

`tch_fr_encode` encodes a `TchFrFrame` (FR, EFR or a stolen FACCH block) into eight
bursts, and `tch_fr_decode` decodes soft bits back with the speech codec it is given.
`tch_fr_facch_encode` and `tch_fr_facch_decode` code FACCH/F on its own. The FR and EFR
bit orders come from [`codec`](../codec/README.md).

A TCH/F block takes half of each of its eight bursts, and the encoders write only those
positions and the stealing flags, as C does. The caller keeps one burst buffer and
encodes each block four bursts after the last. `tch_fr_interleave` writes half of its
destination in the same way.

Differences from C:

- C picks the frame type from the length, and returns -1 for any other length.
  `TchFrFrame` makes the type explicit, and `TchFrFrame::from_bytes` keeps C's length
  rule. C's zero-length frame, which makes the receiver see a bad frame, is `None`.
- `net_order` is `FrBitOrder`, and `TchFrCodec` combines it with `efr`, since only FR
  uses the bit order.
- `tch_fr_decode` returns `frame: None` where C returns -1, and adds `stolen`, which C
  shows only through `n_bits_total`: 456 for FACCH and 378 for speech. Speech bit errors
  count only the convolutionally coded bits. Decoded speech gets its codec's RTP
  signature, as in C.
- `tch_fr_facch_decode` returns `None` when the stealing flags don't mark FACCH. C then
  returns -1 and leaves the counts unset.
- `tch_burst_unmap` always returns the stealing flag and writes the data bits; C lets
  either pointer be null.

Every TCH/F encode, decode and FACCH/F call the harness records is compared with C,
including untouched burst positions, along with the interleaver, the burst mapping and
the bit orders. The FR parameter lengths are a rule rather than a list.

### Convolutional codes and parity

`Gsm0503Code` has a variant for each of the 45 `gsm0503_*` codes, in libosmocore's
order. `c_name` gives the C name, and `code` gives a `ConvCode` for use with
`base::conv`, built from its generator polynomials on first use. C's descriptors are
generated tables; here the output and state tables, and the termination tables of the
recursive AMR codes, are derived from the polynomials. Regular puncturing patterns are
rules and the rest are listed. `xcch_code` returns `Gsm0503Code::Xcch.code()`.

The ten `gsm0503_parity` descriptors are constants on the `base::crc` code types, such
as `FIRE_CRC40` and `CS234_CRC16`.

All 45 descriptors, encodes and decodes, and the parity descriptors with their
compute, set and check results, are compared with C. Upstream's
`conv_gsm0503_test.ok` checks only input and output lengths, because its round trips
use random input, and those lengths are compared too.

### Soft bits

Decoding follows the C SSE and AVX decoders for -127 to 127, which is the documented
soft-bit range. All three C backends give identical results on the recorded frames.
It also accepts -128 without panicking, but that isn't compared with C, because the C
backends disagree there.
