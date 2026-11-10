# Umber — documentation / workflows / plan / flows pass

Date: 2026-11-10 · Author: opencode research claw (docs lane, Bessa-authorized 2026-11-10) · Mode: READ-ONLY
Tree: `/mnt/hdd/data/Projects/umber` (branch `master`, tip `5f1b8626`)
Method: full doc-surface inventory + row truthing per
`~/.hermes/skills/software-development/umber-wave-ops/references/doc-sync-and-row-truthing.md`
(`git log --all`, live greps, live counts, `scripts/scorecard.py --check`).

---

## TL;DR

The repo's **code-truthing discipline is good**; its **doc-truthing has drifted in both
directions** and the drift is now concentrated in the four files a newcomer or an agent
reads first: `SPEC.md`, `docs/specs/requirements.md`, `README.md`, `STATE.md`.

1. **The contract lies about a legal wall.** `SPEC.md:82` still says "no .sbsar/.spp
   execution **ever**"; `.spp` **import** was ratified 2026-10-12 and is a shipped crate
   (`crates/spp`, 3 real-file corpus tests). The PR template repeats the ban. The binding
   ruling is **not in the root `DECISIONS.md`** that SPEC names as its own amendment path.
2. **`requirements.md` is wrong in both directions.** Three rows claim unbuilt features
   that landed (ortho camera, jitter quartet + panel, SVG import); one claims a perf floor
   is NOT REACHABLE while `perf/baseline.json` exists and the nightly gates on it. The file
   is "the CONTRACT of record" per `docs/ROADMAP.md:4`.
3. **`README.md` and `STATE.md` advertise closed work as open.** README lists brush
   angle/jitter and bitmap stamps as still-open waves (landed C2-11, 2026-10-19);
   STATE's status line says wave 1.2 is "next" while 1.2 and 2.1 have landed.
4. **The plan of record is stale by four kills.** `docs/loop/waves/2026-11-09-wave-plan.md:3`
   still says "Status: LIVE — wave 1 is in flight"; waves 1.1, 1.2, 2.1, 3.1, 3.1b, 3.2,
   3.3 have landed and 3.4 is dispatched. One wave-3 premise in it was disproved by its
   own research doc.
5. **Process docs contradict the process.** `build-host.md:3` says every cargo command runs
   on a remote host stood down 2026-10-21; `ROADMAP.md:344-354` says Gate 2 is "ONE CI run
   per wave" while CI became a release-only gate on 2026-11-06 (`safe-push.py` is the
   per-push gate); `CONTRIBUTING.md:63` says "CI runs these exact gates" and omits the two
   cargo-free gates (`check-layers.py`, `scorecard.py --check`) that fail first.
6. **The loop's own tracking docs have three competing conventions.** `queue.md` has 135
   numbered rows with **15 duplicated row numbers** and non-monotonic order at the tail;
   `kill-ledger.md` claims "the live state of every hunt" but stops at 2026-10-23;
   `CHANGELOG.md` has **zero November entries** while 98 kill reports landed since 10-24.
7. **Onboarding dead-ends.** README → quick start → app, with **no link to
   `docs/user/getting-started.md`** anywhere in the README (the user guide is not even in
   the repository-layout tree). The keyboard reference claims every binding but omits the
   viewport-local `P` (ortho) and `F` (frame) keys.

What is healthy and should not be churned: `AGENTS.md` = `CLAUDE.md` (byte-identical),
`DECISIONS.md` vs `docs/loop/decisions.md` (declared distinct by scope, and they are),
`docs/perf/protocol.md` (current through the A/B instrument wave), the two-gate model as
stated in `AGENTS.md`, the 361 kill reports, and the vendored `docs/references/` +
`docs/claw-artifacts/` dumps.

---

## Findings table

Severity: **P0** = misleads a contributor, agent or security/legal reader into a wrong
action or a false claim · **P1** = stale or duplicated but harmless to act on · **P2** =
polish.

