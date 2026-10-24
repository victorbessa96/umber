# Umber full-project audit (2026-10-24): what works, what's missing, what to improve

Date: 2026-10-24 (10:0x–). Author: dragon, solo rows verified live +
two read-only lens claws (tools, UI/UX — their reports beside this
file). Every number re-derived from the tree at the session's head;
the perf claims cite the live run.

## The headline

**186,170 LOC across 16 crates, 2,251 test functions** (up from
166.5k/2,061 at the 2026-10-17 audit: +19.6k LOC and +190 tests in
seven days — a depth week, not a breadth week). **112 commits since
the last audit.** The perf nightly is **GREEN on the pinned 24.04
image** (run 40075780768, 2026-10-24 08:51Z, 19m20s — the first
green after the 10-23 red; the stale-baseline mismatch class the
seed's own note prescribed reseeding away is DEAD, killed by
`ec7ccb8`'s image pin). The scorecard generator's `--check` passes
(README current). The requirements CONTRACT is stable: 90 of 100
rows ticked — UNCHANGED since 10-17 despite the 112 commits, because
the week's kills were quality depth on already-ticked rows (the
C2-9 follow-ups, the perf pin, the flatpak chain, the battery
hardening, the W-C4 runway), not new requirement surface. That is
the audit's first structural finding: **the project has crossed
from feature-march to consolidation** — the open risk is no
longer "can a painter finish an asset" (closed 10-15) but "what
does the release gate need and what stays gated on the partner."

## The week since the last audit (the 112-commit ledger, grouped)

- **The stencil lane's full arc** (8 stencil-tagged commits +
  the W-C3a/C3b closes + FREEZE-4): the paint-through screen mask
  end to end — engine, panel wiring, viewport shell, the set-switch
  re-stage, the cross-tile observer, the two-kills-closed W-C3a
  recovery, the tangent-wiring third site.
- **The W-C4 runway** (the night of 10-23/24, four kills, all
  battery-green + CI-green): L1 the set-switch glue (C2-9's
  headless half — `umber_app::set_switch`, the stencil module
  bin→lib, the five-pin headless test); L2 the per-channel dab
  density (both arms — the dab conversion + the brush ring — on one
  channel-aware fn; the write-only `texels_per_uv` field found dead
  and deleted; the four same-class arms mapped as L2b); L3 the
  ID-mask pre-warm (the whole-mesh bake at tool entry, the first
  click hits); L5c the auto-assign (paint tiles map to their
  materials' sets at mesh load — majority rule, the spec's §4/§5
  slice closed, the census correction: the ray half had already
  landed as `0955ac8`).
- **The packaging chain** (7 packaging commits + the flatpak trio):
  the flatpak job's wall-4 arc — the 26.08 platform/SDK install,
  `--disable-rofiles-fuse`, `librsvg2-common`, and the honest
  revert of `appstream-compose` (the cleanup-phase compose dies;
  disabled, not papered).
- **The battery + machinery hardening**: the machine-fit defaults
  (nproc×3, the striped debug), the feature-on leg extended from
  compile to RUN, the perf nightly's image pin, the loop restart
  into all of it.
