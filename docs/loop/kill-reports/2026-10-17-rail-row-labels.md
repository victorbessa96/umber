# Kill report: rail-row-labels (2026-10-17)

The named follow-up of W-GUI-3 shell-halves: the Properties ContextRail's
Ctrl+F search now has a vocabulary. Each of the five context bodies gains
a pure label fn beside it, and the Properties arm registers the active
kind's labels before `rail.ui` every frame. HEAD at dispatch: a394876.

**What this kill does NOT do:** Ctrl+F still greys nothing and scrolls
nothing. The rail greys a row and scrolls to a hit only when the body
draws that row through `PropertyRows::row(ui, index, …)`
(`crates/umber-gui/src/widgets/context_rail.rs:490`). The five bodies
still draw their rows directly, and the brief froze their draw code.
After this kill `rail.matches()`, `current_hit()`, Enter / Shift+Enter
cycling and `ContextRailOutcome::jumped_to` all work on real labels, but
nothing on screen shows it. That visible half is the follow-up (see
Deferred).

## 1. Built

Files touched (all six are under the `rail-row-labels` lock;
`tile_selection.rs` needed no change):

- `crates/umber-app/src/brush_panel.rs`
  - `const ROW_LABEL_KEYS` (44 keys) and `pub fn row_labels()`, after
    `impl Default for BrushPanel`.
  - The doc on that const holds the shared inclusion rule and the static
    v1 boundary.
  - `#[cfg(test)] mod row_label_tests`, 1 test.
- `crates/umber-app/src/document.rs`
  - `const LAYER_ROW_LABEL_KEYS` (3) and `pub fn layer_properties_row_labels()`,
    after `layer_blend_options`.
  - `const MASK_ROW_LABEL_KEYS` (1) and `pub fn layer_mask_row_labels()`,
    after `layer_mask_ui`.
  - Two `#[cfg(test)] pub(crate)` helpers: `drawn_source` and
    `assert_row_label_keys`. They live here because `document.rs` is
    compiled into both the lib and the bin; `main.rs` is bin-only.
  - `#[cfg(test)] mod row_label_tests`, 1 test.
- `crates/umber-app/src/fill_layer.rs`
  - `const ROW_LABEL_KEYS` (12) and `pub fn row_labels()`, before
    `fill_section_ui`.
  - `mod row_label_tests`, 1 test.
- `crates/umber-app/src/path_tool.rs`
  - `const ROW_LABEL_KEYS` (8) and `pub fn row_labels()`, before
    `path_tool_rows`.
  - `mod row_label_tests`, 1 test.
- `crates/umber-app/src/main.rs`
  - `fn properties_row_labels(ContextKind) -> Vec<String>`, the pure
    dispatch, after `properties_kinds`.
  - `const TEXTURE_SET_LABEL_KEYS` (3) and `fn texture_sets_row_labels()`,
    after `texture_sets_panel_ui`.
  - The arm wiring:
    `self.properties_rail.set_labels(properties_row_labels(body_kind))`,
    placed right after `let body_kind = kind;` and before `.ui(ui, &rail_ctx, …)`.
  - `#[cfg(test)] mod properties_row_label_tests`, 4 tests.

**Tests added: 8.**

| # | File | Test | What it pins |
|---|---|---|---|
| 1 | brush_panel.rs | `the_brush_row_labels_are_the_rows_the_panel_draws` | (a)+(b); count 44; the order's ends are "Tool" and "Dash length" |
| 2 | document.rs | `the_layer_and_mask_row_labels_are_the_rows_the_bodies_draw` | (a)+(b) for Layer and Mask; exact lists `["Set opacity", "Set blend mode", "Rename layer"]` and `["Masks"]` |
| 3 | fill_layer.rs | `the_fill_row_labels_are_the_rows_the_section_draws` | (a)+(b); count 12; ends "Fill layers" and "Capture clicks (lasso)" |
| 4 | path_tool.rs | `the_path_row_labels_are_the_rows_the_tool_draws` | (a)+(b); the exact 8-label list |
| 5 | main.rs | `the_texture_set_row_labels_are_the_rows_the_panel_draws` | (a)+(b), with the grep run on `texture_sets_panel_ui`'s own source slice; the exact list |
| 6 | main.rs | `each_kind_registers_its_own_bodys_labels` | (c): each of the five kinds equals its body's fn; the five lists are pairwise distinct, so swapping two arms fails; the Fill union (fill + path) is unique and has 20 labels |
| 7 | main.rs | `the_registered_labels_drive_the_rails_search` | The real `ContextRail` consumes the vocabulary (detail below) |
| 8 | main.rs | `the_properties_arm_registers_the_labels_before_drawing_the_body` | Source order in the arm: kind read < `set_labels` < `.ui(ui, &rail_ctx,` |

Test 7 in detail:

