# syntax=docker/dockerfile:1

# Multi-stage build for the Fiducia CLI.
FROM rust:1.97.0-slim-bookworm@sha256:6d220bf85c74e842a79da63997af8d2e74455c0b8847d8bb3a5888572334991d AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates

WORKDIR /build

ARG INTERFACES_SHA=487e470c45ab5851e8f6f3b1dc048fe067fbf408

# Fetch the path dependency by immutable commit, detach it, and fail closed if
# the resulting checkout is not the requested full SHA.
RUN git init fiducia-interfaces \
    && git -C fiducia-interfaces remote add origin \
       https://github.com/fiducia-cloud/fiducia-interfaces.git \
    && git -C fiducia-interfaces fetch --depth 1 origin "$INTERFACES_SHA" \
    && git -C fiducia-interfaces checkout --detach FETCH_HEAD \
    && test "$(git -C fiducia-interfaces rev-parse HEAD)" = "$INTERFACES_SHA"

COPY . fiducia-cli.rs

WORKDIR /build/fiducia-cli.rs

RUN cargo build --locked --release --bin fiducia \
    && strip target/release/fiducia

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:ce0d66bc0f64aae46e6a03add867b07f42cc7b8799c949c2e898057b7f75a151

COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/target/release/fiducia /usr/local/bin/fiducia

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/fiducia"]
