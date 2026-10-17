# Kill report: preset-editor-i18n (the rail follow-up's prerequisite)

- Authorship: dragon (the 18:5x loop session), taken while the
  sibling holds `stencil-core` and its claw builds the stencil math.
- Kill-lock: none registered at write time — `brush_panel.rs` was
  free (the rail-row-labels lock released at its commit `8eb02c9`);
  registering `preset-editor-i18n` was skipped because the HOST is
  down (kill-locks live on the host's /tmp) — noted here as the
  honest gap; the tree diff is this file only.
- Implements the brush panel module docs' named i18n deferral: the
  preset editor's hardcoded English rows go through `tr()`, so the
  pt-BR locale renders them localized — and the rows join
  `ROW_LABEL_KEYS` (the rail doc's own stated condition for joining:
  "they join this list when that kill lands").

## Built (one file: `crates/umber-app/src/brush_panel.rs`)

- `tr()` wiring: Preset (`brush.preset`), Name (`brush.name`),
  Alpha (`brush.alpha`), Hardness (`brush.hardness`), Pressure gamma
  (`brush.pressure-gamma`); the group titles Stabilizer
  (`brush.stabilizer`), Lazy mouse (`brush.lazy-mouse`), Spacing
  (`brush.spacing`); the group rows Min cutoff (`brush.min-cutoff`),
  Beta (`brush.beta`), d cutoff (`brush.d-cutoff`), Radius px
  (`brush.radius-px`), Strength (`brush.strength`), Dabs per radius
  (`brush.dabs-per-radius`).
- `tr_args()` for the two count badges: `{n} broken` →
  `brush.broken`, `{n} overridden` → `brush.overridden` (both already
  carry Fluent plurals in BOTH catalogs).
- `ROW_LABEL_KEYS` +14 keys (58 total), in draw order: the preset
  selector/editor rows append AFTER the method rows (the panel draws
  the tool/channel/mask/cavity/stamp sections first, then the preset
  block — the draw-order census pinned by reading `show`).
- Both module docs updated (the deferral ticked; the registry's
  exclusion rule updated).
- The rail test's pins updated: 44 → 58 rows; the last label is now
  "Dabs per radius" (was "Dash length").
- NOT migrated (Phase 2 scope, unchanged): the curve editor's
  "Pressure → …" section headers, the preset save rows ("No preset
  selected" is a status readout — excluded by the rule).

## The catalog keys were ALREADY THERE

The en-US and pt-BR catalogs both carried every key before this kill
(preset/name/alpha/hardness/pressure-gamma/stabilizer/min-cutoff/
beta/d-cutoff/lazy-mouse/radius-px/strength/spacing/dabs-per-radius +
broken/overridden with plural forms) — the module doc said so
("wiring them is deferred, not forgotten"); this kill is purely the
wiring. ZERO catalog changes needed.

## Verification ladder (host-down protocol: no cargo)

1. rustfmt `--check --edition 2021`: PASS (one reflow fixed in place).
2. API resolution: `tr`/`tr_args`/`FluentValue::from` all match the
   established call shapes (bakes_panel.rs:376-441's tr_args
   precedents; the import line updated to `use crate::i18n::{tr,
   tr_args}`).
3. Python port + mutation rung: every one of the 58 keys verified
   present in BOTH catalogs; the test's pinned ends (en "Tool" first,
   "Dabs per radius" last) verified against the en-US catalog's own
   values; count == 58. Mutations: a bogus key is caught (PASS), a
   dropped key changes the count (PASS). Port verdict: PASS.
4. Blast radius: one file, no API changes, no new deps, no catalog
   edits — nothing outside brush_panel.rs can observe this except the
   rail's label list (the intended consumer).

## Unverified (host-gated — the battery parks on the host's return)

- `cargo test -p umber-app` (the rail test's 58-row pin +
  assert_row_label_keys against the real drawn source).
- clippy -D warnings.
- The host went down (the wgpu parallel-init deadlock class the
  sibling root-caused at `94469e8`, or a fresh wedge) at ~18:5x;
  this kill follows the outage protocol: verify-kill-style static
  gates + the Python port ran host-free; the battery runs first thing
  on the host's return.
