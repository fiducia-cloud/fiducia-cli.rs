# .github

GitHub-platform configuration for the `fiducia-cli` repository.

- `dependabot.yml` — weekly dependency-update PRs for Cargo crates and GitHub Actions.
- `workflows/` — cross-platform CI, generated-contract drift checks, and reusable
  flags2env consumer compliance.
- `scripts/` and `fixtures/` — credential-free source and final-image smoke
  canaries used by the compliance workflow.

These files drive automation on GitHub only; they are not part of the CLI binary.
