# Kill report: upstream-reports (2026-10-15)

- Author: subagent. Writing and research only. No code changed, nothing filed upstream.
- Lock `upstream-reports`: `docs/upstream/` (new), this report.
- Source: the 2026-10-14 mesh-panic-hunt dependency findings #9–#24, and the 2026-10-15 mesh-hardening "Still open" list.

## Built

- `docs/upstream/README.md`: index with tracker, counts and status per crate.
- `docs/upstream/gltf-1.4.1.md`: status table for #9–#16, a shared copy-pasteable repro program, one new-issue draft, and two comment drafts for existing issues.
- `docs/upstream/tobj-4.0.5.md`: one new-issue draft covering #17–#18.
- `docs/upstream/openusd-0.7.0.md`: a no-file record. All findings are fixed in 0.8.0. The repro is kept.

| crate | tracker (verified via crates.io API `repository` + crate `Cargo.toml`) | findings | drafted new | duplicate / comment on existing | fixed upstream |
|---|---|---|---|---|---|
| gltf + gltf-json 1.4.1 | https://github.com/gltf-rs/gltf/issues | 8 | 1 (#11 short `byteStride`) | #14 already reported as gltf-rs/gltf#478. #12, #13, #15 drafted as a comment on #478. #10 drafted as a comment on #479 | #9, #16 on `main` (PR #471, merged 2026-05-11). **No release contains it**: 1.4.1 (2024-05-10) is still the latest |
| tobj 4.0.5 | https://github.com/Twinklebear/tobj/issues | 2 | 1 (covers both) | none (closest: #18, closed 2020, positive `v` index) | no: 4.0.5 is the latest, and `master` @ 7cc3123 still panics |
| openusd 0.7.0 | https://github.com/mxpv/openusd/issues | 6 | 0 | none as issues | **all 6 fixed in 0.8.0** (2026-10-11) by PR mxpv/openusd#138 |

Every reproducer was executed, not only read. A scratch probe outside the repo ran each finding's literal input against the pinned version and against upstream HEAD or the newest release, in separate processes, in debug and release:

- gltf 1.4.1: all 8 panic at the kill report's sites. On `main` @ 50d6522: #9 → `Ok`, #16 → `Err(Index out of bounds)`, and #10–#15 still panic (sites moved to `util.rs:403/408/419`).
- tobj 4.0.5 and `master`: both panic in debug, in the single- and multi-index paths (4.0.5 `lib.rs:852/860/1055/1096`).
- openusd 0.7.0: all 13 flip offsets reproduce. 36/104/188/389 abort with exit 134 (1–13 TB allocations). The other 9 panic at the listed sites. On 0.8.0 all 13 return `Err` or a field-level `Err`.

New facts that correct or sharpen the 2026-10-14 report:

- **#11 is not debug-only.** In release, `byteStride: 4` panics at gltf 1.4.1 `accessor/util.rs:266` (`assert!(slice.len() >= 3 * size_of::<T>())` in `<[T;3] as Item>::from_slice`). Umber's `validate_accessors` already rejects it.
- #9, #10 in release: no panic, and `read_positions()` silently returns an empty iterator. #12 and #13 in release: wrong-layout data, or `None`, with no error.
- #17/#18 in release: small out-of-range magnitudes give a typed `Err`. A chosen magnitude (`f 1//-6148914691236517207`) wraps `vn * 3` to a valid offset and loads the wrong normal with `Ok`. That is silent data corruption, and it is in the tobj draft.

## Unverified

- Duplicate searches covered GitHub issues and PRs by keyword (`gh search issues/prs`, plus full `gh issue list` for tobj and openusd). Discussions, forks and private security reports were not searched.
- gltf `main` was tested at commit 50d6522. Whether the maintainers plan a 1.4.2 is unknown. #465 asks for one and has no maintainer reply.
- openusd 0.8.0 was tested only on the 0.7.0-written flips via `CrateData::open(_, true)` + `try_field`. Umber's own `usdc.rs` was not compiled against 0.8.0, and the bump's API impact beyond the `CrateData::open` signature change is not measured.
- Release-mode results are from x86_64 Linux, rustc stable from 2026-06 (cargo 1.97.0).

## Deferred

- **Filing or commenting upstream.** That needs the repo owner's explicit approval. The drafts are ready: gltf Draft A plus the comments on #478 and #479, and the tobj issue. The gltf comments are written in a "more cases in the same class" voice. #478 and #479 were filed by another fuzzing reporter (Lynthar) in 2026-09.
- Bumping `openusd = "0.7.0"` → `0.8.0` in the workspace `Cargo.toml`, then un-ignoring `bug_usdc_huge_count_allocation_aborts`. This is outside the lock. It is the one fix that removes the uncatchable ABORT (#19).
