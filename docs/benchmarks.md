  # Benchmarks

Criterion benchmarks for the performance-sensitive paths, tracked over time. An
optimization lands only if these numbers show a real gain.

The benches live in `osmocore-rs/benches/` and aren't part of the published
package. CI runs each bench once to check that it works. It doesn't measure
performance, because shared runners are too noisy. Full runs are local.

## Results

Median time per operation in nanoseconds, lower is better. Numbers are only
comparable within one machine. Each feature that could affect performance adds a
column on the right; a `-` means the bench didn't exist yet. Results measure the
default build with no Cargo features.

<!-- bench-table:begin -->
| Benchmark | v0.1.0 | v0.1.0 repeat | Decoder saturation fix | Thread-safe handles | xCCH | Conv ACS loop | Timer batch reuse | TLV | gsm_utils 7-bit | A5 | use_count | KASUMI and GEA | Channel coding baseline | GSM 05.03 codes | TCH/F |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `a5/a5_1` | - | - | - | - | - | - | - | - | - | 1,092 | 1,100 | 1,100 | 1,092 | 1,099 | 1,087 |
| `a5/a5_2` | - | - | - | - | - | - | - | - | - | 1,464 | 1,457 | 1,456 | 1,461 | 1,461 | 1,460 |
| `a5/a5_3` | - | - | - | - | - | - | - | - | - | - | - | 476 | 475 | 477 | 473 |
| `a5/a5_4` | - | - | - | - | - | - | - | - | - | - | - | 467 | 465 | 466 | 464 |
| `bits/pack_184` | 105 | 104 | 104 | 105 | 104 | 104 | 104 | 107 | 104 | 104 | 104 | 104 | 104 | 104 | 103 |
| `bits/pack_456` | 256 | 255 | 254 | 256 | 255 | 258 | 258 | 256 | 255 | 254 | 255 | 255 | 253 | 257 | 253 |
| `bits/pack_offset_184` | 119 | 119 | 118 | 119 | 118 | 118 | 118 | 119 | 118 | 119 | 118 | 118 | 118 | 118 | 118 |
| `bits/pack_offset_456` | 298 | 294 | 293 | 294 | 293 | 292 | 292 | 299 | 293 | 293 | 292 | 296 | 293 | 296 | 292 |
| `bits/unpack_184` | 100 | 99.7 | 99.8 | 101 | 100 | 99.7 | 99.7 | 100 | 100 | 99.8 | 99.7 | 99.8 | 99.6 | 99.7 | 99.2 |
| `bits/unpack_456` | 252 | 252 | 251 | 252 | 252 | 251 | 252 | 253 | 251 | 251 | 252 | 252 | 250 | 256 | 250 |
| `bits/unpack_offset_184` | 92.5 | 92.8 | 91.9 | 90.3 | 92.1 | 91.8 | 92.1 | 92.6 | 92.5 | 92.1 | 92.2 | 92.0 | 91.7 | 92.4 | 91.6 |
| `bits/unpack_offset_456` | 239 | 235 | 233 | 236 | 236 | 235 | 235 | 240 | 235 | 235 | 235 | 236 | 234 | 237 | 233 |
| `conv/k5_n2/decode_clean` | 6,313 | 6,270 | 5,727 | 5,743 | 5,732 | 2,379 | 2,385 | 2,402 | 2,395 | 2,384 | 2,389 | 2,403 | 2,377 | 2,408 | 2,451 |
| `conv/k5_n2/decode_noisy` | 6,310 | 6,290 | 5,708 | 5,769 | 5,727 | 2,376 | 2,385 | 2,402 | 2,389 | 2,384 | 2,384 | 2,392 | 2,380 | 2,439 | 2,375 |
| `conv/k5_n2/decoder_reuse_noisy` | 22,669 | 23,131 | 24,220 | 22,279 | 22,458 | 22,174 | 22,222 | 22,427 | 22,323 | 22,153 | 22,240 | 22,435 | 22,188 | 22,818 | 22,534 |
| `conv/k5_n2/encode` | 608 | 604 | 601 | 560 | 561 | 559 | 559 | 566 | 560 | 560 | 561 | 601 | 556 | 576 | 560 |
| `conv/k7_n3/decode_clean` | 26,003 | 25,988 | 20,067 | 20,124 | 20,140 | 3,959 | 3,974 | 3,973 | 3,954 | 3,948 | 3,967 | 3,993 | 3,377 | 4,284 | 3,940 |
| `conv/k7_n3/decode_noisy` | 26,328 | 26,100 | 20,071 | 20,288 | 20,122 | 3,949 | 3,958 | 3,975 | 3,952 | 3,949 | 3,955 | 4,008 | 3,388 | 4,019 | 3,941 |
| `conv/k7_n3/decoder_reuse_noisy` | 113,461 | 112,508 | 111,139 | 109,044 | 118,524 | 107,569 | 107,660 | 107,851 | 108,432 | 108,845 | 107,559 | 111,059 | 110,370 | 111,720 | 110,278 |
| `conv/k7_n3/encode` | 674 | 680 | 668 | 632 | 639 | 628 | 630 | 631 | 636 | 631 | 631 | 671 | 626 | 648 | 629 |
| `crc/crc16/ccitt_1500_bytes` | 2,317 | 2,311 | 2,302 | 2,311 | 2,311 | 2,381 | 2,311 | 2,320 | 2,304 | 2,301 | 2,296 | 2,328 | 2,298 | 2,337 | 2,293 |
| `crc/crc16/ccitt_64_bytes` | 81.3 | 82.0 | 80.8 | 81.2 | 80.8 | 80.7 | 80.7 | 81.4 | 80.8 | 80.6 | 80.7 | 81.8 | 80.5 | 82.6 | 80.5 |
| `crc/crc16/crc16_1500_bytes` | 2,323 | 2,313 | 2,297 | 2,311 | 2,310 | 2,304 | 2,303 | 2,312 | 2,303 | 2,299 | 2,296 | 2,312 | 2,292 | 2,324 | 2,293 |
| `crc/crc16/crc16_64_bytes` | 81.0 | 80.9 | 80.6 | 81.0 | 81.0 | 82.0 | 80.7 | 81.7 | 81.0 | 80.7 | 80.7 | 81.6 | 80.6 | 82.6 | 80.6 |
| `crc/generic_fire40/1024_bits` | 1,133 | 1,133 | 1,122 | 1,138 | 1,131 | 1,128 | 1,129 | 1,139 | 1,133 | 1,129 | 1,128 | 1,143 | 1,123 | 1,146 | 1,124 |
| `crc/generic_fire40/184_bits` | 199 | 201 | 197 | 199 | 199 | 198 | 199 | 200 | 199 | 199 | 198 | 200 | 197 | 201 | 198 |
| `fsm/dispatch` | 3.5 | 3.7 | 3.5 | 3.3 | 3.3 | 3.4 | 3.4 | 3.4 | 3.3 | 3.3 | 3.3 | 3.3 | 3.3 | 3.4 | 4.0 |
| `fsm/state_change` | 6.6 | 6.5 | 6.5 | 5.9 | 5.8 | 5.9 | 5.9 | 6.5 | 6.0 | 5.9 | 5.9 | 5.9 | 5.9 | 6.0 | 5.8 |
| `gea/gea3_1500_bytes` | - | - | - | - | - | - | - | - | - | - | - | 14,310 | 14,180 | 14,444 | 14,184 |
| `gea/gea4_1500_bytes` | - | - | - | - | - | - | - | - | - | - | - | 14,324 | 14,179 | 14,416 | 14,177 |
| `gsm0503/codes/cs3/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | 3,788 | 3,688 |
| `gsm0503/codes/cs3/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | 1,055 | 1,026 |
| `gsm0503/codes/mcs1_dl_hdr/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | 5,267 | 5,209 |
| `gsm0503/codes/mcs1_dl_hdr/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | 106 | 105 |
| `gsm0503/codes/tch_afs_12_2/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | 2,919 | 2,834 |
| `gsm0503/codes/tch_afs_12_2/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | 736 | 725 |
| `gsm0503/codes/tch_f24/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | 14,475 | 14,391 |
| `gsm0503/codes/tch_f24/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | 287 | 280 |
| `gsm0503/tch_fr/efr/decode_clean` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 3,559 |
| `gsm0503/tch_fr/efr/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 3,560 |
| `gsm0503/tch_fr/efr/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 1,265 |
| `gsm0503/tch_fr/fr/decode_clean` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 3,511 |
| `gsm0503/tch_fr/fr/decode_noisy` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 3,511 |
| `gsm0503/tch_fr/fr/encode` | - | - | - | - | - | - | - | - | - | - | - | - | - | - | 1,193 |
| `gsm0503/xcch/decode_clean` | - | - | - | - | 7,566 | 4,162 | 4,171 | 4,248 | 4,174 | 4,123 | 4,131 | 4,191 | 4,198 | 4,258 | 4,176 |
| `gsm0503/xcch/decode_noisy` | - | - | - | - | 7,529 | 4,156 | 4,157 | 4,183 | 4,191 | 4,123 | 4,125 | 4,191 | 4,185 | 4,250 | 4,173 |
| `gsm0503/xcch/encode` | - | - | - | - | 1,219 | 1,193 | 1,178 | 1,180 | 1,173 | 1,170 | 1,175 | 1,219 | 1,169 | 1,198 | 1,177 |
| `gsm_utils/decode_7bit` | - | - | - | - | - | - | - | - | 267 | 266 | 266 | 266 | 264 | 268 | 263 |
| `gsm_utils/encode_7bit` | - | - | - | - | - | - | - | - | 218 | 216 | 215 | 215 | 216 | 224 | 220 |
| `kasumi/encrypt_block` | - | - | - | - | - | - | - | - | - | - | - | 77.5 | 76.7 | 77.6 | 76.7 |
| `kasumi/kgcore_1500_bytes` | - | - | - | - | - | - | - | - | - | - | - | 14,315 | 14,160 | 14,357 | 14,166 |
| `msgb/alloc_1500` | 45.7 | 40.2 | 39.2 | 40.5 | 46.1 | 39.8 | 40.1 | 48.3 | 39.4 | 46.5 | 40.3 | 40.0 | 39.2 | 39.9 | 45.5 |
| `msgb/alloc_64` | 34.3 | 34.0 | 33.1 | 34.2 | 33.9 | 34.0 | 34.7 | 35.3 | 33.8 | 34.0 | 38.6 | 34.0 | 33.6 | 33.9 | 34.0 |
| `msgb/put_pull_1500` | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.9 | 2.9 | 2.7 |
| `msgb/put_pull_64` | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.9 | 2.9 | 2.7 |
| `msgb/put_pull_u32` | 3.4 | 4.3 | 3.2 | 3.4 | 3.5 | 3.2 | 3.4 | 3.5 | 3.4 | 3.2 | 3.4 | 3.6 | 3.4 | 3.4 | 3.2 |
| `timer/update/expire/1` | 37.8 | 37.7 | 37.7 | 35.7 | 36.4 | 35.7 | 32.9 | 33.0 | 32.7 | 32.7 | 32.7 | 33.5 | 32.8 | 33.1 | 32.8 |
| `timer/update/expire/16` | 356 | 344 | 342 | 338 | 332 | 337 | 313 | 313 | 317 | 313 | 315 | 316 | 313 | 313 | 311 |
| `timer/update/expire/256` | 7,337 | 7,318 | 7,265 | 7,172 | 7,194 | 7,300 | 7,236 | 7,300 | 7,209 | 7,245 | 7,274 | 7,252 | 7,193 | 7,240 | 7,277 |
| `timer/update/idle_256_pending` | 6.2 | 6.2 | 6.1 | 6.1 | 6.2 | 6.2 | 2.2 | 2.2 | 2.2 | 2.2 | 2.2 | 2.2 | 2.2 | 2.2 | 2.2 |
| `tlv/encode` | - | - | - | - | - | - | - | 85.6 | 85.9 | 84.3 | 85.7 | 86.0 | 87.6 | 85.5 | 83.5 |
| `tlv/parse` | - | - | - | - | - | - | - | 186 | 184 | 181 | 185 | 186 | 189 | 186 | 181 |
| `tlv/parse_two_occurrences` | - | - | - | - | - | - | - | 130 | 129 | 131 | 129 | 131 | 131 | 130 | 130 |
| `use_count/get_put_last_token` | - | - | - | - | - | - | - | - | - | - | 14.3 | 14.9 | 15.4 | 15.5 | 15.4 |
| `use_count/total` | - | - | - | - | - | - | - | - | - | - | 2.7 | 2.6 | 2.6 | 2.7 | 2.6 |
<!-- bench-table:end -->

