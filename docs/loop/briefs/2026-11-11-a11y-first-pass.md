# Kill brief: wave-a11y-first-pass (G7 keyboard-only journeys, actually landed)

- Status: BRIEF, written 2026-11-11 by the dragon. The gui-plan row
  `g7-keyboard-only` is ticked ✓ at e008dcc, but the 2026-10-24 lens audit
  (docs/loop/audit-2026-10-24-lens-uiux.md:32) records it OPEN: the tab
  order / focus-ring / Esc / number-menu journeys were never exercised.
  ROADMAP row 234 carries wave-a11y-first-pass as open. This kill closes
  the gap with kittests, not with a tick.
- Census/plan sources: docs/specs/gui-plan-2026-10-15.md §G7;
  docs/loop/audit-2026-10-24-lens-uiux.md G7 row; ROADMAP.md row 234.
- What already exists: AccessKit sweep d9fa931 (`widget_info` role+label
  on every painter-drawn widget, kittest per panel; audit F11 added icon
  labels); density scale; polish batch. So ROLE/LABEL coverage is largely
  done — this pass is the KEYBOARD JOURNEYS plus any role gaps the
  journeys expose.

## Dispatch lock (razul-a11y-first-pass)

- `crates/umber-gui/src/` (widgets: tab order support, focus ring token,
  Esc handling primitives)
- `crates/umber-app/src/` panels + `main.rs` input dispatch (number-key
  menu picks, Esc closes popups, panel focus traversal across the dock)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (only if new hint keys)
- `docs/loop/queue.md` + `docs/loop/kill-reports/2026-11-11-a11y-first-pass.md`

## Build

1. **Kittests first (they define done):** keyboard-only journeys
   import → paint → export: Tab/Shift+Tab traversal reaches every dock
   panel and every interactive widget in the focused panel in a sane
   order; the focused widget has a VISIBLE focus ring (theme token, not
   ad-hoc color — theme-aware per the U4 rules); Esc closes any open
   popup/menu/dialog without side effects; number keys 1-9 pick the
   corresponding menu item when a menu is open. Drive them with the
   existing kittest harness (references/kittest-egui-testing.md).
2. **Fix what the journeys expose**, minimal edits in the locked surface:
   egui `Memory::focus` / `ui.memory_mut(|m| m.request_focus(id))` wiring
   for custom widgets that swallow Tab; a focus-ring visual in
   `theme.rs` helpers; Esc routing in the app's input dispatch; number-key
   menu picks in the menu bar code.
3. **AccessKit role/label gaps** the journeys find (a widget reachable by
   Tab but reporting no role) get `widget_info` like the d9fa931 sweep.

## Rules

- Battery before commit: rustfmt --check on touched .rs, check-layers,
  scorecard --check, `cargo clippy -p umber-gui -p umber-app --all-targets -- -D warnings`,
  `cargo test -p umber-gui -p umber-app` 0 failed, all under
  `flock /tmp/umber-cargo.lock` with `UMBER_ASSETS_DIR=<worktree>` exported.
- No behavior change to pointer/mouse paths; theme-aware colors only
  (never raw Color32); both .ftl catalogs complete if keys are added.
- No weakened tests. Snapshot tests: regenerate only if a panel snapshot
  legitimately changes (UPDATE_SNAPSHOTS=1 ... --test panel_snapshot, then
  delete *.new.png/*.diff.png).
- If the full journey set is too big for one kill, land import→paint and
  name export as the deferred follow-up with the reason.

## Dragon verification

- Cross-review diff vs this brief, battery on the landed commit
  (UMBER_ASSETS_DIR set), ls-remote tip check, worktree/branch prune.
