# Brief: 2026-10-19 idmask-tile-cache (W-C3a lane P1)

Mandate: Bessa, 2026-10-19: "Continue working on the umber long horizon
project in all the steps, do not stop." Kill plan:
`docs/loop/next-wave-plan-draft.md` §1 lane P1 (verified anchors).

## What you are closing

The ID-mask tool re-bakes the whole ID map on every click
(`crates/umber-paint/src/paint_state.rs:2390-2398` — "no cached
store", the queue's named follow-up), and the bake spans the 0..1
frame (`crates/umber-brush/src/id_mask.rs:46-49`), so per-tile UDIM
bakes are wrong today: a mesh spanning more than tile 1001 picks the
wrong texel on every other tile. One kill, two halves: multi-tile
correctness + a single-entry cache. `bake_and_select_id(mesh, uv)`
KEEPS ITS SIGNATURE — the shell call site does not change.

## Contract

1. **Multi-tile.** When the mesh spans more than tile 1001
   (`umber_mesh::present_tiles` — grep its definition), bake
   `umber_bake::ao::filter_mesh_for_tile(mesh, tile)` (pub; the CLI
   uses it at `crates/umber-cli/src/main.rs:959`; it rebases the
   tile's UVs onto 0..1). You MUST rebase the click UV by the same
   integer tile offset before `select_id_mask` — `select_id` clamps
   an out-of-frame UV to the edge, which is exactly the wrong-texel
   bug. REUSE an existing tile-local helper (grep `local_texel`'s
   neighbourhood in paint_state, and umber_mesh's udim fns) — do not
   re-derive it. A single-tile mesh takes the whole-mesh path
   UNCHANGED (the CLI's `whole_mesh` precedent), byte-identical to
   today's behavior.
2. **Cache.** A single-entry `Option<CachedIdBake { key, tile, width,
   height, bytes }>` on `PaintState`. The key is content-derived
   INSIDE paint_state (no mesh generation counter exists, and
   shell-side invalidation would need files you do not own): a
   `std::collections::hash_map::DefaultHasher` over the mesh's
   `indices` + the `uvs` f32 bit patterns + `positions.len()`. The
   key ALSO includes the `IdFlavor`, so a future material/island
   flavor never hits a Triangle bake (grep the IdFlavor enum first).
   Hit = same (key, flavor, tile, dims) -> no GPU bake. Miss = bake +
   replace. A resize changes dims and misses naturally. State the
   cost in the docs: one O(tris) CPU hash per click (~20 MB touched at
   1M tris) against a 16M-texel compute bake + 64 MB readback at 4K,
   and ~64 MB resident for one 4K entry. Single entry is the judgment
   call: the last-clicked tile only, no LRU — name it in the report.
3. The refusal mid-stroke stays FIRST (grep the existing refusal —
   it precedes the bake today and must still precede everything). The
   cache is never touched on an error path.

## Files you own (the ONLY files you may write)

- `crates/umber-paint/src/paint_state.rs`
- `crates/umber-brush/src/id_mask.rs` (module-doc boundary update
  ONLY — the per-tile deferral note)
- `docs/loop/kill-reports/2026-10-19-idmask-tile-cache.md`

## Tests you write (do NOT run them — the dragon gates on the build host)

- a CPU test of the pure key fn: same mesh -> same key; one UV
  nudged -> different key.
- a CPU test of the UV rebase helper: tile 1001 identity, tile 1002,
  tile 1011.
- a GPU test (skip-graceful when no device — grep the paint_state
  id-mask GPU-test precedent for the exact skip idiom): a two-tile
  mesh, a click on tile 1002 selects the region there, and a second
  click on the same tile is a CACHE HIT observed through a
  `#[cfg(test)]`/test-hooks counter of bakes run.
- count the tests in the report.

## Discipline (binding)

- The tree is SHARED. Dirty or untracked files you did not create
  (`main.rs`, `viewport.rs`, `.dragon/*`, other kills' reports) belong
  to other live sessions: never touch, restore, checkout, or commit
  anything. `git log/diff/grep` reads are fine; NO git mutations.
- NO cargo, rustc, build, or test runs of any kind. You may grep/read
  files and run `python3` for derivation checks (a python port of the
  UV rebase — tile 1002 click at u=1.25 -> local 0.25 -> texel
  `round(0.25*w-0.5)` — is a named verification; include its output
  in the report). Match the files' existing formatting style.
- NO lock operations (`scripts/kill-locks.sh` any subcommand,
  `/tmp/umber-kill-locks/*`), never write to `/tmp` or anywhere
  outside the three files above.
- If any file:line anchor has drifted, re-derive from live source.
  Never invent an API — grep its definition first.

## Report shape

Write `docs/loop/kill-reports/2026-10-19-idmask-tile-cache.md`:
header naming authorship (claw: opencode/deepseek-v4.1-flash-free),
kill-lock `idmask-tile-cache` + its files, design source
(`docs/loop/next-wave-plan-draft.md` §1 lane P1 + this brief); then
**Built** (per-file bullets), **Judgment calls** (each with the
alternative rejected), **Unverified**, **Deferred**, **Process note**
(as needed). End your run with a one-screen summary: files changed +
every test name + what it pins.
