# The Umber Merge Story

**Requirements §11:124**: "Merge story documented (per-layer files = two artists
editing different layers never conflict)".

## The format's merge properties

Umber projects are directories:

```
project/
  project.json          — the scene, texture sets, settings
  layers/<layer-id>.json — one file per layer
  assets/<content-hash>.png|exr — textures by content hash
```

### Property 1 — different layers never conflict

Each layer is a separate file. Two artists editing different layers touch
disjoint files: git merges trivially, no rebase, no conflict markers, no
coordination required. This is the headline property the requirements row
asks for, and it holds by construction.

### Property 2 — the same layer merges textually

`layers/<id>.json` is serde JSON, canonicalized (sorted keys, stable field
order, no timestamps). Two artists editing the SAME layer produce a textual
diff git can auto-merge when the edits are in different fields (name vs
opacity vs mask params). Same-field edits conflict loudly — as they should:
they are genuinely contradictory intentions, and the conflict markers name
the exact field.

### Property 3 — textures are content-addressed and binary-neutral

Painted pixels live as `assets/<sha256-of-content>.png` (or `.exr` for
float channels). Two consequences:

- **Binary neutrality**: git's merge treats assets as opaque content —
  no diff noise, no false conflicts, no merge-translation attempts.
  Two artists painting the same tile produce two different hashes;
  the layer file's asset reference is the single point of contention
  and it merges textually like any other field.
- **Deduplication as a side effect**: identical content = identical hash
  = one stored blob, project-wide and across saves.

### Property 4 — the journal is append-only local state

The autosave journal (`journal/`, §8) is crash-recovery state, not
collaboration state. It is machine-local and gitignored in shared
workflows: recovery replays against whatever `project.json` + layers
the working tree holds. Journals never merge.

### Property 5 — `project.json` is small + conflict-scoped

The scene/texture-set/settings file is deliberately tiny (mesh paths,
set names, channel lists, bake/export settings). Real edit traffic goes
to layer files; `project.json` changes rarely. When two artists DO
both change it, the conflict is small, textual, and names its fields.

## The workflow this buys

Two artists, one branch, no lock files:

1. Both work; commits interleave freely (different layers = clean
   fast-forwards).
2. Same-layer edits auto-merge on different fields; same-field edits
   surface as ordinary git conflicts, resolved field-by-field.
3. Textures never conflict as binaries — only their references do,
   and those are JSON fields.

The honest limit: Umber v1 has no built-in realtime sync or layer-level
history UI for foreign edits — the merge story is git-shaped. If a
team wants lock-based coordination, layer-level `.lock` markers are the
natural extension point; they are deliberately not in v1.
