# Kill report: wave9-inpaint-core (wave-texture-inpainting, engine half)

- Authorship: dragon (solo, host-down night pass; the ROADMAP's Wave 9
  horizon row "the healing-brush class — the one retouch tool class
  the engine lacks").
- Kill-lock `wave9-inpaint-core`: `crates/umber-brush/src/retouch.rs`,
  `crates/umber-brush/src/lib.rs` (export).
- Builds the ENGINE half of the inpaint brush: a pure CPU retouch op
  in the exact house shape (footprint + radial_falloff + quick-mask
  gating identical to smudge/clone, per the module's own contract).
  The TOOL wiring (BrushTool variant, panel row, the stroke's source
  sampling UX) is the shell half, queued like the others.

## Built

- `InpaintParams { search_radius: i32, candidates: u32 }` (Default:
  radius 12, 96 candidates — a per-dab CPU budget).
- `inpaint_dab(dst, source, dab, sample_center, params, mask)
  -> Option<DirtyRect>`:
  - The dab's disc fills from the best-matching patch near the
    sampled region, blended by the same gated-coverage math as
    `clone_dab_masked` (the coverage, falloff and mask behave
    identically to every other retouch tool).
  - **Patch match v1, deterministic:** candidate offsets walk a
    Chebyshev-bounded, distance-then-fixed-angle-sorted spiral
    around the naive sample delta, capped at `candidates`. Score =
    SSD over 4 boundary pairs (a texel inside the disc edge paired
    with one just outside along the same ray — the ring where
    continuity is visible), comparing source-under-offset against
    the dst's CURRENT state (so mid-stroke dabs keep the stroke's own
    earlier fills). Ties resolve to the first candidate = the
    nearest = the naive delta → the smoothest fill when everything
    matches equally.
  - Out-of-source candidates score `None` (skipped); a zero-alpha dab
    is a no-op (the house contract); the masked texel is untouched.
- `spiral()` helper: complete over the Chebyshev square, ordered,
  deterministic — pinned by its own test (49 offsets at radius 3,
  nondecreasing distance, stable across calls).
- Exported: `InpaintParams` joins the `retouch` re-export list.

## Tests (5)

- `inpaint_fills_a_hole_from_the_sampled_region` — the classic
  healing case: solid + hole, sampled from clean solid; the hole's
  core refills exactly (ties → naive delta → the solid).
- `inpaint_prefers_the_boundary_continuity_match` — the
  distinguishing case: a two-tone layout where the NAIVE sample sits
  in the wrong-color half; the boundary ring's evidence must migrate
  the match to the surrounding color. (Hand-derived before pinning:
  the winner is delta (5,0) within the radius-10 spiral — the
  west/north/south pairs force the sample left; the fill reads gray,
  not the naive tan.)
- `inpaint_respects_the_quick_mask` — the masked texel untouched,
  neighbors filled.
- `inpaint_zero_alpha_dab_is_a_noop`.
- `the_spiral_is_deterministic_distance_ordered_and_complete`.

## Judgment calls

- **Boundary-ring score, not whole-disc SSD**: the disc's interior
  carries no continuity evidence (it is the hole being filled); the
  ring is where a match is visible. The inside term still reads the
  dst's CURRENT state so a stroke's second dab does not undo the
  first's fill.
- **Ties → nearest**: the naive delta is candidate zero; a uniform
  source ties every candidate at the same score and the naive wins —
  the "smoothest" default. The fixed-angle tie-break makes the
  search deterministic (byte-pinnable tests).
- **Chebyshev spiral + cap, no multiscale**: the ROADMAP row names
  patch-match; the coarse v1 is the bounded honest version. Multiscale
  (coarse-to-fine pyramid) is the named follow-up when the core earns
  its place in a real stroke.
- **Engine half only**: no BrushTool variant, no panel, no stroke
  sampling — those are shell surfaces behind the locks, queued with
  the other halves.

## Unverified

- Battery (build host down since ~18:25, ~10h — morning-only per
  Bessa). Static: rustfmt PASS; every fixture API read from source
  (MaskView::new's slice shape, RetouchDab/RgbaBuffer, footprint +
  gated_coverage); all three behavioral expectations hand-derived
  twice (the first derivation caught an east-pair mistake in my own
  reasoning; the corrected walk confirms delta (5,0) wins within the
  radius). Ad-hoc verified, not suite-verified.

## Deferred

- The tool wiring (BrushTool::Inpaint, the panel row, the Alt-click
  sample UX) — shell surfaces, W-GUI-3/4.
- Multiscale patch match (the follow-up named above).
- The composite-sample seam integration (the ROADMAP row's "rides
  the composite-sample seam" — the app's readback mirror already
  exists for smudge/clone and serves this unchanged).
