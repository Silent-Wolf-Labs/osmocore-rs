# CI pipeline stages; build one with `buildah build --target <stage> .`.
ARG RUST_IMAGE=docker.io/library/rust:alpine3.23

FROM ${RUST_IMAGE} AS toolchain
RUN apk add --no-cache bash git ca-certificates shellcheck coreutils findutils grep tar
# Use the toolchain already installed in the official Rust image.
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}
RUN rustup component add clippy rustfmt
ARG CARGO_DENY_VERSION=0.20.2
RUN set -eu; \
    arch=$(uname -m); \
    case "$arch" in \
        x86_64) sha256=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f ;; \
        aarch64) sha256=995c82be0defc7a025cae49a2aa2644ce8245c9a3318fc4103907c6a285e8c7d ;; \
        *) echo "cargo-deny: unsupported architecture $arch" >&2; exit 1 ;; \
    esac; \
    name="cargo-deny-$CARGO_DENY_VERSION-$arch-unknown-linux-musl"; \
    wget -q -O /tmp/cargo-deny.tar.gz \
        "https://github.com/EmbarkStudios/cargo-deny/releases/download/$CARGO_DENY_VERSION/$name.tar.gz"; \
    echo "$sha256  /tmp/cargo-deny.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/cargo-deny.tar.gz -C /usr/local/bin --strip-components=1 "$name/cargo-deny"; \
    rm /tmp/cargo-deny.tar.gz

# Fetch dependencies against stub targets so this layer only changes with the lockfile.
FROM toolchain AS deps
WORKDIR /src/osmocore-rs
COPY osmocore-rs/Cargo.toml osmocore-rs/Cargo.lock ./
RUN mkdir -p src examples \
    && touch src/lib.rs examples/runtime_service.rs \
    && cargo fetch --locked \
    && rm -rf src examples

FROM deps AS source
ENV CARGO_NET_OFFLINE=true
COPY scripts /src/scripts
COPY .githooks /src/.githooks
COPY osmocore-rs /src/osmocore-rs

FROM source AS lint
RUN find ../scripts ../.githooks -type f \( -name '*.sh' -o -name pre-commit \) \
        -print0 | xargs -0 -r shellcheck \
    && ../scripts/ci/test-release-gate.sh
RUN cargo fmt --check
RUN for features in '' tracing net net,tracing runtime runtime,tracing runtime,net runtime,net,tracing; do \
        cargo clippy --locked --all-targets --no-default-features --features "$features" -- -D warnings \
        && RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --no-default-features --features "$features" \
        || exit 1; \
    done
RUN cargo deny --locked --all-features check licenses sources
RUN for features in runtime runtime,tracing; do \
        cargo deny --locked --no-default-features --features "$features" --exclude-dev check bans \
        || exit 1; \
    done

# Fetches the live RustSec database; pass a fresh ADVISORIES_RUN so the result is never cached.
FROM source AS advisories
ARG ADVISORIES_RUN
RUN echo "Advisory check $ADVISORIES_RUN" \
    && CARGO_NET_OFFLINE=false cargo deny --locked --all-features check advisories

FROM source AS test
ARG FEATURES=''
RUN timeout 600 cargo test --locked --no-default-features --features "$FEATURES"

FROM source AS package
RUN cargo build --locked --no-default-features --features runtime --example runtime_service \
    && timeout 30 target/debug/examples/runtime_service
RUN cargo build --release --locked --all-features \
    && cargo package --locked

# Export with `--output type=local,dest=dist`.
FROM scratch AS artifact
COPY --from=package /src/osmocore-rs/target/package/*.crate /
