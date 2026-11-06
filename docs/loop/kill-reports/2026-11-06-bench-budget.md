# Kill report — bench reference mesh fits the bake budget (installers gap #4, queue row 248)

- Date: 2026-11-06 ~19:0x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-bench-budget` (crates/umber-cli/src/bench.rs, STATE.md, this report).
- Gap closed: installers smoke (row 240, `2026-11-06-installers-smoke.md`)
  gap #4 — `umber-cli bench` (defaults or `--size 64 --rays 4`) always
  died with `position map: mesh has 49152 triangles, over the 4096 budget
  for this bake path`, because the generated reference mesh
  (`BENCH_SUBDIVISIONS = 64` → 6·64²·2 = 49,152 tris) exceeded
  `umber_bake::position::MAX_TRIS_PER_BAKE = 4096`, the shared-memory bake
  path's hard brute-force ceiling (enforced up front in `position.rs`,
  `gbuffer.rs`, `id.rs`, `transfer.rs`).

## The change (crates/umber-cli/src/bench.rs only)

- `BENCH_SUBDIVISIONS`: 64 → **18** → 6·18²·2 = **3,888 triangles**, under
  the 4,096 ceiling. The mesh is still a deterministic cube grid (same
  generator, identical bytes per machine), still large enough to exercise
  all six bakers + UV padding + PNG encode.
- The ceiling itself was NOT raised: `MAX_TRIS_PER_BAKE` is load-bearing
  (workgroup storage caps measured at 16 KiB–64 KiB; the shader streams
  triangles in fixed shared-memory batches). Sizing the reference mesh to
  the path is the correct fix, not growing the path.
- Module doc + constant doc updated to state the budget relationship.
- Tests: existing mesh test updated (3,888 tris, 6·19² positions); new pin
  `the_reference_mesh_fits_the_bake_paths_triangle_budget` asserts the
  reference mesh stays `<= umber_bake::position::MAX_TRIS_PER_BAKE`, so this
  gap cannot regress silently.

## Verification

- `cargo fmt --all -- --check`: clean.
- Battery (full rebuild after the 2026-11-06 `cargo clean`; flock, nice 19,
  -j3, log /tmp/umber-bench-battery.log): clippy -p umber-cli --all-targets
  -D warnings; cargo test -p umber-cli --all-targets (incl. the new pin);
  cargo build -p umber-cli. Results: see battery log; live run below.
- Live: `./target/debug/umber-cli bench --size 64 --rays 4` on this host
  (Intel Vulkan ICD): prints the stage table + `bench json:` line instead
  of the budget error.

## Records note

Queue row 248 + COORDINATION STATE line are DEFERRED: docs/loop/queue.md
and .dragon/COORDINATION-next-wave.md are currently held by the sibling
lock `razul-bundled-assets` (active 18:17–18:24, no live process at claim).
Append row 248 ("LANDED ... bench budget") when that lock releases.

## Not done / next

- Installers gap #2 (exe-relative runtime assets) — blocked on the same
  sibling lane (bundled.rs). Gap #5 (no-display launch poisons GPU fallback
  to GL) — next free kill; touches umber-gpu/umber-app main.rs (main.rs is
  sibling-locked too, so likely after the sibling lands). Gap #3 (v0.1.0
  tag) stays partner-gated.
