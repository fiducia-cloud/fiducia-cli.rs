#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
contract="$repo_root/.cli-flags.toml"
fixture="$repo_root/.github/fixtures/regions.json"

install_zed_if_needed() {
  command -v zed >/dev/null 2>&1 && return 0

  local archive expected tmp
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      archive=zed-x86_64-unknown-linux-gnu.tar.gz
      expected=b8f81a8e4943cbaeb47153386819a911dcf02666a8709c5bd013e4f35b1ba1b9
      ;;
    Darwin-arm64)
      archive=zed-aarch64-apple-darwin.tar.gz
      expected=46052288d8a5ca178e7f942aeb03197022416ce13c3ac4ebd0fea66963977e62
      ;;
    Darwin-x86_64)
      archive=zed-x86_64-apple-darwin.tar.gz
      expected=618d2739b252aab84877b280759f70b2d0352eb7d2c4a9e91a37baa152f48ad7
      ;;
    *)
      echo 'flags2env runtime smoke needs zed on this platform' >&2
      return 1
      ;;
  esac

  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp:-}"' EXIT
  curl --fail --location --retry 3 \
    --output "$tmp/$archive" \
    "https://github.com/zed-pkg/zed-cli/releases/download/v0.2.3/$archive"
  if [[ $(uname -s) == Linux ]]; then
    printf '%s  %s\n' "$expected" "$tmp/$archive" | sha256sum --check --strict
  else
    test "$(shasum -a 256 "$tmp/$archive" | awk '{print $1}')" = "$expected"
  fi
  tar -xzf "$tmp/$archive" -C "$tmp"
  PATH="$tmp:$PATH"
  export PATH
}

# ores-clis-core is intentionally a Zed-owned source SDK. The compliance smoke
# must establish the same frozen package state as normal CI before Cargo runs;
# never fall back to a direct Git dependency just to make this lane build.
install_zed_if_needed
(
  cd "$repo_root"
  zed validate --require-lock
  zed install --frozen --install-mode copy
)

# fiducia-interfaces and fiducia-client remain rev-pinned Cargo Git dependencies.
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
