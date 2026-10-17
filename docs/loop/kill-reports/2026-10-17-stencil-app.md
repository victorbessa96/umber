# Kill report: stencil-app (wave-stencil, shell half — sequencing step 2)

- Contract: `docs/specs/stencil-engine-design.md` "The design" + "What
  lands in which crate". HEAD `b2b2e8f`, coded against the PARKED,
  uncommitted `crates/umber-core/src/stencil.rs` (read on disk; not
  modified).
- Kill-lock `stencil-app`: `crates/umber-app/src/stencil.rs` (new),
  `crates/umber-app/src/main.rs` (additive hunks only),
  `crates/umber-app/src/display_panel.rs`, `assets/i18n/en-US.ftl`,
  `assets/i18n/pt-BR.ftl`.

## Headline (read this first)

**The stencil does NOT gate paint yet.** umber-app has no path to the
gate texture, so the brief's "hand the composed bytes to
write_mask_region" can't be a main.rs hunk. The real call path is:

1. `PaintState::begin_stroke` arms `stroke_mask_gate = quick_mask().gates()
   || cavity_gates()` (umber-paint `paint_state.rs:1714`).
2. Per stage, `sync_mask_tile` (`:5205`) → `publish_mask` (`:5227`)
   → `gate_bytes` (`:5184`) → `gate_plane` (`:5158`: the quick-mask
   plane × the cavity plane, `:5171-5179`).
3. → `PaintThreadCommand::WriteMask` (`umber-gpu paint_thread.rs:100`)
   → `PaintTarget::write_mask_region` (`paint_thread.rs:292` →
   `paint.rs:579`).
4. Dabs get `with_mask_gate(true)` (`paint_state.rs:4848`).

The panel side is only `brush_panel.rs:1434` `quick_mask_body` →
`PaintState::set_mask_active` / `set_mask_mode` (`paint_state.rs:2057-2066`).

Every link in that chain is **private** to umber-paint, and no `pub fn`
queues caller bytes as `WriteMask`. I grepped every `pub fn .*mask|gate`
in `paint_state.rs`; the only byte-taking one is `publish_layer_mask`
(`:2243`), which is the LAYER-mask lane, not the gate. Even with a lane,
two things would still break it:

- With the quick mask off, nothing arms `stroke_mask_gate`, so the
  shader ignores the texture.
- With it on, every resync or incremental stamp (`:5082`) overwrites
  shell-written bytes with `gate_plane`'s quick-mask-only bytes.

So the composition has to live INSIDE `gate_plane`. I built the pure half
plus a marked seam (`StencilState::stage_gate`,
`TODO(stencil-paint-hook)`). The panel says so on screen
(`overlays.stencil-gate-pending`) instead of implying the stencil works.

## Built

`crates/umber-app/src/stencil.rs` (new; registered as `mod stencil;` at
**main.rs:165**, bin target only; see judgment call 2):

- `StencilState { image, transform, active, overlay_opacity, path,
  load_error, staged }` with a manual `Default`: inactive, no image,
  `StencilTransform::default()`, opacity 0.5.
  - `gate_bytes(&self, w, h) -> Option<Vec<u8>>` is `None` when inactive
    or imageless, otherwise `Some(rasterize_gate(..))`.
  - `reset(view_w, view_h)` calls `fit_to_viewport`.
  - `adopt(path, Result, view)` takes a load's outcome.
  - `stage_gate(target, quick)` is the seam.
- `StagedGate { width, height, values }` + `open_percent()`.
- `compose_gate(stencil, quick) -> Option<Vec<u8>>`: the brief's
  "GateCompose" (snake_case as a fn). Cases:
  - none → `None`;
  - one gate → passed through;
  - both → per-byte `min`;
  - length mismatch → fail closed, all 0.
- `quick_mask_gate_bytes(&QuickMask, tile)`: the quick-mask half of
  umber-paint's private `gate_plane` + `gate_bytes`, mirrored with the
  same clamp and round, without cavity.
- `DEFAULT_MAX_STENCIL_PIXELS = 4096*4096` and
  `check_stencil_size(w, h, max)` (`checked_mul`, inclusive cap).
