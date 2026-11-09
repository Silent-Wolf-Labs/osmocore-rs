# GSM utilities

`gsm` holds the counterpart of libosmocore's `src/gsm`. [Library source guide](../README.md).

| Module | Covers |
| --- | --- |
| [gsm0502](gsm0502.rs) | TDMA frame arithmetic, burst sizes, block starts, hopping and paging |
| [gsm_utils](gsm_utils/mod.rs) | Bands, ARFCNs, power, GSM time, TLLIs, classmarks, RACH and 7-bit coding |
| [a5](a5.rs) | A5/0, A5/1 and A5/2 cipher streams |
| [auth](auth/mod.rs) | GSM and UMTS authentication: key types, vectors, SQN, resync, XOR-2G and XOR-3G |
| [comp128](comp128/mod.rs) | COMP128 v1, v2 and v3 |
| [milenage](milenage.rs) | Milenage f1 to f5*, AUTS and the USIM's AUTN check (`auth` feature) |
| [tlv](tlv/mod.rs) | Tag-length-value parsing, encoding and validation |

## GSM 05.02

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

## GSM utilities

[gsm_utils.rs](gsm_utils/mod.rs) provides the numeric helpers from C's `gsm_utils`: bands,
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

### 7-bit coding

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

## A5 ciphers

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

## GSM and UMTS authentication

[auth](auth/mod.rs) computes GSM and UMTS authentication vectors. Results match C.

- **GSM:** `GsmAlgorithm::vector` gives SRES and Kc from Ki and RAND with COMP128 v1, v2
  or v3 ([comp128](comp128/mod.rs)), XOR-2G or XOR-3G.
- **UMTS:** `UmtsSubscriber` holds K, OP or OPc, AMF, SQN and IND. `vector` gives AUTN,
  RES, CK, IK and the derived SRES and Kc with Milenage or XOR-3G, and `resync` takes a
  USIM's AUTS first.
- **Conversions:** `c2` to `c5` derive GSM values from UMTS ones and back, and `Algorithm`
  gives libosmocore's names.

[milenage](milenage.rs), with the `auth` feature, gives the Milenage functions themselves:
f1 to f5*, OPc from OP, AUTS for both sides and the USIM's AUTN check. It uses AES-128
from the RustCrypto `aes` crate.

Milenage moves SQN on by one SEQ for each vector and sets this client's IND, as C does.
XOR-3G uses SQN as it is, and resync makes it the USIM's SQN. Key types wrap their bytes,
and their `Debug` output doesn't show them. Nothing claims to run in constant time, and
keys aren't cleared from memory when dropped, as in C. COMP128v1 is broken and the XOR
algorithms are for test SIMs, so use them to interoperate, not to protect anything.

| Input | C | Rust |
| --- | --- | --- |
| COMP128 or XOR-2G with a UMTS subscriber | Reads Ki from the UMTS fields, which share its memory, or returns `-ENOTSUP` | `GsmAlgorithm::vector` takes a `Ki`, so there's no UMTS subscriber to pass |
| Milenage with a K, OP or OPc other than 16 bytes, or IND out of range | Returns `-EINVAL`, -2 or -3 | `AuthError`, and nothing changes |
| A RES length other than 4 or 8 for Milenage | Returns `-EINVAL` | `ResLength` has only those two |
| Resync with AUTS that matches, then an IND out of range | Moves SQN to the USIM's, then fails | Checks IND first, fails and changes nothing. `compat::auth` keeps C's result |
| Resync with a bad AUTS and IND out of range | -1 | `AuthError::IndRange` |
| XOR-3G resync | Checks AUTS against RAND, not the RAND it was made for | The same |
| The USIM's AUTN check | Compares SQN before the MAC, with no IND or age limit | The same, documented on `Milenage::check` |

C keeps GSM and UMTS subscriber data in a union. `compat::auth` keeps them apart, so where
C would read the other part's memory, it reads its own fields instead. Those cases aren't
recorded.

COMP128's substitution tables are part of the algorithm, so the crate holds the same
values. They come from the public descriptions of COMP128 recovered from SIMs.
Milenage's rotations and constants come from 3GPP TS 35.206.

## TLV

[tlv.rs](tlv/mod.rs) parses messages described by a `TlvDefinition` into a `TlvParsed`,
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
