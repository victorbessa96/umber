# Brief — W-C4 L3: ID-mask bake-on-tool-entry (the first-click hitch)

Written 2026-10-22 12:4x by local-7de7d3ed (charter cycle, host-free).
Source: the W-C4 plan's L3 lane + P1's kill report
(docs/loop/kill-reports/2026-10-19-idmask-tile-cache.md — read it
first; its cache seams are the surface this kill reuses). Anchors from
the WORKTREE 2026-10-22 (b10 bytes in main.rs, P1's in paint_state.rs);
re-derive at dispatch.

## Timing

POST-CLOSE, after L2 (dispatch order: tangent wiring → L1 → L2 → THIS
KILL → L5 → L6). main.rs (b10) + paint_state.rs (P1) are the parked
files this kill extends.

## The seam today

- Tool entry (main.rs:530-534, every frame): `idmask_armed = brush.strip
  == ToolKind::IdMask` → `self.viewport.set_idmask_armed(idmask_armed)`.
- The click pays the bake: `bake_and_select_id(mesh, uv)` (shell call
  site main.rs:580, P1 kept its signature) — on a cache miss it bakes
  the tile's ID map (filter_mesh_for_tile + rebase) BEFORE selecting.
  P1's cache is single-entry `CachedIdBake { key, tile, width, height,
  bytes }` keyed on mesh content (DefaultHasher over indices, UV bit
  patterns, positions.len) + flavor tag + tile + dims; hit-test never
  disturbs the entry, take-and-restore on hit, replace-after-success
  on miss, refusal-first (`stroking` → `Err(StrokeInFlight)` — the
  cache and bake are never reached in flight).
- Test observability (P1, `#[cfg(test)]`): `id_bakes` counter +
  `id_bakes_run()`; GPU-test precedent `two_tile_strip()` with
  `try_request_device` + coverage asserts.

## The change (pre-warm the likely first click)

1. **A pub warm method on PaintState** (paint_state.rs, beside P1's
   cache code): `warm_id_bake_for_active_tile(&mut self, mesh: &…,
   flavor)` — the SAME per-tile bake the click would run for
   `self.active_tile()`, stored into the cache WITHOUT selecting.
   Same disciplines P1 established: refusal-first (`if self.stroking {
   return }` — a tool switch is between-strokes by construction, but
   the guard rides anyway), single-entry replace, and the SAME flavor
   resolution the click path uses (re-derive where the flavor is
   chosen at dispatch — the key must match what the first click will
   compute or the warm is a guaranteed miss).
2. **The transition edge in the shell** (main.rs, at the :530-534
   site): fire the warm ONCE per not-idmask → idmask transition — a
   local `prev_idmask_armed` bool in the app state (the
   PenFlagTracker edge-detection precedent, pen_flags.rs:116-131),
   NOT per frame: the site runs every frame and a per-frame warm would
   re-bake constantly. The warm needs the mesh handle the :580 click
   site already uses.
3. **P1's cache is reused, not rebuilt**: single-entry stays (a click
   on a different tile still re-bakes — P1's contract, unchanged);
   only the entry the first click most likely hits is warm.

Judgment call (accepted per the plan's lane): the warm bakes even if
the user never clicks — the per-tile bake is the cheap filtered-mesh
kind, and the hitch moves off the click where it reads as lag. The
rejected alternative (hover-triggered warming) carries pointer-tracking
complexity and per-hover churn for no extra coverage.

## The test plan (P1's counter is the observable)

1. Warm → `id_bakes_run() == 1`; first click on the WARMED tile →
   counter stays 1, the region selects correctly (coverage assert per
   P1's two_tile test) — the click's bake path never ran. The
   without-warm baseline: first click alone = counter 1; the win is the
   click's latency, pinned by the counter staying flat across warm+click.
2. A click on a DIFFERENT tile after the warm → counter 2 (single
   entry: a tile switch re-bakes — P1's contract riding through).
3. Two consecutive armed frames → warm fires once (the edge discipline;
   counter 1 after both frames).
4. Warm mid-stroke → no-op (counter unchanged, the refusal guard).
5. Mesh-content invalidation: warm, change the mesh, click → re-bake
   (the key miss; P1's content-key contract through the warm path).

## Non-goals

- NO multi-entry or per-tile cache map (P1's single-entry stays; a
  bigger cache is its own lane if the hitch ever shows elsewhere).
- NO hover-triggered warming.
- NO `bake_and_select_id` signature or behavior change.
- NO flavor changes — the warm uses exactly what the click computes.
- NO edit before the close (b10/P1 dirt is another row's cargo).

## Verification ladder

rustfmt --check per touched file; the umber-paint (GPU legs,
skip-graceful) + umber-app battery legs when sanctioned; cross-review
by the other claw; the plan row + requirements row + CHANGELOG in the
same commit; no CI claims until the wave push (M2).
