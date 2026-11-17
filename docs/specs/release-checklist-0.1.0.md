# The 0.1.0 Release Checklist

The final `v0.1.0` tag fires on Bessa's release decision. The alpha tag already
fired and is evidence for packaging, not a substitute for the remaining human
and release-quality gates.

Re-verified against live Git and the GitHub release on 2026-11-17: Gate 6 is
MET; Gate 7 is MET (SPEC v1.1 ratified by Bessa on 2026-11-16); Gate 9 is MET
(`v0.1.0-alpha.1` published as an unsigned pre-release at `bc4d642f`, with
checksums and distribution artifacts). Gate 8, the real dogfood pass, remains
NOT MET. Earlier, the strategy review found that process gates alone were not
enough; Gate 0 remains the first function gate.

## Gate 0 — the journey test (the FUNCTION gate, added by the review)

FBX → 4K pressure paint → see lit → undo/redo → save/reopen → ORM
export decode-verified. One continuous session exercising the six
core-loop criticals JOINED — not each in isolation. The harness:
`crates/umber-app/tests/gate0_journey.rs` (`15c072e`; the compositor
assertion is the headless 'see lit' equivalent, the shell frame named
as the shell-only remainder). [PASSED — CI run 38255484123 at `70dc67a`,
lavapipe + WARP. The old C2-17 caveat (the FBX fixture carried no UV
layer, so the FBX leg proved only the import) is CLOSED: the generated
`cube_uv_ascii7400.fbx` carries UVs and Gate 0 asserts the unwrap —
queue row C2-13..19, closed 2026-10-15.]

## The gate (what must be true when the tag fires)

1. **Gate 0 green** — the journey test passes on CI (lavapipe + WARP),
   the core-loop phase's exit gate.
2. **The P0 floor assembled** — the three hunting rows land + green:
   the fill layer + projections, the view modes, the eraser (the
   chain, 22:47). The blend census verifies the math. [LANDED — the
   fill layer, view modes, and eraser kills all landed and CI-green
   in the 2026-10-11/12 era; the blend census carried the math]
3. **The compile debt cleared** — every code-only commit gates on CI
   per WAVE (Bessa 2026-10-15: one push per large wave): the full
   workspace suite green on both platforms. [Green through the W-G5 wave
   on Linux; the latest per-wave verdicts live in `docs/loop/queue.md`'s
   push log]
4. **The changelog current** — ✓ done (`4b234a1`).
5. **The requirements marked** — ✓ the census current through the
   core-loop phase: all six criticals' rows marked ✓ code with the
   code-vs-reachable split (the last marks: `472b774`).
6. **The perf baselines** — MET. Every sub-item the 10-15 audit named is
   closed (queue row C2-13..19, ALL SEVEN CLOSED): the nightly runs
   (C2-14), the gate is leak-proof (C2-15), and the 60 fps / 1M-tri /
   4K budget is measured on real hardware (C2-16 — the RTX 3070
   record: viewport p95 0.2034 ms vs the 16.7 ms budget, kill report
   `2026-10-19-real-gpu-budget`). The baseline was reseeded from
   authentic runs after the runner-fleet drift conviction
   (`c53a08d`/`b1d3660`, triage in `docs/perf/protocol.md`) and the
   nightly is drift-hardened — median-of-3 plus a same-tip canary leg
   (`dc4b355`). Hardened schedule runs subsequently passed on pushed tips.
7. **SPEC ratified** (Bessa) — MET 2026-11-16: Bessa answered the
   alignment form "ratify v1.1 now" (draft `spec-v1.1-amendment-draft.md`
   + `spec-ratification-brief.md` were ready).
