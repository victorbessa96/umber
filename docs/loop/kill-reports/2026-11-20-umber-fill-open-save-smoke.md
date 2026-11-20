# Verification report: umber-fill-open-save-smoke (2026-11-20)

No code changes. This closes the native open-project smoke deferred from the
Fill cache-lifecycle kill. Base and verification commit: `4abfb021`.

## Real execution

- Built the release app with `cargo build -p umber-app --release` under the repo
  flock and slow envelope.
- Seeded a minimal one-set `.umber` project, then used the debug CLI agent to
  `project_open`, `layer_add` one Fill, and `project_save`.
- Drove the release app over `--control`: `project.open`, `mesh.load` with the
  cube fixture, export, `file.save_project`, reopen with `project.open`, reload
  mesh, and export again.
- The baseColor export hashes were byte-identical across save/open/reopen:
  `d3087d6855c1f80d1862b9fa85817cbfb1f89a00d7a42e40c21b426f635cf66e`.
- Result: `OPEN_SAVE_EXPORT_IDENTICAL=PASS`.

Evidence directory: `~/.hermes/cache/scratch/umber-fill-open-1qy8t_cv`.
Script: `~/.hermes/cache/scratch/fill-open-save-smoke.py`.

## Meaning

The lifecycle path now has a real app-level check: a saved Fill can be opened,
reopened, and exported without double-counting or changing pixels. The headless
unit tests cover cache identity, dormant observation, open seeding, pruning,
and retry; this smoke covers the native save/open path end to end.

## Unverified

No Windows/macOS run and no human UI interaction. This was the Linux release
app on the local machine, not a cross-platform release gate.
