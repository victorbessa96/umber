# Kill report — 2026-11-14 w5 fuzz-corpus (b) (wave-fuzz-corpus (b), queue row 386)

**Kill:** wave-fuzz-corpus (b) — cargo-fuzz + nightly + bounded corpus budget over
the importer seams, per `docs/loop/waves/2026-11-09-wave-plan.md` line 184 and
row 385's dispatch. Dispatched subagent on branch
`hermes-subagent/subagent-sa-0-fd974512` (worktree isolation: commit lands on the
branch, parent ff-merges to master). **Kill lock** `razul-fuzz-corpus-b` over
`fuzz`, `Cargo.toml`, `docs/loop/queue.md`, `.dragon/COORDINATION-next-wave.md`,
this report; removed in the same step as the commit via `dcommit --kill-lock`.
No crate sources were edited — findings below are PROPOSED kill rows.

## Built (wiring only, docs+fuzz, no `crates/` changes)

- Root `Cargo.toml`: `exclude = ["examples", "fuzz"]` so the root workspace never
  builds the fuzz crate.
- `fuzz/Cargo.toml` — own cargo-fuzz workspace (libfuzzer-sys 0.4, path deps
  `umber-mesh` + `umber-spp`), bins `mesh_load` + `spp_open`. Carries its own
  `[workspace]` table because in subagent worktrees the crate lives under the
  hidden `.worktrees/` dir, which cargo's workspace walk-up skips (it resolved
  the MAIN root and refused the build until the explicit `[workspace]` root).
  `fuzz/Cargo.lock` committed for reproducible resolution.
- `fuzz/fuzz_targets/mesh_load.rs` — byte 0 selects one of 13 lanes: the `load`
  dispatcher once per format extension plus each direct pub entry
  (`load_obj`, `gltf::load_gltf`, `fbx::load_fbx`, `usd::load_usda`,
  `usdc::load_usdc`, `usdc::load_usdz`) — the same matrix as
  `crates/umber-mesh/tests/hostile_corpus.rs`. The rest of the input is written
  to a temp file with the lane's extension and fed to the entry. The target
  replaces libfuzzer-sys's abort-on-panic hook with a printing one, so
  umber-mesh's own `catch_unwind` boundaries (`load`, `load_gltf_with_parts`,
  `usd`) keep working: a crash artifact therefore means exactly "a pub entry
  panics on hostile input with NO boundary catching it".
- `fuzz/fuzz_targets/spp_open.rs` — input written to a temp `.spp`, fed to
  `umber_spp::open` (same hook override).
- `fuzz/corpus/mesh_load/` — 94 seeds: each of the 47 hostile-corpus files from
  `crates/umber-mesh/tests/corpus/` (row 156-A, incl. the 6 kill-2 regression
  mutants) prefixed with the lane byte for its format's `load` lane and direct
  lane. `fuzz/corpus/spp_open/` — 4 synthetic seeds (HDF5 magic shapes; the real
  `.spp` oracle files under `~/corpora/umber/spp/` are NEVER committed per
  `crates/spp/tests/corpus.rs`). `fuzz/README.md` documents layout + run
  commands.

## Toolchain + runs (minihost, local cargo under `flock /tmp/umber-cargo.lock`,
`CARGO_BUILD_JOBS=3`, `nice 19`; the fuzzer binary itself runs without the lock)

- `rustc 1.102.0-nightly (8eb778d04 2026-11-12)` (rust-src present),
  `cargo-fuzz 0.13.2`, `libfuzzer-sys 0.4.13` (registry cache), ASan+sancov
  release build via `cargo +nightly fuzz build` — green in 7m28s cold.
- `mesh_load`: `-max_total_time=2400 -max_len=65536 -rss_limit_mb=3072
  -malloc_limit_mb=3072 -ignore_*`, seed corpus 94. **7,496,749 execs, 3,122
  exec/s avg, peak rss 602 MB, final cov 10,143 edges / 29,022 features,
  in-session corpus grew 94 → 5,190 units. 0 crashes, 0 OOMs, 0 timeouts.**
  3,207 panics were caught by umber-mesh's own boundaries (all inside the gltf
  stack: `gltf-json-1.4.1/src/mesh.rs:151:43` ×3,137 and
  `gltf-1.4.1/src/image.rs:128:46` ×70) → typed `Err` in production; the
  always-on `hostile_corpus` suite confirms the boundary (see Verification).