| ID | Sev | Location | Issue | Evidence | Proposed edit |
|---|---|---|---|---|---|
| F1 | **P0** | `SPEC.md:82` (also `:23`) | "No Adobe SDKs, no .sbsar/.spp execution **ever**" reads as "no `.spp` support". `.spp` **import** is ratified and shipped. | Ruling 1 in `docs/loop/decisions.md:13-18` (2026-10-12, BINDING); `crates/spp` + `crates/spp/tests/corpus.rs` (3 real Painter files, hash-pinned); `AGENTS.md` hard wall 10 says import is ratified, write is the wall. | Rewrite the non-goal to the ratified split: import-only, clean-room, never write. Add the amendment note + a root `DECISIONS.md` entry (see F2). |
| F2 | **P0** | `DECISIONS.md` (whole file) | The binding 2026-10-12 `.spp`-import reversal is **absent** from the append-only log that `SPEC.md:109` names as the only amendment path. | `grep -c "spp\|SPP" DECISIONS.md` → **0**; `grep -c "^## \[" DECISIONS.md` → 16 entries, none about `.spp`; the ruling lives only in `docs/loop/decisions.md`. | Append one entry `## [2026-10-12 …] spp-import-ratified` to `DECISIONS.md` citing ruling 1 + `30b966e`, then amend SPEC. |
| F3 | **P0** | `.github/PULL_REQUEST_TEMPLATE.md:16` | Checklist item "No Adobe format/SDK touch (.sbsar/.spp remain forbidden)" tells every outside contributor that the `spp` crate is off-limits. | Same evidence as F1; `CONTRIBUTING.md:28-32` already states the correct split. | Replace with the exact two-line split (draft in §P1). |
| F4 | **P0** | `docs/agent-contract-optimized.md:3,8-11,32` | Stale duplicate of the agent contract: "14-crate workspace" (live 16), "No `cargo build`, no `cargo test` … CI is the runtime gate" (CI is release-only; claws run no cargo at all; the dragon runs the battery), "no .sbsar/.spp execution **or parsing**". | `ls crates/*/ | wc -l` → 16; `AGENTS.md:14-28` (claws: no cargo); `ci.yml:1-20` (release gate); only inbound link is a 2026-10-19 kill report (`docs/loop/kill-reports/2026-10-19-docs-audit.md`). | Delete the file, or (if provenance matters) keep it and prepend the `wave-plan.md`-style RETIRED banner naming `AGENTS.md` as the live contract. |
| F5 | **P0** | `docs/specs/requirements.md:15` and `:154` | Row says "**angle, alignment and the jitter quartet are NOT built**"; row 154 says "the jitter quartet has no panel rows". Both landed. | `BrushParams { stamped, stamp_angle, jitter: Jitter{size, angle_degrees, position, flow} }` (`crates/umber-brush/src/dab_adapter.rs:105-231`); panel rows `crates/umber-app/src/brush_panel.rs:1318-1380`; keys `assets/i18n/en-US.ftl:428-431`; commits `47d8fed` + `487fd49` + `90ec048`; queue C2-11 ✓. | Replace the two status cells with `✓ built (`47d8fed`+`487fd49`+`90ec048`, C2-11) — v1 boundary: alignment modes beyond the stamped/tangent set` (draft §P2). |
| F6 | **P0** | `docs/specs/requirements.md:81` | "Camera: orbit/pan/zoom, perspective + orthographic … **ORTHO UNBUILT**". | `Projection::Orthographic` (`crates/umber-gpu/src/camera.rs:35-48,152-160`); P toggle while hovered (`crates/umber-app/src/viewport.rs:2195`); commit `40f3e5d` "orthographic projection mode (P toggles while hovered)". | `✓ built (`40f3e5d`) — P toggles while the viewport is hovered (viewport-local key, not in BINDINGS)`. |
| F7 | **P0** | `docs/specs/requirements.md:87` | Perf floor "**NOT REACHABLE**: `perf/baseline.json` absent … re-baseline on the 4096-default path = worklist #2". | `perf/baseline.json` exists (2026-11-06, plus `breakdown-2026-11-06.json`); nightly gates a same-job A/B ratio (`perf-nightly.yml:3-13`, `docs/perf/protocol.md:556-590`); reseed landed `c53a08d`/`b1d3660`; worklist #2 is ticked in `queue.md`. | `✓ code + measured (`e4925a1` facade/HUD, `fd49627` baseline, RTX 3070 p95 0.2034 ms vs 16.7 ms budget) — gate = the nightly A/B ratio`. |
| F8 | **P0** | `README.md:20` and `:69` | Front door lists "brush angle/jitter, bitmap stamps" as open feature waves. Both landed. | Same as F5 (C2-11, 2026-10-19); ROADMAP archive line 163 already says "Bitmap stamps + angle modes + jitter … (C2-11)". | Drop those two from both lists; keep "Win32 tablet path, a11y pass, the 0.1.0 release gates" (draft §P3). |
| F9 | **P0** | `STATE.md:24-28` | "1.2 the malformed-triangle policy asymmetry **next**; … Live row log: `docs/loop/queue.md` (rows 307-310)". | 1.2 landed `fce31e7c`; 2.1 landed `b228767f` (2026-11-10); queue max row is 343. | Rewrite the status line to name waves 1.1/1.2/2.1 LANDED and 3.x LANDED + 3.4 dispatched; point at "rows up to 343". |
| F10 | **P0** | `docs/loop/build-host.md:3` | "Umber's every cargo command runs here (Bessa, 2026-10-15)". The host was stood down 2026-10-21 and local cargo is the lane. | Skill `umber-wave-ops` §"Build-host outage": "Centralhost checking is STOOD DOWN (Bessa 2026-10-21); local cargo is the lane"; `AGENTS.md:14-28` describes the local battery under `flock /tmp/umber-cargo.lock`. No banner anywhere in `build-host.md`. | Prepend a status banner: "STOOD DOWN 2026-10-21 — local cargo is the lane. This page is the recovery runbook if the host returns." |
| F11 | **P0** | `docs/ROADMAP.md:344-354` ("The wave protocol") | Says Gate 1 = `wave-battery.sh` and Gate 2 = "ONE push per wave + **ONE CI run** (`ci-watch.sh` backgrounded)". Since 2026-11-06 CI does not run per push; the per-push gate is `safe-push.py`, and CI is a release gate. Gate 1 also omits `check-layers.py` + `scorecard.py --check`. | `ci.yml:1-20` ("CI is a RELEASE gate, not a per-push gate (Bessa 2026-11-06)"); `DECISIONS.md` `[2026-11-06 15:5x] ci-release-gate-not-per-push`; `AGENTS.md` hard rule 1. | Replace the section with a 6-line summary that defers to `AGENTS.md` (draft §P4). |
| F12 | **P0** | `docs/loop/waves/2026-11-09-wave-plan.md:3`, `:99-104`, `:106-112` | Plan of record says "Status: LIVE — wave 1 is in flight"; wave 3.2's premise ("a descriptor-only ABR is a legal ABR that the current reader refuses") was disproved by the project's own research; wave 3.3's "fade, jitter, angle map onto the engine" overstates what landed. | `fce31e7c` (1.2), `b228767f` (2.1), `72b409cc` (3.1), `14d9dbf1` (3.1b), wave 3.2 landed 2026-11-10 (queue row 338), 3.3 landed (row 335); `docs/research/2026-11-10-abr-v8-descriptor-analysis.md` (row 333) says v8/11 are "unattested", descriptor-only is a content class; 3.3 counted steps/control as `dynamics_unmapped` and invented no fade param. | Status line per wave + top-of-file "waves closed" table; rewrite 3.2 and 3.3 (draft §P5). |
| F13 | **P0** | `docs/specs/architecture.md:38-52,91-95` | Paint data flow described as a GPU-resident loop ("one wgpu compute dispatch per dirty 256² tile", "Stroke = command with lazy first-write dirty-tile blits; no GPU readback"). The landed engine is `umber-paint` (CPU-side per-layer planes, composite-on-read flush, GPU via `umber_gpu` re-exports). `umber-paint` appears nowhere in the doc or its crate diagram. | `crates/umber-paint/src/lib.rs:1-20`; `crates/umber-paint/src/paint_state.rs` (planes keyed `(owner,tile,channel)`, composite-on-read); AGENTS.md points agents here: "threading and data flow → architecture.md". | Add a "Landed shape (2026-10-14+)" section at the top: `umber-paint` owns the paint session; the Wave-0 GPU-resident sketch is the design rationale, not the code. (Draft §P6.) |
| F14 | **P1** | `docs/specs/tech-stack.md:27,39,42,60-76` | Stale versions and crate list: openusd **0.7** (live 0.8.0), wasmtime **49** (live `=50.0.0-rc.1`), "Windows installer: cargo-dist (**MSI** + scripts)" (MSI dropped, C2-19), workspace shape lists **11 crates + `xtask/`** (live 16 crates, no `xtask`; missing `umber-paint`, `umber-gui`, `umber-wasm`, `umber-plugin-sdk`, `spp`). | `Cargo.toml:81` `openusd = "0.8.0"`, `:102` `wasmtime = "=50.0.0-rc.1"`; `ls -d xtask` → absent; `requirements.md:146` "no MSI … C2-19". | Update the three rows and replace the workspace block with a pointer to `AGENTS.md`'s workspace map (which is current). |
| F15 | **P1** | `docs/ROADMAP.md:103,110` | "**186,170 LOC across 16 crates, 2,251 test functions**" and "**90 of 100 rows** ticked". | Live: 230,053 LOC (`find crates -name '*.rs' | xargs cat | wc -l`), 2,872 `#[test]` attrs; requirements tally = 89 ✓ / 2 ◐ / 4 empty out of **93** requirement rows. | Restate with the live numbers and the correct denominator (93), or delete the two numbers and point at `scorecard.py`. |
| F16 | **P1** | `docs/ROADMAP.md:217-244` | Table titled "**The OPEN waves**" contains **13 rows that say LANDED**. | `sed -n '217,245p' docs/ROADMAP.md | grep -c LANDED` → 13. | Retitle to "Open + recently landed (status in the row)" and move the fully closed rows into the LANDED archive section. |
| F17 | **P1** | `docs/ROADMAP.md:234` | Projection row: "Remaining: depth/occlusion culling (v3, wave 2)". | v3 landed `b228767f` 2026-11-10 (`ProjectionState::occlusion`, default on). | Drop v3 from Remaining; add `(v3 `b228767f`, 2026-11-10)` to the closed list. |
| F18 | **P1** | `docs/ROADMAP.md:250` | "L4: winink live hook | S1's parked bytes (`winink.rs`) are **NOT in the tree**". | `crates/stylus/src/winink.rs` is 307 lines of passive decode (`GetPointerInfo`/`GetPointerPenInfo`), `#[cfg(all(windows, feature = "winink"))]`. | The gate is the **WndProc hook + HWND hand-off**, not the file. Reword. |
| F19 | **P1** | `docs/ROADMAP.md:238` | wave-abr row: "Still open: cross-library stamp dedup, `patt` patterns, v8+, dynamics→engine wiring". Three of four landed. | `72b409cc` (patt), wave 3.3 (dynamics, 2026-11-10), v8 refused as unattested (row 333); dedup dispatched (row 339). | Keep only "cross-library stamp dedup (blake3), in flight (row 339)". |
| F20 | **P1** | `docs/ROADMAP.md:325` | Parity scoreboard: "Retouch + masks … PARITY (persistence gaps on quick mask + symmetry)". | Quick mask persistence `73f8724` (2026-10-31); symmetry persistence landed 2026-10-24 (row 229). | Drop the parenthetical. |
| F21 | **P1** | `docs/specs/requirements.md:112` | "Image formats: … SVG (**later**)". | `crates/umber-core/src/svg.rs` + `pub mod svg` (`lib.rs:35`); `5d4fcccc`; CLI `stencil` converter `06d29936`. | `✓ built (`5d4fcccc`/`06d29936`) — rasterized at import; live vector paths stay the path-tool wave`. |
| F22 | **P1** | `docs/specs/requirements.md:141` | Stylus/tablet row has **no status cell at all** (3 cells) while its components landed. | XI2 backend enabled for the app (`crates/umber-app/Cargo.toml:74` features `["xi2"]`); pressure via raw `Event::Touch` marked ✓ on row 142; winink passive decode in tree. | Add a 4th cell: `◐ partial — XI2 + Touch pressure ✓; winink decode ✓, live WndProc hook open; Wintab open`. |
| F23 | **P1** | `docs/specs/requirements.md:146` | One cell contradicts itself: "✓ code, **NOT PROVEN** … NO installer has ever been produced" then "✓ built … shell/PS proven `9f875b3`". | Same cell, two clauses. | Keep the second, delete the first, and state the honest boundary (Flatpak/AppImage config-only until CI; the 2026-10-23 run 40040567126 produced the first real Flatpak bundle). |
| F24 | **P1** | `docs/specs/requirements.md` (7 cells) | Seven cells still say `✓ built (`PENDING`)` — the sha was never filled by the dragon (AGENTS hard rule 9 says the dragon fills it). | `grep -c PENDING docs/specs/requirements.md` → 7 (lines 43, 79, 121, 122, 146, 156, 167). | Fill each from its named commit, or replace `PENDING` with the commit already named in the same cell. |
| F25 | **P1** | `CONTRIBUTING.md:63-70` | "CI runs these **exact** gates on Linux and Windows; run them first" — false (CI is a release gate) and incomplete (omits `check-layers.py`, `scorecard.py --check`, the bake/stylus/ocio legs, the e2e smoke). | `ci.yml:1-20` triggers; `ci.yml:34-47` the `gates` job; `ci.yml:143-197` the extra legs. | Reword to "what CI will run when it does" + add the two cargo-free gates as step 0 (draft §P7). |
| F26 | **P1** | `docs/loop/queue.md` (whole tail) | 135 numbered rows, **15 row numbers duplicated** (282,283,284×3,287,289,296,297×3,298×4,311,329,330,332,337,338,339 — 34 entries), and order goes backwards at the tail (row 337 dispatch appears after row 342 watch). | `grep -oE '^- [0-9]+-' docs/loop/queue.md | sort | uniq -c`; tail order 342 → 337 → 343 → 338 → 339. | Process fix (see §Process-1): a `## Next row` counter at the top, and the watch cron writes to its own file. |
| F27 | **P1** | `docs/loop/queue.md` rows 330-343 | The Umber queue carries another repo's state: most of every WATCH row is L-AI CI billing, arms, receipts. The header says "Umber core-loop queue — single source of loop truth". | Rows 330-343: each 800-1,500 chars, ~60-75% L-AI. | Split the watch log to `docs/loop/watch-log.md`; keep Umber rows to a fixed field set (§Process-1). |
| F28 | **P1** | `docs/loop/kill-ledger.md:3-6` | "One row per in-flight or recently-landed kill … **Updated at dispatch, at landing, at consumption**". Last entry is the 2026-10-23 flatpak chain; the format line (6 columns) does not match the actual tables (3 columns). | `grep -c "^|" docs/loop/kill-ledger.md` → 113; tail = flatpak walls; queue.md is the live log since. | Retire with the `wave-plan.md` banner, or fold the still-relevant rows into queue and delete. |
| F29 | **P1** | `docs/loop/next-wave-plan-draft.md:1-3` | A 2026-10-19 DRAFT with no RETIRED banner (unlike `docs/loop/wave-plan.md`, which has one). Its nine lanes are consumed/landed; its "stale notes found" list is itself stale. | `docs/loop/wave-plan.md:1-13` shows the intended retired-stub pattern; lanes P1/G1/A1-S1/C1 all landed (queue rows). | Prepend the RETIRED banner (draft §P8), or move to `docs/loop/artifacts/`. |
| F30 | **P1** | `CHANGELOG.md:7` (newest section) | Newest entry is **2026-10-23**; no November entries at all, while 98 kill reports landed 2026-10-24 → 2026-11-10. `CONTRIBUTING.md:80` and the superseded `loop-prompt.md` rule 11 still require a line per user-visible change. | `grep -c "2026-11" CHANGELOG.md` → 0; `ls docs/loop/kill-reports | awk …` → 98 since 10-24; `git log -1 -- CHANGELOG.md` → `14c94c52` (2026-10-23). | Either catch it up with one "November waves" section (see §P9) or retire the requirement explicitly in `AGENTS.md` and point at ROADMAP's delta section. Do not leave the rule implied. |
| F31 | **P1** | `docs/user/keyboard.md:3-5` and `docs/user/getting-started.md:153` | "Generated from `BINDINGS`" + "Help ▸ Keyboard Shortcuts lists **every binding**". The viewport-local keys are absent from both: `P` (ortho) and `F` (frame). | `viewport.rs:2195` (P) and `:2200` (F), hover-gated, deliberately not in `actions::BINDINGS`; the in-app window renders `actions::shortcut_rows` (`main.rs:4699-4724`). | Add a "Viewport-local keys (hover the viewport)" block to the doc **and** to the shortcuts window, or register P/F in BINDINGS. |
| F32 | **P1** | `docs/user/getting-started.md:145` | Export section lists "PNG, EXR, TIFF or JPEG" — omits PSD, which README (`:64`) and `requirements.md:100` both claim is shipped. | `crates/umber-export/src/psd.rs` (ag-psd round trip, `40909ea`). | Add "PSD (one layer per map)". |
| F33 | **P1** | `docs/loop/queue.md:1-3` vs tail | Three generations of row conventions in one file (markdown tables for Phase A/B/C1/C2, push-log bullets, `- NNN- date …` prose). No legend says which is current. | Head: tables; middle: push-log bullets; tail: prose rows. | Add a 6-line "Row format (current)" block at the top and freeze the tables as history. |
| F34 | **P2** | `.github/workflows/ci.yml:51` | Comment says "the matrix runs on **EVERY push to master**", but the trigger block above (`:14-26`) has no push-to-master trigger. | Same file, 30 lines apart. | Delete the clause; the comment's own first paragraph already explains the release-gate model. |
| F35 | **P2** | `SPEC.md:60` and `:74` | Acceptance criterion + Wave 6 deliverable both promise "**Windows MSI**". MSI was dropped (C2-19) and `requirements.md:146` says so. | `requirements.md:146` "no MSI: the installers design dropped it, C2-19"; `docs/specs/installers-design.md`. | Replace "Windows MSI" with "Windows installers (shell + PowerShell; no MSI, C2-19)". |
| F36 | **P2** | `SPEC.md:69` | "first backend of the owned `stylus` crate = raw Windows Ink WM_POINTER path". The tree's winink is a passive decode with no window hook; the shipped first backend is winit Touch (pressure) + XI2 (Linux). | `crates/stylus/src/winink.rs:1-20` ("passive decode, no window ownership … Live pointer-pump integration is Wave 2"); `queue.md` C2-30. | Reword to "first shipped pressure path = raw winit Touch; winink's decode is built, its WndProc hook is gated (L4)". |
| F37 | **P2** | `docs/specs/projection-fill-plan.md:106` (§7) | "Named follow-ups (not v1)" still lists "backface/depth culling". Both landed. | v2c backface 2026-11-09; v3 depth `b228767f` 2026-11-10. | Strike both from §7; note them as landed with shas. |
| F38 | **P2** | `docs/specs/stencil-engine-design.md:210` | Points at `docs/loop/kill-reports/2026-10-18-stencil-core.md`, which does not exist (the real one is `2026-10-17-stencil-core.md`). | `ls docs/loop/kill-reports | grep stencil` → `2026-10-17-stencil-core.md`. | Fix the path. |
| F39 | **P2** | `docs/loop/COORDINATION-*.md` (3 files) | One-off coordination notes from 2026-10-15/17 sit in `docs/loop/` root with no archive marker, beside live docs. | `COORDINATION-g1-axis-gizmo.md` (20 lines), `-host-gpu-wedged.md` (37, RESOLVED), `-wgui2-battery-green.md` (54). | Move to `docs/loop/artifacts/coordination-2026-10/` or add `(RESOLVED, historical)` to each title (one already has it). |
| F40 | **P2** | `README.md:105-122` | Repository-layout tree omits `docs/user/` (the getting-started + keyboard guide) and `docs/upstream/`; `docs/` link row is the only onboarding path. | `ls docs/user` → `getting-started.md`, `keyboard.md`; README never links either file. | Add the two tree lines + a "New here? → `docs/user/getting-started.md`" line under Quick start (draft §P10). |
| F41 | **P2** | `docs/loop/loop-prompt.md:10-11` (banner) | The banner says "do not follow v1.2's stale specifics" but rule 10's "No .sbsar/.spp **parsing**" is a hard wall a reader may still quote. | Banner at `:3-10`; rule 10 at §1. | Add one clause to the banner: "rule 10's `.spp` ban is superseded — import is ratified (ruling 1, 2026-10-12)". |
| F42 | **P2** | `.github/ISSUE_TEMPLATE/bug_report.md:17` | Asks for `RUST_LOG=info` paste, but `getting-started.md:153-155` says the app writes `umber.prev.log` as "the file to attach to a bug report". | Both files. | Add "or attach the app's log (`Help ▸ Show Log`; the previous session is `umber.prev.log`)". |