8. **One dogfood asset painted end to end** (Bessa first) — NOT MET: the
   dogfood pass has not happened. The 10-15 blocker is GONE — painting
   reaches every channel and export writes every painted channel
   (C2-5 + C2-6, queue rows closed 2026-10-15); what remains is the
   pass itself (Bessa's two-material prop, C2-24).
9. **A pre-release dry-run tag** (`v0.1.0-alpha.1`) — **MET 2026-11-16.**
   The version-bump and packaging commits produced the tag at `bc4d642f` and
   a GitHub pre-release at 17:34Z. Its Linux/Windows app and CLI archives,
   shell/PowerShell installers, source archive, `sha256.sum`, and
   `dist-manifest.json` were published; the largest archive was 18.6 MB,
   below the 150 MB budget. The post-tag commit `d49d0acc` restored the
   development workspace version to `0.1.0` and pinned Flatpak to the tag.
   The alpha is intentionally unsigned under C2-22; checksums were verified.

## Alpha tag shape (published)

- **Tag:** `v0.1.0-alpha.1` at `bc4d642f0696233ac5293683c88b2e860e52b3b2`.
- **Release object:** GitHub pre-release, published 2026-11-16 17:34Z, not a
  draft and not the final `v0.1.0` release.
- **Artifacts:** umber-app Linux `.tar.xz` 14.0 MB; umber-app Windows `.zip`
  18.6 MB; umber-cli Linux `.tar.xz` 5.5 MB; umber-cli Windows `.zip` 8.4 MB;
  source archive 9.2 MB; shell/PowerShell installers; checksums; distribution
  manifest. Every archive is under the 150 MB budget.
- **Integrity and signing:** SHA-256 verification passed; signing is
  deliberately deferred under C2-22.
- SPEC ratification + a dogfood cohort are gates 7-8 above (the
  review's IMPROVEMENTS §4.4).

## Release phases (added 2026-10-14, absorbed from Blender's release cycle)

Blender cuts each release through Alpha → Beta → RC, and freezes its
Python API at Beta so add-on authors get a stable target before the
release (developer.blender.org/docs/handbook/release_process/release_cycle).
Umber has two surfaces outside users build against — the JSONL agent
protocol and the `.umber` file format — so the same freeze applies.

| Phase | Entry | What may land | Exit |
|---|---|---|---|
| **Alpha** (now) | — | anything; risky changes go here, early | the gate list above all green except Gate 0's shell-frame remainder |
| **Beta** | Bessa calls it | bug fixes, docs, perf; **no** agent-protocol step changes, **no** `.umber` / paint-sidecar schema changes, **no** new user-visible strings (the pt-BR pass needs a frozen catalog) | one dogfood asset painted end to end (gate 8) with no open P0 |
| **RC** | Beta exit | critical fixes only, each reviewed | 1 week with no new critical, then the tag |

**What the freeze points at.** `.umber` carries `CURRENT_PROJECT_VERSION`
/ `LAYER_PLANES_PROJECT_VERSION`, the paint sidecar `paint_save_version`,
and the agent protocol `PROTOCOL_VERSION` (umber-cli `agent.rs`, answered
by the `version` step, pinned by a test; added 2026-10-14 because the
protocol had no number to freeze). After 0.1.0, any step change bumps it.

## The known-deferred (honest, in the tag's companion notes not the message)

- **Windows code signing (C2-22, DECIDED 2026-11-16 by Bessa):** the
  alpha ships unsigned with the SHA-256 checksums and the SmartScreen
  "More info → Run anyway" note (`docs/CODE_SIGNING.md`); the signing
  route (SignPath Foundation free OSS vs OV cert $150–300/yr) is decided
  AFTER the alpha artifact exists. Azure Artifact Signing is out
  (individuals: US/Canada only). EV no longer bypasses SmartScreen since 2024.

(Sweep history, preserved as the list shrinks — the known-deferred is
down to Windows signing alone. The 2026-10-19 pass, kill
`release-checklist-truth`, removed five: PSD import LANDED `a4bf81a`
(`mod psd` in umber-app + umber-export, queue row 18); the full-32
blend set LANDED `d861482` (all 32 variants in `layers.rs`'s
`BlendMode`, counted); the OCIO config panel LANDED `9a1a808` (wired
in `display_panel.rs`); the path tools LANDED (`path_tool.rs` + the
parity row's smudge/clone/symmetry/path/polygon ✓); the post-effects
stack LANDED (requirements row 84: built `86e77ff`+`a523647`+`112b378`,
persisted `f35cf48`). The 2026-11-16 docs-only truth refresh (no findings
removed; the known-deferred stays Windows signing alone; gate 6 moved
to MET on the hardened nightly's repeated green schedule runs). The
2026-10-24 pass, M1 pass 4, removed the
last: .usdz read beyond the first layer LANDED `d36cfb1` — load_usdz
walks the sublayer stack (root precedence, authored order,
depth-first, resolved inside the package; cycle-guarded + depth-bounded;
escaping paths rejected; found-but-malformed Mesh = hard error; 8
pinned tests + the python port 12/12) — the checklist's own named
deferral, closed by the W-C3b sweep. Review-audit note: nothing P0
hides here — the P0 holes all live in the census's code-vs-reachable
split now.)
