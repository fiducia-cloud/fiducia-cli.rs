# src

Source for the `fiducia` CLI, split into a pure library and a thin I/O binary.

- `lib.rs` — pure, unit-tested core: parsing the `edge-regions.json` region list
  (`{name, url}`), computing per-region median latency, and ranking regions
  (unreachable ones sort last). No I/O lives here.
- `main.rs` — the binary: argument/env parsing (defaults ← env ← CLI flags, per the
  flags-2-env model) and the actual latency probing over HTTP (ureq). Implements the
  `regions` (list) and `region`/`closest` (probe) subcommands.

The chosen region name is what a client sends as the `X-Fiducia-Region` header so the
load balancer routes to a nearby shard leader.
