# .nix

Nix flake defining the reproducible development shell for this repo.

- `flake.nix` — a `devShells.default` with the Rust toolchain (rustc, cargo, rustfmt,
  clippy, rust-analyzer) plus supporting tooling (git, direnv, just, bacon, node/pnpm,
  pkg-config, openssl).
- `flake.lock` — pinned input revisions (do not hand-edit).

Entered via `nix develop ./.nix`, the repo-root `shell` wrapper, or automatically
through direnv (`.envrc`).
