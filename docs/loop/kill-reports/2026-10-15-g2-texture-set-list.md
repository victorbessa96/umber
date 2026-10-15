# Kill report — g2-texture-set-list (resize confirm)

Date: 2026-10-15. Author: dragon. Lock: `g2-texture-set-list` over
`crates/umber-app/src/main.rs` (the panel lives there),
`assets/i18n/*.ftl` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g2-texture-set-list`, the
"per-set resolution with the lock pair and the slow-recompute warning"
item. Sources: Painter B4 ("The UI warns that a change can trigger long
recomputation"), plan rule 8 ("Destructive = confirm").

## Found first

The Texture Sets panel resized a set THE MOMENT the size combo changed.
A resize clears that set's whole canvas and the stroke undo history. One
stray click on the combo wiped a painted set with no way back: the undo
history was part of what it cleared. The warning text existed, but as a
static line under the combo, read after the damage.

## Built

- The size combo now ARMS a `PendingResize { set, from, to }`. A warning
  group names the set and both sizes and says what it clears, with
  Resize / Cancel buttons. `resize_set` runs only on Resize.
- An armed resize is dropped when the selected set changes, and on
  File > New (`reset_for_new`; the existing boot-fresh test pins it).
- i18n: `resize-confirm` (a Fluent placeable message), `resize-apply`,
  `resize-cancel`, en-US and pt-BR.

## Judgment calls

1. **Confirm even for an empty set.** A rule that depends on "is there
   paint here?" is a rule users cannot predict. The confirm is one click.
2. **The other plan items in this row are deferred:**
   - per-set eye / Show all / Hide all / Invert / Focus Mode;
   - double-click rename;
   - the non-square lock pair.
   The first two need per-set visibility in the GPU mesh pass: today it
   renders the mesh as ONE draw, with no per-material sub-draws (verified:
   `material_names` reaches umber-gpu only as test fixtures). That is a
   renderer kill, `g2-texture-set-visibility`. Non-square sizes need
   `resize_set` call sites and the save format to carry (w, h)
   separately; the API takes both already.

## Unverified

- No snapshot of the panel (bin-only module; the G0 kittest-harness
  kill's blocker).
