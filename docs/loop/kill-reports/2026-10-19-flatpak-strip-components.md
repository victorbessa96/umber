# Kill report: the flatpak strip-components fix — the offline-deps verdict-gate debt collected (2026-10-19)

Dragon-authored by the Razul main session, collecting the
offline-deps kill's verdict-gate debt (the sibling's lane, closed
and unlocked; their coordination note informed). Kill-lock
`flatpak-strip-components`, holding `flatpak/gen-cargo-sources.py`,
`flatpak/org.umber.Umber.json`, and `flatpak/org.umber.Umber.json.in`.

## Context

The flatpak verdict-gate dispatch (39306564270, the first sandbox
run to reach the build commands) passed extraction — the
archive-type fix (`5e7f4dd`) is verified live: all 621 crates
downloaded, sha256-verified, and extracted past the ab_glyph stage
that killed dispatch 39263847856 — then died at the FIRST build
command: `vendor-checksums: unparseable vendor dir '.cargo'`.

## Root cause

flatpak-builder's archive sources default to `strip-components: 1`.
A real .crate tarball carries its top-level `<name>-<version>/`
directory (verified: ab_glyph-0.2.32/.cargo_vcs_info.json, ...),
so the default strips the per-crate dir and dumps every crate's
raw contents flat into `vendor/` — 621 crates colliding, and
whichever crate ships an inner `.cargo/` directory poisoning the
vendor root where the checksum script's sorted scan hit it first.
The generator never emitted `strip-components`, and the manifest
carried none: the layout `vendor-checksums.py` parses
(`vendor/<name>-<version>/`, the `cargo vendor` shape) was never
what the sandbox produced.

## Built

- `gen-cargo-sources.py` gains `"strip-components": 0` on every
  emitted source (docstring example + the emit dict).
- The manifest regenerated: 621/621 sources carry the key, the
  generator `--check` idempotent, `json.tool` valid.

## Gates run (host-free, this session)

Ad-hoc 4/4: the failure class REPRODUCED (a real .crate extracted
with strip=1 flattens — no per-crate dir), the fixed shape PROVEN
(strip=0 keeps `vendor/<name>-<version>/` with the checksum slot
beside the crate files), the script's `split_dirname` seam accepts
the dirname, and 621/621 sources carry the key. Host-gated
unknown: the next sandbox run — the verdict gate re-dispatches
after the next push (this row rides it).

State: VERIFIED host-free; the flatpak verdict gate re-dispatch
after the next push is the live proof for BOTH the archive-type
half and this half.