- Brush `"jitter"` matches rows `[34, 35, 37]`. The position jitter's label is "Scatter", so it doesn't match.
- Enter moves to 35, and re-registering the same list keeps 35.
- After switching to Layer, `"blend"` hits row 1.

What `assert_row_label_keys`, used by tests 1–5, checks on every list:

- Non-empty.
- No empty key.
- `tr(key) == tr_en(key)` and `tr(key) != key`. `tr` falls back to the key itself, so a typo'd key would otherwise pass.
- Rendered labels are non-empty.
- Keys are unique, and so are the rendered en-US labels.
- Every key appears as a quoted literal `"key"` in the body source. That source is `drawn_source`: the file cut before `mod tests {`, with the module's own `const …_KEYS` block removed, so the list cannot satisfy its own check.
  - The match is on the quoted literal, not on `tr("key")`, because the brush jitter rows pass their keys through `tr(key)` from a tuple array.

No i18n keys were added, so the `.ftl` files are untouched.

## 2. Judgment calls

1. **The lists are static, and the v1 boundary is written in every const's doc.**
   - Each list is the union of every labelled row the body can draw, de-duplicated, in first-draw order. That matches the brief's zero-arg `row_labels() -> Vec<String>`.
   - The tool-, stamp-, method-, projection-, source- and session-gated rows stay listed while hidden. So a search can count a hit on a row the body isn't drawing this frame.
   - This was chosen over state-filtered lists for two reasons:
     - Row indices stay stable, which the `PropertyRows::row(i)` follow-up needs.
     - `ContextRail::set_labels` restarts the search on any list change, so a state-filtered list would reset an in-progress search every time a toggle flipped.
2. **One inclusion rule for all five bodies**, stated on `brush_panel::ROW_LABEL_KEYS`:
   - Included: section titles, a labelled row's leading label, and a checkbox that is its own row.
   - Skipped: radio and combo option values, hints and weak notes, buttons, status readouts (`fill.poly-points`, `fill.poly-pick`, `path.points`, `path.selected`), empty-state and fallback lines (`fill.none`, `path.no-paint`, `path.need-points`, `texturesets.no-paint`), glyph rows (R/G/B/A, ◐/○), and data labels (layer, set and channel names).
3. **The brush preset editor's hardcoded English rows are skipped.** These are "Preset", "Name", "Color", "Alpha", "Hardness", "Pressure gamma", the Stabilizer, Lazy mouse and Spacing groups, and the two pressure-curve headers.
   - They draw no `tr()` key. That is the module docs' existing i18n deferral.
   - The catalog does hold `brush.alpha`, but listing it would put a label in the list that the body never draws, and it would not match the drawn text in pt-BR.
   - Consequence: Alpha and Hardness, probably the most-searched brush params, are not searchable until that i18n kill lands. They then join the list.
4. **Layer order follows the code, not the brief.** The brief named name/opacity/blend. `layer_properties_row` draws opacity, then blend, then rename (`document.rs:1343`), so the list is `history.op-set-opacity`, `history.op-set-blend`, `history.op-rename`. The blend row stays listed for Material layers, which skip it (the static boundary in call 1).
5. **The Mask list is the section title only**: `["layers.masks-section"]`. Each per-layer mask row is labelled by the user's layer name. That is data, not a catalog key, and it isn't guaranteed unique, which would break the uniqueness pin. The row's controls are buttons and a glyph toggle.
6. **Fill = `fill_layer::row_labels()` followed by `path_tool::row_labels()`.**
   - That is the arm's draw order. `fill_section_ui` draws `polygon_fill_rows` itself, so the polygon rows sit inside the fill list.
   - `fill.scale` is drawn twice (triplanar scale and noise scale) and is listed once.
   - The section repeats its rows per fill layer, and the list holds one layer's worth.
   - The concatenation lives in `properties_row_labels`, not in either module, because neither body knows about the other.
7. **The path tool reuses `brush.smudge-strength`, `brush.smudge-smear` and `brush.erase-channels`**, because its draw code does. The Fill union stays unique: the brush rows live only in the Brush context, and test 6 pins that.
8. **Visibility.**
   - The four module label fns are `pub`. `document.rs` is also compiled into the lib target, where a `pub(crate)` fn with no lib consumer would trip dead_code under `-D warnings`. The other three are `pub` to match.
   - The test helpers are `#[cfg(test)] pub(crate)`.
   - `properties_row_labels` and `texture_sets_row_labels` are private to the bin.
