# Design brief: umber-vector-panel-miter-limit (K8c, 2026-11-21)

Kill lock: `razul-vector-panel-miter-limit`.
Base: K8b implementation/origin `22bdd3e4be7f8fbc493d3d1e6014a0958c599ea1`.

## What exists

K8b landed `Stroke.miter_limit` in `umber-core`: finite `>= 1`, default 4,
additive legacy JSON, cache identity, normative SVG interior-angle geometry,
root/group/path import inheritance, non-default export, and native roundtrips.
The Vector Properties rail (`crates/umber-app/src/vector_panel.rs`) still exposes
stroke width, color, cap, join, dash pattern, and dash offset, but has no
miter-limit field. The stroke toggle constructs the stored default `miter_limit:
4.0`.

Normative source: W3C SVG 1.1 (Second Edition), Painting §11.4,
https://www.w3.org/TR/SVG11/painting.html#StrokeMiterlimitProperty . The limit
is `miterLength / stroke-width`, must be `>= 1`, and defaults to 4.

## Target

K8c exposes the already-stored value as an editable panel row:

- Add one localized `vector.stroke-miter-limit` label in
  `assets/i18n/en-US.ftl` and `assets/i18n/pt-BR.ftl`.
- In the stroke section, after the join combo, show a numeric `ValueField` only
  while `stroke.join == StrokeJoin::Miter`; Round/Bevel hide it because the
  value has no join geometry effect there, while the stored value and SVG export
  still roundtrip unchanged.
- Range the field `1.0..=1000.0` as a deliberate UI cap inside the core domain
  (finite `>= 1`; the core builder has no upper bound), avoiding non-finite or
  sub-1 UI input. Any field change rides the existing
  `LayerCommand::set_vector(id, edited)` path, so one edit is one undoable
  history entry.
- Add the key to `ROW_LABEL_KEYS` in draw order and update the panel label test's
  expected English list.
- Add focused tests for the Miter-only visibility behavior and for an edit
  producing one undoable `SetVector` with the new limit while preserving the rest
  of the stroke.

## Concurrency and safety

Locked files: `crates/umber-app/src/vector_panel.rs`,
`assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`, this brief, the kill report,
`docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`, `STATE.md`, and
`docs/ROADMAP.md` if the vector row is truth-updated. No core model, SVG parser,
exporter, schema/project-version, dependency, release, public-PR, or out-of-lock
work.

## Verification

- `cargo fmt --edition 2021 --check` on touched Rust.
- `python3 scripts/check-layers.py` and `python3 scripts/scorecard.py --check`.
- `cargo clippy -p umber-app --all-targets -- -D warnings` under the repo flock.
- Focused `cargo test -p umber-app vector_panel` plus the full `umber-app`
  battery with zero failures.
- Catalog parity/completeness tests for the new key in both locales.
- Bounded independent review of the final diff.

## Non-goals

No core geometry change, no miter-clip join, no CSS style parsing, no opacity or
group effects, no schema/project-version bump, no release tag/PR, and no public
posting.
