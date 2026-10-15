# Upstream bug-report drafts

These are drafts for the dependency panics Umber's mesh panic hunt found (`docs/loop/kill-reports/2026-10-14-mesh-panic-hunt.md`, findings #9–#24). **Nothing here has been filed.** Public posting needs the repo owner's go-ahead. Umber currently guards these with pre-validation (`umber-mesh/src/gltf.rs`, `validate_accessors`) and a `catch_unwind` boundary in `umber_mesh::load`. See `docs/loop/kill-reports/2026-10-15-mesh-hardening.md`.

Every reproducer was re-run on 2026-10-15 against the pinned version and against upstream HEAD or the newest release.

| crate | tracker | findings | drafted (new) | already reported upstream | fixed upstream | draft |
|---|---|---|---|---|---|---|
| gltf / gltf-json 1.4.1 (latest release) | https://github.com/gltf-rs/gltf/issues | #9–#16 | 1 new issue (#11, short `byteStride`); comments for #479 (#10) and #478 (#12, #13, #15) | #14 → [#478](https://github.com/gltf-rs/gltf/issues/478) | #9, #16 fixed on `main` by PR #471, **unreleased** | [gltf-1.4.1.md](gltf-1.4.1.md) |
| tobj 4.0.5 (latest release) | https://github.com/Twinklebear/tobj/issues | #17–#18 | 1 new issue (covers both) | none | no (still on `master` @ 7cc3123) | [tobj-4.0.5.md](tobj-4.0.5.md) |
| openusd 0.7.0 | https://github.com/mxpv/openusd/issues | #19–#24 | 0 | none | **all six fixed in 0.8.0** (PR #138) | [openusd-0.7.0.md](openusd-0.7.0.md) |

Follow-ups owed in Umber (outside this lock):

- Bump `openusd` 0.7.0 → 0.8.0. This clears the uncatchable allocation ABORT (#19), so `bug_usdc_huge_count_allocation_aborts` can go live. The API changed: `CrateData::open` now takes `impl Into<ar::AssetBuffer>`.
- gltf: keep `validate_accessors`. Even an eventual 1.4.2 would still have #10–#15.