No dead links were found outside intentionally vendored dumps: a full relative-link scan
over 544 markdown files returns 42 misses, **all** inside `docs/references/*-README-*.md`
and `docs/loop/artifacts/resvg-api-refs/` (foreign READMEs whose links point into their
own repos). Backtick path references are clean except F38 and two foreign-repo references
in `THIRD_PARTY.md:97` / `docs/research/absorption-2026-11-01-texturecraft.md:39`
(`docs/control-protocol.md` is *TextureCraft's* path, correctly quoted as provenance).

---

## Detailed proposals (draft text)

### P1 — the `.spp` split, stated once (F1/F2/F3)

**`SPEC.md` non-goals, replace the `.spp` clause of line 23 and line 82:**

> - **No Adobe format *execution*, ever:** `.sbsar` runs only under Adobe's closed
>   Materials SDK terms … `.spp` **writing** is the permanent wall. `.spp` **reading** is
>   ratified (binding ruling 2026-10-12, `DECISIONS.md`): import-only, clean-room from
>   published community HDF5-layout docs, per-project import report, geometry stays
>   FBX/USD. Umber ships an **original node-graph engine** whose graphs serialize as
>   **MaterialX documents with declared custom nodedefs** for painter-specific nodes.

**`SPEC.md` — add to the amendment note (line 5-11):**

> > AMENDMENT NOTE 2 (2026-10-12, ruling 1): `.spp` IMPORT is ratified and in scope
> > (read-only, clean-room, `crates/spp`); the non-goal above applies to writing and to
> > `.sbsar` execution only. Recorded in `DECISIONS.md` `[2026-10-12] spp-import-ratified`.

**`DECISIONS.md` — append (at the bottom, newest-at-bottom is this file's convention):**

> ## [2026-10-12 23:20] spp-import-ratified
> Decision: `.spp` IMPORT is ratified and pulled to wave 3 before PSD import — read-only
> forever, clean-room from published community HDF5-layout docs, per-project import
> report; writing `.spp` stays the hard wall.
> Context: the pull-users-from-Substance story matters most; the ban on *execution* was
> being read as a ban on *reading*.
> Options rejected: keep the flat ban (loses the migration story); write support (the wall).
> Owner: Bessa (binding ruling, docs/loop/decisions.md ruling 1).
> Revisit when: never for writing; import fidelity lanes are tracked in the .spp census.

**`.github/PULL_REQUEST_TEMPLATE.md:16`, replace the line:**

> - [ ] No Adobe **execution**: no `.sbsar`, no Adobe SDK, and **no `.spp` writing**.
>       `.spp` **reading** is allowed (clean-room, `crates/spp`) — see `CONTRIBUTING.md`.

### P2 — requirements row 15 / 154 (F5)

Replace the trailing status of row 15 with:

> ✓ built (`47d8fed` + `487fd49` + `90ec048`, C2-11) — bitmap stamps, `StampAngle`
> (none / stroke / random) and the jitter quartet (size / angle / position / flow) live in
> `BrushParams` (`dab_adapter.rs:105-231`) with panel rows (`brush_panel.rs:1318-1380`)
> and both catalogs (`en-US.ftl:428-431`); v1 boundary: alignment is the stamped/tangent
> set, no camera-aligned mode.

Row 154: delete "the jitter quartet has no panel rows" and replace with
"jitter + angle panel rows ✓ (`brush_panel.rs:1318-1380`)".

### P3 — README open-work lists (F8)

Line 20: replace the wave clause with

> the open feature waves — the Win32 tablet path, the a11y pass, the power-user tail and
> the 0.1.0 release gates — are tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) …

Line 69: replace with

> Still open is tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) — the Win32 tablet path,
> the a11y pass, channel manage, the four post nodes, and the 0.1.0 release gates among
> them.

