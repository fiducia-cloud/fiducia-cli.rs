# workflows

GitHub Actions pipelines for `fiducia-cli`.

- `ci.yml` — on push/PR: checks out the sibling `fiducia-interfaces` repo, then runs
  `cargo fmt`, `clippy`, the bin/lib tests, and `cargo audit`.
- `cli-flags.yml` — when the flag declarations or the flags-2-env submodule change,
  audits `.cli-flags.toml` with the pinned `flags2env` tool so the flag/env contract
  stays valid.