| Column | Source | rustc | Date | Notes |
| --- | --- | --- | --- | --- |
| v0.1.0 | `9756920` | 1.99.0 | 2026-11-01 | Starting baseline |
| v0.1.0 repeat | `9756920` | 1.99.0 | 2026-11-01 | Same code run again, to show noise |
| Decoder saturation fix | `adc549d` plus the fix | 1.99.0 | 2026-11-01 | `decode` metrics saturate; `conv` benches use +/-127 |
| Thread-safe handles | `ee4eea0` plus the change | 1.99.0 | 2026-11-01 | Owner tokens are integers instead of `Rc`; handles are `Copy` |
| xCCH | `30ab3d8` plus the change | 1.99.0 | 2026-11-02 | Adds `gsm0503` benches; no existing code path changed, so other rows show only noise |
| Conv ACS loop | `389a5ae` plus the change | 1.99.0 | 2026-11-02 | Compiler-vectorized add-compare-select loop in the correlation decoder; `decoder_reuse_noisy` uses `ConvDecoder`, which didn't change |
| Timer batch reuse | `a07e35c` plus the change | 1.99.0 | 2026-11-02 | `update` returns early when nothing is due and reuses its batch buffer |
| TLV | `f7107f5` plus the change | 1.99.0 | 2026-11-02 | Adds `tlv` benches; no existing code path changed, so other rows show only noise |
| gsm_utils 7-bit | `315f0dd` plus the change | 1.99.0 | 2026-11-03 | Adds `gsm_utils` benches; no existing code path changed, so other rows show only noise |
| A5 | `336bd98` plus the change | 1.99.0 | 2026-11-03 | Adds `a5` benches; no existing code path changed, so other rows show only noise |
| use_count | `28439f7` plus the change | 1.99.0 | 2026-11-03 | Adds `use_count` benches; no existing code path changed, so other rows show only noise |
| KASUMI and GEA | `634ce4b` plus the change | 1.99.0 | 2026-11-09 | Adds `kasumi` and `gea` benches and `a5/a5_3` and `a5/a5_4`; no existing code path changed, so other rows show only noise |
| Channel coding baseline | `5c3acea` | 1.99.0 | 2026-11-19 | Before the channel coding work; no bench or measured path changed since the previous column. `conv/k7_n3/decode_clean` and `decode_noisy` are about 15% faster, reproducibly. Rebuilding the merges in between puts the step at `7d33b4a` (gsm48), which doesn't touch `base/`, the benches or Cargo files, so it isn't a decoder change. The next column traces it to codegen-unit partitioning |
| GSM 05.03 codes | `5c3acea` plus the change | 1.99.0 | 2026-11-19 | Adds the `gsm0503/codes` benches; `xcch_code` now comes from the shared code builder, with the same tables. Other rows are within about 2%, except `conv/k7_n3/decode_clean` and `decode_noisy`, back to their speed before the previous column. No conv code changed: built with `codegen-units = 1`, this tree and the one before it both measure about 3,500 ns, so the swing comes from how the crate is split into codegen units |
| TCH/F | `e284abb` plus the change | 1.99.0 | 2026-11-21 | Adds the `gsm0503/tch_fr` benches; xCCH now shares its coded-bit helpers with FACCH, and its rows are within 3%. `conv/k7_n3/decode_clean` moved 8% with no conv change, the codegen-unit swing the previous column describes. `msgb/alloc_1500` and the few-nanosecond `fsm` rows are noise, as in earlier columns |

