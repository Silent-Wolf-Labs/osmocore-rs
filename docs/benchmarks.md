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
| Benchmark | v0.1.0 | v0.1.0 repeat | Decoder saturation fix |
| --- | ---: | ---: | ---: |
| `bits/pack_184` | 105 | 104 | 104 |
| `bits/pack_456` | 256 | 255 | 254 |
| `bits/pack_offset_184` | 119 | 119 | 118 |
| `bits/pack_offset_456` | 298 | 294 | 293 |
| `bits/unpack_184` | 100 | 99.7 | 99.8 |
| `bits/unpack_456` | 252 | 252 | 251 |
| `bits/unpack_offset_184` | 92.5 | 92.8 | 91.9 |
| `bits/unpack_offset_456` | 239 | 235 | 233 |
| `conv/k5_n2/decode_clean` | 6,313 | 6,270 | 5,727 |
| `conv/k5_n2/decode_noisy` | 6,310 | 6,290 | 5,708 |
| `conv/k5_n2/decoder_reuse_noisy` | 22,669 | 23,131 | 24,220 |
| `conv/k5_n2/encode` | 608 | 604 | 601 |
| `conv/k7_n3/decode_clean` | 26,003 | 25,988 | 20,067 |
| `conv/k7_n3/decode_noisy` | 26,328 | 26,100 | 20,071 |
| `conv/k7_n3/decoder_reuse_noisy` | 113,461 | 112,508 | 111,139 |
| `conv/k7_n3/encode` | 674 | 680 | 668 |
| `crc/crc16/ccitt_1500_bytes` | 2,317 | 2,311 | 2,302 |
| `crc/crc16/ccitt_64_bytes` | 81.3 | 82.0 | 80.8 |
| `crc/crc16/crc16_1500_bytes` | 2,323 | 2,313 | 2,297 |
| `crc/crc16/crc16_64_bytes` | 81.0 | 80.9 | 80.6 |
| `crc/generic_fire40/1024_bits` | 1,133 | 1,133 | 1,122 |
| `crc/generic_fire40/184_bits` | 199 | 201 | 197 |
| `fsm/dispatch` | 3.5 | 3.7 | 3.5 |
| `fsm/state_change` | 6.6 | 6.5 | 6.5 |
| `msgb/alloc_1500` | 45.7 | 40.2 | 39.2 |
| `msgb/alloc_64` | 34.3 | 34.0 | 33.1 |
| `msgb/put_pull_1500` | 2.9 | 2.9 | 2.7 |
| `msgb/put_pull_64` | 2.9 | 2.9 | 2.7 |
| `msgb/put_pull_u32` | 3.4 | 4.3 | 3.2 |
| `timer/update/expire/1` | 37.8 | 37.7 | 37.7 |
| `timer/update/expire/16` | 356 | 344 | 342 |
| `timer/update/expire/256` | 7,337 | 7,318 | 7,265 |
| `timer/update/idle_256_pending` | 6.2 | 6.2 | 6.1 |
<!-- bench-table:end -->

| Column | Source | rustc | Date | Notes |
| --- | --- | --- | --- | --- |
| v0.1.0 | `9756920` | 1.99.0 | 2026-11-01 | Starting baseline |
| v0.1.0 repeat | `9756920` | 1.99.0 | 2026-11-01 | Same code run again, to show noise |
| Decoder saturation fix | `adc549d` plus the fix | 1.99.0 | 2026-11-01 | `decode` metrics saturate; `conv` benches use +/-127 |

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

## Reading the numbers

- **Noise differs by bench.** The median gap between the two v0.1.0 runs is 0.4%,
  but `msgb/put_pull_u32` moved 25% and `msgb/alloc_1500` moved 12% on identical
  code. Don't draw conclusions from small changes in benches this short.
- **`conv` amplitude.** The v0.1.0 columns used +/-120 soft bits, because `decode`
  returned `ArithmeticOverflow` for clean +/-127 frames. Later columns use +/-127.
- **`conv` backends.** For these codes, `decode` uses the correlation decoder.
  `decoder_reuse_noisy` uses `ConvDecoder`, a different algorithm, so compare each
  only with its own earlier numbers.
- **`timer/update/expire`.** Each iteration includes the fixed cost of reading the
  clock around `update`, which is why rearming can stay outside the measurement.
  This matters most for `expire/1`, so compare it only with itself.
- **Build target.** These are default baseline x86-64 builds, so SIMD gains from
  `target-cpu=native` won't appear unless a run says so.
