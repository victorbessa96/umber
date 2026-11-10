# Kill brief: wave-5 channel manage (R-B4 tail) — add/remove/rename channels

- Lock: `razul-channel-manage` over:
  - `crates/umber-core/src/lib.rs` (TextureSet channel command API + tests)
  - `crates/umber-core/src/project.rs` (persistence path if the save path needs a hook)
  - `crates/umber-app/src/main.rs` (texture_sets_panel_ui — the manage section)
  - `crates/umber-app/src/document.rs` (if a Document-level wrapper is needed)
  - `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys, both catalogs, same kill)
- Implements ROADMAP:240 / census R-B4 tail: "channel add/remove/rename open".

## Measured state

- `TextureSet { name, resolution, channels: Vec<Channel>, tiles }` (umber-core/src/lib.rs:83). No add/remove/rename commands exist. The texture-sets panel (main.rs `texture_sets_panel_ui` ~3501) lists per-channel size overrides for the selected set but has no channel management.
- Removing a channel must not strand data silently: the panel rule already keeps "materialized-but-unlisted" channels editable. Decide and document: remove = hide from document declaration (paint planes stay, per the existing convention) or remove + clear planes? DEFAULT: declaration-only removal (consistent with the existing "undeclared channels still edit" rule); clearing planes is a separate destructive confirm. Rename = re-declare under the new name; planes keyed by channel name must follow (check paint_state channels_present / tile channel storage and rename there too, or document the strand rule).

## Build

1. Core: `TextureSet::add_channel(name, kind) -> Result<(), _>` (dup/empty refused), `remove_channel(name)` (baseColor protected — the panel already retains BASE_COLOR_CHANNEL; enforce in core too), `rename_channel(old, new)` (dup refused, baseColor rename refused). Unit tests for each, incl. serialization round-trip via project.json.
2. App: a "Channels" manage section in `texture_sets_panel_ui` for the active set: list declared channels with a rename affordance (text edit committing via the existing `rename_commit` convention) and a remove button with confirm (destructive rule), plus an add row (name field + ChannelKind choice + add button). Wire through Document so the change persists to project.json on save.
3. .ftl keys in BOTH catalogs for every new string.
4. Tests: core unit tests + an app-side headless test if the panel has one (follow existing patterns).

## Rules

- Edit only the locked files. Verify: `cargo test -p umber-core -p umber-app`, `cargo clippy -p umber-core -p umber-app --all-targets -- -D warnings`, rustfmt --check on touched .rs, ftl catalog completeness (both catalogs same key set — there is a completeness script/check). No commit/push.
- Minimal diffs, existing style. If the paint-session rename-follow turns out large, land declaration-level rename with the strand rule DOCUMENTED in the kill report and the paint-side rename as a named follow-up — do not half-wire it.

## Dragon verification

Battery + ftl completeness + commit with every payload path locked + safe-push.
