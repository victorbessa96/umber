# Kill report: flatpak offline deps (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, wakeup #3). Kill-lock
`flatpak-offline-deps`, holding: `flatpak/org.umber.Umber.json`,
`flatpak/gen-cargo-sources.py`, `flatpak/vendor-checksums.py`,
`.github/workflows/packaging.yml` (plus the template
`flatpak/org.umber.Umber.json.in` and the README tick riding the same
commit row).

Context: CI packaging run 38883955783 (2026-10-16) — the Flatpak job
red, root-caused LIVE this wakeup from the failed log pulled via `gh`:
the `umber` module's `cargo --locked build` runs inside
flatpak-builder's no-network sandbox; `Updating crates.io index` →
"Could not resolve host: index.crates.io" → exit 101. The flatpak
README recorded the deferred item itself ("NOT hermetic/offline; a
Flathub submission would need vendored sources"). This kill closes it.

## Built

- `flatpak/gen-cargo-sources.py` — Cargo.lock (v3/v4) → one
  flatpak-builder `type: archive` source per registry crate
  (`url = https://static.crates.io/crates/<n>/<n>-<v>.crate`,
  `sha256 = ` the lock's own checksum, `dest = vendor`), spliced into
  the template's `@@CARGO_SOURCES@@` slot. Zero network to generate:
  the lock's checksum IS the `.crate` archive hash. `--check` mode =
  CI's drift gate (regenerate must equal the committed manifest).
  Workspace members (no checksum, no source) are skipped — the git
  source IS the workspace; a non-registry `source` or a checksum-less
  registry package is a loud refusal, never a silent drop.
- `flatpak/org.umber.Umber.json.in` — the hand-maintained template
  (app metadata, finish-args, module, build-commands), gaining: an
  inline `.cargo/config.toml` source (cargo's documented
  source-replacement: crates.io → `vendored-sources` → `vendor/`),
  `build-options.env.CARGO_HOME` inside the build dir, and
  build-commands running `python3 flatpak/vendor-checksums.py` then
  `cargo --offline --locked build --release -p umber-app`.
- `flatpak/org.umber.Umber.json` — the GENERATED manifest, committed
  (3799 lines, 621 crate sources). CI's existing json.tool +
  `flatpak-builder --show-manifest` gates validate it; the new lint
  step re-runs the generator and fails on drift.
- `flatpak/vendor-checksums.py` — build-command #1, inside the
  sandbox: for every staged `vendor/<name>-<version>/` it writes
  `.cargo-checksum.json` exactly like `cargo vendor` (a full files
  map over the extracted tree + the lock's package checksum), which
  is the contract cargo's source-replacement consumer requires.
  Idempotent; refuses on vendor/↔Cargo.lock desync.
- `.github/workflows/packaging.yml` — the lint job gains the drift
  gate right after the JSON-parse step.
- `flatpak/README.md` — the deferred-item paragraph becomes the
  landed hermetic shape; the Files table gains the generator, the
  template, and the prep script.

## Judgment calls

1. Vendored-source replacement over registry-cache staging: the
   source-replacement layout is cargo's DOCUMENTED offline-vendor
   contract (no registry dir-hash suffixes, no index questions); the
   cache-staging alternative depends on cargo-internal layout details.
2. `.cargo-checksum.json` written at BUILD time, not committed: the
   alternative (shipping the checksum files inside the repo) would
   mean a ~621-file tree churn on every dependency bump; the prep
   script is 60 lines over data already staged.
3. The manifest stays a committed artifact (not built in CI on the
   fly): CI's manifest-shaped gates (json.tool, --show-manifest,
   desktop/appstream validation against it) and Flathub review both
   want a reviewable file — with the drift gate enforcing
   regeneration.
4. `--offline` AND `--locked`: the replacement removes the network
   need, `--offline` makes any accidental regression of that a hard
   error instead of a wedged build.

## Verification (the unusual part — LIVE, this wakeup)

- The failed run's log pulled via `gh run view 38883955783
  --log-failed`: the root cause is quoted verbatim in the README
  paragraph (DNS failure at the crates.io index fetch inside the
  sandbox).
- The load-bearing fact verified against the REAL CDN:
  `ab_glyph-0.2.32.crate` downloaded from the exact URL shape the
  generator emits; its sha256
  (`01c0457472c38ea5bd1c3b5ada5e368271cb550be7a4ca4a0b4634e9913f6cc2`)
  equals the lock's checksum byte-for-byte. The URL layout and the
  checksum contract are fact, not recollection.
- Generator output cross-checked against Cargo.lock: 621 registry
  packages ↔ 621 manifest entries, key sets equal, every checksum
  carried verbatim.
- `python3 -m json.tool` on the generated manifest: VALID; the
  generator's `--check` re-run: CLEAN (idempotent).

## Unverified (the honest boundary)

- No flatpak-builder run (needs the 24.08 runtime/SDK install — a CI
  host concern, not this machine). The remaining risk concentrates
  in exactly two sandbox behaviors: (a) flatpak-builder accepting 621
  archive sources + extracting `.crate` gzipped tars into `vendor/`
  root dirs, (b) cargo consuming the replacement config + checksum
  files offline. Both follow their documented contracts; the real
  gate is the packaging workflow, re-dispatchable on the tip after
  the recovery push (`workflow_dispatch`), exactly like the
  perf-nightly re-dispatch.
- The Flatpak job also still builds with the rust-stable SDK
  extension toolchain, not the repo-pinned 1.97.0 (the README's
  standing honest note — unchanged by this kill).

State: STATICALLY + LIVE-CDN VERIFIED, IN FLIGHT — the packaging
workflow re-dispatch after the recovery push is the verdict gate;
no battery scope of its own (packaging is CI-proven, not
suite-proven). Commits in the same recovery session as the parked
kills, its own commit row per the manifest labels.
