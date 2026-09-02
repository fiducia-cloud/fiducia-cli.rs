# syntax=docker/dockerfile:1

# Multi-stage build for the Fiducia CLI.
FROM rust:1.98.0-slim-bookworm@sha256:1469a27c125cb5a3aebfa4f4e4665d935b02fb72cc093b2c974b3d740e43f157 AS build

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

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:adcd20c7b4c988b73cbfbddb26d2eee574571e6d7c9ffea29b3821e0690efb77

COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/target/release/fiducia /usr/local/bin/fiducia
COPY --from=build --chown=65532:65532 /build/fiducia-cli.rs/.cli-flags.toml /usr/local/share/fiducia-cli/.cli-flags.toml

USER 65532:65532

# --- sops: this final stage has no shell (distroless/scratch), so runtime
# decryption cannot run inside the container. Inject secrets HOST-SIDE at
# `docker run` instead — never at build, never as --build-arg:
#     just env-docker-run prod <image>        # decrypts env/enc/prod.env.enc
#                                             # and passes --env-file, no plaintext on disk
# or render a platform secret from the same ciphertext. See env/README.md.
ENTRYPOINT ["/usr/local/bin/fiducia"]
