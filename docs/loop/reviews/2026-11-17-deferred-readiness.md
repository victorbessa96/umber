# Deferred Items Readiness Audit — next-steps sections C/D (items 9–15)

Source: `docs/loop/next-steps-2026-11-16.md:41-58` (sections C + D).  
Battery: `check-layers.py` GREEN (16 crates) `scripts/check-layers.py:138`; `scorecard.py --check` GREEN `scripts/scorecard.py:96`.  
CI: 5 most recent runs all `success` on `master` and `v0.1.0-alpha.1` tag `gh run list --repo victorbessa96/umber --limit 5`.  
Release `v0.1.0-alpha.1` published 2026-11-16 with 18 assets (app + cli, linux + windows) `gh release view v0.1.0-alpha.1 --repo victorbessa96/umber`.

---

## 9. R-A2 v3 depth/occlusion culling

**Current gate**  
Design brief + kill brief landed; implementation **not started**. Gate is the standing wave protocol: brief → kill-lock → claw → dragon battery (clippy `-D warnings`, check-layers, scorecard, rustfmt, crate tests, CI) → kill report → `dcommit --path-limited` → `safe-push` bare → release lock.  
`docs/loop/briefs/2026-11-09-projection-occlusion-design.md:95-99` (gate clause); `docs/loop/briefs/2026-11-09-projection-occlusion-kill-brief.md:4-5` (lock `razul-projection-occlusion-w21`); `docs/loop/waves/2026-11-09-wave-plan.md:210-226` (standing order).

**What would open it**  
A dragon dispatch decision to brief the kill. The briefs are complete and measurable (pins named, cost class claimed 64 MiB at 4096² `docs/loop/briefs/2026-11-09-projection-occlusion-design.md:57-63`).

**Concrete next action**  
Dispatch kill-lock `razul-projection-occlusion-w21` to a claw; surface files enumerated in kill brief `docs/loop/briefs/2026-11-09-projection-occlusion-kill-brief.md:7-15` (new `crates/umber-mesh/src/depth.rs`, `projection.rs`, `display_panel.rs`, `main.rs`, both `.ftl` catalogs).

**Worth before v0.1.0 stable?**  
**No.** ROADMAP row 238 explicitly: "CLOSED to v3 (2026-11-10)" `docs/ROADMAP.md:238`. The v1 decal projector + v2a tiling + v2b follow-camera + v2c backface culling + v2d one-pass raster + W1.1 UV conditioning are LANDED; v3 depth/occlusion is the named v3 follow-up. v0.1.0 checklist does not require it.

---

## 10. R-A2 §2.2 product questions (seam/symmetry expansion, decal channel, bake family)

**Current gate**  
Product decisions required — not technical gates. Wave plan §2.2: "These stay queued, not scheduled: each needs a decision on what the artist sees, not just an implementation." `docs/loop/waves/2026-11-09-wave-plan.md:88-94`.

**What would open it**  
Bessa's call on: (a) seam/symmetry expansion of projections (how tiling + symmetry interact with the projector), (b) whether a dedicated decal channel exists distinct from fill layers, (c) the bake family (re-bake projections into texture sets).

**Concrete next action**  
None until product direction is given. Document the questions in a product brief if Bessa wants them framed.

**Worth before v0.1.0 stable?**  
**No.** Explicitly queued as product calls, not engineering work. v0.1.0 ships the v2d projector; v3 questions are post-stable.

---

## 11. L4 winink live hook / wave-stylus-win32

**Current gate**  
**Windows session required** (partner-gated). The passive decode (`crates/stylus/src/winink.rs`, 307 lines, `GetPointerInfo`/`GetPointerPenInfo`, `#[cfg(all(windows, feature = "winink"))]`) is in tree `docs/ROADMAP.md:254`; `crates/stylus/src/winink.rs:1-20`. The gate is the **WndProc hook + HWND hand-off**, not the decode bytes (module doc: "Live pointer-pump integration is Wave 2" `crates/stylus/src/winink.rs:1-20`). L4 is gated on this hook; wave-stylus-win32 is the Windows dual-path verification leg `docs/ROADMAP.md:256`.

