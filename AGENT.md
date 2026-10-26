# Agent guide

## Purpose and locations

This project builds idiomatic Rust implementations of selected libosmocore APIs.
Recorded outputs from the C library define the behavior; the C source is
consulted to understand that behavior and decide what carries over to Rust.
See the development approach in `docs/README.md`.

Project identity: `osmocore-rs`, hosted at
`git@github.com:Silent-Wolf-Labs/osmocore-rs.git`. The Cargo package is
`osmocore-rs` and Rust imports use `osmocore_rs`. The repository checkout
retains its local `libosmocore-rs` name; the nested crate is `osmocore-rs/`.

- Repository root: `/home/jshilts-test-env/RustroverProjects/libosmocore-rs`.
- Rust crate: `osmocore-rs/` within this repository. Its manifest is
  `osmocore-rs/Cargo.toml`; implementation is in `osmocore-rs/src/`.
  Functional modules live directly in `src/`; substantial FSM, message-buffer,
  time and rate-counter implementations have private subsystem files. GSMTAP
  lives in `src/gsmtap/mod.rs`. Distinct C contracts are exported through
  `compat::<subsystem>`; there is no public `core` namespace.
  Target current stable Rust, edition 2024, without a declared MSRV.
- **Required C reference:** `libosmocore/` within this repository, at
  `/home/jshilts-test-env/RustroverProjects/libosmocore-rs/libosmocore`.
- Testing utility and reference harness project: `~/RustroverProjects/gsmtap-rs`
  (`/home/jshilts-test-env/RustroverProjects/gsmtap-rs`). This is a separate
  repository that must be kept up to date as the Rust conversion progresses.
- Rust compatibility tests: `osmocore-rs/tests/`; committed C observations:
  `osmocore-rs/tests/compatibility/vectors/`.
  Cargo discovers `tests/{utilities,conv,fsm,msgb,rate_ctr,time,gsmtap,net,runtime}/main.rs`,
  which register their respective conformance test modules. Shared independent
  fixture readers live in `tests/support/` and are selectively registered with
  `#[path]`; keep crate-root reader aliases and nested fixture module names intact.
  Do not register all support in every target. Keep moved tests registered exactly
  once, including integrity/corruption validators and subsystem integration tests.
  API-to-source locations, conversion status and verification are documented
  in `osmocore-rs/src/README.md` and the functional subsystem READMEs.
- C harnesses and generation instructions: `gsmtap-rs/reference/` and its
  `README.md`. Read that repository's agent guidance before changing it.

## Use only the local C reference

The in-repository `libosmocore/` checkout is intentionally ignored by Git. It
exists as reference material for the conversion, not as tracked project code.
Keep it ignored; do not force-add it, vendor it, or commit its build artifacts.

Use this checkout for C source, headers, tests, and reference builds. Do not use
the accidentally created duplicate checkout elsewhere, create another checkout,
or silently substitute a system-installed copy. If the required checkout is
missing or unsuitable for a harness, report the issue instead of falling back
to another copy. Do not delete other checkouts as part of routine work.

Check harness paths and build/link settings before generating observations.
Ensure C headers and linked libraries come from this checkout, including when
using `pkg-config`. Preserve its source behavior; do not modify the C reference
to make a Rust test pass.

## Process for each implementation or behavior change

1. Check Git status and existing guidance in each affected repository. Preserve
   unrelated work.
2. Maintain the corresponding direct-call C harness in `gsmtap-rs/reference/`
   as the conversion progresses. Add or update cases for the APIs and behavior
   being converted, including meaningful boundary and error cases. Update its
   fixture validation and regeneration documentation where applicable.
3. Generate expected results from C only. Record and verify the reference
   revision and source hashes according to the harness instructions. Review
   fixture diffs; explain changed behavior or bytes before accepting them.
   Never generate expected C results from the Rust implementation.
4. Consult the local C implementation, headers and C tests to understand the
   recorded return values, output bytes, buffer mutations and error behavior,
   find boundary cases to add to the harness, and decide what carries over to
   Rust. Do not copy C code, comments or data tables into the crate; reproduce
   exact values only where compatibility requires them.
5. Implement idiomatic Rust and test it against the committed C observations.
   Cover caller-visible buffer changes as well as return values. Document any
   deliberate Rust safety differences for calls that would be unsafe in C.
6. Keep shared fixtures consistent between both repositories. Conversion
   records such as BCD and hexparse belong in their dedicated fixture locations
   in `gsmtap-rs`; do not feed them to GSMTAP packet or replay loaders. The
   Rust-versus-C behavior comparison belongs in this project's conformance
   tests; `gsmtap-rs` also validates fixture integrity and provenance.
7. Run the relevant checks in both repositories and update documentation for
   the finished scope. Report changes and validation for both projects. A
   conversion is not finished while required testing-utility updates remain
   undone. If access or permissions block those updates, state exactly what
   remains and why; a handoff alone does not complete the work.

`gsmtap-rs` supplies testing utilities and C observations; it is not the oracle
for C behavior or a dependency of this Rust crate. Do not add unrelated
conversion APIs to its GSMTAP library merely to share tests.

## Validation

From this repository root:

```bash
cargo fmt --manifest-path osmocore-rs/Cargo.toml --check
cargo test --manifest-path osmocore-rs/Cargo.toml
```

When updating the testing utility, run from `~/RustroverProjects/gsmtap-rs`:

```bash
cargo fmt --check
cargo test
```

Also run the relevant reference harness when changing a harness or regenerating
fixtures. Ordinary Rust tests consume committed fixtures and do not require a
C installation. Review the final diff and Git status in each affected
repository. Do not commit, push, or publish unless requested.
