# 2026-10-23 — flatpak compose wall 5: the missing gdk-pixbuf SVG loader (fix: install librsvg2-common)

Authorship: dragon (Razul, wake session local-c309922d).
Chain position: the flatpak verdict chain, walls 1-4 preceding (01f69a6 vendor
parser, e3ae21d runtime 26.08, e76be0c install trio, 334af5b --disable-rofiles-fuse).
This kill supersedes the wall-4 conclusion in
2026-10-23-flatpak-compose-rofiles.md: that fix's rofiles attribution was
FALSIFIED by its own verdict run, and this report names the mechanism the
attribution hid.

## Wall 4's miss, owned

The wall-4 reasoning was sound on its evidence (identical inputs compose green
on a plain tree; an unreadable icon uniquely reproduces the error pair) but
the conclusion — that the rofiles-fuse layer was the runner-only difference —
was wrong. The verdict run 40035240398 (fix tip 334af5b) processed the REAL
files dir (`build-dir/files`, no rofiles path in the log) and died the
identical death at the identical step. The wall-4 report's own Unverified
section flagged exactly this risk ("if the re-dispatch still fails at
compose, the new log names the next wall"). It did. The --disable-rofiles-fuse
flag STAYS (it removes a fragile-on-runner layer from a disposable workspace;
the 26.04 rotation red was that family), but its comment no longer claims it
fixed the compose.

## The mechanism, proven end to end

1. flatpak-builder's export/cleanup phase runs `appstreamcli compose`
   HOST-SIDE (builder-manifest.c:3128-3159, flatpak_spawnv) against the
   app files dir.
2. asc-compose finds the SVG icon via the contents index (it exists and is
   readable — absent icons raise a DIFFERENT tag, `icon-not-found`), reads
   it, and hands the bytes to `asc_image_new_from_data`
   (asc-compose.c:1029-1057).
3. asc-image loads EVERYTHING — SVGs included — through
   `gdk_pixbuf_new_from_stream_at_scale` (asc-image.c:457-473). gdk-pixbuf
   has no built-in SVG support: the SVG loader module
   (`libpixbufloader-svg.so`) ships in **librsvg2-common** — a
   recommends-only package on noble.
4. The flatpak job installs with `--no-install-recommends` (and so did every
   prior dispatch), so the runner has libappstream-compose0 and librsvg2-2
   (the library) but NOT the pixbuf loader. Every desktop has it; the
   runner does not.
5. With no svg loader registered, `gdk_pixbuf_new_from_stream_at_scale`
   fails with GError **"Unrecognized image file format"**; asc returns NULL;
   the hint fires at the second raise site — `file-read-error` on
   org.umber.Umber — and the component is dropped, starving the filtered
   output (`filters-but-no-output`), exit 1.

## Local proof chain (zero CI rounds burned on the fix)

The compose stack was provisioned on the tower with no system mutation
(`apt-get download appstream-compose libappstream-compose0` + `dpkg -x` +
LD_LIBRARY_PATH — byte-identical noble packages, appstream-compose
1.0.2-1build6, the runner's own). With flatpak-builder's exact argv
(pinned from builder-manifest.c:3105-3132):

| Variant | Result |
|---|---|
| as-is tree (valid SVG, full desktop env) | Success — SVG scaled to 5 PNGs |
| full tree incl. the release binary | Success |
| stripped env (`env -i`), GIO_USE_VFS=local | Success |
| icon unreadable (chmod 000) | E: file-read-error + filters-but-no-output — raise site 1 (read) |
| icon empty / truncated / malformed | same pair — raise site 2 (load) |
| icon absent | E: icon-not-found (different tag) |
| desktop file absent | E: gui-app-without-icon (different tag) |
| **loader cache WITHOUT the svg module (the runner's exact loader state)** | **E: file-read-error + filters-but-no-output, msg "Unrecognized image file format", exit 1 — THE RUNNER'S SIGNATURE** |

The last row is the reproduction: the tower's loaders.cache regenerated
without `libpixbufloader-svg.so` (gdk-pixbuf-query-loaders over the other
13 modules) turns the green compose into the runner's exact failure, with
the hidden `msg` field finally visible via --print-report=full. The
runner-side confirmation facts: the old job log shows librsvg2-common ONLY
inside apt's recommends OR-alternatives display line, never in the
installed set; the tower (green) has librsvg2-common:amd64.

Why the two raise sites share one tag: asc-compose.c raises
`file-read-error` both when `asc_unit_read_data` fails (site 1) and when
`asc_image_new_from_data` fails (site 2) — the tag map this hunt built is:
absent icon = icon-not-found, absent desktop = gui-app-without-icon, any
failure to get a loaded image = file-read-error.

## The fix

One package in the flatpak job's apt line:
`sudo apt-get install -y --no-install-recommends flatpak flatpak-builder
librsvg2-common`. The compose then loads the SVG the same way every desktop
does. The workflow's build-step comment is rewritten so no false attribution
survives on disk.

## Judgment calls

- Pre-scaled PNG icons instead of the package: rejected as the primary fix
  (changes packaging content to dodge a missing CI package; png/gif/jpeg
  loaders are builtin but svg would stay broken for any future metainfo
  artwork). Named as the fallback if librsvg2-common ever leaves noble.
- Reverting --disable-rofiles-fuse: rejected — it is orthogonal hardening on
  a disposable workspace; reverting would re-arm the bwrap/rofiles family
  the 26.04 pin exists to avoid.
- No probe job needed: the restricted-cache reproduction closed the case
  before the probe design shipped. The probe concept (compose on a
  synthesized tree with --print-report=full) is recorded here as the
  reusable pattern for the next runner-side compose mystery.

## Unverified

- The dispatch run on this tip is the verdict gate (chain's standing
  pattern). Confidence is high: the loader-state reproduction produced the
  exact error pair and the exact hidden msg string on the same lib bytes
  the runner runs.

## Process note

- The scratch repro lives at /tmp/ascompose-repro (the extracted compose
  stack, the trees, the no-svg cache, the appstream v1.0.2 clone). /tmp is
  ephemeral; this report carries everything durable.
- The dead run's log: /tmp/flatpak-job4.log (wall 4's run) plus the
  40035240398 step log pulled via `gh run view --log-failed`.
- Coordination: .dragon/COORDINATION-next-wave.md carries the wall-4
  land block; the wall-5 correction block rides this push.
