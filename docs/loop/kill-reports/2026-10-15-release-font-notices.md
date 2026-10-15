# Kill report: the release archives ship the embedded fonts' licences

Date: 2026-10-15. Author: dragon. Lock: `release-font-notices` over
`dist-workspace.toml`, THIRD_PARTY.md, `LICENSES/THIRD_PARTY_FONTS.txt`
and this report. Found by a licence sweep of the whole `Cargo.lock`, run
from registry manifests with no cargo.

## Finding

Every Umber binary embeds egui's default fonts (`epaint_default_fonts`
0.36.2 via eframe's `default_fonts`, `include_bytes!`): Ubuntu Light,
Hack, Noto Emoji and emoji-icon-font. Their licences (Ubuntu Font Licence
1.0, OFL-1.1, MIT) require the licence text to accompany each copy
(OFL §2: "provided that each copy contains the above copyright notice and
this license"). The release archives carried only README, LICENSE and
CHANGELOG (cargo-dist's auto-includes). The alpha would have shipped
non-compliant.

(Umber's own Inter + JetBrains Mono, `fonts.rs`, are not in the tree yet
and not compiled in; when they land, their OFL texts join this file.)

## Fix

- `LICENSES/THIRD_PARTY_FONTS.txt`: a table of the four fonts plus all
  four licence texts, copied verbatim from the crate's own `fonts/`
  directory.
- `dist-workspace.toml`: `include = ["LICENSES/THIRD_PARTY_FONTS.txt",
  "THIRD_PARTY.md"]`. The key exists in cargo-dist 0.32.0's
  `[dist]` config (`config/v0.rs:144`), and the file parses as TOML.
- THIRD_PARTY.md: the fonts section, plus a whole-tree licence summary.
  636 packages, all permissive or permissive-optional. The non-trivial
  ones are named with why they are fine: MPL `colored` is test-only (via
  `dify` ← egui_kittest), and the GPL-2.0/LGPL alternatives
  (`self_cell`, `ittapi`, `r-efi`) are taken under their permissive
  option.
- `cargo-audit` against the advisory DB as of 2026-10-14 (updated today
  from 2026-09-09): 0 vulnerabilities, 2 unmaintained-transitive
  warnings, recorded in THIRD_PARTY.md.

## Verify (host-gated, folded into the alpha's step 3)

`dist plan` must list `THIRD_PARTY_FONTS.txt` and `THIRD_PARTY.md` under
each archive's `[misc]`, as the dry run listed CHANGELOG, LICENSE and
README.

## The Linux packages too (locks `pkg-license-files`, `pkg-license-readme`)

- **Flatpak** (`org.umber.Umber.json`) installed no licence file at all.
  It now installs LICENSE + THIRD_PARTY_FONTS.txt + THIRD_PARTY.md to
  `/app/share/licenses/org.umber.Umber/`. They come from the git source,
  which is the repo root; the JSON is valid.
- **AppImage** (`packaging.yml`) copied LICENSE to `AppDir/usr/bin/`, a
  binaries dir. All three files now go to `usr/share/licenses/umber/`;
  the YAML is valid.
- Both READMEs describe the new layout.

Verify: `packaging.yml` runs on the alpha tag. Its lint job
(desktop-file + AppStream validation) does not check this; the real
check is listing the built AppImage and the Flatpak build dir.
