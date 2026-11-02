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
| Benchmark | v0.1.0 | v0.1.0 repeat | Decoder saturation fix | Thread-safe handles | xCCH | Conv ACS loop | Timer batch reuse |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `bits/pack_184` | 105 | 104 | 104 | 105 | 104 | 104 | 104 |
| `bits/pack_456` | 256 | 255 | 254 | 256 | 255 | 258 | 258 |
| `bits/pack_offset_184` | 119 | 119 | 118 | 119 | 118 | 118 | 118 |
| `bits/pack_offset_456` | 298 | 294 | 293 | 294 | 293 | 292 | 292 |
| `bits/unpack_184` | 100 | 99.7 | 99.8 | 101 | 100 | 99.7 | 99.7 |
| `bits/unpack_456` | 252 | 252 | 251 | 252 | 252 | 251 | 252 |
| `bits/unpack_offset_184` | 92.5 | 92.8 | 91.9 | 90.3 | 92.1 | 91.8 | 92.1 |
| `bits/unpack_offset_456` | 239 | 235 | 233 | 236 | 236 | 235 | 235 |
| `conv/k5_n2/decode_clean` | 6,313 | 6,270 | 5,727 | 5,743 | 5,732 | 2,379 | 2,385 |
| `conv/k5_n2/decode_noisy` | 6,310 | 6,290 | 5,708 | 5,769 | 5,727 | 2,376 | 2,385 |
| `conv/k5_n2/decoder_reuse_noisy` | 22,669 | 23,131 | 24,220 | 22,279 | 22,458 | 22,174 | 22,222 |
| `conv/k5_n2/encode` | 608 | 604 | 601 | 560 | 561 | 559 | 559 |
| `conv/k7_n3/decode_clean` | 26,003 | 25,988 | 20,067 | 20,124 | 20,140 | 3,959 | 3,974 |
| `conv/k7_n3/decode_noisy` | 26,328 | 26,100 | 20,071 | 20,288 | 20,122 | 3,949 | 3,958 |
| `conv/k7_n3/decoder_reuse_noisy` | 113,461 | 112,508 | 111,139 | 109,044 | 118,524 | 107,569 | 107,660 |
| `conv/k7_n3/encode` | 674 | 680 | 668 | 632 | 639 | 628 | 630 |
| `crc/crc16/ccitt_1500_bytes` | 2,317 | 2,311 | 2,302 | 2,311 | 2,311 | 2,381 | 2,311 |
| `crc/crc16/ccitt_64_bytes` | 81.3 | 82.0 | 80.8 | 81.2 | 80.8 | 80.7 | 80.7 |
| `crc/crc16/crc16_1500_bytes` | 2,323 | 2,313 | 2,297 | 2,311 | 2,310 | 2,304 | 2,303 |
| `crc/crc16/crc16_64_bytes` | 81.0 | 80.9 | 80.6 | 81.0 | 81.0 | 82.0 | 80.7 |
| `crc/generic_fire40/1024_bits` | 1,133 | 1,133 | 1,122 | 1,138 | 1,131 | 1,128 | 1,129 |
| `crc/generic_fire40/184_bits` | 199 | 201 | 197 | 199 | 199 | 198 | 199 |
| `fsm/dispatch` | 3.5 | 3.7 | 3.5 | 3.3 | 3.3 | 3.4 | 3.4 |
| `fsm/state_change` | 6.6 | 6.5 | 6.5 | 5.9 | 5.8 | 5.9 | 5.9 |
| `gsm0503/xcch/decode_clean` | - | - | - | - | 7,566 | 4,162 | 4,171 |
| `gsm0503/xcch/decode_noisy` | - | - | - | - | 7,529 | 4,156 | 4,157 |
| `gsm0503/xcch/encode` | - | - | - | - | 1,219 | 1,193 | 1,178 |
| `msgb/alloc_1500` | 45.7 | 40.2 | 39.2 | 40.5 | 46.1 | 39.8 | 40.1 |
| `msgb/alloc_64` | 34.3 | 34.0 | 33.1 | 34.2 | 33.9 | 34.0 | 34.7 |
| `msgb/put_pull_1500` | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.7 | 2.9 |
| `msgb/put_pull_64` | 2.9 | 2.9 | 2.7 | 2.9 | 2.9 | 2.7 | 2.9 |
| `msgb/put_pull_u32` | 3.4 | 4.3 | 3.2 | 3.4 | 3.5 | 3.2 | 3.4 |
| `timer/update/expire/1` | 37.8 | 37.7 | 37.7 | 35.7 | 36.4 | 35.7 | 32.9 |
| `timer/update/expire/16` | 356 | 344 | 342 | 338 | 332 | 337 | 313 |
| `timer/update/expire/256` | 7,337 | 7,318 | 7,265 | 7,172 | 7,194 | 7,300 | 7,236 |
| `timer/update/idle_256_pending` | 6.2 | 6.2 | 6.1 | 6.1 | 6.2 | 6.2 | 2.2 |
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
