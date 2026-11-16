# Umber next-steps — the complete to-do list, 2026-11-16 (~06:00)

Authored at Bessa's ask ("make a complete and detailed to-do list of all
the next and remaining steps and follow it to completion according to
your suggestions"). Sources of truth read live before writing:
`docs/ROADMAP.md`, `docs/loop/waves/2026-11-09-wave-plan.md`,
`docs/loop/queue.md` (rows through 486), `docs/specs/release-checklist-0.1.0.md`,
the 2026-10-24 audit lenses, and live CI (`gh run list`).

## A. Docs truthing — executable now, docs-only (this session)

1. ROADMAP row 234 (`wave-a11y-first-pass`) still reads open; it LANDED
   2026-11-12 (queue row 378, lock `razul-a11y-first-pass`, brief
   `docs/loop/briefs/2026-11-11-a11y-first-pass.md`). Mark LANDED.
2. ROADMAP parity scoreboard is stale: "channel add/remove/rename open"
   landed (rows 370 + 374); "four post nodes open" landed (rows 375/376);
   the "ribbon GAP" is a declared NON-GOAL
   (`docs/specs/gui-audit-and-roadmap.md:265`: "The ribbon? No. Never.")
   — mark it decided, not open. Keep open: displacement preview, D2-D5.
3. Wave-plan status header: confirm it reflects rows 444+ (W4 closed,
   W5 closed except replay-exact).

## B. Ungated engineering kills — the real remaining runway (in order)

4. **D2 auto-cage** (baker power row; SPP lens `audit-2026-10-24-lens-spp.md:37`).
   Brief first, then claw dispatch, then dragon battery.
5. **D3 opacity baker.** Same loop.
6. **D4 ID sources** (multi-source ID bake). Same loop.
7. **D5 ground plane.** Same loop.
8. **Displacement preview** in the viewport (scoreboard Viewport row).
   Same loop.

Each follows the standing order of operations (wave plan §"standing
order"): scout → brief lands first in `docs/loop/briefs/` → kill-lock →
claw writes code → dragon battery (rustfmt, check-layers, scorecard,
clippy `-D warnings`, crate tests, all under `flock
/tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR`) → kill report + queue row →
`dcommit.py --path-limited` → `safe-push.py` bare → release lock with
the commit.

## C. Deferred by decision, not by neglect (post-0.1.0 / v3)

9. R-A2 v3 depth/occlusion culling — projection fill is "CLOSED to v3"
   (ROADMAP row 238); design briefs exist
   (`2026-11-09-projection-occlusion-{design,kill-brief}.md`).
10. R-A2 §2.2 product questions (seam/symmetry expansion of projections,
    a decal channel, the bake family) — queued as product calls.

## D. Gated — no derivation until the gate opens

11. L4 winink live hook / wave-stylus-win32 — needs a real Windows
    session (partner-gated; decode already in tree).
12. L7 color_button delegate conversion — G1-gated.
13. wave-wasm-shaders / tessellation / hdr / ray-query — eframe/wgpu
    upgrade postures.
14. .spp staged import R1-R4 — fidelity report's staged lanes.
15. **Never (hard walls):** .spp WRITING; .sbsar execution; fake
    interchange (declared custom nodedefs).

## E. Partner-gated — Bessa's calls, queued and named

16. Release decisions C2-16, 22, 23-25, 30.
17. SPEC v1.1 ratification packet (`docs/specs/spec-v1.1-amendment-draft.md`
    + `spec-ratification-brief.md` ready).
18. Release checklist walkthrough (gates 7-9: SPEC ratification,
    dogfood pass C2-24, `v0.1.0-alpha.1` dry-run tag C2-25 — approved
    2026-10-15, awaiting his tag push).
19. Upstream bug drafts (`docs/upstream/`) — PR gate applies.
20. i18n native-speaker pass (pt-BR).
21. wave-replay-exact default decision — prep brief ready
    (`docs/loop/briefs/2026-11-14-replay-exact-decision-prep.md`).

## Execution record

- 2026-11-16 ~06:0x: items 1-3 landed (ROADMAP row 234 + scoreboard
  truthing) in the same commit as this list; pushed via dcommit +
  safe-push.
- Next: item 4 (D2 auto-cage) brief dispatched.
