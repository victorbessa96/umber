# Kill brief: R-D1 baker sync + linked bake groups (W5, census D1, P1)

- Status: BRIEF ONLY, written 2026-11-11 while the dispatch is LOCK-BLOCKED
  (see "Dispatch gate" below). No code touched by this brief.
- Census source: docs/research/spp-parity-census-2026-10-16.md §D row D1:
  Painter has linked bake groups across texture sets, sync/copy/paste baker
  settings, per-map auto-rebake, per-UV-tile bake exclusion. Umber state
  (verified 2026-11-11): auto-rebake LANDED; linked groups ABSENT;
  settings copy/paste ABSENT.

## Verified starting state (measured on 73bc5f51)

- The bake model is app-side: `crates/umber-app/src/bakes_panel.rs` carries
  `BakeSelection` (:167), `BakeRecord` (:262), `BakeSource` (:283),
  `BakeJob` (:293), `BakePlan` (:305, already multi-job — the fan-out
  shape exists), `BakeSettingId` (:383, the settings-row identity the bake
  log jumps to), `BakeLog`/`BakeLogEntry` (:401/:480), `SetCheck` (:540),
  `AutoRebakeFire` (:552), and the auto-rebake state: field :747, getter
  :1047, fire gate :1243 (`!auto_rebake || is_baking || !rebake_due`),
  checkbox :1512. Baker modules live in `crates/umber-bake/src/`
  (ao/curvature/dilation/gbuffer/id/margin/normal_map/position/skew/
  thickness/transfer).
- `crates/umber-core/src/project.rs` holds the .umber doc model the
  persistence half needs (it currently has no bake-settings block).

## Dispatch gate

- Lock `razul-w5-channel-rename-planes` (held since 2026-11-11 14:0x by the
  dispatched rename subagent) covers `crates/umber-core/src/project.rs`,
  `crates/umber-app/src/main.rs`, both `assets/i18n/*.ftl` and queue.md.
  R-D1 needs project.rs (persistence) and the .ftl files (new `bakes.*`
  keys), so THIS KILL QUEUES BEHIND THE RENAME KILL. Do not dispatch until
  that lock is released.

## Build (dispatch-time lock: `razul-rd1-baker-sync`)

- Lock surface at dispatch:
  - `crates/umber-app/src/bakes_panel.rs`
  - `crates/umber-core/src/project.rs` (+ its tests)
  - `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl`
  - `docs/loop/queue.md` + this brief's kill report
- 1. **Settings copy/paste**: an internal (in-memory, session-scoped)
  clipboard carrying the selected set's bake settings as a
  `(baker-kind -> ParamValue map)` plus the out-dir row. Copy from the
  settings group; paste applies to the selected target set(s). Refuse a
  paste whose target has no baker of a carried kind (no silent drops);
  a partial paste is a refusal, not a best-effort apply.
- 2. **Linked groups**: a named group of texture sets that share ONE
  settings block. Editing any member's bake settings writes the group
  block; baking a member fans out one `BakeJob` per member set (the
  existing `BakePlan` multi-job shape) with the shared settings. Unlink
  materializes the group block into the member as its own per-set copy, so
  later edits diverge. baseColor-protected sets follow the channel rules
  already in core.
- 3. **Persistence**: the group membership + the shared/per-set settings
  block serialize into the `.umber` via `project.rs` (new optional block,
  absent = no groups, so old files load unchanged). Round-trip byte-stable
  where the existing project codec is.
- 4. Panel UI: a "Link sets" / "Unlink" action next to the settings group
  plus copy/paste buttons; all new labels via `bakes.*` keys in BOTH
  catalogs (ftl completeness is a battery gate).

## Tests

- Persist round-trip: group + settings save/load identical; an old .umber
  without the block loads with zero groups.
- Fan-out: baking a 3-member linked group emits exactly 3 jobs with equal
  settings; editing through member A is visible when reading member B.
- Unlink: after unlink, editing A leaves B unchanged; B keeps the
  materialized copy.
- Copy/paste: paste onto a set with the same baker set applies every
  carried value; paste onto a set missing one baker refuses with the
  missing kind named.
- No test weakened to pass; new behavior gets new tests.

## Rules

- Edit only the locked surface at dispatch. No behavior change to existing
  bakers or to auto-rebake's fire gate.
- Battery before commit: `rustfmt --edition 2021 --check` on touched .rs,
  `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`,
  `cargo clippy -p umber-app -p umber-core --all-targets -- -D warnings`
  (add other touched crates), `cargo test -p umber-app -p umber-core` 0
  failed, under `flock /tmp/umber-cargo.lock`.
- Commit via `.dragon/dcommit.py --kill-lock razul-rd1-baker-sync --lock
  <paths> -- -m "..."`; push via `scripts/safe-push.py` in background.

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit,
  ls-remote origin hash check.
