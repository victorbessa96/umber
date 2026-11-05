# Design brief — CommandSpec unification (queue row 154), 2026-11-02

Status: DESIGN ONLY. No code written, no cargo (envelope). This is the
"own design wave" row 154 calls for; implementation is a later wave.

## Problem

Umber has three parallel action vocabularies:

1. **Shell actions** — `toolbar::ShellAction` (enum, toolbar.rs:90) with the
   key table in `actions.rs` (`Binding`/`Chord`/`resolve`) and ONE dispatch
   site `App::dispatch` (main.rs:2472, a big `match`; 62 `ShellAction::`
   references in main.rs). Menus are built from the same enum.
2. **Agent steps** — `agent.rs::AgentStep` (26 variants, agent.rs:122) with
   per-step field structs, driven by the JSONL `umber-cli agent` protocol
   (PROTOCOL_VERSION 1) and the 17-tool `umber-cli mcp` catalog (mcp.rs).
3. **MCP tool names** — `mcp_*` names in mcp.rs mapping onto Session steps.

An agent cannot discover what the GUI can do; the GUI cannot be driven by
id from scripts; shortcuts and menu rows live in different tables. Texturecraft's
`CommandSpec` (absorption ledger row 154) is the pattern: every menu action
= `{id, label, menu, shortcut, params, enabled, run}` and menus/CLI/agent/MCP
all dispatch by id.

## Target shape

```rust
// umber-app (shell side); a mirrored, serde-shaped spec crosses the wire.
pub struct CommandSpec {
    pub id: &'static str,            // "layer.add", "viewport.preset.apply" — stable, dotted
    pub label_key: &'static str,     // i18n key, resolved per locale (en/pt catalogs)
    pub menu: Option<MenuPath>,      // File > Export > …; None = not in menus (agent-only)
    pub shortcut: Option<Binding>,   // reuses actions.rs Binding/Chord
    pub params: ParamSchema,         // typed params (enum / range / path…), empty for most
    pub enabled: fn(&App) -> bool,   // guard, evaluated at menu build + dispatch
    // run lives app-side as fn(&mut App, Params) — NOT serializable; the
    // wire spec carries everything except run.
}
```

Registry: one static table `COMMANDS: &[CommandSpec]` + `run_command(app, id, params)`
dispatching by id (the current `dispatch` match becomes a table lookup with
params destructured). Menus iterate the table filtered by `menu` + `enabled`.
`actions.rs::resolve` maps key+mods → binding → command id.

## Agent/CLI surface

- `command.list` step/tool: returns id, label (localized), menu, shortcut
  label, params schema, enabled — the full GUI surface, discoverable.
- `command.run {id, params}`: dispatches by id through the same path as a
  menu click; unknown id / disabled / bad params = typed error, never a panic.
- `session.inspect`: current document state (exists as `state`/`inspect`;
  align its shape with command.list's param schemas).
- Mapping to today: each `AgentStep` variant gets a CommandSpec id
  (`paint.stroke` ← PaintStroke, `layer.add` ← LayerAdd, …) so the JSONL
  protocol v2 and MCP tools become thin adapters over the same registry
  (PROTOCOL_VERSION bump 1 → 2 at the switch; pre-tag, so free).

## Migration waves (each its own kill, battery-gated)

1. **Table extraction**: move the `dispatch` match arms into
   `run_command` keyed by ShellAction's discriminant string; behavior
   byte-identical (kittest baselines + prop-menu `with_shortcut` consumers
   prove it). No new ids yet.
2. **Id assignment**: `ShellAction` → dotted ids (`"file.new"` etc.);
   `action_from_name`/`action_name` become id lookups. i18n label keys
   attached per command (catalogs gain a `cmd.*` namespace; parity gate
   834/834 enforces en/pt).
3. **Menu build from table**: menus iterate COMMANDS filtered by menu
   path; `enabled` guards evaluated at build time (replaces per-site
   `ui.add_enabled` conditionals where they exist).
4. **Shortcut binding by id**: `Binding` rows reference command ids;
   `resolve` returns the id. Prop-menu Assign Shortcut (landed 2026-11-01)
   writes id-based bindings — the persisted format must migrate or be
   namespaced v2.
5. **Agent adapter**: AgentStep variants map to command ids; `command.list`
   + `command.run` added to agent.rs Session + mcp.rs tool catalog
   (tools become `command_list` / `command_run` + the existing
   specialized ones kept as aliases during the transition).

## Risks / traps

- `main.rs` is the hottest surface in the repo and sibling lanes lock it
  often — waves 1-3 each touch it; take kill locks per wave, keep each
  commit behavior-identical and kittest-covered.
- egui menu building is immediate-mode: `enabled` guards must be
  evaluated per frame from the table, not cached.
- i18n: 834 keys en/pt parity is CI-enforced inline; every new `cmd.*`
  key lands in BOTH catalogs in the same commit.
- Persisted shortcuts (Assign Shortcut) and the agent protocol version
  both need a migration story before wave 4/5 land.
- Do NOT bolt this onto a feature kill (row 154's own warning): each wave
  is standalone.

## Verification plan (battery sessions)

- Existing kittest/panel_snapshot baselines regenerate unchanged at wave 1-3
  (behavior-identical proof).
- New pins: dispatch-by-id equals menu click for a sample of commands;
  `command.list` returns exactly the menu-visible ids with correct labels
  in both locales; `command.run` with a disabled command returns the typed
  error; unknown id errors; params validation rejects out-of-range.
- clippy -D warnings + layers gate + ftl parity stay green at every wave.

## Non-goals

- No rmcp/schemars swap (row 155, separate).
- No app-side `--control` channel (row 153, separate) — though its
  `ui.inspect` tool should eventually read from `command.list`'s schema.
- No GUI redesign; menus keep their current layout.

## Landing log (2026-11-05)

- w1 `7ee9d8a`: dispatch match extracted verbatim into `commands.rs::run_command`; `ShellAction::id()`/`from_id()` added (claw-built, dragon cross-review + battery).
- w2a `b79710d`: `run_command_id` + `CommandError` (UnknownCommand/NeedsParams/Disabled) + `COMMAND_IDS`; round-trip unit tests.
- w2c `2532dc2`: `command.list` / `command.run` on the app's `--control` channel (UI-thread queue; drain collects before dispatch), `commands::command_list_json`, `scripts/control_call.py` client. The GUI is now agent-drivable by id over loopback.
- Remaining for w2b: `actions.rs::resolve` returns ids (Binding rows reference command ids), `cmd.*` label keys in BOTH ftl catalogs (parity gate), persisted-shortcut migration story, then agent.rs/mcp.rs `command_list`/`command_run` adapters over the bridge.
