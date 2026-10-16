# Kill report (UNRUN, host-gated): PSD import read caps

Date: 2026-10-15. Author: dragon. Lock: `psd-read-caps`
(`psd_import.rs`, this report). Found while auditing ag-psd's
allocations for `wave-abr`.

## Finding

- `read_pending_psd_from_disk` did `std::fs::read(path)` with no size
  check, so a multi-GB file was read whole into memory before any cap.
- `import_psd_bytes` used ag-psd's default `ReadOptions`: a 2 GiB
  decoded-pixel budget, eight times the one 256 MiB frame this import
  keeps (`MAX_PSD_BYTES`).

## Fix

- `MAX_PSD_FILE_BYTES` = 2 GiB (the PSD format's own limit), checked from
  metadata before reading.
- `PSD_DECODE_BUDGET` = 4 × `MAX_PSD_BYTES` = 1 GiB, passed as
  `ReadOptions.total_memory_limit`. That covers an 8K composite plus
  layer canvases.

## Risk

A legitimate large layered PSD (many full-canvas 8K layers) without an
embedded composite could now fail with ag-psd's typed
`ExceededMemoryLimit` where it used to load. It already failed at 2 GiB;
the margin is 4 frames instead of 8. The error is typed and reported,
never a crash.

## Host

The existing psd_import tests must stay green; the corpus has no PSDs.