- **The docs' honesty**: the 2026-10-12 heightnormal amendment
  (the pre-research judgment restated post-research), the
  per-set-masked-bakes census correction, the loop charter's
  stand-down branches exercised (the Deck's five dark cycles).

## The standing debts (the roadmap's six, live-verified)

| Debt | State |
|---|---|
| Split-redo under merged undo entries | OPEN (retouch.rs:21's named follow-up — no split_redo symbol) |
| Merge below the top undo entry | OPEN (the same doc's deferred arm) |
| Cached ID-mask bake | **LANDED SILENTLY** — the P1 tile cache (single-entry, per-tile bakes + the hit/replace-after-success semantics) + the night's L3 pre-warm built on it; the roadmap's row is STALE |
| The winink live-HWND question | OPEN — and now DEEPER-GATED: winink.rs is NOT in the tree (S1's parked bytes never landed); the W-C4 L4 lane waits on it |
| DPI re-raster at runtime | OPEN — the manual ui_scale field exists (main.rs:1991-1992); the runtime-change re-raster stays the named follow-up |
| AssetStore GC caller in the shell | OPEN — `collect_garbage` has NO caller in main.rs (the engine landed 10-16; the shell wiring still queued on the main.rs lock) |

## The gated lanes (the honest walls)

- **L4 (winink)**: GATED — the parked bytes are absent from the
  tree; the lane cannot even derive until S1's row 4 lands them.
- **L5(b) — the OccluderScope panel row**: the enum landed with the
  ray half (`0955ac8`); the PANEL row + its i18n keys remain,
  double-gated on the .ftl catalog lock state (the UI lens
  reports it below).
- **L7 (the color_button delegate conversion)**: G1-gated.
- **The release track (0.1.0)**: perf re-baseline ✓ (the nightly
  green on the pinned image), the SPEC v1.1 ratification packet +
  the checklist walkthrough + the release decisions (C2-16, 22,
  23-25, 30) = partner decisions, queued on him.

## The perf verdict (the morning's gate)

The pinned-image nightly: **GREEN** (run 40075780768, 08:51Z,
19m20s, first green after the 10-23 red on the rotating image). The
pin (`ec7ccb8`) did exactly what its kill claimed: the
apples-to-apples compare. `docs/perf/protocol.md` carries the
triage record.

## The lenses' reports

All six banked beside this file:

- `audit-2026-10-24-lens-tools.md` — claw-run (opencode/Muse
  Spark, the one lens that finished before the free tier's wall):
  the complete paint-tool inventory, 10/10 surfaces verified, the
  two persistence gaps + the retouch-undo seam named.
- `audit-2026-10-24-lens-uiux.md` — dragon: the G0-G7 scorecard
  (36/43 landed), the panels, the i18n verdict (the lock LIFTED),
  the shortcuts inventory, the desktop smokes unfired.
- `audit-2026-10-24-lens-spp.md` — dragon: the SPP parity
  comparison, the census-delta (2 of Tier-1 closed this week),
  the family verdicts, the pain-ordered gaps.
- `audit-2026-10-24-lens-formats.md` — dragon: the format
  matrix, the .spp staged R1-R4 state, the agent protocol's step
  inventory + the GUI-only remainder.
- `audit-2026-10-24-lens-engine.md` — dragon (the scripted
  census): the crate health table (no crate below 7/kLOC), the
  architecture debts, the perf state, the robustness posture.
- `audit-2026-10-24-lens-platform.md` — dragon: the workflow
  matrix, the packaging state, the release critical path (no tag
  ever cut), the security verdict, the .ftl lock LIFTED.

**The claw dispatch's own audit** (the machinery lesson, recorded):
opencode's free tier died mid-fleet ("Free usage limit reached"
on lens B); claude's headless auth is expired ("OAuth session
expired" — Bessa's interactive re-auth is the only fix, his
call); pi answers short prompts but wedged on all three
lens-sized briefs (the mini-probe works, the real briefs die
after the warning banner). The audit completed one-claw +
dragon-solo. The re-auth + a Go subscription are partner calls.

## The improvement queue (this audit's own prescriptions)

1. **The ROADMAP refresh** (this audit's companion commit): the
   new 2026-10-24 edition — every closed row moved to the
   dedicated LANDED archive, the open waves ungated-first, the
   parity scoreboard live-verified.
2. **The next ungated kills** (the runway, pain-ordered): the
   Assets panel (R-C1, P0) and the layer-stack flatten (R-B1,
   P0) lead; then the color picker (g4), the quick-mask +
   symmetry persistence pair, the material picker (R-A10), the
   OccluderScope panel row (now ungated), L6 the unified
   timeline, the GC caller, L2b/L3b.
3. **The partner queue** (named, not acted): the release
   decisions C2-16/22/23-25/30, the SPEC v1.1 packet, the
   claude re-auth (his interactive step), the upstream bug
   drafts, the i18n native-speaker pass, the desktop smokes
   session (his hands on the desktop).
