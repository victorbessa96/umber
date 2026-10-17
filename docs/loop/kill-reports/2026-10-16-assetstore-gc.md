# Kill report: assetstore-gc (standing debt)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `assetstore-gc`: `crates/umber-core/src/assets.rs`.
- Closes the ROADMAP standing debt "AssetStore GC (§8's v1 boundary:
  sidecars authoritative, no store GC)" — the v1 boundary HOLDS: the
  GC never touches `paint/` sidecars, only content-addressed blobs
  under `assets/` whose sole reference lives in the manifest.

## Built

- `AssetStore::collect_garbage(live, prune) -> GcReport`:
  - Walks the shard dirs; a blob `<hash>.<ext>` is live iff in `live`
    (the manifest's fresh set, passed by the caller after a save
    commit), else orphan.
  - A crashed `store()` rename leftover (`<hash>.tmp.<pid>`) or a
    foreign/unparsable file always counts tmp-or-unparsable and is
    prunable whatever the live set says.
  - Report-only by DEFAULT (`prune: false` counts, deletes nothing);
    `prune: true` unlinks orphans + tmps and removes shard dirs the
    pass itself emptied (never `assets/`).
  - An absent store (no root dir) reports all-zero — loading a
    project with no assets GCs cleanly.
- `GcReport { live, orphan, orphan_bytes, tmp_or_unparsable,
  shards_removed }` — the report line's data.
- Tests (3): report-only counts and TOUCHES NOTHING (live + orphan +
  hand-placed crashed-tmp all still present after); prune keeps the
  live blob, deletes the orphan, works on a real shard layout;
  absent-store → zero report.

## Judgment calls

- **Report-only default, prune opt-in**: the debt's first landing is
  observability — a caller (the shell's save path, queued on the
  main.rs lock) reports first, prunes only on an explicit action.
- **Hash+ext as the live key** (not AssetRef): the store's filename
  IS the pair; a HashSet of tuples avoids validating 64-hex per entry
  twice (the walk already checks length).
- **The tmp check on the EXTENSION** (`ext.starts_with("tmp.")`): the
  rename leftover's real name is `<hash>.tmp.<pid>`, not `tmp.<pid>`
  — the first draft checked the wrong field; the sim caught it before
  the battery could.
- **Shard removal only for dirs THIS pass emptied**: a shard holding
  a non-file entry or a live blob is never removed; the walk cannot
  race a concurrent `store()` into deleting a live shard's parent.

## Unverified

- Battery (host down since 18:2x). Static: rustfmt PASS; the walk
  semantics simulated in Python (report-only preserves 3/3 files;
  prune keeps live, removes orphan + tmp) — ad-hoc verified, not
  suite-verified.

## Deferred

- The caller: the shell's post-save GC report line (main.rs,
  lock-queued like the other shell halves).
- A CLI flag (`umber-cli assets gc --prune`) once a user-facing
  surface is wanted.
- Cross-ext dedup question (same bytes stored twice with different
  exts) — separate hashes, separate blobs; if that ever shows up in
  the wild, a size-aware pass is the follow-up.
