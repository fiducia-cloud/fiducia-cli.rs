#!/usr/bin/env bash
set -euo pipefail

readonly interfaces_sha="bd718cd72d72aa330534f3688f8fb1ce90c19d10"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
interfaces_root="$(cd -- "$repo_root/.." && pwd -P)/fiducia-interfaces"
contract="$repo_root/.cli-flags.toml"
fixture="$repo_root/.github/fixtures/regions.json"

if [[ ! -e "$interfaces_root/generated/rust/Cargo.toml" ]]; then
  if [[ -e "$interfaces_root" ]]; then
    echo "fiducia-interfaces exists without generated/rust/Cargo.toml" >&2
    exit 1
  fi
  git init "$interfaces_root"
  git -C "$interfaces_root" remote add origin \
    https://github.com/fiducia-cloud/fiducia-interfaces.git
  git -C "$interfaces_root" fetch --depth 1 origin "$interfaces_sha"
  git -C "$interfaces_root" checkout --detach FETCH_HEAD
fi

actual_interfaces_sha="$(git -C "$interfaces_root" rev-parse HEAD)"
if [[ "$actual_interfaces_sha" != "$interfaces_sha" ]]; then
  echo "fiducia-interfaces must be checked out at $interfaces_sha" >&2
  exit 1
fi

cargo build --locked --manifest-path "$repo_root/Cargo.toml" --bin fiducia
binary="$repo_root/target/debug/fiducia"

root_help="$("$binary" --help)"
grep -Fq -- "--regions" <<<"$root_help"
grep -Fq -- "FIDUCIA_SAMPLES" <<<"$root_help"

subcommand_help="$("$binary" regions --help)"
grep -Fq -- "fiducia regions" <<<"$subcommand_help"
grep -Fq -- "--json" <<<"$subcommand_help"

regions_output="$(
  FIDUCIA_FLAGS_CONFIG="$contract" \
    "$binary" regions --regions="$fixture" --json
)"
grep -Fq -- '"name": "smoke-region"' <<<"$regions_output"

readonly sentinel="must-remain-environment-only"
if rejected_output="$(
  FIDUCIA_FLAGS_CONFIG="$contract" \
    "$binary" regions --api-token="$sentinel" 2>&1
)"; then
  echo "undeclared secret-bearing CLI option was accepted" >&2
  exit 1
fi
if [[ "$rejected_output" == *"$sentinel"* ]]; then
  echo "rejected CLI option value was reflected in diagnostics" >&2
  exit 1
fi
grep -Fq -- "--api-token" <<<"$rejected_output"

echo "flags2env runtime smoke: ok"