### P4 — ROADMAP wave protocol (F11)

Replace `## The wave protocol (standing, the two-gate model)` body with:

> Two gates per kill, per `AGENTS.md` (the authoritative statement) and skill
> `umber-wave-ops`:
>
> - **Gate 1 (dragon, local):** `cargo fmt --all -- --check`,
>   `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test` on the touched
>   crates, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check` —
>   one battery at a time under `flock /tmp/umber-cargo.lock`.
> - **Gate 2 (dragon):** `dcommit.py --path-limited` with the kill's locks, then
>   `scripts/safe-push.py --session <id>` **bare**. CI is a **release** gate since
>   2026-11-06 (`DECISIONS.md`): it runs on a version tag, on `release-ci.sh` dispatch, on
>   PRs and weekly — not per push. `scripts/release-ci.sh` is the tag gate.
>
> Kills batch as unpushed commits between gates. Briefs are written from live anchors at
> dispatch; kill reports and the touched rows update in the same commit.

### P5 — the wave plan's status line and wave 3 (F12)

Replace line 3 with:

> Date: 2026-11-09. Author: dragon. Status: **W1 closed (1.1 `7aad67c3`, 1.2 `fce31e7c`);
> W2 closed (2.1 `b228767f`); W3 3.1 `72b409cc`, 3.1b `14d9dbf1`, 3.2 + 3.3 landed
> 2026-11-10, 3.4 in flight (row 339). W4-W6 open.**

And add under the title a closed-waves table:

> | Wave | Kills | Closed | Evidence |
> |---|---|---|---|
> | W1 correctness | 1.1, 1.2 | 2026-11-09 | `7aad67c3`, `fce31e7c` |
> | W2 R-A2 close | 2.1 | 2026-11-10 | `b228767f` |
> | W3 wave-abr tail | 3.1, 3.1b, 3.2, 3.3 | 2026-11-10 | `72b409cc`, `14d9dbf1`, rows 335/338 |

Replace 3.2's body (lines 99-104) with:

> **What the 2026-11-10 analysis changed:** v8/v11 are **unattested** in the public
> record, and "descriptor-only" is a content class, not a header number (a v6.2 file with
> zero samples already imports). The kill therefore hardened the per-brush failure path
> (skip-and-count, `skipped_brushes`) instead of adding a v8 reader. See
> `docs/research/2026-11-10-abr-v8-descriptor-analysis.md`.

Replace 3.3's body with:

> Landed 2026-11-10: `szVr` jitter → `Jitter.size`, minimum → a `radius_curve` floor,
> steps/control counted as `dynamics_unmapped`. No fade parameter was invented; a fade
> would be a new engine param and its own kill.

### P6 — architecture.md: add the landed shape (F13)

Insert after the bird's-eye diagram:

> ## Landed shape (2026-10-14 onward) — read this before the sketches below
>
> The paint engine is **`umber-paint`**, not a GPU-resident loop in `umber-app`:
> per-layer planes keyed `(owner, tile, channel)`, stroke/retouch routing, a
> composite-on-read flush, tombstones/anchors/instances, the `paint/` sidecar with a
> two-phase commit. It has no egui/eframe dependency (points are `emath::Pos2`) and
> reaches the GPU only through `umber_gpu` re-exports. The shell, the CLI agent session
> and perf harnesses all drive the same engine.
>
> The "Paint data flow" and "Virtual texturing" sections below are the **Wave-0 design
> rationale** (why the tile pool exists, why stamps rasterize in UV space). They describe
> the intent, not today's call graph; when they disagree with `crates/umber-paint/src/lib.rs`,
> the crate wins.

### P7 — CONTRIBUTING gate list (F25)

Replace lines 61-70 with:

> ## Before you open a PR
>
> Run the cargo-free gates first (seconds, no GPU), then the cargo ones:
>
> ```bash
> python3 scripts/check-layers.py        # the crate layering contract
> python3 scripts/scorecard.py --check   # README counts must match the tree
> cargo fmt --all --check
> cargo clippy --workspace --all-targets -- -D warnings
> cargo test --workspace
> cargo test -p umber-gpu --features gpu            # the GPU golden family
> cargo test -p umber-bake --features gpu -- --test-threads=1
> ```
>
> CI is a **release gate**, not a per-push gate: it runs on tags, on
> `scripts/release-ci.sh` dispatch, on PRs and weekly. Your PR gets the full matrix —
> Linux lavapipe + Windows WARP, plus the stylus/winink and OCIO feature legs — so run
> what your change touches before pushing.

### P8 — retired-stub banner for `next-wave-plan-draft.md` (F29)

> # Next-wave plan (DRAFT, 2026-10-19) — **RETIRED**
>
> This plan was consumed by the 2026-10-23 W-C3a fleet and the 2026-11-09 wave plan. It is
> kept for provenance only. The plan of record is
> `docs/loop/waves/2026-11-09-wave-plan.md`; the work list is `docs/loop/queue.md`.

### P9 — CHANGELOG catch-up (F30)

Add one section above the 2026-10-23 entry:

> ## The November waves — decal projector to v3, the ABR tail, the SVG import (2026-10-24 → 2026-11-10)
>
> - **Added — the R-A2 decal projector lane, closed:** v1 decal projector (`c573dc87`),
>   tiling (`…`), follow-camera, backface culling, one-pass raster (`98fb6e8e` — a ~34 min
>   UI freeze at 4K became ~0.6 s) and depth/occlusion culling (`b228767f`, default on).
> - **Fixed — `UvTri` triangle conditioning (`7aad67c3`)** and the mesh-wide malformed
>   triangle policy (`fce31e7c`): needle triangles are not valid footprints on CPU or GPU.
> - **Added — wave-abr tail:** `patt` reader (`72b409cc`), `PatternTexture` in
>   `.umberbrush` + CLI pattern sidecar (`14d9dbf1`), per-brush skip-and-count, dynamics →
>   engine wiring.
> - **Added — SVG import (`5d4fcccc`)** with a 16 MiB input budget and the CLI `stencil`
>   converter (`06d29936`).
> - **Added — the tree moved to `/mnt/hdd/data/Projects/umber`** (symlink at the old path).
>
> Per-kill evidence: `docs/loop/kill-reports/` (98 reports in this window).

Then keep the rule honest by adding to `AGENTS.md` (see Process-3).

### P10 — README onboarding (F40)

Add to the tree:

```
│   ├── user/          # the newcomer path: getting-started.md, keyboard.md
```

and after the Quick start block:

> **First time?** Follow [`docs/user/getting-started.md`](docs/user/getting-started.md):
> open a mesh → bake → paint → save → export, every step naming the real menu. Shortcuts:
> [`docs/user/keyboard.md`](docs/user/keyboard.md).

---

## Process / loop proposals (the three that caused real friction)

### Process-1 — queue row numbers: a counter, not a memory

**Evidence of friction:** 15 duplicated row numbers in one file; the tail is
non-monotonic (342 → 337 → 343 → 338 → 339). The skill already records the cause
("two sessions appending from remembered numbers collided five times in one day",
`umber-wave-ops` §Queue row numbers) and `dcommit.py` refuses *added* duplicates — which
is why the duplicates that exist are historical, and why the guard fires only after the
collision. The WATCH cron and the dragon are two writers with two "max row + 1" reads.

**Proposal (three parts):**

1. Add a single counter line at the top of `queue.md`, and make it the only allocation
   source:
   ```markdown
   ## Next row
   **344**  (take this, then bump in the same edit; never re-read "max")
   ```
2. Split the watch log out. `docs/loop/watch-log.md` gets the cron's per-30-min WATCH
   rows (umber + L-AI), with its own counter. `queue.md` keeps dispatch / landed / research
   rows only. This removes ~70% of queue bytes and stops another repo's CI state from
   living in Umber's "single source of loop truth".
3. Add a row-format legend to the top (6 lines) and freeze the Phase A/B/C1/C2 tables as
   history under a `## History (frozen)` heading.

