# The 0.1.0 Release Checklist

The tag fires on Bessa's word (the standing rule from the packaging
commit). This is the readiness state, so the word is the only
remaining input.

Updated 2026-10-15 against the C2 gap audit (`docs/loop/audit-2026-10-15-c2.md`):
gates 6-9 are NOT MET. Earlier, 2026-10-12, per the strategy review: every
gate below checked process; none checked FUNCTION. Gate 0 (the
journey test) is now the first gate — the process gates follow it.

## Gate 0 — the journey test (the FUNCTION gate, added by the review)

FBX → 4K pressure paint → see lit → undo/redo → save/reopen → ORM
export decode-verified. One continuous session exercising the six
core-loop criticals JOINED — not each in isolation. The harness:
`crates/umber-app/tests/gate0_journey.rs` (`15c072e`; the compositor
assertion is the headless 'see lit' equivalent, the shell frame named
as the shell-only remainder). [PASSED — CI run 38255484123 at `70dc67a`,
lavapipe + WARP. Caveat (C2-17): the FBX fixture carries no UV layer, so
the FBX leg proves the import, not a UV paint on it.]

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
6. **The perf baselines** — seeded at the 4K default (`fd49627`), but
   from a local Intel HD 530 over GL, not the nightly's lavapipe. NOT MET
   until: the nightly actually runs (C2-14: `runs-on: [self-hosted]` has
   no hosted fallback), the gate stops passing missing metrics (C2-15),
   and the 60 fps / 1M-tri / 4K budget is measured as specified (C2-16).
7. **SPEC ratified** (Bessa) — NOT MET: SPEC.md v1.0 is DRAFT, and the
   v1.1 amendment draft waits.
8. **One dogfood asset painted end to end** (Bessa first) — NOT MET, and
   blocked on the C2 Tier 1-2 rows: painting reaches Base Color only, and
   export writes Base Color only from paint.
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
   - **APPROVED by Bessa 2026-10-15.** It fires after W-C2g's CI is green.
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
        packages, as in the dry run.
     4. `git tag v0.1.0-alpha.1 && git push origin v0.1.0-alpha.1`, then
        watch `release.yml` + `packaging.yml` (the tag triggers both).
     5. Record the artifact sizes in the §12 table (installer budget
        < 150 MB) and download the Windows zip to check its `.sha256`.
     6. A follow-up commit returns the version to `0.1.0`, whose metainfo
        entry keeps the alpha's release line as history.

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

- PSD import (P3-designed), .usdz read beyond the first layer, the
  full-32 blend set, OCIO configs (the adoption probe's rule), the
  path tools, the post-effects stack — all tracked, none blocking
  the 0.1.0 floor. (Review-audit note: nothing P0 hides here — the
  P0 holes all live in the census's code-vs-reachable split now.)
