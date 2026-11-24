# Umber next steps — reconciled 2026-11-23

This supersedes `next-steps-2026-11-16.md` as the current work map. Git refs and live source outrank dated audits and queue prose. Base at reconciliation: `7f0c976bdedf923cb671f9372624c25034063bf3` plus the L7 brief/claim commits `12471890` and `b391649b`.

## A. Docs truthing — executable now

1. **Finish the L7 row updates.** `docs/ROADMAP.md` and `docs/loop/waves/2026-11-09-wave-plan.md` still list the picker delegate as G1-gated even though G1 and A3 landed. This kill moves those rows to the measured implementation and verification.
2. **Correct the live roadmap's hidden runway.** “No ungated engineering runway remains” is false. Quick-mask undo landed `c5909efe`; folder kind exists (`cc381ce6`), grouped flatten undo landed `e008dcc8`, and U7 landed the icon rail/collapsible columns. Only the Assets mesh category remains from R-C1: drag `769c2544`, thumbnails `e9c02bd0`, custom dirs `68b45a69`, import dialog `77f58d41`, browser kittest `09867156`, saved searches `38a4fd21`, env `0d6f08f8`, and filters `05105aa4` are landed. The bottom wave protocol must also say release-only CI plus safe-push, not one CI run per wave.
3. **Refresh stale agent and requirement pointers.** `CLAUDE.md`, `AGENTS.md`, and `docs/agent-contract-optimized.md` still list GUI 0b, PSD export, displacement preview, and the AI-provider interface as future work. GUI shell chrome and those requirements have landed (`9e0517bf`, `40909ea`, `0dbda8e`). `requirements.md` must also acknowledge dynamic-material v1 (`f85cb0aa`, `2c6497ec`), the AssetStore GC caller (`1f0e438`), and XI2 wiring (`d2fd7b7`), and replace literal `PENDING` evidence cells with real hashes.
4. **Correct dated readiness/check documents without reopening landed work.** D2/D3/D4 and GPU capability portability are closed by `11c4359d`, `52fe2142`, `90aff880`, and `687b32ca`. The deferred-readiness audit must mark projection occlusion `b228767f` landed and acknowledge f16 preview decode without claiming full-precision planes. `desktop-checks.md` check 4's parked-S1 prerequisite is stale (`8d7bf8a0` landed). The dogfood prep must not claim the published alpha can exercise post-alpha D2–D5/displacement; Gate 9 is MET, and its C2-30 label must not be confused with the closed Windows Ink row.
5. **Use the independent 20/20 census as evidence, not as landed code.** `deleg_3ba91c0f` verified this map read-only against the pin and live source. It did not edit, run cargo, commit, or push.

## B. Ungated engineering — dependency-ordered candidates

