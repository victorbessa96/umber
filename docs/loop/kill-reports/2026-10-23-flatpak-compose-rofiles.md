# 2026-10-23 — flatpak compose wall 4: the rofiles-fuse icon read (fix: --disable-rofiles-fuse)

Authorship: dragon (Razul, wake session local-c309922d).
Kill-lock: flatpak-compose-wall4 — files: .github/workflows/packaging.yml
(this report rides the same commit).
Chain position: the flatpak verdict chain, wall 4. Walls 1-3: 01f69a6
(vendor parser, verified 621 crates in production), e3ae21d (runtime
24.08->26.08 for the rustc 1.97 wall), e76be0c (the 26.08 install trio).
This kill takes the prior session's named handoff verbatim: "NEXT
SESSION'S FIRST KILL: the compose error's context".

## The failure (run 40026754260 on e76be0c)

The build compiled (621 crates, release profile 9m03s), every install
command ran (binary, .desktop, metainfo, SVG icon, three license files),
and the export died at flatpak-builder's `Running appstreamcli compose`:

    Only accepting components: org.umber.Umber, org.umber.Umber.desktop
    Processing directory: .../.flatpak-builder/rofiles/rofiles-0PF1qs/files
    general:        E: filters-but-no-output
    org.umber.Umber: E: file-read-error
    Error: ERROR: appstreamcli compose failed: Child process exited with code 1

`filters-but-no-output` is downstream: the component was dropped, so the
filtered output was empty. The real fault is one: file-read-error on the
org.umber.Umber component. Full job log persisted locally at
/tmp/flatpak-job4.log (lines 2765-2792).

## Local reproduction (zero CI rounds burned)

The tower is Ubuntu 24.04.5 with appstreamcli 1.0.2 — the same
`appstream-compose` stack the pinned runner installs (noble/universe
1.0.2-1build6, confirmed in the job's own apt output). The compose addon
binary was provisioned with no system mutation: `apt-get download
appstream-compose libappstream-compose0` + `dpkg -x` + LD_LIBRARY_PATH,
then run with flatpak-builder's EXACT compose argv, pinned from
builder-manifest.c:3105-3132 of the same 1.4.2 the runner runs:

    --prefix=/ --origin=org.umber.Umber --result-root=<files>
    --data-dir=<files>/share/app-info/xmls
    --icons-dir=<files>/share/app-info/icons/flatpak
    --components=org.umber.Umber,org.umber.Umber.desktop <files>

Tree-shape correction that matters: inside the flatpak files dir the
/app prefix IS the tree root (module installs land at files/share/...),
so the repro tree carries share/ directly under files/.

| Variant | Compose result |
|---|---|
| as-is inputs, plain tree | Success — catalog written, SVG scaled to 5 PNG sizes, 2 informational hints only |
| as-is + GIO_USE_VFS=local (runner env parity) | Success |
| SVG present, unreadable (chmod 000) | E: file-read-error + E: filters-but-no-output, exit 1 — the runner's exact pair |
| SVG absent | E: icon-not-found (a different tag — so the runner FOUND the icon) |
| .desktop absent | E: gui-app-without-icon (a different tag — so the desktop file loaded fine) |

Conclusion: the runner's compose located the SVG at
share/icons/hicolor/scalable/apps/org.umber.Umber.svg and FAILED
READING it. The inputs are innocent (byte-identical inputs compose
green locally with the identical compose library); the only structural
difference left standing is the rofiles-fuse view the runner's compose
reads through. The runner class (bwrap/rofiles on the GitHub image)
already bit this workflow once: the 26.04-rotation red that the
ubuntu-24.04 pin (f62d112, kill report 2026-10-23-flatpak-runner-pin.md)
worked around. The compose-phase icon read is the same family one
level deeper — the pin made the layer work for every phase up to the
installs, and the first phase that reads content through the view
(the export compose) is where it breaks.

## The fix

One flag, flatpak-builder's own documented workaround, present in the
runner's exact version (1.4.2, builder-main.c:103): --disable-rofiles-fuse
on the Build-the-Flatpak invocation. The cleanup/export phases then
operate on the real files dir — the same shape the green local run
proves. rofiles' read-only guard buys nothing in this job: the CI
workspace is disposable per run (--force-clean, fresh checkout, no
.flatpak-builder cache restore).

## Judgment calls

- Pre-rendered PNG hicolor icons instead of the flag: rejected — it
  changes packaging content to work around a CI filesystem layer, and
  the PNGs would still be read through rofiles with the flag absent.
- Pinning a flatpak-builder version without rofiles: rejected — rofiles
  is integral since 1.0; the flag is the precise scalpel.
- No manifest/metainfo change: the local repro proves those inputs
  green, so the packaging metadata stays untouched.

## Unverified

- The exact mechanism inside rofiles-fuse that fails the icon read
  (metadata/readdir succeeds; the content read of the SVG does not) is
  NOT reproduced locally — the tower's kernel/FUSE stack is not the
  runner's, and the fix removes the layer wholesale, so the mechanism
  does not gate the fix. If the re-dispatch still dies at compose, the
  new log names the next wall and the mechanism hunt resumes with
  better evidence.
- The dispatch itself is the verdict gate (the chain's standing
  pattern): the packaging run on this tip decides.

## Deferred

- The docs pass remnant (release-checklist .usdz row, kill-ledger
  sweep->LANDED, M1 pass 4) stays queued for the next cycle.
- Re-enabling rofiles once a runner image ships a fixed bwrap/rofiles
  stack: revisit together with the runner-pin's own revisit note.

## Process note

- The ci-watch on the dead run 40026754260 is closed (its verdict was
  consumed by the prior session's 18:39 post-close turn; no live watch
  processes at this session's start, pgrep-verified).
- Coordination: .dragon/COORDINATION-next-wave.md carries the
  flatpak-compose-wall4 CLAIMED block; the land line and STATE update
  ride the push.