- `spp_open`: 240 s single run (1,618,598 execs, 9,042 exec/s, peak rss 525 MB)
  + 180 s `-fork=2 -ignore_crashes=1` run → **10 crash artifacts, all ONE bug
  class** (see Findings).

## Findings (proposed kill rows — NOT fixed here, `crates/` out of scope)

1. **HDF5 link-message length arithmetic overflow — `crates/spp/src/hdf5.rs:1956`
   (`if payload.len() < cursor + name_len`)**. `name_len` is read from the file
   (`uint_le(...) as usize`, up to u64::MAX) and added to `cursor` unchecked:
   debug-assert builds panic "attempt to add with overflow"; release builds wrap
   and then panic on the slice range `payload[cursor..cursor + name_len]`. Either
   way `umber_spp::open` has NO `catch_unwind` boundary (unlike umber-mesh's
   entries), so a hostile `.spp` (~120–320 bytes, HDF5 magic) panics the caller —
   app crash on File → Open. Bug class: unchecked arithmetic / missing bounds
   check in link-name parsing + missing panic boundary at the spp entry.
   Severity: HIGH (remote-less but user-triggerable panic/DoS on open).
   Artifacts: `fuzz/artifacts/crash-*` (10 files, all this site), e.g.
   `crash-7b620a13ebfebe4040768f221a7d4dde5cf69776` (183 B). Repro:
   `cd fuzz && cargo +nightly fuzz repro spp_open artifacts/crash-7b620a13ebfebe4040768f221a7d4dde5cf69776`.
   Proposed kill row: checked arithmetic (`cursor.checked_add(name_len)` /
   bound `name_len <= payload.len() - cursor`) in `hdf5.rs` link parsing +
   `catch_unwind` boundary at `umber_spp::open` mirroring umber-mesh + one
   regression corpus file per artifact.
2. **No uncaught panics in umber-mesh importer seams** — 7.5M execs over the
   13-lane matrix, zero crash artifacts. The only panics seen are inside the
   gltf dependency and are caught by `load_gltf_with_parts`'s boundary.

## Verification

- `rustfmt --edition 2021 --check` on both fuzz targets: clean.
- `python3 scripts/check-layers.py`: GREEN (16 crates).
- `python3 scripts/scorecard.py --check`: current (no README regen needed —
  no tests added; README untouched).
- `cargo test -p umber-mesh --test hostile_corpus` (warm shared target,
  `UMBER_ASSETS_DIR` set): **2 passed, 0 failed** — sanity that nothing broke.
- No `crates/` file touched, so no crate clippy leg applies.
- Fuzz build itself is the compile check of the wiring (`cargo +nightly fuzz
  build` green).

## Process notes / deviations

- Committed on the subagent branch per worktree isolation (parent ff-merges);
  push is `git push origin hermes-subagent/subagent-sa-0-fd974512`, verified by
  `git ls-remote origin` — NOT a master push from this claw.
- A sibling k6 battery held `/tmp/umber-cargo.lock` ~14 min at scout; the first
  `flock -w 900` build window expired in the race and was relaunched — this
  pushed the session past the ~100 min timebox, so the mesh run was bounded at
  2400 s instead of 3600 s.
- The fuzz runs grew the working corpus in place (mesh 94 → 5,190 units / ~2 MB,
  spp → 190 units); only the 94+4 regenerated seeds are committed — the grown
  corpus is reproducible by re-running and stays out of git.
- `fuzz/target/` (build output) and `fuzz/artifacts/` (crash artifacts) stay on
  disk uncommitted; `.gitignore` is outside this kill's lock and was NOT
  touched.
- Lock `razul-fuzz-corpus-b` removed in the same step as the commit.

## Open

- Fix kill for finding 1 (spp hdf5.rs:1956 + `open` boundary) — proposed row
  above; dispatch next.
- spp fork run found no second site in 180 s forked; a longer spp budget and a
  mesh budget >2400 s remain open if the wave wants deeper coverage.
- `.gitignore` entries for `fuzz/target/` + `fuzz/artifacts/` deferred to a
  kill that owns `.gitignore`.
