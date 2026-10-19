# packaging-error-hint — the split_dirname error now explains the failure class

2026-10-19, session local-3d2151f3, Bessa's steer ("anything you
can do meanwhile?"). Kill-lock: packaging-error-hint
(flatpak/vendor-checksums.py + this report). Deferred item from
the claude cross-review of the strip-components fix — the claw
named it, the dragon took it.

## The change

split_dirname's death message grew the diagnostic the 39306564270
failure actually needed: what the dir shape must be
(`<crate>-<version>/` as cargo stages them), what flat crate
CONTENTS (src/, benches/, .cargo/) mean (the flatpak-builder
archive sources stripped their top dir), and the fix (every
.crate source needs "strip-components": 0 — the archive default
is 1). The next person who breaks this now reads the cure in the
error itself instead of re-walking the whole staging chain.

## Verification (the real script, run twice)

- Broken staging: a synthetic vendor/ with `.cargo/` + `src/`
  flat inside + a minimal lock → exit 1, the full new message
  with the strip-components hint.
- Happy path: the repo's REAL Cargo.lock + a real crate from it
  (ab_glyph-0.2.32) staged crate-shaped → exit 0, "1 crate
  checksum files written".
- The stricter guards untouched: crate-shaped dirs missing from
  the lock still die ("vendor/ and Cargo.lock are out of sync"),
  empty vendor/ still dies — the hint widens only the unparseable
  branch.

The manifest's lint gate re-runs this script's checks on push;
the Flatpak job exercises it live on the next dispatch.
