# workflows

GitHub Actions pipelines for `fiducia-cli`.

- `ci.yml` — on push/PR: checks out the sibling `fiducia-interfaces` repo at the
  exact commit also pinned by the Dockerfile, then runs locked Clippy and tests
  on Linux, macOS, and Windows. Ubuntu also enforces formatting, workflow lint,
  and a pinned `cargo-audit`.
- `cli-flags.yml` — when the flag declarations or the flags-2-env submodule change,
  audits `.cli-flags.toml` with the pinned `flags2env` tool and rejects drift in
  the generated Rust `CliConfig`.
- `flags2env-compliance.yml` — calls the immutable reusable upstream policy,
  exercises canonical Bash/Zsh completion, builds and runs the real CLI, rejects
  undeclared secret-bearing flags without reflecting values, and repeats those
  checks against the final distroless image.

## Security baseline

Every executable workflow uses explicit least-privilege permissions, immutable
third-party action or container references, non-persisted checkout credentials,
concurrency control, and a job timeout. The main CI workflow validates this
directory with the digest-pinned actionlint container. Smoke tests apply
`FIDUCIA_FLAGS_CONFIG` to individual commands only; they do not persistently
mutate the runner environment.