- `png_dimensions` (IHDR peek), `stencil_from_png_bytes(bytes, max)`,
  `load_png_rgba(path)`.
- `StencilLoadError` (thiserror): `Io` / `TooLarge { width, height, max }`
  / `Decode` / `Image`. No `source` field.
- `panel_section(ui, &mut StencilState, Option<&PaintState>)`: the thin
  egui adapter. It draws the rows, loads a picked file, Resets, and
  re-stages after a load, toggle or Reset.

`crates/umber-app/src/display_panel.rs`:

- Module-doc section "The stencil rows".
- `DEFAULT_STENCIL_OPACITY`.
- `StencilRow<'a>` (a struct of field borrows).
- `StencilRowResponse { toggled, reset, picked }`.
- `stencil_rows(ui, StencilRow) -> StencilRowResponse`. Its rows:
  - section label;
  - read-only path label + `Load…` (an rfd PNG dialog);
  - the load-error line;
  - the enable checkbox, disabled without an image;
  - an opacity `ValueField` (0..=1, step 0.05, the wireframe-opacity
    shape);
  - Reset;
  - the disabled N-hint label;
  - the disabled gate-pending line.
- `overlay_rows` and `OverlaySettings` are untouched.

`crates/umber-app/src/main.rs` (+12 lines, 0 deletions; `git diff
--stat`):

