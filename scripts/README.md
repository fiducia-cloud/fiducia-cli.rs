# scripts

Helper scripts for building and running the CLI.

- `with-flags2env.sh` — the flags-2-env launcher. It parses CLI flags against
  `.cli-flags.toml` using the pinned `flags-2-env` submodule, exports them as the
  environment variables the Rust CLI reads, then execs the target command
  (e.g. `cargo run --locked -- region`). This is an optional compatibility path:
  the Rust executable also embeds the pinned parser, audits the contract, and
  performs typed coercion for direct invocation on every supported platform.
