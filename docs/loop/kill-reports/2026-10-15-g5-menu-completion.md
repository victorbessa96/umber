# Kill report: g5-menu-completion, part 2 (Revert, Window, About)

Date: 2026-10-15. Author: dragon. Lock: `g5-menu-completion` over
`crates/umber-app/src/{main,shell_menu}.rs`, both ftl catalogs and this
report. Plan: `docs/specs/gui-plan-2026-10-15.md` row `g5-menu-completion`
(P:A1-A5, B:A3.2-A3.4). Part 1 was File ▸ Open Recent (ede999b).

## Built

- `shell_menu.rs` holds the pure parts:
  - `default_dock()`: the boot layout, now the one source for both
    `UmberApp::new` and Reset Layout.
  - `ALL_PANELS`, plus `toggle_panel` / `is_open` over egui_dock's
    `find_tab` / `remove_tab` / `push_to_focused_leaf`.
  - `revert_availability(has_dir, dirty)`, which gives a reason key the
    same way `actions::availability` does.
  - `AboutInfo`: version, commit, GPU adapter and backend, as lines.
- `main.rs`:
  - **File ▸ Revert to Saved.** Disabled with its reason when the
    project is unsaved or clean. When enabled it asks first, in the same
    confirm-window shape as File ▸ New, then runs `open_project_at` on
    the project dir, the same path as Open.
  - **Window ▸ Panels.** A checkbox per panel (titles from the new
    `panel_title`, shared with the dock tabs). Unchecking closes the tab
    and checking docks it back.
  - **Window ▸ Reset Layout.** Restores the first-launch layout.
  - **Help ▸ About.** This is now a real window: version, commit, and the
    live adapter name and backend from `GpuContext.adapter`. Copy to
    Clipboard copies the same lines for bug reports. Before, it was a
    label that flashed for one frame inside the menu and read "Wave 2 in
    progress".
- Catalogs: 10 new `menu.*` keys in both locales. The stale
  `menu.about-text` is removed (no caller left).

## Tests (6, `shell_menu::tests`)

The default layout holds each of the 11 panels exactly once. Hide, then
show, returns the exact panel set, and Reset restores it. Closing every
panel and reopening them all loses none. The Revert gate has four cases.
About prints the version, the commit (and "unknown" without one), and
the GPU line. All 10 keys exist in both catalogs through
`I18n::has_key`, not a text split (the CRLF lesson from 121afdd). Gate 1
(umber-app): fmt, clippy, 448 tests.

## Judgment calls

- **The commit hash reads `UMBER_GIT_HASH` at build time, and nothing
  sets it yet.** `release.yml` is cargo-dist generated, and a `build.rs`
  shelling out to git changes the build surface (and breaks tarball
  builds without `.git`). About says "commit: unknown" rather than guess.
  Follow-up: a `build.rs` that reads `GITHUB_SHA` or `git rev-parse` and
  falls back silently.
- **Revert is a reopen, not an in-memory rollback.** It reloads from
  disk through the exact Open path, so a revert can never disagree with
  what an Open would show.
- **Save As / Save Copy / Close not built.** Save is already Save-As
  shaped (it always shows the dialog). A true Save (silent, to
  `project_dir`) plus Save As is a save-semantics change with its own
  overwrite-safety story, so it is its own kill. Close has nothing to
  close to until the home screen (g5-home-screen) exists.
- **Hide UI (Tab) and Help ▸ Shortcuts / Show log are not here.** Hide
  UI is g5-focus-modes. A shortcuts sheet comes with the keymap editor
  (g3-keymap-editor).

## Unverified

- The menu and window draws have no kittest. The Revert reload path is
  `open_project_at`, which shares its coverage with Open.
