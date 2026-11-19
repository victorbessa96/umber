# Kill report: umber-vector-widgets (K4, 2026-11-19)

Authorship: dragon implementation and verification, with bounded independent
read-only review by Kimi K3.1 Pro. Kill lock: `razul-vector-widgets`. The lock
held `crates/umber-gui/src/widgets/context_rail.rs`,
`crates/umber-core/src/vector.rs`,
`crates/umber-app/src/{vector_panel,vector_edit,main}.rs`, both i18n catalogs,
`README.md`, `docs/ROADMAP.md`, the K4 brief, this report,
`docs/loop/queue.md`, `STATE.md`, and `.dragon/COORDINATION-next-wave.md`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-widgets.md`: Target and
decisions 1–5, tests, and exact lock surface. K1 is `46d4bff5`, K2 is
`0b5cb72d`, K3 is `98fdbb80`, and the K4 brief is `7f1c6cc3`. The K4 code
landing is recorded in the follow-up hash-truthing commit. K5 SVG
conversion/export remains separate.

## Built

- Dedicated `ContextKind::Vector` after Fill in the Properties rail, using the
  existing `spline` atlas glyph. Its index, `ALL` ordering, localized tooltip,
  and every headless fixture line up. Vector targets expose Layer + Vector;
  other targets do not inherit the new context.
- New `crates/umber-app/src/vector_panel.rs`: one collapsible section per
  vector layer with Closed, Fill sRGBA, Stroke-enabled, Stroke width, and
  Stroke sRGBA controls. Stroke-only rows remain in the static search
  vocabulary. Both catalogs cover the context and every row.
- Every property change clones the current params, or the default for an
  uninitialized layer, and runs exactly one K3 `LayerCommand::set_vector` per
  egui change. No direct `Layer.vector` mutation or second journal path.
- `Default for VectorParams`: empty open path, opaque white fill, no stroke.
  `VectorShape::empty()` remains transparent for geometry/raster short-circuits.
  K3 fresh-layer adoption now uses the visible default, so the first drawn path
  is visible while the document baseline remains `None` until an actual edit.
- Stroke enablement defaults to width `0.01` UV and opaque black. Re-enabling
  an existing stroke preserves its values; disabling removes only the stroke.
  Width uses the house `ValueField` with finite `0.0001..=1.0` UV range.
- Native egui `color_edit_button_srgba_unmultiplied` edits the model's `[u8; 4]`
  directly, with no manual transfer-function approximation.

## Judgment calls

- Added a dedicated rail context rather than presenting vector controls under
  Fill. Existing enum order now places Vector after Fill, and all tooltip
  arrays were updated in that same order.
- Kept `VectorShape::empty()` transparent and put the visible default on
  `VectorParams`, so raster empty-input semantics did not change. The default
  affects editor/panel adoption only; deserialization and old projects keep
  their stored bytes and `None` baseline.
- Followed the Fill panel's one-command-per-egui-change convention. K3's
  one-command-per-pointer-gesture rule remains specific to canvas drags.
- Used the existing atlas glyph and native color editor rather than adding
  assets, dependencies, or a second color pipeline.

## Verification

- Focused core vector suite: **19 passed / 0 failed**, including the visible
  default and transparent `empty()` distinction.
- Full `umber-gui` suite: **150 passed / 0 failed**, including generic rail
  order/index/atlas checks after the variant addition.
- Vector panel tests: **3 passed / 0 failed**, covering drawn/localized row
  labels, stroke defaults and preservation, first property edit creating
  params, and undo restoring `None`.
- K3 editor lifecycle: **11 passed / 0 failed**, including first-anchor white
  fill and undo-to-`None` after the default change.
- Properties rail decisions: **3 passed / 0 failed**; i18n: **15 passed / 0
  failed**.
- Final static gate: rustfmt clean on every touched Rust file;
  `check-layers.py` GREEN (16 crates); `scorecard.py --check` current;
  `git diff --check` clean.
- Final five-crate clippy + test battery: **clippy 0 errors; 22 suites, 2,133
  passed / 0 failed; `FINAL_BATTERY_EXIT=0`**. Receipt:
  `~/.hermes/cache/scratch/vector-widgets-final-battery-r1.log`.
- Bounded Kimi K3.1 Pro review of the exact K4 diff: **SHIP**, no HIGH/MEDIUM
  findings. Receipt: `~/.hermes/cache/scratch/vector-widgets-review-kimi.log`.

## Unverified

- No automated claim is made about real pointer feel or visual appearance in a
  running desktop session. Native widget behavior is covered by the existing
  egui/gui suites and the new state tests; Bessa's dogfood remains the visual
  and interaction check.
- K5 SVG conversion/export does not exist yet. Fill-rule, cap/join, dash,
  gradient, and multi-shape controls remain out of scope.

## Deferred

- K5 SVG conversion/export and the richer vector-property families named in
  the brief's non-goals.
- Fill cache-lifecycle parity remains the separate named follow-up.
- A reusable full-desktop Properties pointer journey if later vector work
  needs visual-event automation beyond the current headless rail coverage.

## Process note

Kimi K3.1 Pro was the sole review lane. No claw edits, claw cargo, or claw
commits were used. The final battery and SHIP verdict both postdate every K4
code change. The first docs-first commit attempt hit the expected untracked
path guard and ledgered nothing; staging only the brief allowed the same locked
commit to proceed as `7f1c6cc3`.