- `mod stencil;` (`:165`)
- `PanelViewer.stencil` field (`:443-444`)
- the Display arm's one call, `stencil::panel_section(ui, self.stencil,
  self.paint.as_deref())` (`:1175-1177`)
- `AppState.stencil` (`:1296-1299`, filled by `..AppState::default()`
  at the `:1839` literal)
- the viewer construction `stencil: &mut self.state.stencil` (`:4088`)

i18n: **12 new keys, in BOTH catalogs**, under the existing `overlays`
group header. The three refusal reasons are `overlays.stencil-unreadable`,
`overlays.stencil-too-large` (`$width $height $max`) and
`overlays.stencil-unsupported`. The other nine:

- `overlays.stencil`
- `overlays.stencil-enable`
- `overlays.stencil-opacity`
- `overlays.stencil-reset`
- `overlays.stencil-load`
- `overlays.stencil-no-image`
- `overlays.stencil-load-failed` (`$reason`)
- `overlays.stencil-n-hint`
- `overlays.stencil-gate-pending` (`$width $height $open`)

All 12 are added to `overlay_keys_exist_in_both_catalogs`.

Tests: **13 new**: 12 in `stencil.rs` (items 1-10, 12 and 13 below) and
1 kittest in `display_panel.rs` (item 11). The existing catalog test also
gains 12 keys.

1. `default_state_is_inactive_empty_at_half_opacity`
2. `gate_bytes_need_both_an_image_and_activation`: inactive gives None,
   imageless gives None, and active + identity gives exactly the 4x4
   PATTERN's bytes.
3. `compose_is_the_and_table_and_passes_single_gates_through`: the
   255/0 table `[255,255,0,0] ∧ [255,0,255,0] = [255,0,0,0]`, both
   single-gate cases, None/None, soft `min(128,64)=64` (this kills a
   bitwise-`&` rewrite), and a mismatch failing closed.
4. `the_size_guard_refuses_past_the_cap`: 4096² ok, 4097×4096 refused
   with the exact fields, the inclusive boundary at 6 vs 5, and
   u32::MAX².
5. `an_oversized_header_is_refused_before_decoding`: an IHDR declaring
   5000² with a junk CRC gives `TooLarge`. The decoder alone would give
   a CRC error, so this proves the peek runs first. Under `usize::MAX`
   the same bytes give `Decode`.
6. `png_bytes_load_top_down_with_the_alpha_cut`: an in-test PNG via
   `umber_graph::encode_png_rgba8` (value.rs:367). Alphas 0/255/127/128
   give `[0,255,0,255]`. A 1×2 top-opaque image gives `[255,0]`, which
   pins top-down rows; `read_png_rgba8` would give `[0,255]`. The
   decoded-size cap is also checked.
7. `a_missing_file_is_an_io_refusal` (a relative nonexistent path, no
   temp dir).
8. `adopt_activates_and_fits_or_keeps_the_old_image`: 400×100 in
   200×300 gives scale 0.5 at (100,150). A refusal keeps the
   image/path/transform and records a message. Only its presence is
   pinned, because Fluent isolation marks make an exact pin fragile.
9. `reset_refits_and_is_a_no_op_without_an_image`: 4×4 in 8×16 gives
   scale 2 at (4,8).
10. `quick_mask_bytes_follow_gating_and_the_empty_value`: not armed
    gives None, armed gives `[0;4]`, mask mode gives None.
11. `stencil_rows_toggle_only_with_an_image` (kittest): the click flips
    `active` and reports `toggled`; with no image it is disabled and
    nothing flips.

12. `stage_gate_composes_and_needs_a_target`: staged == PATTERN at 37 %
    open (6/16), a blocking quick mask gives all 0, and no target or an
    inactive stencil gives nothing staged.
13. `each_refusal_maps_to_its_own_catalog_key`: all four variants map to
    the pinned key, and each key exists in en-US and pt-BR.

## Judgment calls

1. **The upload is a seam, not a call** (the headline). `staged` holds
   the exact bytes the hook will carry. This is the brief's own
   fallback ("pure half + TODO-style seam"), and the frame loop is not
   restructured.
2. **`mod stencil;` sits in main.rs only.** `display_panel` compiles into
   BOTH targets (`lib.rs:58` and `main.rs:124`). If it named
   `crate::stencil::StencilState`, the lib target would fail with E0433,
   and lib.rs is outside the lock. So the display rows are type-free
   (`StencilRow` borrows fields) and `stencil::panel_section` adapts.
   `DEFAULT_STENCIL_OPACITY` lives in display_panel for the same reason,
   and `stencil::DEFAULT_OVERLAY_OPACITY` aliases it, so there is one
   value.
3. **v1 host: the Display panel, not the Overlays popover.** The popover
   body is drawn at `viewport.rs:820` (`overlay_rows`), and viewport.rs
   is step 3's surface. `OverlaySettings` is `Copy`, so a `Vec`-backed
   image can't ride it. display_panel.rs's module docs (:86-89) already
   bless "a Display-panel section calls the same fn". Step 3 adds the
   same `stencil_rows` call to the popover.
4. **The decoder is `umber_graph::decode_png_rgba8`** (value.rs:233, the
   umber-graph dep is already at umber-app Cargo.toml:20), **not** the
   shell's usual `umber_export::png::read_png_rgba8` (main.rs:1410,
   brush_panel.rs:1269). `read_png_rgba8` flips rows to GPU bottom-up
   order (png.rs:191-197), which would mirror a y-down screen stencil;
   test 6 pins this. The brief's pointer is off: **mesh_map_import.rs
   has no decoder** (it only queues bytes; main.rs decodes them), and
   psd_import.rs has its own PSD reader. No new dependencies.
5. **The cap precedent is psd_import.rs:118-126** (`MAX_PSD_SIDE`,
   `MAX_PSD_BYTES`, `MAX_PSD_FILE_BYTES`), **not mesh_map_import.rs**:
   that file has no caps (I grepped `const MAX_` across umber-app).
   - The cap runs twice. First on the IHDR's declared dims, before the
     decoder accumulates IDAT or allocates its output: bytes 12..16
     must be `IHDR`, with the big-endian w/h at 16..24.
   - Then again on the decoded dims, which are authoritative.
   - The pixel cap does not bound the file read itself: `fs::read`
     happens first. A file-byte cap like `MAX_PSD_FILE_BYTES` is a
     cheap follow-up (see Deferred).
6. **Composition is `min`, not bitwise `&`.** The two are identical for
   binary 0/255 gates. Only `min` is correct once soft edges land
   (design Open question 3), and test 3 pins that.
7. **The meaning of "order"**: `min` commutes. The contract "stencil
   first, quick mask second" is about who writes the texture last. The
   quick mask's incremental rect republish must land on top of the
   composed gate, which is why the hook belongs in `gate_plane`.
8. **The gate target is the active tile's quick-mask plane**:
   `quick_mask_for_tile(active_tile()).dimensions()`
   (paint_state.rs:2020, :4260). The quick mask is target-sized per set,
   and it is the plane the stencil ANDs with. Reset fits to that same
   size, which matches core's v1 boundary "target texel space IS the
   transform's screen space". With no paint session, Reset fits to the
   image's own size (scale 1), and nothing is staged.
9. **Reset keeps `fit_to_viewport`'s largest-fit scale**, which can be
   above 1. The design's chord text says "scale 1"; core's judgment
   call 11 handed that conflict to this kill. I kept the built behavior,
   because the real Reset needs a real viewport rect, and that only
   arrives with step 3's chords. Step 3 should settle it (e.g. clamp
   with `.min(1.0)`).
10. **A successful load activates and Resets.** Loading is the request
    to paint through it. A refused load keeps the previous image (test
    8).
11. **Opacity does not re-stage.** It is overlay-only, and the gate is
    binary regardless.
12. **Mismatched gate lengths fail CLOSED** (all blocked, logged),
    following umber-paint's "nothing paints ungated" rule (the
    `publish_mask` doc).
13. **The N hint and the pending line are disabled labels**, following
    the `overlays.face-orientation-pending` precedent. Neither N nor the
    paint gate works yet, and the UI says so.
14. **One catalog key per load refusal**, following mesh_map_import.rs:142's
    `UnmatchedReason` → `import.reason-*` mapping.
    `StencilLoadError::reason_key()` maps:
    - `Io` → `overlays.stencil-unreadable`
    - `TooLarge` → `overlays.stencil-too-large` (`$width $height $max`)
    - `Decode` and `Image` → `overlays.stencil-unsupported`; both mean
      "not an image we read"

    `localized()` renders the key inside the `overlays.stencil-load-failed`
    frame. The English thiserror detail goes only to `log::warn!`.
17. **`stage_gate` runs on the UI thread on every load, toggle or Reset.**
    It rasterizes at the quick-mask plane size, which at a 4K target is
    about 16M f64 bilinear samples plus two 16 MB buffers: a visible
    hitch. Today its only consumer is the `open %` label. The hook kill
    inherits this cost and should either move the rasterize off-thread
    or keep it change-driven (it is never per frame).
15. **No requirements tick (rule 9).** g6-stencil-overlay closes with
    step 3 and the hook.
16. **The screen↔UV gap** (core judgment call 4) is carried forward
    unchanged.

## Unverified

- **NOTHING COMPILED OR RAN.** No cargo: the host is down and the brief
  forbids it. The battery (`wave-battery.sh umber-app umber-core`) is
  the first compile. It needs the parked umber-core stencil module
  committed or present.
- **Rung 1 (rustfmt --check): NOT RUN.** `~/.cargo/bin/rustfmt` and bare
  `rustfmt` both returned `This command requires approval`. I
  hand-formatted against the defaults (max_width 100, fn_call/chain
  width 60, struct_lit_width 18), and rewrote chains that might have
  been ambiguous into let-bindings (e.g. the opacity `ValueField`, the
  test `load()` helper). `wc -L`: stencil.rs 95, display_panel.rs 100
  (≤ 100). Brace/paren balance: stencil.rs `{}` 103/103, `()` 487/487;
  display_panel.rs `{}` 175/175, `()` 784/784. **Run `cargo fmt --all
  --check` first.**
- **Rung 2 (API resolution): RUN, every API checked against the tree.**

  | API used | Anchor |
  |---|---|
  | `StencilImage::from_alpha/width/height/coverage`, `StencilTransform` (+Default, PartialEq, Copy), `StencilError`, `rasterize_gate(&img, &t, w, h)`, `fit_to_viewport(u32,u32,f32,f32)` | umber-core stencil.rs:94-336 (on disk, parked) |
  | `umber_core::stencil` path | umber-core lib.rs `pub mod stencil;` (parked, the stencil-core lock) |
  | `umber_graph::{decode_png_rgba8, encode_png_rgba8, ImageError}` | umber-graph lib.rs:33; value.rs:233, :367, :49 |
  | `QuickMask::{new, dimensions, gates, coverage, empty_value, set_active, set_editing}` | umber-brush quick_mask.rs:140-240 |
  | `PaintState::{active_tile, quick_mask_for_tile}` | paint_state.rs:4260, :2020 |
  | `ValueField::new(&mut f32).context_menu(prop_menu_labels()).default_value().range().step()` | display_panel.rs `overlay_rows` (same chain) |
  | `i18n::tr`, `i18n::tr_args(key, [(&str, FluentValue)])` | i18n.rs:271, :291 |
  | `rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_file()` | brush_panel.rs:1265-1267 |
  | kittest `build_ui_state` / `get_by_label().click()` / disabled click | display_panel.rs `overlay_popover_rows_round_trip_the_toggles` |
  | `thiserror` / `log` deps | umber-app Cargo.toml:34-35 |
- **Rung 3 (Python port + mutation): WRITTEN, NOT EXECUTED.**
  `python3 - <<'EOF'` returned `This command requires approval`. The
  port (below) covers only MY logic: composition, the gate_bytes
  branches, the stage seam, open_percent, the guard and the quick-byte
  gating. The rasterize math is core's, and core already
  mutation-verified it. Hand-traced expectation:

  | # | Mutation | Killed by (Rust test / port case) |
  |---|---|---|
  | C1 | `min` → bitwise `&` | test 3, soft case (128&64=0 ≠ 64) |
  | C2 | `min` → `max` | test 3, table row (gives [255,255,255,0]) |
  | C3 | mismatch fails OPEN (stencil passes) | test 3, mismatch row |
  | G1 | gate_bytes drops the `active` check | test 2 (inactive gives Some), stage test (inactive staged) |
  | S1 | stage composes even with no stencil (quick-only staged) | stage test, last case (inactive + quick [255;16] staged) |
  | P1 | open_percent rounds instead of floors | stage test (37.5 → 38 ≠ 37) |
  | Z1 | guard `<=` → `<` | test 4 (4096² and 6-vs-6 refused) |
  | Q1 | quick gating ignores `editing` | test 10 (mask mode gives Some) |

  ```python
  M = None
  def compose(s, q):
      if s is None and q is None: return None
      if q is None: return list(s)
      if s is None: return list(q)
      if len(s) != len(q): return [0] * len(s) if M != 'C3' else list(s)
      if M == 'C1': return [a & b for a, b in zip(s, q)]
      if M == 'C2': return [max(a, b) for a, b in zip(s, q)]
      return [min(a, b) for a, b in zip(s, q)]
  PAT = [255,0,0,0, 255,255,0,0, 0,255,255,0, 0,0,0,255]
  def gate_bytes(active, image):
      if M != 'G1' and not active: return None
      if image is None: return None
      return list(image)  # identity rasterize == image bytes (core-pinned)
  def stage(active, image, target, quick):
      if target is None: return None
      st = gate_bytes(active, image)
      if st is None and M != 'S1': return None
      v = compose(st, quick)
      return None if v is None else (target, v)
  def open_pct(v):
      if not v: return 0
      n = sum(1 for x in v if x != 0)
      return round(n * 100 / len(v)) if M == 'P1' else (n * 100) // len(v)
  def guard(w, h, mx): return (w * h < mx) if M == 'Z1' else (w * h <= mx)
  def quick_bytes(active, editing, plane, empty, n):
      gates = active if M == 'Q1' else (active and not editing)
      if not gates: return None
      if plane is not None and len(plane) == n:
          return [round(min(max(v, 0), 1) * 255) for v in plane]
      return [round(empty * 255)] * n
  def tests():
      f = []
      if not (gate_bytes(False, PAT) is None and gate_bytes(True, None) is None
              and gate_bytes(True, PAT) == PAT): f.append('gate_bytes')
      s, q = [255,255,0,0], [255,0,255,0]
      if not (compose(None, None) is None and compose(s, None) == s and compose(None, q) == q
              and compose(s, q) == [255,0,0,0] and compose([128], [64]) == [64]
              and compose(s, [255]*3) == [0]*4): f.append('compose')
      if not (guard(4096, 4096, 4096*4096) and not guard(4097, 4096, 4096*4096)
              and guard(3, 2, 6) and not guard(3, 2, 5)): f.append('guard')
      if not (quick_bytes(False, False, None, 0, 4) is None and quick_bytes(True, False, None, 0, 4) == [0]*4
              and quick_bytes(True, True, None, 0, 4) is None): f.append('quick')
      st = stage(True, PAT, (4, 4), None)
      ok = st == ((4, 4), PAT) and open_pct(st[1]) == 37
      st2 = stage(True, PAT, (4, 4), [0]*16)
      ok = ok and st2[1] == [0]*16 and open_pct(st2[1]) == 0
      ok = ok and stage(True, PAT, None, None) is None and stage(False, PAT, (4, 4), [255]*16) is None
      if not ok: f.append('stage')
      return f
  print('baseline:', tests() or 'PASS')
  for m in ['C1', 'C2', 'C3', 'G1', 'S1', 'P1', 'Z1', 'Q1']:
      M = m; r = tests(); M = None
      print(m, 'killed by ' + ','.join(r) if r else 'SURVIVED')
  ```
  Expected: the baseline PASSES and all 8 mutants are killed.
- **Rung 4 (blast radius): RUN.**
  - `git diff -U0` was refused (`requires approval`). `git diff --stat`
    ran instead: main.rs **+12 / −0**, display_panel.rs +162 / −0, each
    .ftl +12 / −0. Every hunk is additive.
  - `grep -n stencil main.rs` lists exactly my 5 hunks, plus a
    pre-existing `depth_stencil_format` comment at :1778.
  - `git status` does not list brush_panel.rs, viewport.rs or lib.rs as
    modified, and `grep -c stencil` gives 0 for each of them.
  - umber-core files: read, not modified.
- **The brief's test 4 is only partly met.** It asked for
  `load_png_rgba` on a generated PNG. The /tmp ban rules out writing a
  file, so the PNG tests drive `stencil_from_png_bytes`.
  `load_png_rgba`'s success path (`fs::read` → decode) has **no test**;
  only its `Io` refusal is covered (test 7). The function adds
  `fs::read` and the default cap, nothing else.
- **Runtime / GPU: nothing.**
  - The kittest is written to the file's own precedent but never ran.
  - Fluent may wrap placeables in Unicode isolation marks; no test pins
    the rendered pending/load-failed strings.
  - Whether rfd's dialog behaves inside the Display tab's ScrollArea is
    untested; it's the same inline call as the brush_panel stamp
    chooser.
- **Clippy: not run.** By reading:
  - Every non-test item in the bin-only `stencil` module has a non-test
    caller, so there is no dead_code and no `#[allow]`.
  - No fn has more than 7 params; there is no derivable Default (opacity
    0.5 is tuned); no `source` field; every `matches!` has 2 args; no
    field access inside format strings.
  - The Response from `ui.add` is acknowledged with `let _ =`.

