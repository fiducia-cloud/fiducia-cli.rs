#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
contract="$repo_root/.cli-flags.toml"
fixture="$repo_root/.github/fixtures/regions.json"

# fiducia-interfaces and fiducia-client are rev-pinned git dependencies, so
# cargo resolves them from Cargo.lock; nothing has to be staged beside the repo.
cargo build --locked --manifest-path "$repo_root/Cargo.toml" --bin fiducia
binary="$repo_root/target/debug/fiducia"

# The reusable compliance workflow checks the consumer out below its own
# workspace. Pass the same absolute contract to every normal invocation instead
# of relying on the caller's current working directory.
run_fiducia() {
  FIDUCIA_FLAGS_CONFIG="$contract" "$binary" "$@"
}

root_help="$(run_fiducia --help)"
grep -Fq -- "--regions" <<<"$root_help"
grep -Fq -- "FIDUCIA_SAMPLES" <<<"$root_help"

subcommand_help="$(run_fiducia regions --help)"
grep -Fq -- "fiducia regions" <<<"$subcommand_help"
grep -Fq -- "--json" <<<"$subcommand_help"

regions_output="$(run_fiducia regions --regions="$fixture" --json)"
grep -Fq -- '"name": "smoke-region"' <<<"$regions_output"

readonly sentinel="must-remain-environment-only"
if rejected_output="$(run_fiducia regions --api-token="$sentinel" 2>&1)"; then
  echo "undeclared secret-bearing CLI option was accepted" >&2
  exit 1
fi
if [[ "$rejected_output" == *"$sentinel"* ]]; then
  echo "rejected CLI option value was reflected in diagnostics" >&2
  exit 1
fi
grep -Fq -- "--api-token" <<<"$rejected_output"

# The help table and the completion scripts are rendered by the flags-2-env core
# from .cli-flags.toml at runtime. These assertions are what keep that true: if
# anyone reintroduces a hand-written usage string, the generated rows below stop
# matching the contract.
grep -Fq -- "Commands:" <<<"$root_help"
grep -Fq -- "completion" <<<"$root_help"

scoped_help="$(run_fiducia health --help)"
grep -Fq -- "--url" <<<"$scoped_help"
root_help_has_url=0
grep -Fq -- "--url" <<<"$root_help" && root_help_has_url=1
if ((root_help_has_url)); then
  echo "command-scoped --url leaked into the root help table" >&2
  exit 1
fi

for shell in bash zsh; do
  script="$(run_fiducia completion --shell "$shell")"
  grep -Fq -- "fiducia" <<<"$script"
done
run_fiducia completion --shell bash | bash -n -

# Exit codes are part of the contract: 2 = bad invocation, 3 = broken config.
set +e
run_fiducia completion --shell fish >/dev/null 2>&1
usage_status=$?
FIDUCIA_FLAGS_CONFIG=/nonexistent/.cli-flags.toml "$binary" regions >/dev/null 2>&1
config_status=$?
set -e
if ((usage_status != 2)); then
  echo "an unsupported --shell must exit 2, got $usage_status" >&2
  exit 1
fi
if ((config_status != 3)); then
  echo "an unreadable contract must exit 3, got $config_status" >&2
  exit 1
fi

echo "flags2env runtime smoke: ok"
