# Kill report: g5-recent-projects (File ▸ Open Recent)

Date: 2026-10-15. Author: dragon. Lock: `g5-recent-projects` over
`crates/umber-app/src/{recent,main}.rs`, both ftl catalogs and this report.
Plan: `docs/specs/gui-plan-2026-10-15.md` row `g5-menu-completion`, the
File ▸ Recent part (P:A1, B:A3.2). The rest of that row (Revert, Save As /
Save Copy, Close, the Window and Help items) stays open.

## Built

- `recent.rs`: `RecentProjects`. It is pure: most recent first, deduped,
  capped at 10 (`note`, `remove`, `clear`). `existing(exists)` flags dirs
  that are not reachable. `to_text`/`from_text` store one path per line
  and tolerate blank lines, CRLF, padding and duplicates. `load` treats
  a missing file as empty, and `save` creates the parent dir. The file is
  `recent`, beside the locale preference in the per-user Umber dir.
- `main.rs`:
  - The list loads at startup.
  - `open_project` splits into the dialog plus `open_project_at(dir)`,
    so a Recent row runs exactly the dialog's open path.
  - A successful open or save notes the dir and persists the list.
  - A dir that exists but fails to load leaves the list.
  - File ▸ Open Recent is a submenu under Open Project. Rows show the
    dir name, with the full path on hover. An unreachable dir draws
    disabled, with the reason as its hover text. Clear Recent empties
    the list. With an empty list the entry is disabled.
- Catalogs: `menu.recent`, `.recent-empty`, `.recent-missing`,
  `.recent-clear`, both locales.

## Tests (6, `recent::tests`)

Move-to-front + dedup + the cap (the oldest falls off), remove/clear,
text round-trip plus a messy hand-edited file, load/save through a real
file (missing reads as empty, the parent dir is created), existence
flags + display names, and the file living beside the locale pref.

## Judgment calls

- **Unreachable stays listed, broken leaves.** A missing dir (an
  unplugged drive) stays in the list, greyed. A dir that loads with an
  error is dropped, since reopening it would fail the same way.
- **No hover preview** (the plan row's "with a hover preview"): it needs
  a project thumbnail, which projects do not store yet. The hover shows
  the full path instead.
- **One file, no schema.** One path per line is the least that works.
  A hand-edited file cannot stop the app from starting.

## Unverified

- The submenu draw and click have no kittest. Gate 1: umber-app fmt,
  clippy, 442 tests.

## Deferred

- The rest of g5-menu-completion. The home-screen recent cards
  (g5-home-screen) read this same list.
