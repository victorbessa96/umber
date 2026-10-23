# Brief: 2026-10-19 channel-export-resample (W-C3a lane A1)

Mandate: Bessa, 2026-10-19: "Continue working on the umber long horizon
project in all the steps, do not stop." Kill plan:
`docs/loop/next-wave-plan-draft.md` §1 lane A1 (verified anchors).

## The bug you are closing

A texture set painted at one size and exported at another silently
drops ALL painted pixels. `painted_channel_tile`
(`crates/umber-app/src/bake_sources.rs:573`) returns `None` when the
channel target's dims != `(size, size)`, and
`painted_base_color_tile` (same file, its
`target.dimensions() != (size, size)` guard) does the same for Base
Color — so a 4K-painted set exported at the dialog's 2K preset exports
flat/baked bytes with no notice. The export size is the dialog's own
preset combo (`crates/umber-app/src/export_dialog.rs:566`),
independent of the set size. Base Color's decline is documented at
`painted_at_size` (`export_dialog.rs:245-252`): "the driver's painted
bridge declines any other size". This kill closes BOTH readbacks.

## Contract

1. Both readbacks, on a dims mismatch, read back and FIT to
   `size x size` through
   `umber_paint::material_layer::resample_image_rgba8` (pub,
   `crates/umber-paint/src/material_layer.rs:429` — bilinear,
   texel-centre aligned, renormalizes when `channel == "normal"`).
   Neither returns `None` for a present-but-differently-sized target.
   A target that is genuinely absent keeps today's behavior.
2. `painted_at_size` (`export_dialog.rs:245-252`) becomes "a session
   exists". Its existing tests pin the OLD decline: they are
   DELIBERATELY re-pinned. This is a behavior change — name it in the
   report, never edit a test silently.
3. The decision is a pure fn
   `fit_channel_to_export(bytes, (w, h), size, channel) -> Vec<u8>`
   (identity passes bytes through EXACTLY, no copy semantics change
   beyond the fit). `channel` rides through to the resample so the
   normal arm renormalizes.
4. Export always writes at the export size. Per-channel native-size
   export of passthrough files is the deferral (packed outputs force
   one size anyway — `validate_set` in
   `crates/umber-export/src/driver.rs:584` enforces one size per
   MapSet).
5. Before claiming the CLI is clean, GREP the CLI/agent export path:
   `crates/umber-cli/src/main.rs:965` builds MapSets from bakes only,
   so there is likely NO painted-channel drop there. Confirm against
   live source and record the verdict in the report.
6. Document the aliasing boundary in the touched module docs:
   bilinear downsampling of 4K -> 1K aliases; box-filtering is the
   named deferral.

## Files you own (the ONLY files you may write)

- `crates/umber-app/src/bake_sources.rs`
- `crates/umber-app/src/export_dialog.rs`
- `docs/loop/kill-reports/2026-10-19-channel-export-resample.md`

## Tests you write (do NOT run them — the dragon gates on the build host)

Hand-derived pins:
- identity: same-size bytes pass through byte-exact.
- constant image stays constant on a 2 -> 4 upsample (exact).
- a 2x1 ramp `[0, 255]` upsampled to 4x1 gives the exact
  edge-clamped bilinear bytes (derive them by hand from
  `resample_image_rgba8`'s formula — read it first).
- the normal arm: a flat (128, 128, 255) image stays flat after the
  renormalize.
- the existing GPU tests at `bake_sources.rs:1019` and `:1333` keep
  passing untouched.
- the `painted_at_size` decline tests re-pinned to the new contract.

## Discipline (binding)

- The tree is SHARED. Dirty or untracked files you did not create
  (`main.rs`, `viewport.rs`, `.dragon/*`, other kills' reports) belong
  to other live sessions: never touch, restore, checkout, or commit
  anything. `git log/diff/grep` reads are fine; NO git mutations.
- NO cargo, rustc, build, or test runs of any kind. You may grep/read
  files and run `python3` for derivation checks. Match the files'
  existing formatting style; the dragon runs rustfmt at review.
- NO lock operations (`scripts/kill-locks.sh` any subcommand,
  `/tmp/umber-kill-locks/*`), never write to `/tmp` or anywhere
  outside the three files above.
- If any file:line anchor has drifted, re-derive from live source.
  Never invent an API — grep its definition first.

## Report shape

Write `docs/loop/kill-reports/2026-10-19-channel-export-resample.md`:
header naming authorship (claw: opencode/deepseek-v4.1-flash-free),
kill-lock `channel-export-resample` + its files, design source
(`docs/loop/next-wave-plan-draft.md` §1 lane A1 + this brief); then
**Built** (per-file bullets), **Judgment calls** (each with the
alternative rejected), **Unverified**, **Deferred**, **Process note**
(as needed). End your run with a one-screen summary: files changed +
every test name + what it pins.