## Deferred

- **THE HOOK (the next kill, umber-paint surface):**
  - a `PaintState::set_stencil_gate(Option<(w, h, Vec<u8>)>)` that
    `gate_plane` (`paint_state.rs:5158`) multiplies in, the way it does
    the cavity plane (`:5171-5179`);
  - `|| self.stencil_gates()` added to `stroke_mask_gate` (`:1714`);
  - clearing `mask_synced` on a stencil change.

  After that, `stage_gate` hands `staged` over instead of parking it.
  The quick-mask mirror (`quick_mask_gate_bytes`) and the shell-side
  compose then become redundant for the upload, but stay useful as the
  pinned contract. Recorded in stencil.rs's module docs (v1 boundary)
  and on `stage_gate` (`TODO(stencil-paint-hook)`).
- **The screen↔UV projection** (core judgment call 4): the hook kill or
  step 3.
- **Step 3:**
  - the popover call to `stencil_rows` (viewport.rs:820);
  - the overlay quad draw (`overlay_opacity`, `forward_map` corners);
  - the S+LMB/RMB/MMB chords;
  - the N toggle;
  - Reset against a real viewport rect, settling the scale-1 conflict.
- A file-byte cap before `fs::read` (the `MAX_PSD_FILE_BYTES`
  precedent).