Draft legend:

```markdown
## Row format (current)
`- NNN- YYYY-MM-DD HH:MM KIND (author; lock <name> over <files>): <what>.
 Code <sha>. Report <path>. NEXT: <one action>.`
KIND ∈ DISPATCH | LANDED | RESEARCH | PLAN | DOCS | WATCH(moved to watch-log.md).
```

### Process-2 — one canonical kill-report shape

**Evidence of friction:** two shapes are in force for the same artifact.
`AGENTS.md` "Report (end every task with exactly this)" mandates **Built / Judgment calls /
Unverified / Deferred / Gates run**. The skill (`umber-wave-ops` §Kill report shape)
mandates **Built / Judgment calls / Unverified / Deferred / Process note (as needed)** and
says the report *opens with* an authorship + lock + design-doc header. The repo's own
reports use both (e.g. `2026-11-09-projection-occlusion.md` opens with authorship + lock,
then Built…), so agents must guess which contract the reader enforces.

**Proposal:** make the skill's shape canonical (it is the richer one) and update
`AGENTS.md` to match, with `Gates run` folded in as a required fifth section and the
process note optional sixth:

```markdown
1. Header — authorship (dragon/claw + model), kill-lock + files it held, design-doc
   sections implemented.
2. Built — per-file bullets, each traceable to a design-doc section.
3. Judgment calls — decision, rejected alternative, why, follow-up it creates.
4. Unverified — holds by construction, no dedicated test; say it plainly.
5. Deferred — deliberate non-work, so it is not rediscovered.
6. Gates run — verify-kill, fmt, clippy, tests, check-layers, scorecard: verbatim
   pass/fail, or UNRUN + why.
7. Process note (as needed) — lock overlaps, concurrent-claw interactions.
```

