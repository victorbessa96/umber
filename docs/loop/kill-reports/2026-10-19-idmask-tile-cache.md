# Kill report: ID-mask per-tile bake + single-entry cache (W-C3a lane P1)

Date: 2026-10-19. Author: opencode claw (deepseek-v4.1-flash-free), for the
dragon. Lock: `idmask-tile-cache` over
`crates/umber-paint/src/paint_state.rs`,
`crates/umber-brush/src/id_mask.rs` (module-doc boundary only) and this
report.

Design source: `docs/loop/next-wave-plan-draft.md` §1 lane P1
(`idmask-tile-cache`) + `docs/loop/briefs/2026-10-19-idmask-tile-cache.md`.

## Context

The ID-mask tool re-baked the whole ID map on every click
(`bake_and_select_id`, the "no cached store" judgment call), and
`umber_brush::id_mask::select_id` clamps an out-of-frame UV to the frame
edge. Together those two facts made per-tile UDIM selection wrong: the bake
spans the mesh's `0..1` UV frame, and a click on tile 1002 (say `u = 1.25`)
was clamped to `u = 1.0` before selection, picking the wrong texel on every
tile but 1001. This kill fixes the multi-tile path and adds a single-entry
bake cache. `bake_and_select_id(mesh, uv)` keeps its signature — the shell
call site (`main.rs:580`) is untouched.

## Built

**`crates/umber-paint/src/paint_state.rs`**

- `CachedIdBake` (new private struct, module scope): `{ key, tile, width,
  height, bytes }` — the last bake, its content key and its frame.
- `PaintState.id_bake_cache: Option<CachedIdBake>` (new field), initialized
  `None` in `with_target_size`; the constructor is the only `Self { … }`
  site.
- `PaintState.id_bakes: u64` (`#[cfg(test)]`, initialized 0) +
  `id_bakes_run()` accessor (`#[cfg(test)]`) — the test observability the
  brief asks for.
