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

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && useradd --uid 10001 --user-group --home-dir /nonexistent --shell /usr/sbin/nologin fiducia

COPY --from=build --chown=10001:10001 /build/fiducia-cli.rs/target/release/fiducia /usr/local/bin/fiducia

USER 10001:10001

ENTRYPOINT ["/usr/local/bin/fiducia"]
