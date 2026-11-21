# Channel coding

`coding` holds the counterpart of libosmocore's `src/coding`. [Library source guide](../README.md).

## GSM 05.03

[gsm0503](gsm0503/mod.rs) keeps one public path, `coding::gsm0503`. Its private files
follow libosmocore's split: [coding.rs](gsm0503/coding.rs) for the channels,
[codes.rs](gsm0503/codes.rs) for the convolutional codes,
[interleaving.rs](gsm0503/interleaving.rs), [mapping.rs](gsm0503/mapping.rs),
[parity.rs](gsm0503/parity.rs) and [tables.rs](gsm0503/tables.rs) for the puncturing
patterns. Only xCCH (SACCH, SDCCH and BCCH) has channel coding so far. TCH, RACH, SCH,
PDTCH and AMR aren't implemented.

### xCCH

`xcch_encode` encodes a 23-byte L2 frame into four bursts of unpacked bits, and
`xcch_decode` decodes soft bits back into a frame with bit-error counts.

`xcch_decode` returns `data: None` where C returns -1. `bit_errors` and `total_bits`
are set either way. Slices are fixed-size arrays, so C's unchecked pointer lengths
have no equivalent, and the C option to pass null count pointers isn't kept.
`XCCH_STEALING_FLAG_OFFSETS` gives where the `hl` and `hn` flags sit in a burst, which
C leaves implicit in its burst mapping.

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
