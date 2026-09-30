# Deterministic API and MCP documentation

`fiducia docs` is an offline ingress adapter around the shared `ORESoftware/api-docs` renderer. Fiducia does not maintain a second OpenAPI, OpenRPC, Connect, Hyper-Schema, HTML, or MCP canonicalizer.

The command reads `.api-docs.toml` from the current project, validates the referenced route map through `ores-api-docs`, renders one digest-bound publication bundle, and replaces the configured output directory with exactly that bundle. Replacing the tree prevents stale artifacts from making the filesystem result depend on a previous generation.

```toml
schema_version = "ores.api-docs.publication-config.v1"
route_map = "contracts/api.route-map.json"
out_dir = ".fiducia/docs"
publication_mode = "consumer_project"
producer = "my-project"
```

Run:

```sh
fiducia docs
```

## Publication modes are not interchangeable

`consumer_project` is for an end user's or tenant project's own generated documentation. The output is project-owned and must not be presented as authoritative Fiducia Cloud documentation.

`publisher_external` is for Fiducia-owned external developer documentation. Selecting that string does not grant authority: the Fiducia publication boundary must attach and verify Fiducia-controlled provenance before the bundle is served as official platform documentation.

The semantic API projections and digest-bound MCP discovery artifact come from the same validated catalog in both modes. Publication identity/provenance changes the publication envelope, not the service contract.

## Determinism contract

The shared renderer is a pure function of the validated route-map-derived catalog, explicit publication mode, and explicit producer identity. It does not depend on the clock, network, Git checkout, host, request origin, environment, or a model. Equal inputs produce byte-identical rendered files.

The CLI additionally clears and recreates the configured output directory before writing the bundle, so repeated runs cannot retain stale files from an older renderer version.