- Clipboard source (Open question 1), per-set stencils (Open question 2)
  and soft edges (Open question 3); the composition is already
  `min`-correct for soft edges.
- `staged` is a snapshot taken at stencil-change time. Quick-mask edits
  made after it don't refresh it. That's moot once the hook composes
  inside `gate_plane`.

## Gates run

- `~/.cargo/bin/rustfmt --check --edition 2021 …/stencil.rs`: **NOT RUN**;
  verbatim `This command requires approval`. Bare `rustfmt`: the same.
- `bash scripts/verify-kill.sh crates/umber-app/src/stencil.rs
  crates/umber-app/src/display_panel.rs crates/umber-app/src/main.rs
  assets/i18n/en-US.ftl assets/i18n/pt-BR.ftl`: **NOT RUN**; verbatim
  `This command requires approval`.
- `python3 - <<'EOF'` (the port): **NOT RUN**; verbatim `This command
  requires approval`.
- `wc -L`: **RUN**, 95 / 100.
- Brace/paren count (`grep -o | sort | uniq -c`): **RUN**, balanced.
- `git diff --stat` / `git status --short` / blast-radius greps: **RUN,
  PASS** (additive only, no off-lock file touched).
- cargo fmt / clippy / test: **NOT RUN** (forbidden; the host is down).