1. **L7 picker delegate — implemented locally.** `eb2237a9` landed under `razul-picker-delegate` with R2 SHIP and the full crate 151/151. Only the report/status commit, safe-push, and origin verification remain; no duplicate kill.
2. **PerChannel K1 — implemented locally.** `4383e303` landed under `razul-perchannel-k1`: normal=31, color semantic and the empty layer context=28, scalar/unknown non-empty=25, only Tint/Saturation/Color hidden, Value kept, saved-document/driver behavior unchanged. Full app 1,260/0 and replacement review SHIP. Final report/queue hashes, safe-push, and origin verification remain; no duplicate kill.
3. **Bake/Export history `record_context`.** Production currently records only FilterApply and QuickMask. Wire real bake/export job context rather than relying on tests/document helpers; named at `requirements.md:155`.
4. **Material picker R-A10.** Implement the all-channels mode first, then pin the complete gesture/apply path in an e2e kittest. Current v1 reads only the active channel (`crates/umber-app/src/main.rs`, `uv_view.rs`; roadmap row 335).
5. **Assets browser mesh category.** Add the mesh outcome, thumbnail kind, category, loader, and apply flow. This is the only surviving R-C1 category follow-up (`crates/umber-app/src/assets_panel.rs`; roadmap row 333).
6. **OccluderScope panel threading kittest.** Pin combo → request → resolution at `crates/umber-app/src/bakes_panel.rs`. The separate `Sets(BTreeSet)` set-list picker stays deferred.
7. **XI2 hotplug re-query.** Re-query device mappings on `XI_HierarchyChanged`; current mappings are snapshot-only (`crates/stylus/src/xi2.rs:61-63`).
8. **Bounded vector runway.** Brief secondary-contour canvas editing before compound-specific panel affordances. Alternatively take one measured SVG slice, never the whole CSS backlog: strict Unicode/NBSP handling; named colors; alpha functions/`rgba`; `currentColor`; CSS4/non-RGB conversion; stylesheets/selectors/classes; paint servers; root/group `vector-effect`; source-style preservation. Each needs its own contract and typed refusals. Add one pointer-driven GUI import smoke to close the K8h/K8i validation gap; it is not another feature kill.
9. **Finish the live control-channel smoke tail.** `desktop-checks.md` check 5 has a successful headless partial. Complete wrong-token and MCP bridge read-back against a current build, then fill its Result line without claiming the four hardware/desktop checks.
10. **Verify and scope the row-156 idea backlog before scheduling it.** These are recorded candidates, not committed release requirements: EyeDepth camera-visibility stroke masking; COW sparse-tile undo evaluation; a generated parity table; a broader theme matrix; AppImage zsync/deb/rpm/windows-arm64/FreeBSD/packaging-lint deltas; and glb/obj mesh-export round-trip coverage. Each needs verify-open evidence, a brief, and a cost/dependency decision. UV G-buffer and hostile-input work from the same old row are already landed and must not be repeated.

## C. Deferred by explicit decision — do not rediscover as neglect

- **Projection product calls:** seam/symmetry expansion, a dedicated decal channel, and the projection bake family need an artist-facing decision. Depth/occlusion v3 itself already landed in `b228767f` and is not open engineering.
- **Painting boundaries:** camera-aligned stamping and retouch seam-mirroring remain documented v1 boundaries.
- **Masks and PSD v2:** file-backed bitmap-mask import and full mask stacks/effects remain follow-ups; imported PSD layers mapping to texture-set layers remains v2, while flatten-import is intentionally shipped.
- **OccluderScope `Sets`:** the set-list picker is deferred; WholeMesh/OwnSet are shipped and only the panel threading kittest remains in section B.
- **Picker extras:** the inline eyedropper and Linear/Perceptual readout toggle are separate G1 deferrals, not part of L7.
- **Dynamic materials and custom shaders:** dynamic-material v1 is landed (`f85cb0aa`, `2c6497ec`), but full multi-entry shader-declared sub-stacks remain outside v1. Custom user WGSL shaders (`requirements.md:86`) remain an unbuilt P2 wave-6 design track, not a v0.1 blocker.
- **`.spp` staged fidelity R1–R4:** full-precision channels, assbin mesh, ResourceImage decode, and HBO schema remain post-v0.1 fidelity work awaiting a specific priority call. f16 preview decode already exists and must not be described as absent; full-precision planes are the actual R1 gap. `.spp` import itself is shipped and corpus-gated.
- **AI texturing delivery:** the provider interface is landed. Actual model/host binding and inference are deliberately post-0.1.
- **Snapshot expansion:** remaining bin-only surfaces need a deliberate promotion decision; GPU-shaped surfaces need the GPU harness.
- **U5 docking migration:** only the egui 0.37/egui_dock migration remains. The icon rail and collapsible columns already landed through U7 `1fc6ca12` and `89249567`; GUI 0b shell chrome is not open.
- **Other scoped vector boundaries:** CSS transforms/variables, boolean path operations, schema-level arc preservation, raster color/style semantics, broader effects/unit expansion, and source-spelling preservation remain separate design decisions, not a single catch-all kill.

## D. Externally gated — no derivation until the gate opens

