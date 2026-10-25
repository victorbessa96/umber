# Kill report — the OccluderScope panel row (L5b's panel half, the roadmap's ungated tail row)

Executed 2026-10-24 23:0x–23:2x by local-7de7d3ed, on Bessa's
autonomous "continue" stretch (the queue after the picker: the
OccluderScope row, named ungated since the ftl lock lifted).

## What the derivation found

The enum (umber-bake's `OccluderScope`, the L5 ray half) landed
long ago WITH the `_with_occluder` bake variants — but the panel
hardcoded the whole mesh as the occluder at all four live call
sites. The missing piece was exactly what the roadmap named: the
PANEL row + its keys. The v1 ships the enum's two simple variants
(WholeMesh / OwnSet); the `Sets(BTreeSet)` variant stays deferred
(it needs a set-list picker UI — the named follow-up).

## What landed

### bakes_panel.rs

- **The panel fields** — `occluder_own_set: bool` (the row's
  choice) + `applied_occluder_own_set` (the debounce snapshot,
  the rays' own pattern). `false` = the whole mesh, today's
  behavior BYTE FOR BYTE (the default).
- **The sync** — the param-tracking block: a scope change dirties
  the RayLength class (the occluder narrows what the rays SEE —
  the same dirty set a ray change marks).
- **The row** — after the skew row: the combo (Whole mesh / Baked
  set only), `BakeSettingId::OccluderScope` for the jump
  highlight; the AO/thickness/bent log entries' jump stays the
  Rays row (the same group).
- **The request** — `occluder_own_set` rides the BakeRequest to
  the worker.
- **The four call sites** (AO whole, AO tile, thickness, AO+bent
  via `bake_extra_map`): the occluder param resolves inline —
  `if occluder_own_set { raster_source } else { mesh }`. THE
  INSIGHT the derivation found: `raster_source` IS the set's own
  submesh (per-set masked bakes §3's own product) — OwnSet's
  occluder is the raster source itself, no extraction needed.

### i18n

The three keys in BOTH catalogs (the pt-BR: "Escopo do oclusor /
Malha inteira / Apenas o conjunto assado").

## The derivation's own misses (named)

1. The panel fields' DECLARATIONS missing on the first cut (the
   new() inits landed, the struct block didn't) — the compile
   caught it.
2. The `occluder_for` closure mixed two borrows' lifetimes (the
   captured mesh + the param sub) — resolved INLINE at the four
   sites instead (the simpler shape all along).
3. The row-append patch's bad anchor mangled one line (the skew
   row's `.unit`) — repaired in the same patch that appended.

## Tests

The battery's umber-app leg (851 + clippy + the layers leg). The
i18n completeness covers the new keys' parity. The named
follow-up: the OwnSet scope's own test (a two-set mesh bake
comparing whole vs own occluders — the enum's own tests already
pin the resolution; the panel's threading test rides the kittest
class).

## The named follow-ups

- The `Sets(BTreeSet)` variant's panel UI (the set-list picker).
- The OwnSet bake-output test (two-set mesh, the two scopes'
  outputs differ on the boundary).
