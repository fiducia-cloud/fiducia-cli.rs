# fiducia-cli

The `fiducia` command-line tool for [fiducia.cloud](https://fiducia.cloud).
Today it helps clients pick the **region** to route to.

## Why region (not IP)

Clients pick a **region** (a stable enum value) — not their IP, which changes
with NAT/mobility/proxies. A client passes the chosen region with each request as
the `X-Fiducia-Region` header; the load balancer maps it to that region's shard
band so the owning shard's leader is geographically close.

The selectable region set is **generated** by `fiducia-infra` from `topology.toml`
into `edge-regions.json` (`[{ "name", "url" }]`) — one source of truth, never
hand-maintained.

## Usage

```sh
# list the selectable regions (the enum)
fiducia regions --regions edge-regions.json

# probe every region and print the closest by median latency
fiducia region  --regions edge-regions.json --samples 5
# region            median ms  ok/total  url
# us-east                 7.4    5/5    https://aws.lb.fiducia.cloud
# eu-central             82.1    5/5    https://hetzner.lb.fiducia.cloud
#
# closest: us-east  (pass it as  X-Fiducia-Region: us-east)
```

`--regions` defaults to `$FIDUCIA_REGIONS_FILE` or `./edge-regions.json`;
`--path` (default `/healthz`) is the endpoint probed; `--samples` (default 5).

Probing knobs: `--timeout`/`-t` ms per probe (default 2000 — a slower region
counts as unreachable), `--warmup`/`-w` discards the first N probes per region so
the TCP/TLS handshake doesn't skew the median (default 0), and `--only`/`-o`
restricts to a single named region. `--json`/`-j` swaps the table for JSON —
`regions` emits `[{name,url}]`, `region` emits `{ regions: [...], closest }` — so
the output composes straight back into the same shape the CLI consumes:

```sh
fiducia region --json | jq -r .closest          # -> us-east
```

## Region routing model

| Key kind | Routing | Why |
|----------|---------|-----|
| **region-scoped** (region-local data) | `X-Fiducia-Region` → that region's shard band (`shard_for_region`) | low-latency, leader nearby |
| **global** (one lock worldwide) | region-agnostic `shard_for`; leader placed near demand via **leader affinity** | correctness — one shard everywhere |

So region selects locality for region-scoped data; global coordination stays a
single shard with its leader pulled close by affinity. See
[`fiducia-routing`](https://github.com/fiducia-cloud/fiducia-routing.rs).

## Configuration — flags-2-env

Flags are declared once in [`.cli-flags.toml`](.cli-flags.toml), the
[flags-2-env](https://github.com/ORESoftware/flags-2-env) config format. Each flag
maps to an env var. The direct Rust executable audits and parses that contract,
merges only argv-provided overrides over the environment, and then coerces the
result into the generated `CliConfig` type. The precedence is therefore
**CLI flags > environment > TOML defaults**:

```sh
export FIDUCIA_REGIONS_FILE=edge-regions.json   # = --regions / -r
export FIDUCIA_SAMPLES=7                          # = --samples / -n
export FIDUCIA_TIMEOUT_MS=1500                    # = --timeout / -t
export FIDUCIA_WARMUP=1                           # = --warmup  / -w
export FIDUCIA_ONLY_REGION=us-east               # = --only    / -o
export FIDUCIA_JSON=1                             # = --json    / -j
fiducia region                                    # uses env; flags still override
fiducia region -n 3                               # CLI wins over FIDUCIA_SAMPLES
```

[`src/cli_config.rs`](src/cli_config.rs) is generated from the contract and
checked for drift in CI:

```sh
make -C vendor/flags-2-env cli
vendor/flags-2-env/build/flags2env \
  generate rust .cli-flags.toml --name CliConfig
```

The Cargo dependency embeds the pinned native parser in the executable, so
direct invocation behaves the same on Linux, macOS, and Windows. The repository
also pins the same upstream source as a submodule for generation, auditing, and
the optional compatibility launcher:

```sh
git submodule update --init --recursive
make -C vendor/flags-2-env cli
scripts/with-flags2env.sh --samples=3 -- cargo run --locked -- region
```

The launcher is not an enforcement boundary; the Rust command still validates
the contract, rejects unknown options, and performs typed coercion itself.

## Security posture

- **No credentials handled.** The CLI performs only *unauthenticated* HTTPS/HTTP
  `GET` probes of each region's health path; it stores and transmits no auth
  tokens, passwords, or secrets. Every configuration value it reads is a
  non-secret operational knob (regions-file path, sample count, timeout, warm-up,
  region filter, JSON toggle), so all of them may safely be passed as CLI flags.
- **Tokens (if ever added) must be env-only.** Should an authenticated endpoint
  be introduced, any token/credential MUST be supplied via an environment
  variable, never as a plain CLI flag — a flag value leaks into shell history and
  the process argument list (`ps`). Such a var must also be listed under
  `[env].ignore` in [`.cli-flags.toml`](.cli-flags.toml) (and/or marked secret in
  its `help`) so flags-2-env never surfaces it as a flag.
- **TLS verification is on.** Probes use `ureq`'s default rustls stack with
  certificate verification enabled; verification is never disabled and there is no
  "accept invalid certs" switch.
- **No secret output.** Diagnostics are limited to the regions-file path, region
  names/URLs, and parse/IO errors — nothing sensitive is echoed to stdout/stderr.
- **Dependencies.** `cargo audit` is **clean** (0 advisories over 76 crate
  dependencies) as of the last audit; there are no accepted/ignored advisories.

## Build / install

```sh
cargo build --locked --release      # target/release/fiducia
cargo test --locked                 # ranking/median/parse unit tests
```

Building from source compiles the bundled C parser. The Rust `cc` crate selects
the platform compiler: Apple Clang on macOS, MSVC on Windows, and GCC or Clang
on Linux. GCC is neither universal nor a runtime dependency. A packaged
`fiducia` binary and the final container image need no compiler installed.

### Reproducible interfaces dependency

The CLI consumes generated contracts from the sibling `fiducia-interfaces`
repository. CI and the Dockerfile pin it to commit
`bd718cd72d72aa330534f3688f8fb1ce90c19d10` instead of a moving branch. The
Docker build checks that commit out detached and verifies that the resulting
full `HEAD` equals `INTERFACES_SHA`; branches, tags, and abbreviated hashes are
rejected. The final image also includes `.cli-flags.toml` under
`/usr/local/share/fiducia-cli`, so direct container execution keeps the same
typed contract. Update the Dockerfile argument and CI checkout `ref` together
when adopting a reviewed contracts commit.

```sh
docker build \
  --build-arg INTERFACES_SHA=<40-character-commit-sha> \
  -t fiducia-cli:local .
```

## Layout

| File | Responsibility |
|------|----------------|
| `src/lib.rs` | region parsing + latency ranking (pure, unit-tested) |
| `src/cli_config.rs` | generated typed representation of `.cli-flags.toml` |
| `src/flags.rs` | native contract audit, parse, precedence, and typed coercion |
| `src/main.rs` | command execution and the latency probe (ureq) |
