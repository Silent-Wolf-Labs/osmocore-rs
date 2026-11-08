# Channel coding

`coding` holds the counterpart of libosmocore's `src/coding`. [Library source guide](../README.md).

## GSM 05.03 xCCH

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
