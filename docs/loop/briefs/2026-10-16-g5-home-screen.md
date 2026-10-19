# Kill brief: g5-home-screen (GUI plan G5)

You are implementing one kill of Umber's GUI plan, in the Rust workspace
at ~/Projects/umber (current HEAD: ee1a078). Work ONLY on the files your
lock holds. Other sessions are editing other files in the same tree —
do not touch anything outside your lock surface, do not run cargo
(the build host owns cargo; the dragon runs the battery), do not commit.

## What to build

Turn the empty-state hero into a HOME CARD (GUI plan G5, kill
`g5-home-screen`): New / Open with live shortcuts, up to 6 recent
projects (hover ×, context Remove / Clear), a drop hint, "Recover last
session" from the autosave journal, and double-click-empty = Open.

Current state:
- `crates/umber-app/src/empty_state.rs` — the drop-zone hero
  (`paint_hero(ui, rect, files_hovering)`, painter-drawn: zone rect +
  3 text lines; NO widgets — it draws over the viewport).
- `crates/umber-app/src/viewport.rs:543-548` — where the hero paints
  when `self.mesh.is_none()`.
- `crates/umber-app/src/recent.rs` — `RecentProjects` (paths, note,
  remove, clear, existing, load/save; capped? check `note`).
- `crates/umber-app/src/recovery.rs` — the autosave journal (the
  pixel-autosave kill C2-3; find its public API: snapshot presence +
  the recovery prompt's data).

## Requirements (from the plan row)

1. The hero becomes a REAL egui panel (widgets, not painter text):
   allocate the zone rect inside the viewport's Ui when no mesh is
   loaded (`ui.allocate_ui_at_rect`), and inside it draw:
   - Title + hint (existing i18n keys `shell.hero-title` /
     `shell.hero-hint` / `shell.hero-formats` — reuse them).
   - A "New project" button (localized) and an "Open…" button
     (localized) — they do NOT need to work in this kill (the shell's
     file dialogs live in main.rs, outside your lock): expose a
     `HomeCardAction` enum (`NewProject`, `OpenProject`,
     `OpenRecent(PathBuf)`, `RecoverSession`) returned from your
     `home_card(...) -> Option<HomeCardAction>` call, and the caller
     (viewport.rs, inside your lock) forwards it — viewport.rs may
     store it in its return value / the App calls viewport's show
     then reads it. Read how viewport's `show` returns data today
     and follow that pattern. main.rs wiring is the dragon's.
   - Up to 6 recent projects: rows (file name + dimmed parent dir),
     hover shows an × button (remove from recents), right-click a
     context menu with Remove / Clear all. Clicking a row =
     `OpenRecent(path)`. Gray out rows whose path no longer exists
     (`RecentProjects::existing` gives you (path, exists) pairs).
   - "Recover last session" row, shown only when the autosave journal
     holds a snapshot (recovery.rs's public API); click =
     `RecoverSession`.
   - The drop hint (hero-release when hovering) stays: keep the
     accent wash + outline behavior from paint_hero.
2. Keep the existing pure tests in empty_state.rs passing (zone
   geometry, extension routing, formats line) — the zone math is
   your layout's base.
3. Persistence: recents are already persisted by recent.rs; the
   home card only reads. Remove/clear go through RecentProjects.
   Your card needs `&mut RecentProjects` (the App owns it — check how
   the file menu reads it today and mirror the borrow pattern via
   parameters, not global state).
4. i18n: every new string through `crate::i18n::tr()`; BOTH catalogs:
   `~/Projects/umber/assets/i18n/en-US.ftl` and `pt-BR.ftl` (workspace
   root assets/i18n/, NOT crates/umber-app/assets). Fluent syntax.
   The × and Clear/Remove context items need keys too.
5. Tests (pure): the 6-recent cap (does RecentProjects cap at 6? if
   it caps at N≠6, adapt the card to the existing cap and say so);
   the exists-graying decision fn; HomeCardAction selection logic
   where pure. A kittest is NOT required in this kill (the card needs
   the App state; the dragon may add one at integration).
6. Docs: module doc citing docs/specs/gui-plan-2026-10-15.md
   g5-home-screen. Kill report at
   `docs/loop/kill-reports/2026-10-16-g5-home-screen.md` (shape:
   Built / Judgment calls / Unverified / Deferred / Process note).

## Rules

- egui 0.36: context menus via `ui.menu_image_button` are NOT
  available; use `response.context_menu(|ui| ...)` on any response.
  `egui::Modal` does not exist in 0.36.
- Rust 2021, clippy -D warnings clean; no new dependencies.
- Do not edit docs/ROADMAP.md.
- When done, write the kill report and reply with: files changed,
  ftl keys added, test names, judgment calls.