## Dragon consumption record (2026-10-17, this loop session)

- Host still DOWN (second freeze, ~19:00 onward); battery parked.
- Ladder rung 1 EXECUTED: rustfmt --check clean on stencil.rs,
  main.rs, display_panel.rs (the claw's hand-formatting had zero
  drift on all three).
- verify-kill: stencil.rs 103/103, display_panel 175/175; main.rs's
  876/877 raw brace count is PRE-EXISTING AT HEAD (same 876/877 at
  HEAD, 740/740 balanced with string literals stripped) — the
  documented raw-count false-positive class; the claw's main.rs
  hunk added no imbalance.
- Ladder rung 3 EXECUTED: the report's Python port ran (exit 0):
  baseline PASS; mutants C1/C2/C3/G1/S1/P1/Z1/Q1 all killed by
  their pinned cases (9/9).
- State: STATICALLY VERIFIED, IN FLIGHT — battery + commit pending
  the build host's return, landing WITH stencil-core as one wave.


## Cross-review addendum (2026-10-17 20:4x, the 18:55 loop session)

The report's headline claim-chain (the design's shell->write_mask
wiring is impossible; the composition belongs in `gate_plane`)
verified line-anchor by line-anchor against the tree, host-free:

- `stroke_mask_gate` (paint_state.rs:1236, armed in begin_stroke),
  `sync_mask_tile` (:5205), `publish_mask` (:5227), `gate_bytes`
  (:5184), `gate_plane` (:5158), `PaintThreadCommand::WriteMask`
  (paint_thread.rs:100), `write_mask_region` (paint.rs:579),
  `with_mask_gate(true)` on dabs (:4848), `quick_mask_body`
  (brush_panel.rs:1096), `publish_layer_mask` as the LANE-MASK
  pub (:2243) — every anchor exists at the cited line.
- The stencil-core dependency the app claw coded against is
  MATH-VERIFIED (see that report's cross-review addendum: the Python
  port passes all five contract cases, mutations caught). The
  seam design (stage_gate + the on-screen "gate pending" notice) is
  the honest shape for a gap the design doc missed.
- i18n: 9 stencil keys in en-US, 12 matches in pt-BR — both catalogs
  consistent (the pt count includes header/section lines; every
  en-US key has a pt counterpart).

**Verdict: stencil-app's claims verify. The paint-hook kill
(`gate_plane` composition) is the design's true step and is now
NAMED, not implicit — the design doc's wiring section should be
amended when that kill lands (spec-amendment protocol).**
