# Kill report (UNRUN, host-gated): `umber-cli bench` (ROADMAP wave-benchmark-public)

Date: 2026-10-15. Author: dragon. Lock: `wave-benchmark-public`
(`crates/umber-cli/src/bench.rs`, this report). ROADMAP row: "a
`umber-cli bench` subcommand: the bake/export/paint paths timed on a
reference mesh, so community perf reports are comparable".

## Built

- `bench.rs`:
  - `bench_mesh(64)`: a cube grid of 49,152 triangles, generated so no
    asset ships and every machine bakes identical geometry. Pinned by a
    spot value, a determinism check and an index-range test.
  - `run(size, rays)` times mesh generation, GPU context, then each of
    the six `bake-all` bakers (bake + padding) and its PNG encode,
    through the SAME `bake_map` / `pad_map` / `BakeMap::ALL` the real
    commands use.
  - The output is a table plus one `bench json:` line (stages, size,
    rays, triangles, `"name (DeviceType, Backend)"`, version, bake
    total); `--json PATH` writes the same to a file. Option parsing
    validates `--size` (a power of two, 64..=8192) and `--rays`
    (1..=1024) before any GPU work, and is tested.
- `main.rs`:
  - the `bench` route and usage line;
  - `bake_context_with_info()` (the adapter label); `bake_context()` now
    wraps it, with unchanged behaviour.

## Judgment calls

- **49k triangles, not the 1M viewport scene.** That scene lives behind
  umber-gpu's `perf` feature, and a public command must run on a laptop
  iGPU in seconds. The 1M row stays in `docs/perf/protocol.md`.
- **Export is timed as PNG encodes** of the baked maps, the part of
  export independent of a painted document. A preset-driven export run
  needs a document, so it is left out of a fixed benchmark.
- **Paint is not timed:** the headless paint numbers are the `perf`
  benches' (`pool_perf`, input-to-photon). The doc says so rather than
  inventing a stroke.

## Process note

`main.rs` is under `spp-extract-cli`'s lock (mine): this kill's two
hunks (`mod bench`, the route, `bake_context_with_info`) ride that file,
so **the two kills commit together**, the shared-file rule.

## Host

`cargo test -p umber-cli bench::`, clippy, then run `umber-cli bench`
on the host's NVIDIA GPU and on lavapipe, and record both JSON lines in
`docs/perf/protocol.md` as the first public baselines.
