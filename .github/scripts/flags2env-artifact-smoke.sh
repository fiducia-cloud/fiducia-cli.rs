#!/usr/bin/env bash
set -euo pipefail

readonly interfaces_sha="bd718cd72d72aa330534f3688f8fb1ce90c19d10"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
fixture="$repo_root/.github/fixtures/regions.json"
image="fiducia-cli-flags2env-smoke:${GITHUB_RUN_ID:-local}"

docker build \
  --build-arg "INTERFACES_SHA=$interfaces_sha" \
  --tag "$image" \
  "$repo_root"

root_help="$(docker run --rm "$image" --help)"
grep -Fq -- "--regions" <<<"$root_help"

regions_output="$(
  docker run --rm \
    --mount "type=bind,source=$fixture,target=/tmp/regions.json,readonly" \
    "$image" regions --regions=/tmp/regions.json --json
)"
grep -Fq -- '"name": "smoke-region"' <<<"$regions_output"

readonly sentinel="must-remain-environment-only"
if rejected_output="$(
  docker run --rm "$image" regions --api-token="$sentinel" 2>&1
)"; then
  echo "final image accepted an undeclared secret-bearing CLI option" >&2
  exit 1
fi
if [[ "$rejected_output" == *"$sentinel"* ]]; then
  echo "final image reflected a rejected CLI option value" >&2
  exit 1
fi
grep -Fq -- "--api-token" <<<"$rejected_output"

echo "flags2env final-image smoke: ok"
