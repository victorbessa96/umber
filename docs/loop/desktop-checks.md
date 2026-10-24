# Desktop check pack — the owed runtime smokes

Landed 2026-10-20 by the M5 queue-row-runbooks lane, from the W-C3a
plan's D1 `desktop-check-pack` contract. The queue rows (C2-1, C2-3,
C2-9) point here; these checks are the instrument a desktop session
runs — they close nothing by themselves, and the rows stay open until
a Result line below records the observation. kittest coverage is not a
desktop smoke; the two are never conflated.

Every quoted UI label is grep-verified against
`assets/i18n/en-US.ftl` at the cited line. The fixture exists at
`crates/umber-mesh/tests/assets/cube_2mat_ascii7400.fbx`.

## How to run

Launch umber from a terminal on the desktop build so the PID is known
(check 2 needs it). One session can run all checks in order. When
every Result line is filled, tick the queue rows in the same docs pass
that records the observations.

## Check 1 — quit guard, dirty close (closes C2-1's smoke)

1. Launch umber; open or create any project; paint one dab (the
   project is now dirty).
2. Close the window via the OS close button.
   - Expected: the close is intercepted; a centered confirm window
     titled "Unsaved changes" (`en-US.ftl:116`, shell.new-confirm-title)
     offers "Save…" (:118), "Discard" (:119), "Cancel" (:120). The
     window's identity in code: id "quit-guard" on the CancelClose arm.
3. Click Cancel — the app stays open, the project unchanged.
4. Close again; click Discard — the app exits; the file on disk keeps
   its last-saved bytes.
5. Repeat dirty; close; click Save… — the save path runs, then the
   app exits.

Result: ____________________

## Check 2 — kill -9 autosave recovery (closes C2-3's smoke)

1. Launch umber from a terminal; note the PID; create a new project;
   paint a distinctive dab (a known color in a known corner).
2. Wait out the autosave interval (120 s full snapshots to the state
   dir) — about 2.5 minutes with the app focused.
3. `kill -9` the PID from the terminal.
4. Relaunch umber in the same user session.
   - Expected: a prompt titled "Recover unsaved work?"
     (`en-US.ftl:811`, recovery.title) with its body (:812) and a
     "Recover" button (:813).
5. Click Recover — the autosaved copy opens with step 1's dab present.

Result: ____________________

## Check 3 — per-set stacks, the set-switch glue (closes C2-9's check)

1. Launch umber; open the fixture
   `crates/umber-mesh/tests/assets/cube_2mat_ascii7400.fbx` — a
   two-material FBX; the Texture Sets panel (`en-US.ftl:55`,
   panels.texture-sets) lists both sets.
2. Paint a dab on set A; switch to set B; paint a different-color dab.
3. Switch back to set A.
   - Expected: the Layers panel (`en-US.ftl:51`, panels.layers)
     follows the active set; set A shows A's dab and NOT B's; set B
     shows B's dab only; neither set's pixels bleed across.
4. Save (Ctrl+S); close; reopen the saved file.
   - Expected: both sets' dabs intact after the round-trip.

Result: ____________________

A headless variant of this check needs the set-switch glue extracted
from `main.rs` into a lib fn (the W-C3a plan's §5 tail item 4); until
then this page is the instrument.

**2026-10-24: the headless half LANDED (the W-C4 L1 kill) —
`umber_app::set_switch` with the five-contract test
(`crates/umber-app/tests/set_switch.rs`, green through the real
adapter). This page KEEPS its Result line: the desktop smoke stays the
end-to-end instrument; the headless test is the other half.**

## Check 4 — Windows real-pen flags (S1's leg; runs only AFTER the parked fleet lands)

Requires the `winink-pen-flags` kill committed (parked, HOST_BACK-gated)
and a real Windows pen on the Windows build.

1. Draw with the pen tip — a normal paint stroke.
2. Invert the pen (eraser end down) — the stroke erases (an Eraser
   contact, not a tip stroke).
3. Hold the barrel button — one press event; release — one release
   event; no repeat while held steady.

Result: ____________________

## Verification of this document

- Labels grep-verified against `assets/i18n/en-US.ftl` on 2026-10-20:
  :51 (panels.layers = Layers), :55 (panels.texture-sets = Texture
  Sets), :116-120 (shell.new-confirm-title/-body/-save/-discard/-cancel),
  :811-813 (recovery.title/.body/.recover).
- The quit-guard window verified in code: the CancelClose arm
  (`main.rs`, viewport close handling) reuses the shell.new-confirm-*
  keys under window id "quit-guard".
- The fixture exists: `crates/umber-mesh/tests/assets/cube_2mat_ascii7400.fbx`.
- Doc-only, battery-exempt; no code touched.