9. **The const was renamed to `TEXTURE_SET_LABEL_KEYS`**, not `TEXTURE_SET_ROW_LABEL_KEYS`. With the longer name, `.iter().copied().map(tr).collect()` comes to exactly 60 columns, rustfmt's `chain_width`. I couldn't run rustfmt (see Unverified), so I kept every chain and call clear of the 60/100 limits rather than trust my reading of how the boundary rounds.
10. **The registration is `self.properties_rail\n    .set_labels(…)` on two lines.** The one-line chain is 64 columns. The same arm's `self.mesh_import\n    .switch_set(…)` shows the shape rustfmt gives this at statement level.
11. **The wiring test (8) is a source-order grep**, not a rendered kittest of the whole app. The app's Properties arm needs a GPU paint session and the full `UmberApp`, and `tests/panel_kittest.rs` is outside the lock. Test 7 covers the widget half against the real `ContextRail`.

## 3. Unverified

What the ladder covered:

| Rung | Status | Detail |
|---|---|---|
| 1. `rustfmt --check --edition 2021` | **Not run** | Blocked by the session's permission gate, both `~/.cargo/bin/rustfmt` and `rustfmt` on PATH. Substitute: a by-eye check of every added line against max_width 100, fn_call_width / chain_width / array_width 60, vertical arrays for string elements over 10 chars, and rustfmt's import order (self < super < crate). Close calls were restructured off the edges (calls 9 and 10). The dragon's battery must run fmt. |
| 2. API anchors | **Done, by Read** | Details below. |
| 3. Python port + mutation | **Written but not executed** | Mid-session, Bash gated `python3`, `grep`, `awk` and `git`. The port is below for the dragon to run. My hand-trace of its expected output is beside it. |
| 4. Blast radius | **Done, by edit-anchor audit (Read), not by `git diff`** | `git diff` was gated too. Details below. |

Rung 2 details: every anchor was resolved against the tree.

- `ContextRail::set_labels` is at `crates/umber-gui/src/widgets/context_rail.rs:248`. The brief's `context_rail.rs:248` path was missing the `umber-gui/src/widgets/` part. Its unchanged-list keeps-the-hit contract is at :246.
- `PropertyRows::row` is at :490.
- The rest were read at their lines:
  - `layer_properties_row` at `document.rs:1343`
  - `layer_mask_ui` at `document.rs:1474`
  - `fill_section_ui` at `fill_layer.rs:931`
  - `path_tool_rows` at `path_tool.rs:385`
  - `texture_sets_panel_ui` at `main.rs:2455`
  - the Properties arm at `main.rs:935-1035`
  (All pre-edit line numbers.)
- `i18n::tr_en` (`i18n.rs:279`) exists and is `pub`. Test threads get the built-in en-US catalog (`i18n.rs:44-49`).

Rung 4 details:

- Every Edit was an insertion between items, or between two statements in the arm. Each `old_string` anchor's body text was carried over verbatim into `new_string`.
- None of the five body fns had a line changed: `BrushPanel::show` and its row helpers, `layer_properties_row`, `layer_mask_ui`, `fill_section_ui` (and `polygon_fill_rows` / `projection_rows` / `source_rows` / `axis_rows`), `path_tool_rows`, `texture_sets_panel_ui`.
- The dragon should confirm with `git diff -U0 -- crates/umber-app/src | grep '^-[^-]'`. It should print nothing, because every change is additive.

Other gaps:

- No cargo was run (per the dispatch), so none of the 8 tests has been compiled or run.
  - Expected values come from the en-US `.ftl` lines I read. Examples: `brush.tool = Tool` (:476), `brush.method-dash-length = Dash length` (:428), `fill.poly-armed = Capture clicks (lasso)` (:596), and `texturesets.*` (:175-182).
  - The indices `[34, 35, 37]` come from counting the list in order: 0-based, jitter-size is 34, jitter-angle 35, jitter-position 36, jitter-flow 37.
- Type-level assumptions to watch in the first build:
  - `Vec<String> == [&str; N]` in the `assert_eq!`s (`String: PartialEq<&str>`).
  - `assert_ne!(&String, &&str)`.
  - `str::find(&String)` as a `Pattern`.
  - `ContextKind::ALL.into_iter().map(properties_row_labels)`.
- Clippy `-D warnings` was not run. I checked by hand for `redundant_closure` (sidestepped with `.copied().map(tr)`), `useless_vec` (the test uses an array) and dead code across both targets (call 8).
- Runtime behaviour has not been seen. Nothing visible changes yet (see the top), and the per-frame `set_labels` cost (≤ 44 short `tr` lookups plus a Vec compare) hasn't been profiled.

### Rung-3 port (run: `python3 <this block>` from the repo root)

