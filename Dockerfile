# syntax=docker/dockerfile:1

# Multi-stage build for the Fiducia CLI.
FROM rust:1-slim-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates

WORKDIR /build

ARG INTERFACES_REF=main

RUN git clone --depth 1 --branch "$INTERFACES_REF" \
    https://github.com/fiducia-cloud/fiducia-interfaces.git fiducia-interfaces

COPY . fiducia-cli.rs

WORKDIR /build/fiducia-cli.rs

RUN cargo build --release --bin fiducia \
    && strip target/release/fiducia

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/target/release/fiducia /usr/local/bin/fiducia

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/fiducia"]