All columns so far were measured on an AMD Ryzen 9 7900X (Linux, `amd-pstate-epp`
driver, `powersave` governor), pinned to one core.

## Adding a column

From `osmocore-rs/`, with the machine otherwise idle:

```bash
taskset -c 4 cargo bench --locked -- --save-baseline my-feature
```

Then, from the repository root, append the column and add a row to the column
table above:

```bash
scripts/bench-table.py my-feature "My feature"
```

If the toolchain or machine changed since the previous column, also rebuild the
previous commit and measure it with the current setup, so the two columns differ
only by the code. `rust-toolchain.toml` follows `stable`, so check this each time.

## When an optimization counts

An optimization lands only if all of these hold, compared with the commit before
it on the same toolchain:

- The target bench improves by at least 5%.
- The improvement is larger than that bench's noise, meaning the gap between its
  two v0.1.0 columns.
- Criterion's confidence interval for the change excludes zero.
- No other bench gets slower by more than its own noise.

### Accepted exceptions

- **`timer/update/expire/256`, up to about +2%.** Reusing the batch buffer in
  `TimerScheduler::update` made `idle_256_pending` 64%, `expire/1` 8% and `expire/16`
  7% faster. Across four timer-only runs against four of the commit before it,
  `expire/256` got about 140 ns slower per update, or 0.5 ns per timer, which is above
  its noise. The full run in the table reads lower, at 7,236 ns. The cause isn't known.
  This was accepted because idle updates and small batches are the common cases.