```python
import re, pathlib, sys
root = pathlib.Path('.')
ftl = (root / 'assets/i18n/en-US.ftl').read_text()
cat, group = {}, None
for line in ftl.splitlines():
    m = re.match(r'^([a-z][a-z0-9-]*) *=\s*$', line)
    if m: group = m.group(1); continue
    m = re.match(r'^\s+\.([a-z0-9-]+) = (.*)$', line)
    if m and group: cat[f'{group}.{m.group(1)}'] = m.group(2).strip()

LISTS = {
 'brush': ('crates/umber-app/src/brush_panel.rs', 'ROW_LABEL_KEYS'),
 'layer': ('crates/umber-app/src/document.rs', 'LAYER_ROW_LABEL_KEYS'),
 'mask':  ('crates/umber-app/src/document.rs', 'MASK_ROW_LABEL_KEYS'),
 'fill':  ('crates/umber-app/src/fill_layer.rs', 'ROW_LABEL_KEYS'),
 'path':  ('crates/umber-app/src/path_tool.rs', 'ROW_LABEL_KEYS'),
 'tset':  ('crates/umber-app/src/main.rs', 'TEXTURE_SET_LABEL_KEYS'),
}

def keys_and_drawn(path, const):
    src = (root / path).read_text()
    head = f'const {const}: &[&str] = &['
    start = src.index(head); end = src.index('];', start) + 2
    keys = re.findall(r'"([^"]+)"', src[start:end])
    if const == 'TEXTURE_SET_LABEL_KEYS':      # main.rs: the body fn's slice
        s = src.index('fn texture_sets_panel_ui('); drawn = src[s:src.index('\n}', s)]
    else:                                        # drawn_source(): cut tests + the list
        body = src.split('\nmod tests {')[0]
        bs = body.index(head); be = body.index('];', bs) + 2
        drawn = body[:bs] + body[be:]
    return keys, drawn

def check(name, keys, drawn):
    labels = [cat.get(k, k) for k in keys]
    errs = []
    if not keys: errs.append('empty')
    for k, l in zip(keys, labels):
        if l == k: errs.append(f'{k} not in catalog')
        if f'"{k}"' not in drawn: errs.append(f'{k} not drawn')
    if len(set(keys)) != len(keys): errs.append('duplicate key')
    if len(set(labels)) != len(labels): errs.append(f'duplicate label {labels}')
    print(name, len(keys), 'OK' if not errs else errs)
    return labels, not errs

got, ok = {}, True
for name, (p, c) in LISTS.items():
    k, d = keys_and_drawn(p, c); got[name] = (k, d)
    _, good = check(name, k, d); ok &= good
fill_union = [cat[k] for k in got['fill'][0] + got['path'][0]]
print('fill+path', len(fill_union), 'unique' if len(set(fill_union)) == len(fill_union) else 'DUP')
brush = [cat[k] for k in got['brush'][0]]
print('jitter hits', [i for i, l in enumerate(brush) if 'jitter' in l.lower()])

# The mutated branch: put path.size ("Size") into the brush list, which
# already renders brush.size as "Size". The uniqueness check must fail.
k, d = got['brush']
_, mutated_ok = check('brush+path.size (mutated)', k + ['path.size'], d + '"path.size"')
assert ok and not mutated_ok, 'the uniqueness check does not discriminate'
print('PASS: real lists pass, mutated list fails')
```

My hand-trace of the expected output:

```
brush 44 OK
layer 3 OK
mask 1 OK
fill 12 OK
path 8 OK
tset 3 OK
fill+path 20 unique
jitter hits [34, 35, 37]
brush+path.size (mutated) 45 ['duplicate label […]']
PASS: real lists pass, mutated list fails
```

## 4. Deferred

- **Making search visible.** Each body has to draw its listed rows through `PropertyRows::row(ui, i, …)`, with `i` the row's index in its list (the lists are static so those indices are constants). That changes all five bodies' draw code, which this brief froze, and the body closure would have to use its `&PropertyRows` argument (the arm currently ignores it as `_`). Recorded here and in the `ROW_LABEL_KEYS` doc.
- **Searchable hardcoded brush rows** (Alpha, Hardness, …). They need the brush panel's i18n kill, already recorded in `brush_panel.rs`'s module docs. Their keys join `ROW_LABEL_KEYS` then.
- **State-filtered lists**, which would drop hidden rows: deliberately not done (judgment call 1). Revisit only alongside the `PropertyRows` wiring, if a hit on a hidden row turns out to confuse users.
- **Per-layer rows** in the Mask and Fill lists: skipped (calls 5 and 6).
- **Requirements row:** none. This is a GUI-plan follow-up (g2-properties-context), not a `requirements.md` row.

## 5. Gates run

- verify-kill: **not run.** It is my own pre-finish gate, but the session's Bash permission gate blocked it along with everything else. Run it first: `bash scripts/verify-kill.sh` on the six files. It rewrites them in place with rustfmt, so it will absorb anything my by-eye fmt check missed.
- rustfmt `--check`: **not run** (permission gate; see rung 1). Only the by-eye check above.
- cargo clippy / test: **not run** (forbidden by the dispatch; the dragon's battery runs them).
- `python3 scripts/check-layers.py`: **not run** (gated). No crate or dependency edge changed.
