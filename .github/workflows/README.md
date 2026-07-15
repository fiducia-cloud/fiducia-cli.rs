# workflows

GitHub Actions pipelines for `fiducia-cli`.

- `ci.yml` — on push/PR: checks out the sibling `fiducia-interfaces` repo at the
  exact commit also pinned by the Dockerfile, then enforces formatting, locked
  Clippy/all-target tests, and a pinned `cargo-audit` as mandatory gates.
- `cli-flags.yml` — when the flag declarations or the flags-2-env submodule change,
  audits `.cli-flags.toml` with the pinned `flags2env` tool so the flag/env contract
  stays valid.

## Security baseline

Every executable workflow uses explicit least-privilege permissions, immutable
third-party action or container references, non-persisted checkout credentials,
concurrency control, and a job timeout. The main CI workflow validates this
directory with the digest-pinned actionlint container. Environment mutation is
forbidden unless this README documents a repository-specific platform exception.
