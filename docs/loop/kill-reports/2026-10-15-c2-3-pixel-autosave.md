# Kill report: C2-3 pixel autosave + "Recover unsaved work?"

Date: 2026-10-15. Author: dragon. Lock: `c2-3-pixel-autosave` over
`crates/umber-app/src/{main,recovery,document}.rs`, both ftl catalogs,
`docs/user/getting-started.md` and this report. Source: audit C2-3.

## The gap

The autosave journal replays LAYER ops after a crash, and only for a
project that already has a directory. Painted pixels between saves, and
everything in a never-saved document, died with the process.

## Built

- **`recovery.rs`:**
  - **`AutosaveClock`** (pure): snapshot every `AUTOSAVE_SECS` (120 s)
    while dirty. Never mid-stroke, never twice for the same edits (the
    history row count is the edit generation). Saving resets it.
  - **`RecoveryMeta`**: a versioned `key=value` file naming the source
    project (if any), the mesh and the write time. A file without the
    version line is never trusted.
  - `write_meta` uses write-then-rename. `pending` offers a snapshot only
    when both the meta parses and `project.json` exists. `discard`
    removes the folder.
- **The snapshot is a full ordinary project** (metadata + the `paint/`
  sidecar), written into `<state dir>/recovery/snapshot/` through the
  same crash-safe save path as Ctrl+S:
  - `save_project_to` is split, so the write half (`write_project_files`)
    no longer touches the session's project dir, recents or journal;
  - a snapshot never points the session at the state dir;
  - never-saved documents are covered.
- **Lifecycle:**
  - a successful user save, a Discard / Save quit, and a clean close
    each discard the snapshot;
  - a crash or kill leaves it.
- **Launch:** a snapshot left behind raises "Recover unsaved work?"
  (Recover / Discard). Recover:
  1. reloads the mesh named in the meta;
  2. opens the snapshot as the project;
  3. detaches the snapshot's journal (`Document::detach_journal`, new:
     no truncation, the high-water mark kept);
  4. points the session back at the ORIGINAL project dir (or none), so
     Ctrl+S never writes into the state dir;
  5. marks the document dirty, since the work is not on disk anywhere
     the user chose, and drops the snapshot path from Recent.
- New strings: `recovery.title` / `.body` / `.recover` in both catalogs
  (the catalogs are free since the sibling's lock lifted). Discard reuses
  the confirm key.

## Tests (4)

- Meta round trip, plus foreign and empty files rejected.
- `pending` needs both halves; `discard` is idempotent.
- The clock: interval, never mid-stroke, no double snapshot, the reset on
  save. **Mutation check:** allowing a mid-stroke snapshot fails it at
  "never mid-stroke".
- **A real project** written through `save_to_dir` into the snapshot dir
  is offered by `pending` and reads back through `load_from_dir`.
- Gate 1: umber-app 517. fmt and clippy clean.

## Unverified

The shell glue (the 120 s tick, the prompt, the recover sequence) runs in
`App::ui` with no harness over `UmberApp`. A manual check on a desktop
session: paint, wait two minutes, kill -9, relaunch, Recover, and confirm
the paint returns. It is on the release smoke list with the quit guard.

## Judgment calls

- **120 s, full snapshot.** An incremental pixel journal would be cheaper
  per tick but is a new on-disk format. The full snapshot reuses the
  proven save path, and the clean-plane hash skip in
  `save_paint_tiles` keeps unchanged planes cheap on re-snapshot.
- **One snapshot slot**, not per project: the state dir holds the last
  session's work. Two Umber windows at once is not a v1 scenario.