**What would open it**  
A real Windows host with a pen device + Bessa's go-ahead to derive the WndProc subclass + HWND hand-off. The S1 winink-pen-flags kill (`8d7bf8a`) landed the platform-free `PenFlagTracker` `docs/loop/kill-ledger.md:153`; the live hook is the remaining L4 surface.

**Concrete next action**  
Wait for Windows session availability. No code derivation until the gate opens (standing rule: "no derivation until the gate opens" `docs/loop/waves/2026-11-09-wave-plan.md:193-201`).

**Worth before v0.1.0 stable?**  
**No.** XI2 + raw winit `Touch` pressure already ship (Linux pressure ✓ `docs/research/2026-11-10-docs-workflows-pass.md:83`). Winink is a Windows-only ergonomic enhancement; the release checklist does not gate on it. Post-stable.

---

## 12. L7 color_button delegate conversion

**Current gate**  
**G1-gated**. G1 (picker-sliders-recents) landed `21ada34` + `5b38c96` (pushed `e610ed7..96edd1e`) `docs/loop/queue.md:105`. The additive `color_button_with` sits beside stock `color_button` `crates/umber-gui/src/widgets/color_picker.rs:745-956`; three app sites migrated to `color_button_with` `crates/umber-app/src/preferences.rs:412-429`. Rule 6 (wave plan): "After A3 migrates all three sites, the old `color_button` has no app consumer. Rule 6: it becomes a thin delegate to `color_button_with`" `docs/loop/waves/2026-11-09-wave-plan.md:116-117`; `docs/loop/next-wave-plan-draft.md:110-117`.

**What would open it**  
G1 complete (it is) + A3 picker-app-sites complete (it is, landed `5b38c96` `docs/loop/queue.md:105`). The gate is satisfied; the conversion is the next mechanical step.

**Concrete next action**  
Brief the delegate conversion (one file: `crates/umber-gui/src/widgets/color_picker.rs` — replace `color_button` body with a call to `color_button_with` using default `PickerLabels` and a thread-local `RecentColors`), take kill-lock, claw writes, battery, push.

**Worth before v0.1.0 stable?**  
**Yes, low risk.** It is a cleanup delegate (no behavior change, kittests remain consumers `docs/loop/kill-reports/2026-10-20-a3-picker-app-sites.md:54-70`). Can be done in a post-alpha patch release if time permits, but not a blocker.

---

## 13. wave-wasm-shaders / tessellation / hdr / ray-query

**Current gate**  
**eframe/wgpu upgrade postures**. The probes are built; the lane waits on upstream eframe/wgpu exposing the required surface capabilities (WGSL subgroups, ray query, tessellation, HDR swapchains) `docs/ROADMAP.md:257`; `docs/loop/waves/2026-11-09-wave-plan.md:200`. No derivation until the upstream versions land with the needed features.

**What would open it**  
eframe/wgpu releases that expose: (a) compute/tessellation/ray-query pipeline creation, (b) HDR surface formats, (c) WASM shader import path. The current wgpu/eframe in `Cargo.lock` does not have these.

**Concrete next action**  
Monitor upstream. No code work until the gate opens. Keep probes current.

**Worth before v0.1.0 stable?**  
**No.** These are forward-looking GPU feature lanes; v0.1.0 ships on current wgpu/eframe. Post-stable exploration.

---

## 14. .spp staged import R1–R4

**Current gate**  
**Fidelity report's staged lanes** — each R is a bounded kill gated on corpus access + design decision. Formats lens reports: R-chunk-index LANDED (b-tree offset fix) `docs/loop/audit-2026-10-24-lens-formats.md:24-25`; R1 full-precision channels (f16→planes) **staged** `docs/loop/audit-2026-10-24-lens-formats.md:26`; R2 assbin mesh **staged** `docs/loop/audit-2026-10-24-lens-formats.md:27`; R3 ResourceImage decode **staged** `docs/loop/audit-2026-10-24-lens-formats.md:28`; R4 HBO layer-stack schema **test-armed but not imported** (spp test module carries `hbo_file/probe_hbo_summary_line` `crates/spp/src/lib.rs:249,526-530`) `docs/loop/audit-2026-10-24-lens-formats.md:29-32`.

Gap map source: `docs/research/spp-import-audit-2026-10-16.md:51-110` (gaps 1–10 → requisites R1–R4 `docs/research/spp-import-audit-2026-10-16.md:126-139`).

