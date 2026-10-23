# Kill report — usdz-sublayer-walk (W-C3b kill 1)

Date: 2026-10-20. Session: local-4d2e8a71 (dragon). Host-free under
the 2026-10-20 code steer (FREEZE-4, centralhost dark); battery at
HOST_BACK via scripts/w-c3b-close.sh (to be written at sweep close),
which runs AFTER w-c3a-close.sh completes.

## Claim

Lock `usdz-sublayer-walk` (2 paths: `crates/umber-mesh/src/usdc.rs`,
this report). Claimed in the coordination note 16:2x; the first claim
splice's anchor assert refused the main session's 16:19 STATE rewrite
mid-flight — zero clobber, re-spliced against fresh bytes (recorded in
the note's Recent events).

## What this closes

- `docs/specs/release-checklist-0.1.0.md` known-deferred
  `.usdz-beyond-first-layer` (usdc.rs:43, re-verified still true by the
  2026-10-19 D2 sweep) — the tick rides the w-c3b close script's docs
  row, not this kill (queue/STATE/checklist stay the recovery's docs
  rows' territory).
- Requirements row 111's usdz boundary ("its first layer's first Mesh
  prim").

## Built — `crates/umber-mesh/src/usdc.rs` only

- `load_usdz`: root-layer precedence preserved (a root that authors a
  Mesh never reads a sublayer — first-layer behavior byte-compatible
  for every package that loads today), then the root's `subLayers`
  walked in authored order (USD opinion strength: the first sublayer is
  the strongest).
- `sublayer_names`: reads the pseudo-root's `subLayers` through
  `try_field(abs_root, "subLayers")`, accepting both spellings —
  `Value::StringVec` (what openusd's usda parser stores; its own test
  pins `["./someAnimation.usd", ...]`) and `Value::AssetPathVec`.
- `normalize_sublayer`: lexical package-relative normalization
  (`@./geo.usda@` → `geo.usda`); `..` segments, absolute paths, and
  empty names yield `None` (USDZ forbids escaping the package).
- `walk_sublayers`: depth-first, cycle-guarded (`visited` seeded with
  the root's own entry name), depth-bounded (`MAX_SUB_LAYER_DEPTH = 16`).
  Missing or unreadable sublayers are SKIPPED (USD's layer-registry
  tolerance rule); a found-but-malformed Mesh is a hard error.
- 8 pinned tests, all fixtures programmatic (no shipped binaries):
  root-mesh precedence, the sublayer load, nested sublayers, the
  missing-sublayer tolerance, the cycle termination, the escape
  rejection, the depth bound enforced (20-deep chain errs), and the
  depth bound reachable (15-deep chain loads). Fixture shape mirrors
  the panic-hunt test's own `usdz_package` builder
  (tests/panic_hunt_mesh.rs:573 — `usdz::ArchiveWriter`, stored
  entries, raw .usda layer texts).

## Judgment calls

1. **Tolerance, not fatality, for absent/unopenable sublayers.**
   Alternative rejected: hard-error the walk. Studio packages routinely
   sublayer session/asset paths that are NOT shipped inside the usdz;
   USD's own layer registry tolerates a missing sublayer so the load
   continues. Follow-up: none — a package whose whole mesh lives in an
   absent sublayer still fails with the honest `no Mesh prim` error.
2. **A found-but-malformed Mesh is fatal.** Alternative rejected:
   skip and continue to the next sublayer. The importer's contract is
   "the first Mesh prim"; silently skipping a broken one would import a
   WRONG (later) mesh and mask real corruption. The up-front topology
   validation block (`build_mesh_from_spec`) already gives the precise
   error.
3. **Twin guards: cycle set + depth bound.** Alternative rejected:
   cycle guard alone — a hostile 10k-deep non-cyclic chain still
   stack-overflows without a bound; 16 covers real composition depth
   (root + sublayer chains in shipped assets stay < 5) with margin.
4. **Escapes rejected lexically, never resolved.** Alternative
   rejected: resolving `../` against the package root. USDZ's spec
   forbids escaping the package; resolving would silently violate it
   and hand the importer a filesystem path shaped by untrusted input.
5. **Only the layer-stack arc (`subLayers`) walks.** Prim-level
   composition arcs (references, payloads, variants, inherits) are NOT
   composed — the first-Mesh import contract does not need stage
   composition, and openusd's `pcp` is a bigger surface than this
   deferral asked for. Named boundary: a usdz whose mesh is only
   reachable through a reference still reports `no Mesh prim`.

## Unverified (until the battery at HOST_BACK)

- The compile + the 8 tests run (rustfmt's clean parse is the static
  syntax gate; every `openusd::` symbol grep-verified in the 0.8.0
  registry bytes: `Archive::contains`/`read`/`first_layer_name`/
  `open` (usdz/reader.rs:51/:151/:60/:30), `ArchiveWriter::new`/
  `add_layer`/`finish` (usdz/writer.rs:42/:56/:72),
  `Value::StringVec`/`AssetPathVec` (sdf/value.rs:70/:76),
  `AssetPath::as_str` (sdf/asset_path.rs:72)).
- `try_field(abs_root, "subLayers")` reachability on **.usdc-authored
  sublayers**: the usda spelling is verified against the parser's own
  test; whether openusd's crate writer/reader round-trips pseudo-root
  fields is version-dependent and deliberately NOT pinned by a test —
  if it does not, a usdc-rooted package keeps today's first-layer
  behavior (no regression; the deferral narrows to usdc-authored
  sublayer roots). The `AssetPathVec` arm covers the other spelling.
- A nested `.usdz` entry as a sublayer composes one level through
  `Archive::read`'s own first-layer rule (reader.rs documents it); not
  test-pinned — the named boundary is its nested sublayers.

## Deferred

- Full stage composition (references/payloads/variants) — see judgment 5.
- Multi-mesh import (the contract stays first-Mesh).
- A real-corpus pass over Pixar's reference packages (needs the host).

## Process note

The python port of `normalize_sublayer` (12/12 cases agree, including
the `@…@`-quoted, multi-`./`, `a/../b`, absolute, and empty shapes)
is the independent re-derivation of the rule. rustfmt canonicalized 5
of my new-byte regions (the parked-fleet pre-recovery sweep precedent;
the reflow diff confined to this kill's bytes). Manifest:
`.dragon/usdz-sublayer-walk-manifest.txt` (16-hex pins, the fleet's
truncated-prefix convention).

## Addendum (2026-10-20 18:5x, local-4d2e8a71): the usdc-sublayer Unverified, narrowed

Kill 1's Unverified section named the open question — whether a
`.usdc`-authored sublayer root round-trips `subLayers` through
openusd 0.8. Answered from the registry source (a read, not a run):

- The crate WRITER walks every spec `data.spec_paths()` reports and
  pre-interns the root `/` at path index 0 (usdc/writer.rs:144-158,
  `pack`) — pseudo-root fieldsets serialize through the same generic
  fieldset path as any prim's.
- The crate format's own test round-trips a PseudoRoot spec carrying
  a field (`primChildren`) through `CrateWriter::write` and back
  (usdc/reader.rs:1807-1812, `compact_paths`) — pseudo-root fields
  demonstrably survive the write/read loop.
- `Value::StringVec` — the variant the usda parser stores `subLayers`
  as — is a first-class serializable variant (sdf/value.rs:222
  dispatches it through the serializer).

Conclusion: a `.usdc` layer authored with subLayers SHOULD expose the
field to the walk (the StringVec arm reads it). The residual is the
live fixture confirmation at the battery — now a confirmation, not
an open question. No code change: the walk already accepts the
spelling. This addendum re-pins the report row in
`.dragon/usdz-sublayer-walk-manifest.txt` (reason recorded in its
header).

## Addendum 2 (2026-10-20 23:5x, local-4d2e8a71): the battery catch

Bessa's local-build grant fired the first real type-check: the
baseline run's compile died on `usdc.rs:165` — `walk_sublayers`'
final line read `Ok(())` where `Ok(None)` belongs (E0308, expected
`Option<MeshData>`). Every static gate passed it: rustfmt parses both
shapes, the python port validated the ALGORITHM's math (hand-mirrored,
never the file's bytes), and no cargo had type-checked the file. The
error was unreachable behavior-wise only by luck — the tail executes
whenever a sublayer chain bottoms out without a mesh, returning
`Ok(Ok(()))`-shaped garbage instead of `Ok(None)`; the caller's
`if let Some(mesh) = walk_sublayers(...)` would have... not compiled,
which is the point: the type system caught it before any runtime
existed. Fixed (one token), the manifest re-pinned with this note's
reason, and the lesson recorded: a python port proves the math, only
a compile proves the file — the exact gap the battery's first leg
closes, closed here by the degraded-mode grant before a single commit
or push rode on the bug.