- `rebase_uv_for_tile(uv, tile)` (new private fn, beside `tile_uv_offset`
  in `local_texel`'s neighbourhood): subtracts the tile's integer UV
  offsets. REUSES `tile_uv_offset` — no re-derivation.
- `id_bake_key(mesh, flavor)` (new private fn): a
  `std::collections::hash_map::DefaultHasher` over `indices`, the `uvs` f32
  bit patterns, `positions.len()`, and a flavor tag.
- `bake_and_select_id` rewritten:
  1. **Refusal first.** `if self.stroking { return
     Err(StrokeInFlight) }` still precedes everything; the cache and the
     bake are never reached in flight.
  2. **Multi-tile gate.** `whole_mesh = umber_mesh::present_tiles(mesh) ==
     [FIRST_TILE]` (the CLI's `whole_mesh` precedent). A single-tile mesh
     bakes the whole mesh and hands `uv` through UNCHANGED (byte-identical
     to v1). An empty mesh (`present == []`) takes the per-tile branch so
     its bake still rejects `EmptyMesh`.
  3. **Per-tile.** Otherwise `tile = tile_of_uv(uv)`, the bake input is
     `umber_bake::ao::filter_mesh_for_tile(mesh, tile)` (tile triangles,
     UVs rebased onto `0..1`), and the click UV is rebased by the SAME
     offset (`rebase_uv_for_tile`) before `select_id_mask`.
  4. **Cache.** Hit test on `(key, tile, width, height)` first, WITHOUT
     taking the entry, so a miss never disturbs the existing entry; on a
     hit the entry is `take`n (to free `self` for `select_id_mask`'s
     `&mut`) and restored. On a miss the bake runs, and the entry is
     replaced only AFTER a successful selection — an error path leaves the
     cache exactly as it was.

**`crates/umber-brush/src/id_mask.rs`**

- The "Single tile" module-doc bullet only: the one-line deferral note is
  replaced with the landed boundary — the shell now supplies a per-tile
  bake (`filter_mesh_for_tile` + UV rebase), so `select_id` stays a pure
  `0..1`-frame mapping; an out-of-frame UV still clamps to the edge here and
  the shell never hands one down. No code changed.

## Tests added (3; `paint_state.rs` `#[cfg(test)]`)

- `id_bake_key_is_content_derived_and_flavor_separated` (CPU) — same mesh
  ⇒ same key; one UV nudged by `+0.001` ⇒ different key; `IdFlavor::Material`
  ⇒ different key than `Triangle`; `positions.len()` changed ⇒ different
  key.
- `rebase_uv_for_tile_subtracts_the_tile_offset` (CPU) — tile 1001 identity
  (`[0.25, 0.75]` and `[1.0, 0.0]` unchanged); tile 1002
  `[1.25, 0.5] -> [0.25, 0.5]`; tile 1011 `[0.5, 1.2] -> [0.5,
  1.2_f32 - 1.0]` (the derived f32 expression, `local_texel`-test
  precedent).
- `two_tile_id_bake_selects_the_clicked_tile_and_caches` (GPU, skip-graceful
  via `try_request_device`, the `:9590` precedent) — `two_tile_strip()`,
  click `[1.25, 0.5]` on tile 1002: bake counter 0 → 1, `coverage(1002)[16 *
  32 + 8] == 1.0` (can-fail: the un-rebased path selects texel `(31, 16)`
  instead), the plane has both `1.0` and `0.0` (two-region guard), tile-1001
  plane absent; second click on the SAME tile ⇒ counter stays 1 (cache hit)
  and the same region id; a click on tile 1001 ⇒ counter 2 (single entry, a
  tile switch re-bakes) and `coverage(1001)` appears.

## Static verification (no cargo)

Python port of the UV rebase, run this session:

```
w=32: local_u=0.25 -> x=8;  buggy clamp x=31.0
w=4096: local_u=0.25 -> x=1024;  buggy clamp x=4095.0
tile 1001: offset=(0,0)  1.25,0.5 -> 1.25,0.5
tile 1002: offset=(1,0)  1.25,0.5 -> 0.25,0.5
tile 1011: offset=(0,1)  1.25,0.5 -> 1.25,-0.5
```

The `w = 32` line is the GPU test's pinned texel (`x = 8`); the buggy
column is the pre-fix clamp `(31, 16)` the test can-fails against.

`bash scripts/verify-kill.sh crates/umber-paint/src/paint_state.rs
crates/umber-brush/src/id_mask.rs` → braces 1235/1235 and 56/56, fmt clean
(in-place aligned), matches!-arity clean, doc-attachment clean. The only
WARNs are the pre-existing `clone_source: Option<Pos2>` field lines, a
false positive (not a thiserror error source).

**cargo UNRUN** on this host by contract (no cargo/rustc/build/test). The
dragon's build host gates fmt + clippy `-D warnings` + the full suites.

## Judgment calls

1. **The click routes by `tile_of_uv`, not `self.route_tile`.** ALTERNATIVE
   REJECTED: `route_tile` returns `FIRST_TILE` whenever `multi_tile` is
   false, and `multi_tile` is armed only by `set_mesh` — which the shell
   does not call on the live paint path (only tests do). Routing through
   `route_tile` would have left the exact wrong-texel bug in place for a
   shell that has not armed the stroke router. `tile_of_uv` is the UDIM
   rule and agrees with `route_tile` whenever `multi_tile` is true, so it is
   strictly more correct here. (Stroke routing itself is outside this
   kill's file surface.)
2. **Single entry, no LRU.** ALTERNATIVE REJECTED: an N-entry map keyed by
   `(key, tile)` — more resident memory (N × 64 MB at 4K) for a work pattern
   (alternating tiles) that is rarer than re-clicking one tile. The brief
   names the single entry as the judgment; recorded in `CachedIdBake`'s
   docs.
3. **Flavor folded into the key via a `u8` tag, no `flavor` field.**
   ALTERNATIVE REJECTED: adding a `flavor: IdFlavor` field to
   `CachedIdBake` and comparing it separately — the brief's struct shape is
   `{ key, tile, width, height, bytes }`, and `IdFlavor` (in `umber-bake`,
   not owned here) does not derive `Hash`, so a tag keeps the same effect
   without editing that crate.
4. **Cache untouched on error paths.** A miss does not `take` the entry;
   the old entry survives a failed bake or a refused selection, and the new
   entry is stored only after success. ALTERNATIVE REJECTED: take-first then
   replace-or-drop (simpler, but it clears a stale entry on a failed click,
   which the brief forbids).
5. **`whole_mesh` mirrors the CLI's exact test** (`present == [FIRST_TILE]`),
   including the empty-mesh case (per-tile branch → `EmptyMesh`). ALTERNATIVE
   REJECTED: `present.len() <= 1` — treats an empty mesh as single-tile and
   would bake an empty whole mesh, a different error path than the CLI's.

## Unverified

- No GPU/runtime run: the two-tile GPU test's bake is reasoned against the
  `filter_mesh_for_tile` + `select_id` mappings (both read) and the python
  derivation, not executed. The build host's suite is the first runtime
  evidence.
- `DefaultHasher` equality/inequality is pinned within one process (its
  construction is deterministic); the test does not assert an exact key
  value, only relations, so it is stable across hosts.
- API signatures confirmed by reading, not compiling:
  `umber_mesh::present_tiles`, `umber_bake::ao::filter_mesh_for_tile`,
  `Vec<u16> == [u16; 1]` (also used at `umber-cli/src/main.rs:934`),
  `Option::is_some_and`, `f32::to_bits`.

## Deferred

- No cache invalidation hook from the shell (none is needed: the key is
  content-derived inside `paint_state`, per the brief). A mesh edit of any
  UV bit or the topology misses naturally.
- No multi-entry cache / tool-entry pre-bake: the brief scopes the cache to
  one entry, last-clicked tile.
- The stroke router's `multi_tile` wiring (if the shell should route strokes
  per-tile too) is another lane's surface; this kill's click path is
  independent of it.

## Process note

`git status` reads and `verify-kill.sh` (rustfmt + the lock audit's
read-only listing) only; no cargo/rustc/build/test, no git mutations, no
`kill-locks.sh`, no writes to `/tmp` or outside the three owned files.

## Addendum (2026-10-20 23:5x, local-4d2e8a71): the battery catch, fixed forward

The first real compile (the local degraded-mode battery, Bessa's
grant) caught what every static gate passed: the test-hook increment
`#[cfg(test)] self.id_bakes += 1;` — an attribute on an assignment
EXPRESSION (E0658, illegal) that therefore never removed the
increment from release builds, where the `#[cfg(test)]`-gated
`id_bakes` field does not exist (E0609). rustfmt parses both shapes;
the claw's cross-review missed it; the type system did not. Fixed as
a cfg-guarded block (the one legal statement form), manifest
re-pinned with this note's reason. Same class as the usdc.rs Ok(())
catch the same battery made an hour earlier: a python port proves the
math, only a compile proves the file.