## Reading the numbers

- **Noise differs by bench.** The median gap between the two v0.1.0 runs is 0.4%,
  but `msgb/put_pull_u32` moved 25% and `msgb/alloc_1500` moved 12% on identical
  code. Don't draw conclusions from small changes in benches this short.
- **Noise across sessions.** Runs back to back can agree within 1% and still differ
  from another session by more. `conv/k7_n3/decoder_reuse_noisy` read 118,524 ns in
  the xCCH column and 108,434 and 108,019 ns on the same code afterwards, and
  `msgb/alloc_1500` moves between about 40 and 46 ns with no change to `msgb`.
- **`conv/k7_n3/decode_*` and codegen units.** These two move by about 15% when
  unrelated code is added to the crate, because rustc's split into codegen units
  changes what the correlation decoder inlines. With `codegen-units = 1` the change
  disappears. Compare them only across columns with the same partitioning.
- **`conv` amplitude.** The v0.1.0 columns used +/-120 soft bits, because `decode`
  returned `ArithmeticOverflow` for clean +/-127 frames. Later columns use +/-127.
- **`conv` backends.** For these codes, `decode` uses the correlation decoder.
  `decoder_reuse_noisy` uses `ConvDecoder`, a different algorithm, so compare each
  only with its own earlier numbers.
- **`timer/update/expire`.** Each iteration includes the fixed cost of reading the
  clock around `update`, which is why rearming can stay outside the measurement.
  This matters most for `expire/1`, so compare it only with itself.
- **Build target.** These are default baseline x86-64 builds, so SIMD gains from
  `target-cpu=native` won't appear unless a run says so. The correlation decoder's
  gain uses only the SSE2 that this target includes.
