# Kill report — exe-relative bundled runtime assets (installers gap #2, row 261)

- Date: 2026-11-07 ~09:3x -03.
- Authorship: code written by Razul session 20261106_175041_ccfbc2
  (2026-11-06 18:17-18:27, lock `razul-bundled-assets`); that session ended
  2026-11-06 ~21:24 with the work uncommitted and no report. Adopted,
  fixed forward, verified and landed by a Razul subagent (isolated worktree
  `subagent-sa-0-3a6c2216`).
- Kill-lock: `razul-bundled-assets-adopt` — crates/umber-core/src/bundled.rs,
  crates/umber-core/src/lib.rs, crates/umber-gui/Cargo.toml,
  crates/umber-gui/src/{icons,fonts}.rs,
  crates/umber-app/src/{env,document,i18n,assets_panel,brush_panel,plugins,main}.rs,
  dist-workspace.toml, Cargo.lock, docs/loop/queue.md,
  .dragon/COORDINATION-next-wave.md, this report.
- Adoption evidence (re-verified 09:32 -03 before any edit): state.db
  (read-only) sessions.ended_at=1794011098 for the holder, last message
  ts 1794010711; `ps` showed no process from it (the only live cargo was
  in ~/Projects/Learning-AI); dirty-file mtimes 18:19-18:27 Nov 6; no
  other kill locks; `git status --porcelain` matched the lock list plus
  Cargo.lock (changed by the lane, never listed; added to the new lock).
- Gap closed: installers smoke (row 240, `2026-11-06-installers-smoke.md`)
  gap #2 — runtime lookups were `CARGO_MANIFEST_DIR` joins or
  CWD-relative, so a clean install found no icon atlas, env maps, starter
  library or examples.

## Built

- `umber_core::bundled` (new): `data_paths(rel)` lists every existing
  location of a repo-root-relative path, best first, over
  `$UMBER_ASSETS_DIR`, `<exe>/../share/umber`, `<exe dir>`, the
  compile-time repo root, then the CWD. `data_path(rel)` returns the best
  one, or the dev repo join when none exists (dev behaviour and error
  messages unchanged). `data_root()` names the winning root for logging.
- umber-gui: `fonts_dir()` and `atlas_dir()` fall back to `data_path`
  after their existing env overrides; umber-gui gains the umber-core
  dependency (L5 -> L0, layers GREEN).
- umber-app: `default_env_candidates`, `env_dirs_with`, `smart_dirs_with`,
  `catalog_dirs`, `starter_dir`, `BrushPanel` starter dir and
  `dev_plugin_dir` resolve through the bundled lookup, each keeping its
  old manifest/CWD join as the fallback when nothing is found. Extras
  dedup now also compares canonical paths. `main` logs
  `bundled data root: <path>` at startup.
- dist-workspace.toml: `assets` and `examples` ship in every archive
  beside the binary (the `<exe dir>` candidate).

## Judgment calls

- Fixed forward (adopter): `data_paths` now drops entries naming the same
  real directory. Under `cargo run` from the checkout the manifest root
  and the CWD are one dir, and the adopted code listed it twice, so every
  directory scan (env maps, smart materials, i18n catalogs) would have
  read the starter set twice. New pin
  `bundled::tests::same_dir_via_two_roots_is_listed_once`.
- Re-pinned (adopter): `share_layout_beats_flat_exe_dir` compared the
  lexical candidate `bin/../share/umber/...` with `share/umber/...` and
  failed on its first run. It now compares canonical paths and the file
  bytes.
- Kept the manifest-root candidate ahead of the CWD: an exe built from
  this tree and run elsewhere still finds its own source tree's assets
  first, matching the old compile-time join.

## Verification

- rustfmt --check clean on every touched .rs; check-layers GREEN
  (16 crates); scorecard current.
- `cargo clippy -p umber-core -p umber-gui -p umber-app --all-targets
  -- -D warnings`: clean.
- `cargo test -p umber-core` 197/0, `-p umber-gui` 148/0,
  `-p umber-app`: 253+756+39 integration tests, 0 failed.

## Unverified

- No packaged-archive smoke was run: the dist build + clean-prefix
  install from row 240 was not repeated, so the archive layout
  (`assets/` beside the binary) is proven by the unit pins, not by an
  installed binary. The `<exe>/../share/umber` layout has no packager
  that produces it yet.

## Deferred

- Re-run the installers smoke (row 240 recipe) against a dist build to
  confirm gap #2 closed on a real install; remaining gaps #3-#5 from
  row 240 stay open. The release tag stays partner-gated.
