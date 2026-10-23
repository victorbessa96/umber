# Kill report — blend-replace-opacity (W-C3b kill 2)

Date: 2026-10-20. Session: local-4d2e8a71 (dragon). Host-free under
the 2026-10-20 code steer (FREEZE-4, centralhost dark); battery at
HOST_BACK via scripts/w-c3b-close.sh, which runs AFTER w-c3a-close.sh.

## Claim

Lock `blend-replace-opacity` (4 paths: umber-core/layers.rs,
umber-gpu/compositor.rs, umber-app/layers_panel.rs, this report). All
three code files verified OUTSIDE every parked manifest before the
first edit (paint_state.rs forms the shell's (blend, opacity) pairs —
parked P1 bytes — but never applies coverage math; every apply path
routes through the two primitives this kill changes, so the parked
file is routed around by construction).

## What this closes

- Requirements row 39's first named v1 boundary: "Replace's
  opacity-bypass (composites as Normal today)".
- The `BlendMode::Replace` variant doc's "an opacity-bypass the
  compositor does NOT implement yet … a named follow-up" clause.

## Built

- **umber-core/layers.rs** — `composite_channel` gains the mode-level
  bypass: Replace returns the blend result unmixed, ignoring the
  opacity argument — even at opacity 0 (the "no effect at o=0" rule
  belongs to opacity, and Replace bypasses opacity). The variant doc
  is rewritten to state the implemented contract. Two new pins
  (o=0.5, o=0) beside the existing full-opacity pin.
- **umber-gpu/compositor.rs** — the CPU oracle's coverage gate:
  `cov = sa_gate × o_gate × mask`, where Replace maps `o_gate = 1.0`
  and `sa_gate = presence` (any alpha > 0 → full; alpha == 0 → no-op).
  The WGSL stack compositor mirrors the same gate
  (`params.mode == BLEND_REPLACE` → `o = 1.0`, `sa` presence-gated)
  AFTER the premultiplied un-premultiply arm, so the un-premultiply
  still divides by the true source alpha. `sa` promoted `let` → `var`.
  Module-doc judgment 2 gains the exception sentence. New 4-pin test
  `replace_bypasses_opacity_and_alpha_grading`: opacity 0 must not
  mute; alpha 0.25 overwrites wholly; alpha 0 never overwrites; the
  mask still clips.
- **umber-app/layers_panel.rs** — the material channel sub-row's
  opacity field greys out (`ui.add_enabled(next_blend != Replace)`)
  when the row's blend is Replace: an enabled editor for a bypassed
  value would be a lie. No new i18n string (both catalogs sit in the
  parked A3 lock); the greying is the signal, and the stored value
  rides along untouched for when the mode changes back.

## Judgment calls

1. **The variant doc's FULL contract, not just the row's name.** Row
   39 says "opacity-bypass"; the variant doc says "ignoring the
   layer's opacity AND the source alpha". The doc is the spec — the
   bypass covers both, implemented as a presence gate: any alpha > 0
   overwrites in full, alpha == 0 is a no-op. Alternative rejected:
   opacity-only bypass. That would leave Replace ≡ Normal for every
   soft-edged source (the exact defect the boundary names) and
   contradict the written contract.
2. **Presence gate, not full alpha-drop.** A wholesale `cov = mask`
   would let a fully-transparent texel overwrite (the premultiplied
   arm zeroes its channels and its own comment relies on `cov == 0`
   — a soft brush's fringe would paint black). Presence preserves
   that load-bearing invariant while honouring "ignoring the source
   alpha" for every texel that exists.
3. **The mask still clips.** Masks are coverage, not opacity; the
   bypass is opacity + alpha grading only. Pinned by test.
4. **Mode-level semantics propagate to the channel rows.** A material
   channel set to Replace now ignores its channel opacity — derived,
   consistent (the mode's contract holds wherever the mode applies).
   The shell's pair formation (parked paint_state.rs:3648-3653) is
   untouched; the apply gates at composite time.
5. **The layer-level opacity slider is NOT greyed.** If that slider
   lives in a parked file (main.rs hosts the shell), it is routed
   around; named as a post-recovery UX tick. The panel file's own
   channel row (free) IS greyed.

## Unverified (until the battery at HOST_BACK)

- The compile + the new tests on all three crates (rustfmt clean
  parse ×3; the compositor canonicalized by the local rustfmt sweep —
  reflow confined to this kill's bytes).
- The WGSL arm is statically mirrored (identical gate shape, same
  placement invariant); no existing GPU golden pins Replace at
  partial opacity, so the WGSL half rides the battery's existing
  golden coverage plus the mirror discipline. A dedicated GPU golden
  for the bypass is named as a follow-up.
- Real-Painter parity (does Painter's Replace really ignore source
  alpha?) is a corpus question; the variant doc ruled the contract,
  and this kill implements the doc.

## Deferred

- PerChannel routing for the non-separable families — row 39's
  SECOND boundary; design-gated (what a triplet mode means on one
  channel is a semantics decision), as claimed in the note.
- The layer-level opacity slider greying (parked shell territory).
- A Replace-at-partial-opacity GPU golden (next wave's golden pass).

## Process note

The first compositor edit pass aborted on anchor count=2 (the
`two_layer` for-loop tail appears in two tests); the per-file atomic
write left the file untouched and the re-run anchored on the
preceding unique case row — the same lesson class as the skill's
"pin anchors from post-format bytes". The python port re-derived the
4 oracle pins from the new gate formula (4/4 agree). The equivalence
invariant (composite_pixel ⟷ composite_channel fold, the cross-
product test at compositor.rs:2042) holds by construction: both
sides bypass identically for Replace.

## Addendum 2026-10-23 (local-7de7d3ed) — the missed blanket-assert fix-forward

The W-C3b close's first battery (14:44) caught what the kill session
missed: the OLD fold test
`opaque_pixels_match_the_naive_reference_fold` pins the layer alpha
"da + (1-da)*o, independent of mode" — a blanket that predates this
kill and was never carved for Replace's exception. With this kill's
bypass live (o_gate = 1.0), Replace at o=0 folds alpha to sa (1.0),
the blanket expected 0.5, RED. The kill's design + its own 4-pin test
are the contract (the pixel is replaced wholesale); the stale blanket
was the wrong side. Fixed: the fold test's alpha assert now carries
the named Replace exception (expected = src[3] for Replace, the
formula for every other mode). Verified: the umber-gpu lib suite 151/151
green (the fold test + replace_bypasses_opacity_and_alpha_grading
together). The triplet twin's identical-looking assert needed NO
change — its mode list never included Replace.
