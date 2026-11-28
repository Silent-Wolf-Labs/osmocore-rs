# Speech codecs

`codec` holds the counterpart of libosmocore's `src/codec` (libosmocodec).
[Library source guide](../README.md).

So far it has only what GSM 05.03 channel coding needs:

- `GSM_FR_BYTES` and `GSM610_BITORDER` for full rate (GSM 06.10), from `gsm610.c`.
- `GSM_EFR_BYTES` and `GSM660_BITORDER` for enhanced full rate (GSM 06.60), from
  `gsm660.c`.

Each bit order maps channel-coding order to codec order. In `GSM610_BITORDER`, entry
`i` is the serial position of the `i`th bit channel coding takes. In
`GSM660_BITORDER`, entry `i` is a position among the 260 bits after GSM 05.03's CRC
and repetition, not in the 244-bit serial frame. The values are spec tables, so
they carry over from C, and both are compared with C. The other libosmocodec tables,
the HR and AMR bit orders, ECUs, silence and homing frames and SID helpers, aren't
implemented. The HR bit orders come with TCH/H channel coding.
