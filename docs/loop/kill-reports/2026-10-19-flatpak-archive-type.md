# flatpak-archive-type — the hermetic manifest's one missing attribute

2026-10-19, session local-3d2151f3 (the post-push verification lane
of the stencil-wave close). Lock: flatpak-archive-type. Files:
flatpak/gen-cargo-sources.py, flatpak/org.umber.Umber.json
(GENERATED), docs/loop/queue.md, STATE.md, this report.

## The defect

The packaging DISPATCH 39263847856 — the flatpak-offline-deps kill's
named verdict gate, re-dispatched by the stencil-wave close — failed
in 3m15s:

    Error: module umber: Unknown archive format of
    '/home/runner/work/umber/umber/.flatpak-builder/downloads/01c04574.../ab_glyph-0.2.32.crate'
    ##[error]Process completed with exit code 1.

Mechanism: flatpak-builder infers an archive source's format from
the URL suffix; `.crate` is not in its table. The failure fired at
EXTRACTION, AFTER the sha256 had verified — the download was the
real 20,967-byte ab_glyph crate (curl printed `100 20967`), not an
error page, so this was NOT a CDN transient. All 621
static.crates.io downloads succeeded; the original no-network red
class (38883955783 — the crates.io fetch dying inside
flatpak-builder's sandbox) stays CLOSED by the kill's URL+checksum
sourcing.

The push-trigger packaging run 39263817851 is NOT a verdict gate:
its "build Flatpak bundle" job is `completed skipped` by trigger
design — its green covers lint + AppImage only.

## The fix

flatpak-builder's official command reference, source property
`archive-type` (string): "The type of archive if it cannot be
guessed from the path. Possible values are 'rpm', 'tar',
'tar-gzip', 'tar-compress', 'tar-bzip2', 'tar-lzip', 'tar-lzma',
'tar-lzop', 'tar-xz', 'tar-zst', 'zip' and '7z'." A `.crate` file
is a gzipped tar. gen-cargo-sources.py now emits
`"archive-type": "tar-gzip"` on every generated source (+ the
docstring contract with the why), and org.umber.Umber.json is
regenerated from it.

## Verification (host-free; no cargo surface)

- regenerate: 621 registry crates written;
- `--check` idempotent — PASS (CI's regenerate+diff drift gate
  stays mechanically green at every push);
- `json.tool` VALID;
- cross-check: 621 archive-type entries == 621 url entries == 621
  registry sources in Cargo.lock (637 lock packages = 621 registry
  + 16 workspace members — the workspace-skip rule holds);
- diff shape: exactly +621 lines in the manifest, every one an
  archive-type insertion, zero removals, zero other additions;
  generator +9 (the attribute + the docstring).

## Docs corrected in the same commit (wrong-claim sweep)

The stencil-wave docs pass recorded the verdict wrong in two
places, both fixed here: queue.md ("the failed dispatch was a CDN
transient — ab_glyph fetched as an error page — dispositioned") and
STATE.md ("The flatpak kill's verdict gate: packaging push-run
GREEN"). Corrected record: the gate is dispatch-only; the failure
was extraction; the bytes were real; the defect was one missing
attribute.

## Verdict gate (owed at the next push)

The packaging workflow re-dispatch on post-push master is the gate:
a green Flatpak bundle = the offline-deps kill fully closed. Per
the two-gate rule this commit rides the next wave's push — no
per-kill push.

## Hardware addendum (this session's lane, recorded here)

The 10-16 protocol's "install rasdaemon" line is NOT satisfiable on
centralhost's distro repos: rasdaemon is absent from
endeavouros/core/extra/multilib (`pacman -Sy` target-not-found
after a clean db sync), mcelog likewise unpackaged, ras-mc-ctl
absent, and EDAC registers no mc* controllers
(/sys/devices/system/edac/mc/ holds only power/subsystem/uevent —
no per-DIMM memory controllers on this boot). The standing decode
is the kernel's in-kernel MCE logging into the journal — the same
channel that produced the 10-16/10-17 bank-14 UECC evidence — and
it reads 0 events this boot. AUR rasdaemon remains an option only
if structured records are wanted; memtest86+ at the next physical
access stays the real fix.
