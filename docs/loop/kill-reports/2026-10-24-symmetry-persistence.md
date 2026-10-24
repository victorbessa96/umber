# Kill report — the symmetry persistence (lens A's gap 2, half of the persistence pair)

Executed 2026-10-24 16:4x–17:1x by local-7de7d3ed, from the
ROADMAP's pain-ordered tail (lens A's two persistence gaps; this
is the smaller of the pair).

## The honest re-read that shaped the scope

The lens named "SymmetrySettings not in .umber/.umberbrush" as a
gap. The derivation found THREE truths that reshaped the kill:

1. **The serde shape was already ready** (SymmetrySettings
   derives Serialize/Deserialize) — the gap was wiring, not
   format.
2. **The `.umberbrush` half is a deliberate non-goal, now
   documented as one**: a brush preset is a brush SHAPE, not a
   tool state; Painter's own symmetry is tool-level. The
   symmetry.rs doc note (:103) updated: the ".umber carries these
   since the persistence kill; the preset deliberately does not."
3. **The quick mask's "gap" is a design, not an omission** — its
   module docs own "not persisted, not undoable: the mask is
   temporary BY DEFINITION". The Substance-contrast (SPP persists
   the quick mask) makes the .umber-persistence a REAL design
   question, not a mechanical row; it stays named in the roadmap,
   argued when the user asks for it (the planes' serialization
   into the save-v2 per-layer structure = a bigger format change).

## What landed

### umber-core (project.rs)

- **`ProjectSymmetry` / `ProjectMirror` / `ProjectRadial`** — the
  core-owned serde MIRROR of the brush's settings. THE LAYERING
  CALL (named in the struct docs): core COULD dep on umber-brush
  (no cycle — brush does not dep core), but brush drags umber-gpu
  transitively, and umber-core's own doctrine is "no GPU,
  headless-testable" — the mirror keeps the doctrine (the
  DisplaySettings precedent works only because umber-color is
  already GPU-free). The axis serializes as `"x"`/`"y"`.
- **`ProjectSettings.symmetry`** — `#[serde(default)]`, the
  additive-carry rule (pre-symmetry files load with symmetry OFF,
  the same rule as `display`/`postfx`; no version bump).

### umber-app (main.rs)

- **The boundary conversions** — `symmetry_carry_from` /
  `symmetry_from_carry`, FREE FNS not From impls (the orphan rule
  bars the app from impling between two foreign crates' types —
  the `postfx_panel::carry_from_chain` precedent). The mercy rule
  lives here: any axis string that is not "y" (case-insensitive)
  loads as X.
- **The save site** — the ProjectSettings build stamps
  `symmetry_carry_from(&self.brush_panel.symmetry)`.
- **The load site** — the open adopts
  `symmetry_from_carry(&model.settings.symmetry)` into the brush
  panel; the paint session syncs on the next stroke through its
  existing `sync_symmetry` adoption path.

### umber-brush (symmetry.rs)

- The doc note rewritten (see the honest re-read above).

## Tests (both green, 0.00s)

- **`pre_symmetry_project_json_loads_default`** — a project.json
  with no symmetry key loads `ProjectSymmetry::default()` (the
  additive rule pinned).
- **`symmetry_carry_round_trip_exact`** — mirror Y@0.25 + radial
  6 with both flips: save→load exact, the axis wire name ("y")
  pinned.

## The derivation's own misses (named)

1. The conversion fns landed between `compose_mask_flow`'s doc and
   its signature (a patch placement mistake) — caught by the
   region read, repaired before any compile.
2. A dead `let _ = s;` + an unused-import class in the first cut
   — cleaned in the repair.
3. `Copy` on the carry derive — broken by the axis String;
   dropped (the carry clones once per save/load, named in the
   doc).

## The named follow-ups

- The quick mask's `.umber` persistence — the DESIGN question
  (the "temporary by definition" doctrine vs the Substance
  contrast), argued when asked.
- The quick mask's undo (the mask-mode strokes as journal steps) —
  the deeper machinery, named in the roadmap.
