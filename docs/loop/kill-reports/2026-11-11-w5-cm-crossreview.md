# Wave-5 channel-manage — cross-review hardening (2026-11-11)

**Kill:** `razul-w5-cm-crossreview` — cross-review of the landed sibling kill
`25ef4708` (wave-5 channel-manage, R-B4 tail, queue rows 354/370).

**Verdict on the landed kill:** sound. The sibling's implementation is
stronger than the parallel one this dragon had in flight (case-folded
duplicate refusal, unsafe-name refusal at declaration time, tiled sets kept
in step, the document marked dirty so `project.json` persists the change, a
headless `egui_kittest` suite driving the panel section). That parallel
implementation was discarded — the landed one supersedes it.

**Verification of the landed commit** (independent battery, this dragon,
`umber-core` + `umber-app` + `umber-gui`): rustfmt `--check` clean,
`check-layers` GREEN, `scorecard --check` current, clippy
`--all-targets -D warnings` clean, tests 0 failed. Battery log:
`~/.hermes/cache/scratch/w5-cm-battery.log` (`BAT_FAIL=0`).

## What the cross-review found

Two defects in the landed kill, both in `umber-core`'s declaration rules.
Both are name/identity defects, not feature gaps.

### 1. The channel-name rule was weaker than the set-name rule

A channel name becomes a `paint/<channel>/` sidecar directory name exactly
like a texture-set name becomes a project directory name. The project's own
rule for set names (`project::validate_set_name`) refuses:

- the Windows reserved device stems — `CON`, `PRN`, `AUX`, `NUL`,
  `COM1`-`COM9`, `LPT1`-`LPT9` — by STEM, so `CON.txt` and `nul.metal` are
  invalid too;
- a trailing dot or space (Windows strips both silently).

The landed channel rule refused the reserved CHARACTERS (`/ \ : < > " | ? *`),
control characters and dot-only names, but accepted the reserved stems and a
trailing dot. Two consequences:

- **A channel named `CON` could be declared** and would then be refused by
  the paint persistence on Windows — the exact failure the declaration-time
  check was added to prevent.
- **`ao` and `ao.` were both declarable.** The duplicate check folds case but
  not the trailing dot, so two DISTINCT declared channels would map to the
  SAME `paint/ao/` directory on Windows: a silent plane mixup, not a
  cosmetic one.

**Fix:** the channel rule now refuses the Windows reserved stems (case-
insensitive, by stem before the first `.`) and a trailing dot, and
`project::WINDOWS_RESERVED_STEMS` became `pub(crate)` so both rules answer
to ONE list instead of two that can drift.

A trailing SPACE stays legal-by-trimming: the name is trimmed before the
check, so `"ao "` lands on `ao` — the same directory Windows would keep.
The pinned test says so explicitly.

### 2. `add_channel` could double an entry in a tile's channel list

`add_channel` pushed the new channel into every tile's channel list
unconditionally. In the divergent case — a tile that already lists the name
(a hand-edited `project.json`, where the tile lists and the declaration have
drifted) — that produced TWO entries with the same name in one tile.

**Fix:** a tile that already carries the name keeps its entry; the
declaration gains the channel, the tile list stays single-valued.

## Judgment calls

- **The fix stays in `umber-core`.** The paint side's own
  `is_safe_channel_component` (the manifest trust boundary) is left alone:
  it parses manifest CONTENT, and a manifest written before this kill can
  still carry a `CON` channel. Refusing such names at DECLARATION time
  stops new ones; refusing them in an old manifest would break loading
  those projects, which is a separate call.
- **No app-side change.** The panel already surfaces the refusal reason
  (`texturesets.channel-error-invalid`), so the two new refusal paths need
  no new string and no new widget.

## Battery (this kill)

`rustfmt --edition 2021 --check` on the touched `.rs` files clean,
`scripts/check-layers.py` GREEN, `scripts/scorecard.py --check` current,
`cargo clippy -p umber-core -p umber-app -p umber-gui --all-targets
-D warnings` clean, `cargo test -p umber-core -p umber-app -p umber-gui`
0 failed. Log: `~/.hermes/cache/scratch/w5-cm-crossreview-battery2.log`
(`BAT_FAIL=0`).

## Tests

- `channel_names_refuse_the_windows_device_stems_and_trailing_dots` — the
  reserved stems (`CON`, `con`, `PRN.txt`, `aux`, `NUL`, `COM1`, `lpt9`,
  `com1.log`) and the trailing-dot names (`ao.`, `ao..`) are refused; the
  trailing-space name trims to `ao`; the ordinary names still pass.
- `add_channel_never_doubles_a_tile_entry` — a tile that already lists the
  name keeps exactly ONE entry, and the declaration gains exactly one.

## Unverified

- The live release app was not launched for this hardening (it is a
  declaration-rule change; the panel path is unchanged and was driven
  headless by the landed kill's `egui_kittest` suite).
- The Windows-only collision (`ao` vs `ao.`) is argued from Windows' name
  rules, not reproduced on this Linux host.

## Deferred (unchanged from the landed kill)

- **`w5-channel-rename-planes`** — the paint-side rename
  (`PaintState::rename_channel`: targets, overrides, sidecar directory).
  The strand rule still holds: a rename re-declares only.
- Plane clearing on remove (a separate destructive confirm), undo for
  channel edits (a channel command family in the journal), channel reorder.
