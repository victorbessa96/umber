# Kill report — WAVE-5 PWYS RESEARCH (2026-10-14)

## 1. Built

- `docs/research/pwys-stroke-buffer.md` (NEW, ~7 sections): the
  architectural research doc the roadmap's wave-pwys row demands
  ("design-doc first — the deepest architectural row left",
  `docs/loop/wave-plan.md:27`). Numbered sections, cited claims, cost
  table, staged-kill table, six named-open questions, judgment calls,
  full source list. No code touched, no behavior changed.
- Tests added: 0 (research kill — the test PLAN is §4.3 of the doc:
  7 items, exact-byte pins + never-crash variants + the perf-floor
  count assertion).

## 2. Judgment calls

- Affirmed the 2026-10-12 design pass (option A, frozen-matrix) by
  re-derivation against the current tree, not by deference: every
  invariant it relied on (per-event staging, drain cadence, seam/
  symmetry hooks, retouch readback) was re-read live and still holds.
  Recorded as J1 in the doc.
- Pulled UDIM cross-tile continuity INTO the v1 kill surface (PARTIAL
  stage) instead of leaving it standalone: the same
  `mirror_positions` expansion site serves camera-freeze and tile
  seams, and splitting them conditions one kill's tests on another's
  landing (J3).
- Left symmetry's tile-local/no-cross-expansion boundary closed (J4):
  orthogonal to the camera question; reopening it is what (a)/(c)
  would force, and is documented as such.
- Costed against §12 budgets by construction + arithmetic (bytes,
  passes, pipeline touch-count), not measurement — no cargo per the
  kill protocol (J5). Measurement obligation sits with the staged
  kills' goldens + perf floor.
- Did NOT mark the requirements §1 pwys row (`requirements.md:30`):
  the standing amendment locks this kill to the 2 files, and the row
  already carries its `✓ built (b2306c6)` v1 note — the follow-up text
  update belongs to the dragon at landing. Flagging, not skipping.

## 3. Unverified

- All Mari/Krita/Photoshop fine-grained behavior claims beyond the
  in-tree corpus transcription are marked `(unverified-live)` in the
  doc (§§1–3, §7): no network fetch this kill per the research-kill
  protocol. URLs transcribed from the in-tree corpus (`05`, `02`,
  `04`) with original fetch dates; re-fetch is owed by whichever kill
  builds the opt-in full buffer (O3), if ever.
- No API signatures needed verification beyond what was read live —
  but note: `OrbitCamera` gained no snapshot accessor; the PROBE
  stage's `snapshot()` shape is proposed, not pinned against a
  review. `glam::Mat4`/`Vec3` are `Copy`, so a plain-data snapshot is
  available with no new types.
- Behavior needing a GPU/runtime run (none run — NO cargo): the §2
  frame-cost claims (fullscreen composite inside 16.7 ms), the 200 Hz
  / 4K no-drop behavior under frozen resolve, lavapipe-vs-WARP float
  rounding on the future goldens (§4.3.6 names the range-check
  carve-out). All recorded as staged-kill measurement obligations.

## 4. Deferred

- Requirements-row marking (`requirements.md:30` 4th cell) — for the
  dragon at landing (surface lock; see §2 last bullet).
- Resolve-at-end (option B) and the full GPU buffer (option (a)):
  named follow-ups with demand signals (O2, O3), not designs — no
  kill should build them without the signal.
- O1 (BVH vs 1M-tri raycast), O4 (real tablet timestamps), O5 (ortho
  sequencing), O6 (dynamic-materials eval invalidation) — all named
  OPEN in §5 for the implementing kills.
- Pre-existing related follow-ups this doc deliberately did not
  absorb: tilt-driven dab shape (`stylus_input.rs:22-26`), per-channel
  dab density (paint_state module docs), split redo under merged
  undo history.

## 5. Gates run

- `cargo fmt` / `clippy` / test suites: NOT RUN — forbidden by the
  kill protocol (NO cargo; research kill, markdown-only surface).
- `bash scripts/verify-kill.sh` on the 2 locked files: PASS, verbatim
  tail: "VERIFY-KILL: docs/research/pwys-stroke-buffer.md
  docs/loop/kill-reports/2026-10-14-pwys-research.md — balanced,
  fmt-clean, trap-scanned, 1 unlocked-but-noted file(s)" (the noted
  file is `crates/umber-app/src/retouch.rs` — another kill's
  dragon-owned surface, not this kill's).
- In-tree citation rule: every `path:line` citation in the research
  doc was read live this kill (viewport.rs full 727 lines;
  paint_state.rs docs + `:1590-1839` + `:4096-4349`;
  stylus_input.rs full; raycast.rs full; udim.rs full; camera.rs
  `:1-120`; seam/udim/pwys-design/requirements/wave-plan/ROADMAP/
  dynamic-materials excerpts as cited; brush-engine research §5–§6 +
  full doc; spp-hdf5-layout.md exemplar full).
- No commits (per protocol). Surface respected: only the 2 locked
  files written.
