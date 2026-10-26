# C compatibility observations

`vectors/` holds the committed C-generated fixtures shared with `gsmtap-rs`.
Functional integration tests consume these observations from their
respective sibling directories. Fixture bytes and provenance are unchanged by
the directory reorganization.

See [the test guide](../README.md) for coverage and regeneration commands.

Primitive observations are in `vectors/prim_vectors.json`, with strict shared
validation and conformance registered under `tests/utilities/main.rs`. They include
operation names and event mapping; see the test guide for scope and provenance.

CRC observations are in `vectors/crc_byte_vectors.json` and
`vectors/crc_generic_vectors.json`; undefined C shifts are excluded. The utility
conformance tests and shared strict validator register under `tests/utilities/main.rs`.

Convolutional C evidence is in `vectors/conv/conv_vectors.ndjson`, with copies
of its content-hash provenance manifests. The shared reader checks semantic
state, operations, buffers and backend identity; Rust behavior is replayed
separately by encoder, generic decoder and correlation/dispatch conformance
tests. `tests/conv/conv_encoder.rs` compares all 14,523
frozen length/encoder observations to the implemented Rust portion. The fixture,
manifests and shared validator are unchanged. Scalar/SSE/AVX correlation
observations agree for the frozen
coverage and define the approved CPU-independent portable tie contract;
unexecuted NEON is outside claimed backend parity. Linked dispatcher observations
are pinned-local-binary evidence, separate from exact-source backend evidence.
See [the compatibility boundary](../../src/conv/README.md).
