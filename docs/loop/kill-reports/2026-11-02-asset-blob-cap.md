# Kill report — 2026-11-02 asset-blob cap (row 156 sweep, part 2)

Lock: `asset-blob-cap`. Envelope: code-only, NO cargo — rustfmt +
check-layers only; the pinned test is UNRUN until the next wave battery.

## Built

`umber-core/src/assets.rs` — `AssetStore::load` now checks the blob's
on-disk size from METADATA before reading:

- over `MAX_ASSET_BLOB_BYTES` (1 GiB; an 8K RGBA8 tile is 256 MiB), or
- disagreeing with the ref's recorded `size` (truncation/tamper — the
  field existed "for sanity checks on load" but was never checked)

→ new `AssetError::SizeMismatch { hash, ext, actual, expected, cap }`.
A hostile project dir with a multi-GB blob under a hash-named path is
now a typed error, not an allocation abort (which no panic boundary
catches). Refs are always constructed with the true size (store() records
`bytes.len()`; the manifest parser round-trips it), so the mismatch check
cannot false-positive on well-formed projects.

## Verified static

- rustfmt --edition 2021 clean; check-layers GREEN (16 crates); no new deps.
- Pinned test UNRUN per envelope: `tampered_blob_size_mismatch_refused_before_read`.

## Deferred

- Paint sidecar manifest parsing (persistence.rs) reads small text files;
  project.json/layer caps landed in 5555530. Tile/blob reads all funnel
  through AssetStore::load — now capped.