### Process-3 — wave plans and CHANGELOG get a close step

**Evidence of friction:** the plan of record says "wave 1 is in flight" after waves 1-3
have closed; `CHANGELOG.md` stops at 2026-10-23 while 98 reports land after it. Both are
"write once at plan time, never close" artifacts. The loop's own history shows the
pattern: the W-C3a/W-C3b closes *did* write CHANGELOG sections — the discipline existed
and was dropped when the wave plan replaced the close ritual.

**Proposal:** add one line to the wave plan's "standing order of operations" (step 9) and
to `AGENTS.md`:

> 9. **Close the wave in the same kill as its last landing:** set the wave header to
>    `LANDED <date> (<shas>)`, add the row to the closed-waves table at the top, and add
>    one `CHANGELOG.md` bullet per user-visible kill. A wave whose header still says LIVE
>    after its kills landed is a doc bug the next session inherits.

If the team prefers to retire CHANGELOG instead, say so explicitly in `AGENTS.md` and
delete the `CONTRIBUTING.md:80` rule — an implied rule that nobody enforces is worse than
no rule (it is how the file reached 18 days of silence).

---

## Do not touch (intentionally historical)

| Path | Why |
|---|---|
| `docs/specs/2026-10-09-full-project-audit.md`, `2026-10-12-strategy-review.md` | Dated audits/reviews preserved verbatim; the disposition lives in `DECISIONS.md`. |
| `docs/loop/audit-2026-10-1*.md`, `audit-2026-10-24-*.md`, `docs/loop/artifacts/*` | Census/lens artifacts; evidence, not current state. |
| `docs/specs/wave3-delivery-audit.md` | Dated delivery audit; already named historical in the queue. |
| `docs/research/*` | Cited research; corrections go through the research-correction issue template, never silent edits. |
| `docs/references/photocraft-README-2026-11-07.md`, `texturecraft-README-2026-11-07.md` | Vendored foreign READMEs; their dead links point into their own repos. |
| `docs/claw-artifacts/**` | Vendored dependency sources with verified-facts READMEs. |
| `DECISIONS.md` history (existing entries) | Append-only by its own header; supersede by adding, never edit. |
| `docs/loop/decisions.md` history | Same, loop-scope. |
| `docs/loop/kill-reports/**` | Evidence. If a report is wrong, add a correction note, do not rewrite it. |
| `CHANGELOG.md` existing sections | Keep-a-Changelog history; the fix is a new section above, not edits below. |
| `docs/loop/loop-prompt.md` | Superseded by the vault charter; keep the banner (F41 is a one-clause banner addition, not a rewrite). |
| `docs/loop/wave-plan.md` | The model for how to retire a doc (13-line stub). Do not delete. |
| `docs/perf/protocol.md` | Current and carefully dated; it already carries the A/B instrument section. |

