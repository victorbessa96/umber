# Umber next steps — reconciled 2026-11-17

This is the current, executable list after the alpha release and the post-alpha
baker-power wave. It replaces the original 2026-11-16 ordering, whose D2–D5
and displacement items are now landed. Git history, the published release,
and committed kill reports outrank this summary.

## Completed since the original list

- Docs truthing A.1–A.3 landed in `7031b5d7`.
- D2 auto-cage (`533560e8`), D3 opacity (`c499255f`), D4 multi-source ID
  (`35097df5`), D5 ground plane (`752bc991`), and displacement preview
  (`9e0517bf`) landed. D5's cross-review HIGH findings were closed in
  `3b72c98f` and `baec964b`.
- The replay-exact default-off decision is implemented as an opt-in toggle
  (`aa1f5903`).
- Bessa ratified SPEC v1.1 and published unsigned pre-release
  `v0.1.0-alpha.1` from `bc4d642f`; the workspace then returned to `0.1.0`
  and Flatpak was pinned to that tag (`d49d0acc`).

## A. Release hardening and human gates

1. **Dogfood C2-24.** Run the prepared first-run walkthrough against the
   published alpha with a two-material prop. Capture every failure, visual
   artifact, output filename, and export round-trip result in the session log.
   This is the immediate release-quality gate.
2. **Resolve dogfood findings.** Every reproduced failure gets a brief, lock,
   fix, regression test where automatable, and a kill report. No beta or stable
   tag before this closes.
3. **pt-BR native-speaker pass.** Review user-visible Portuguese rather than
   merely checking key parity. Keep source and translation edits paired.
4. **Windows signing decision, C2-22.** Alpha is intentionally unsigned with
   checksums. Select and document a signing route before the first signed
   release; do not retroactively claim the alpha was signed.
5. **Release verification.** Before a beta or stable version bump, run
   `scripts/release-ci.sh` on the exact tip, verify the resulting GitHub
   release object and artifacts by read-back, and record size/checksum results.

## B. Post-alpha engineering hardening

1. **D2 auto-cage contract alignment.** The cross-review found the flat-plane
   Inflate test and brief disagree about ray origin/offset semantics. Resolve
   the contract deliberately, then align implementation, test, and docs. Do
   not silently weaken the test.
2. **D3 opacity parity.** The plane shader path uses a fixed origin plane while
   the intended AO-style `PlaneDesc` path is broader. Either implement the
   full plane contract and direct mesh-vs-plane parity test or narrow the
   public contract to the supported mesh-fed route with a documented reason.
3. **D4 persistence completeness.** Preserve every user-authored ID Multi
   source field, including mask paths and manual colours, rather than keeping
   lossy TODO-based serialization. Add a save/reopen regression test.
4. **GPU-environment portability.** The binding-array feature failure on
   lavapipe is an environment capability boundary. Add explicit capability
   detection or a portable fallback before treating D4's GPU path as universal.

## C. Explicitly deferred or externally gated

- **Live Windows Ink and real-pen validation:** passive decode exists, but the
  WndProc/HWND integration and a real Windows pen session are still required.
- **Rendering upgrade postures:** WASM shaders, hardware tessellation, HDR
  surface output, and ray query wait on their eframe/wgpu upgrade path.
- **`.spp` staged fidelity lanes:** import-only remains in scope; writing is
  permanently forbidden. `.sbsar` execution is permanently forbidden.
- **Projection product calls:** seam/symmetry expansion, a decal channel, and
  the bake family need an artist-facing product decision before derivation.
- **Upstream bugs:** prepare locally, but the public-PR gate applies before
  any upstream submission.

## Execution record

- 2026-11-17: this reconciliation replaced completed D2–D5/displacement work
  with the actual release and hardening queue. It makes no claim that dogfood,
  native-speaker review, signing, beta, or stable release have occurred.