**What would open each**  
- R1: corpus f16 channel + decision on `--precision native|8` CLI flag.  
- R2: assbin mesh subset reader (Assimp ASSBIN documented).  
- R3: generalize QDataStream reader to `Alg::ResourceImage` blobs.  
- R4: HBO v11/12 registry-family automap schema (the flagship v2 item).

**Concrete next action**  
None until Bessa prioritizes a specific R. Each follows standing order: brief → kill-lock → claw → battery → push.

**Worth before v0.1.0 stable?**  
**No.** v1 import (flattened RGBA8 channels, HBO graphs reported-not-imported) is shipped and corpus-gated `docs/research/spp-import-audit-2026-10-16.md:42-49`. R1–R4 are v2 fidelity improvements; the release checklist does not require them.

---

## 15. Hard walls (never): .spp WRITING; .sbsar execution; fake interchange

**Current gate**  
**Binding rulings** — permanent, not derivable.  
- `.spp` WRITING: "Writing `.spp` is a permanent wall" `docs/SPEC.md:28`; `docs/DECISIONS.md:129` (ruling 2026-10-12); `docs/AGENTS.md:45`; `docs/CLAUDE.md:45`; `README.md:41`.  
- `.sbsar` execution: "No Adobe format execution, ever: .sbsar runs only under Adobe's closed Materials SDK terms" `docs/SPEC.md:28`; `docs/DECISIONS.md:129`; `docs/research/01-painter-feature-inventory.md:122-125` (legal analysis).  
- Fake interchange (custom nodedefs declared, not standard MaterialX): `docs/specs/dynamic-materials-design.md:177`; `docs/loop/next-steps-2026-11-16.md:57`; MaterialX kill A landed parse-only with declared custom nodedefs `docs/loop/waves/2026-11-09-wave-plan.md:157-158` (byte-identical round-trip, never-panic).

**What would open them**  
Nothing. These are sovereign non-goals. The `.spp` import ratification (2026-10-12) explicitly split reading (ratified) from writing (wall) `docs/DECISIONS.md:129-130`.

**Concrete next action**  
Enforce in reviews: any PR touching write paths for `.spp`, `.sbsar` execution, or fake interchange is rejected by contract `docs/agent-contract-optimized.md:28`.

**Worth before v0.1.0 stable?**  
**N/A — they are walls, not work.** The walls are already enforced in code (`crates/spp` is read-only `crates/spp/src/lib.rs`; no `.sbsar` runtime; MaterialX custom nodedefs declared `crates/umber-graph/src/mtlx.rs`).

---

## Summary table

| Item | Gate | Opener | Next action | Pre-v0.1.0? |
|------|------|--------|-------------|-------------|
| 9. R-A2 v3 depth/occlusion | Wave protocol (briefs ready) | Dragon dispatch | Kill-lock `razul-projection-occlusion-w21` → claw | **No** (v3, ROADMAP:238) |
| 10. R-A2 §2.2 product questions | Product decision | Bessa's call | Wait / frame brief if asked | **No** (queued) |
| 11. L4 winink / wave-stylus-win32 | Windows session + WndProc hook | Partner Windows pen session | Wait for gate | **No** (XI2+Touch ship) |
| 12. L7 color_button delegate | G1-gated (G1 + A3 done) | Gate satisfied | Brief delegate conversion → kill-lock | **Yes, low risk** |
| 13. wave-wasm-shaders/tess/hdr/ray-query | eframe/wgpu upgrade | Upstream feature releases | Monitor | **No** (forward-looking) |
| 14. .spp R1–R4 import | Fidelity report lanes | Per-R brief + corpus | Wait for prioritization | **No** (v2 fidelity) |
| 15. Hard walls | Binding rulings | Never | Enforce in review | **N/A** |

---

## Battery verification (this audit)

```
$ python3 scripts/check-layers.py
=== layers: GREEN (16 crates) ===

$ python3 scripts/scorecard.py --check
scorecard: README.md current
```

CI: 5/5 recent runs `success` (master + v0.1.0-alpha.1 tag).  
Release `v0.1.0-alpha.1`: published, 18 assets, prerelease=true.