---

## Top 5 proposals (plain text)

1. Fix the .spp contract in one pass: SPEC.md's non-goal becomes import-ratified /
   write-forbidden, the binding ruling gets its root DECISIONS.md entry (it is missing
   today), and the PR template's "spp forbidden" checklist line is replaced with the
   correct split. This is the only finding with legal/security reading consequences.

2. Truth the requirements.md rows in both directions: mark jitter/angle/bitmap stamps
   built (row 15 + 154), ortho built (row 81), the perf floor measured (row 87), SVG
   import built (row 112), give the stylus row its status cell (row 141), collapse the
   self-contradicting distribution cell (row 146), and fill the seven PENDING shas. The
   file calls itself the contract of record; right now it understates the product.

3. Correct the front door: README drops brush angle/jitter and bitmap stamps from its
   open list (both landed), gains a link to docs/user/getting-started.md and the user/
   tree line, and STATE.md's status line names waves 1.1/1.2/2.1 as landed and the queue
   at row 343 instead of 307-310.

4. Close the process contradictions: give build-host.md its STOOD-DOWN banner, replace
   ROADMAP's wave-protocol section with the safe-push + release-gate reality, and fix
   CONTRIBUTING's "CI runs these exact gates" block to include check-layers and scorecard
   and to say what CI actually runs and when.

5. Restructure the loop's tracking docs: a single queue row counter plus a separate
   watch-log.md (kills the duplicate row numbers and the other repo's state in Umber's
   queue), one canonical kill-report shape shared by AGENTS.md and the skill, and a wave
   close step that stamps the wave header and writes the CHANGELOG bullet in the same
   kill as the last landing.

Runner-up if a sixth slot exists: docs/specs/architecture.md needs a "Landed shape"
section naming umber-paint, because AGENTS.md sends every agent there for threading and
data flow and the doc currently describes a GPU-resident loop the tree does not have.
