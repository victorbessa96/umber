# The 0.1.0 Release Checklist

The tag fires on Bessa's word (the standing rule from the packaging
commit). This is the readiness state, so the word is the only
remaining input.

Re-verified 2026-10-19 against the live queue rows + kill reports (the
stale-claim sweep, kill `release-checklist-truth`): gate 6 is MET
PENDING the first drift-hardened nightly green on a pushed tip; gates
7-9 are NOT MET (Bessa's). Earlier, 2026-10-12, per the strategy review: every
gate below checked process; none checked FUNCTION. Gate 0 (the
journey test) is now the first gate — the process gates follow it.
The 2026-10-15 C2-audit update's claims that landed since are
corrected in place below.

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
6. **The perf baselines** — MET PENDING the first drift-hardened nightly
   green on a pushed tip. Every sub-item the 10-15 audit named is
   closed (queue row C2-13..19, ALL SEVEN CLOSED): the nightly runs
   (C2-14), the gate is leak-proof (C2-15), and the 60 fps / 1M-tri /
   4K budget is measured on real hardware (C2-16 — the RTX 3070
   record: viewport p95 0.2034 ms vs the 16.7 ms budget, kill report
   `2026-10-19-real-gpu-budget`). The baseline was reseeded from
   authentic runs after the runner-fleet drift conviction
   (`c53a08d`/`b1d3660`, triage in `docs/perf/protocol.md`) and the
   nightly is drift-hardened — median-of-3 plus a same-tip canary leg
   (`dc4b355`). The gate reads MET when the first hardened nightly
   runs green on a pushed tip (the pending W-C3a wave push carries
   it).
7. **SPEC ratified** (Bessa) — NOT MET: SPEC.md v1.0 is DRAFT, and the
   v1.1 amendment draft waits.
8. **One dogfood asset painted end to end** (Bessa first) — NOT MET: the
   dogfood pass has not happened. The 10-15 blocker is GONE — painting
   reaches every channel and export writes every painted channel
   (C2-5 + C2-6, queue rows closed 2026-10-15); what remains is the
   pass itself (Bessa's two-material prop, C2-24).
9. **A pre-release dry-run tag** (`v0.1.0-alpha.1`) — NOT MET: no tag has
   ever run `release.yml` (C2-25).
   - The local half ran 2026-10-15. `dist plan` (cargo-dist 0.32.0) shows
     that a `v0.1.0-alpha.1` tag on today's tree releases NOTHING ("doesn't
     have anything for dist to Release"): dist matches the tag against the
     workspace `version = "0.1.0"`.
   - The dry run needs a version-bump commit first:
     `version = "0.1.0-alpha.1"` in `[workspace.package]`, then the tag.
     With the bump, `dist plan` announces both packages (`umber-app`,
     `umber-cli`), each with the Linux `.tar.xz`, the Windows `.zip`, the
     shell + PowerShell installers and the sha256 sums.
   - Afterwards, set the version back to `0.1.0` for the real tag.
   - Pushing either tag is Bessa's call. It publishes a GitHub pre-release.
   - **APPROVED by Bessa 2026-10-15.** The W-C2g CI-green precondition
     is long met (runs 38872572891 / 38885801883, both platforms);
     what remains is the version-bump commit and the tag push, which
     is Bessa's call (above).
     The exact steps:
     0. Land kill `win-version-resource` first (Windows ProductName/Version
        metadata, which SignPath requires; `2026-10-15-c2-22-signing.md`).
     1. One commit: `version = "0.1.0-alpha.1"` in the root
        `[workspace.package]` (all 17 crates inherit it), plus a
        `<release version="0.1.0-alpha.1" type="development"
        date="<day>">` entry in BOTH metainfo files
        (`flatpak/` and `appimage/`: `packaging.yml` diffs them, so they
        must stay identical). Regenerate `Cargo.lock` on the build host
        (17 `version = ` lines change). The `examples/` workspace is
        separate and unpublished, so it stays 0.1.0.
     2. The wave gate on a clean clone, then push. CI green.
     3. `dist plan --tag v0.1.0-alpha.1` on the host must announce both
        packages, as in the dry run, each archive's `[misc]` now listing
        `THIRD_PARTY_FONTS.txt` + `THIRD_PARTY.md` (the embedded fonts'
        licences require it: `2026-10-15-release-font-notices.md`).
     4. `git tag v0.1.0-alpha.1 && git push origin v0.1.0-alpha.1`, then
        watch `release.yml` + `packaging.yml` (the tag triggers both).
     5. Record the artifact sizes in the §12 table (installer budget
        < 150 MB) and download the Windows zip to check its `.sha256`.
     6. A follow-up commit returns the version to `0.1.0`, whose metainfo
        entry keeps the alpha's release line as history. The same commit
        pins the Flatpak manifest's git source to the tag: replace
        `"branch": "master"` with `"tag": "v0.1.0-alpha.1"` +
        `"commit": "<the tagged sha>"` (flatpak/README.md: "at ship time
        this pins to the release tag"). The sha only exists after step 4.

## The tag shape (drafted, not fired)

- `v0.1.0` on master, the annotated tag.
- The message: the one-paragraph 0.1.0 definition — "the professional
  floor: the brush engine (seam-aware, presets, symmetry, smudge/
  clone, the quick mask), six bakers, the export pipeline (glTF/
  Unreal/Unity/Blender presets + PSD/EXR/MTLX), UDIM, the node graph,
  WASM plugins, headless CLI + the JSONL agent surface, i18n (en/pt-BR),
  installers for Linux + Windows."
- The dist workflow (release.yml) builds on the tag push: the two
  targets, shell + PowerShell installers, checksums — measured at
  11 MB against the 150 MB budget.
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

- **Windows code signing (C2-22, decided 2026-10-15 by Bessa):** the alpha
  ships unsigned with the SHA-256 checksums and the SmartScreen
  "More info → Run anyway" note (`docs/CODE_SIGNING.md`). Apply to
  SignPath Foundation's free OSS signing now; it requires the code signing
  policy page (written) and an already-released artifact, which is what the
  alpha provides. Azure Artifact Signing is out (individuals: US/Canada
  only). An OV certificate ($150–300/yr) is the fallback; EV no longer
  bypasses SmartScreen since 2024.

(Sweep history, preserved as the list shrinks — the known-deferred is
down to Windows signing alone. The 2026-10-19 pass, kill
`release-checklist-truth`, removed five: PSD import LANDED `a4bf81a`
(`mod psd` in umber-app + umber-export, queue row 18); the full-32
blend set LANDED `d861482` (all 32 variants in `layers.rs`'s
`BlendMode`, counted); the OCIO config panel LANDED `9a1a808` (wired
in `display_panel.rs`); the path tools LANDED (`path_tool.rs` + the
parity row's smudge/clone/symmetry/path/polygon ✓); the post-effects
stack LANDED (requirements row 84: built `86e77ff`+`a523647`+`112b378`,
persisted `f35cf48`). The 2026-10-24 pass, M1 pass 4, removed the
last: .usdz read beyond the first layer LANDED `d36cfb1` — load_usdz
walks the sublayer stack (root precedence, authored order,
depth-first, resolved inside the package; cycle-guarded + depth-bounded;
escaping paths rejected; found-but-malformed Mesh = hard error; 8
pinned tests + the python port 12/12) — the checklist's own named
deferral, closed by the W-C3b sweep. Review-audit note: nothing P0
hides here — the P0 holes all live in the census's code-vs-reachable
split now.)
