# syntax=docker/dockerfile:1

# Multi-stage build for the Fiducia CLI.
FROM rust:1.97.1-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777 AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates

WORKDIR /build

# fiducia-interfaces and fiducia-client are ordinary rev-pinned git
# dependencies now, so cargo fetches them itself against Cargo.lock. The `git`
# package above is what lets it do that; there is no sibling checkout to stage.
COPY . fiducia-cli.rs

WORKDIR /build/fiducia-cli.rs

RUN cargo build --locked --release --bin fiducia \
    && strip target/release/fiducia

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:fccdbb0a547c14e23fcf4ce8ad62ca5d43b4faae8d22cd292f490fef9946c96e

COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/target/release/fiducia /usr/local/bin/fiducia
COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/.cli-flags.toml /usr/local/share/fiducia-cli/.cli-flags.toml

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/fiducia"]