- **Rendering upgrades:** hardware tessellation, the real HDR surface switch, ray query, and WASM shader work wait on the required eframe/wgpu capabilities. The displacement preview and HDR capability probe are already landed and are not to be reimplemented.
- **Windows stylus integration:** the live WndProc/HWND hook, Wintab32 polling, and real-pen validation need a Windows session and pen. Passive WinInk decode and Linux XI2/raw-Touch pressure are not substitutes for that gate. XI2 display sharing also needs an eframe raw-handle hook unavailable in 0.36; the second connection is the shipped v1 boundary.
- **Desktop/hardware checks:** `desktop-checks.md` checks 1–3 need the real desktop input path; check 4 needs the Windows pen. Check 5's full Result remains unfilled despite the partial headless evidence. XI2's real-device open/poll/valuator path is separately unverified. Kittest or headless evidence does not close these hardware checks.
- **ABR v8/v11:** stay refused as unattested until a public sample establishes the format; “descriptor-only” is not a header-version classification.

## E. Partner and release gates — queued, not self-opened

1. **Dogfood C2-24:** run the prepared two-material prop through first-run, painting, baking, save/reopen, and export on the published alpha, with real observations and output hashes. Use the alpha only for its actual feature set or a newer explicit build for post-alpha D2–D5/displacement; do not claim the alpha covers those commits.
2. **Answer product questions:** projection calls above, plus the dogfood P1–P12 decisions on cage defaults/UX, ID source gating/blend defaults, ground-plane defaults/units/fit, rebake debounce, bake-log retention, margin default, displacement range/reset, bundled sample mesh, and baked-map export-preset inclusion.
3. **Resolve dogfood findings:** every reproduced failure gets a brief, lock, fix, regression coverage where automatable, and a kill report.
4. **Native-speaker pt-BR review:** review translation quality after string freeze, not only key parity.
5. **Windows signing C2-22:** choose and document the signing route; the existing alpha remains deliberately unsigned with checksums.
6. **Beta/RC/final release:** run `scripts/release-ci.sh` on the intended tip, verify every job and published artifact by read-back, observe the protocol/schema/string freezes, and leave the version bump, Beta/RC phase calls, and tags to Bessa.
7. **Upstream submissions:** local drafts may be maintained, but every public PR remains subject to the public-posting gate.
8. **Production Flatpak/AppImage publication:** remains final-release work, not an autonomous packaging side effect.

## F. Permanent walls and non-goals

- No `.spp` writing, `.sbsar` execution, Adobe SDK dependency, fake interchange, cloud accounts, or telemetry.
- Mesh modeling/sculpting is outside the v0.1 horizon; macOS is contributor-driven later work; real-time collaboration is research-only; MSI was dropped by C2-19.
- The path-tool ribbon UI is an explicit non-goal: “No. Never.”

## Execution record

- 2026-11-23: L7 brief `12471890`, claim `b391649b`, and implementation `eb2237a917e6697f990ffe8cb1da09e57f517029` landed locally under `razul-picker-delegate`. The delegate removed 72 duplicated lines and added one behavior regression. R1's test-filter finding was fixed and the exact focused command ran 2/2; strict clippy and the full crate 151/151 are green; R2 `deleg_2c79a7b1` returned SHIP with zero findings. Final report/queue hashes, safe-push, and origin verification remain at this writing. PerChannel scalar-mode K1 is next.
- 2026-11-24: PerChannel K1 brief `27c11e66`, claim `96f4a53a`, and implementation `4383e30393b10a84a74dca3dfa5eabad7703334a` landed locally under `razul-perchannel-k1`. RED roughness 28→25 reproduced, exact test 1/1 in both targets, strict app clippy clean, full app 1,260/0. First reviewer `deleg_14d8487c` interrupted without a verdict; replacement `deleg_52c65481` returned SHIP with zero findings. Final report/queue hashes, safe-push, and origin verification remain at this writing. Bake/Export history `record_context` is next.
