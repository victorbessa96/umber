# Kill report: umber-vector-canvas (K3, 2026-11-19)

Authorship: dragon implementation and verification, with bounded independent
read-only reviews by Kimi K3.1 Pro. Kill lock: `razul-vector-canvas`. The lock
held `crates/umber-core/src/{vector,layers,journal}.rs`,
`crates/umber-app/src/{vector_edit,uv_view,main,history_panel}.rs`, both i18n
catalogs, `README.md`, `docs/ROADMAP.md`, the K3 brief, this report,
`docs/loop/queue.md`, `STATE.md`, and `.dragon/COORDINATION-next-wave.md`.

Implements `docs/loop/briefs/2026-11-19-umber-vector-canvas.md`: Target and
decisions 1–6, tests, and exact lock surface. K1 is `46d4bff5`; K2 is
`0b5cb72d`; the K3 design brief is `8520c09b`; the K3 code landing is recorded
in the follow-up hash-truthing commit. K4 property widgets and K5 SVG
conversion/export remain separate kills.

## Built

- `umber-core/src/layers.rs`: reversible `LayerCommand::SetVector { id, new,
  old }`, `LayerCommand::set_vector`, and the Fill-precedent `LayerStack` setter.
  Apply/redo installs the complete `VectorParams`; undo restores the exact old
  params, including `None`. Target mapping and JSON/undo round trips are pinned.
- `umber-core/src/journal.rs`: additive `JournalOp::SetVector` forward, revert,
  and replay paths. A fresh layer's `None` state survives the same journal
  semantics as an initialized one.
- `umber-app/src/vector_edit.rs`: session `VectorEditTool` with document
  adoption, working-copy gestures, exact anchor selection/deletion,
  append/insert/delete/closure, and independent in/out-handle editing. Discrete
  edits and completed drags each write one `SetVector`; preview frames write no
  history. No-net-change releases are inert, external mid-gesture edits win,
  non-finite UVs and the 256-anchor cap refuse before mutation.
- Fresh-layer lifecycle: `Some((id, None))` adopts an empty editable shape.
  The explicit adopted `Option<VectorParams>` baseline lets the first anchor
  create params and undo restore `None` without disarming the editor.
- `umber-app/src/uv_view.rs`: K2's exact public centerline plus anchor/handle
  guides. Anchor hit targets register last, so plain drag moves a collapsed
  corner anchor. Shift pulls a collapsed in-handle and Alt a collapsed
  out-handle; separated handles drag directly. Right-clicking any visible
  handle deletes its exact anchor. `C` toggles closure only over the UV canvas
  and outside text input. A localized footer documents the controls.
- `umber-app/src/main.rs`: active-vector adoption, disarm/reset behavior, and a
  shell-level primary-release fallback that completes a drag even when the UV
  tab is hidden. Completion still validates the document baseline and is
  idempotent.
- `HistoryKind::SetVector` plus en-US/pt-BR history labels and the UV editing
  hint. The full History mapping and i18n suites stay green.

## Judgment calls

- Reused K2's centerline rather than adding a second curve flattener or drawing
  a raster-independent approximation. During a drag the guide is live; K2's
  raster plane republishes only after the completed command, keeping pointer
  frames out of the journal and avoiding per-frame raster work.
- Collapsed handles use explicit modifier routing instead of positional hit
  cycling. Plain = anchor, Shift = in, Alt = out; the choice is captured in the
  gesture, so separating handles mid-drag cannot change its target.
- Right-click deletion routes the actual anchor index, not a nearest-point
  search. This removes ambiguity when anchor and handles coincide.
- A hidden-tab release commits through the same `finish_drag` path as a visible
  handle release. External document edits still cancel rather than being
  clobbered by a stale working copy.
- No new egui scene harness was built for this bounded kill. Geometry, command,
  release, deletion, and collapsed-route logic have headless regression pins;
  real-pointer feel remains a dogfood check, not a claimed automated result.

## Verification

- Focused editor suite after review fixes: **11 passed / 0 failed**, including
  fresh-layer first anchor and undo-to-`None`, hidden-release idempotence, exact
  handle deletion, external-edit protection, cap/non-finite refusal, and one
  journal op per completed edit.
- Collapsed-handle routing test: **1 passed / 0 failed**, covering plain,
  Shift, Alt, separated-handle, and shared in/out routes.
- i18n suite after the new hint: **15 passed / 0 failed**.
- Final post-fix static gate: rustfmt clean on every touched Rust file;
  `check-layers.py` GREEN (16 crates); `scorecard.py --check` current;
  `git diff --check` clean.
- Final four-crate clippy + test battery: **clippy 0 errors; 20 suites, 1,978
  passed / 0 failed; `FINAL_BATTERY_EXIT=0`**. Receipt:
  `~/.hermes/cache/scratch/vector-canvas-final-battery-r1.log`.
- Kimi's first bounded review returned **FIX-FIRST** on four issues: fresh-layer
  adoption, exact-handle right-click deletion, collapsed-handle routing, and
  hidden-tab release. All four were fixed and pinned. Its bounded re-review of
  the current fixes returned **SHIP** with no HIGH/MEDIUM findings. Receipt:
  `~/.hermes/cache/scratch/vector-canvas-review-kimi-r2.log`.

## Unverified

- No automated claim is made about real pointer feel in a running desktop
  session. The headless pins cover the logic and release lifecycle; Bessa's
  dogfood remains the interactive check.
- K4 property widgets do not exist yet: fill/stroke values still use their
  current defaults or persisted state. K5 SVG conversion/export remains out.
- This kill adds no performance claim beyond avoiding per-frame journal and
  raster writes during drags.

## Deferred

- K4 vector property widgets and K5 SVG conversion/export.
- Fill-rule/cap/join selectors, gradients, multi-shape layers, and any format
  migration.
- A reusable egui pointer-journey harness if later vector interaction work
  needs full desktop-event simulation.

## Process note

Kimi K3.1 Pro was the sole review lane. The first review's four findings were
reproduced against the live tree, fixed, and re-reviewed before the final
battery. No claw edits, claw cargo, or claw commits were used. The final
battery and the SHIP verdict both postdate every review fix.
