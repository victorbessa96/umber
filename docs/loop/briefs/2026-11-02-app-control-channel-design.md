# Design brief — app-side control channel (queue row 153), 2026-11-02

Status: DESIGN ONLY. No code written, no cargo (envelope). Row 152
(Hermes `mcp add` registration) is gated on a fresh build; this brief
preps row 153 so the umber-app lane can start when the desktop/battery
session opens.

## What exists already

- `umber-cli serve --port N`: loopback TCP JSONL over the agent session
  with token auth (`UMBER_CONTROL_TOKEN` or a fresh token written to
  `<tmp>/umber-control-<port>.token`, owner-only 0600) and workspace
  confinement (`security.rs`, `workspace.rs`). Landed 2026-11-01
  (texturecraft wave 1, `3331bcb`).
- `umber-cli mcp`: hand-rolled MCP stdio, 17 tools over the same session.
- umber-app arg parsing precedent: `--safe-gpu` (main.rs:5491).
- PNG encoding is already an app dependency (`umber_export::png::write_png`).

## Target

`umber-app --control <port>` starts a loopback JSONL server INSIDE the
running app exposing the GUI's live state; `umber-app --bridge <port>`
forwards MCP stdio to an `umber-cli mcp` child (texturecraft's
apps `--control` + `automation/bridge.rs` pattern). Tools v1:

- `viewport.screenshot` — capture the 3D viewport region via egui's
  frame-output pixels at end of frame, encode PNG through
  `umber_export::png::write_png`, return base64 + dims. (egui 0.36:
  read the frame's pixels from the `Context` after `end_pass`; keep the
  capture on the render thread, never cross into the listener thread.)
- `ui.inspect` — dump the widget tree (egui `Context::memory`/debug
  layer or the app's own panel state) as JSON: panel list, active tool,
  brush params, layer census. Grounds row 154's future `command.list`.
- later: `command.list` / `command.run` once the CommandSpec registry
  (brief 2026-11-02-commandspec-unification-design.md) lands.

## Concurrency model (the main risk)

The app is single-threaded egui. The listener runs on a dedicated
`std::thread` owning only a `TcpStream` pair + a command queue
(`std::sync::mpsc`); at frame start (`fn ui` entry) the app drains the
queue and applies commands, exactly like the UMBER_THEME_FILE mtime poll
(`1e9155d`). Replies flow back through the socket from the same thread.
No egui state crosses threads; screenshots are taken on the UI thread at
frame end and sent through the queue.

## Security model

- Bind 127.0.0.1 only; refuse any other interface argument.
- Token: reuse `umber-cli/src/security.rs::server_token` semantics —
  `UMBER_CONTROL_TOKEN` env or a fresh token at
  `<tmp>/umber-app-control-<port>.token` (0600, owner-only). The CLI's
  `security`/`workspace` modules are L5; the app is L6 and may depend on
  umber-cli's lib? NO — check-layers: umber-cli is a bin crate. Copy the
  ~40-line token helper into umber-app (or lift it to umber-core L1 —
  preferred if both need it; core has no UI deps).
- Auth before any method dispatch; one authed session per connection,
  same as serve.rs.
- `--bridge` spawns `umber-cli mcp` as a child process with stdio pipes;
  the app relays MCP frames verbatim and never interprets them.

## Migration / rollout

1. Token helper lifted to umber-core (or duplicated app-side) — small kill.
2. Listener thread + queue + auth + `ping`/`version` methods — kill.
3. `viewport.screenshot` — kill (needs a battery session for the PNG
   golden).
4. `ui.inspect` — kill (kittest-pinnable via widget_info rows).
5. `--bridge` — kill (spawn + relay; test with a mock stdio child).
6. Row 152 registration: `hermes mcp add umber ...` after the next build.

## Verification

- Kittests for ui.inspect shape; a scripted socket client for
  auth-fail/auth-ok/ping; screenshot golden regenerates at battery.
- Manual: desktop session, `umber-app --control 8377`, curl the loopback,
  confirm 401 pre-auth and the token file's 0600.
- clippy -D warnings + layers + fmt stay green per kill.

## Non-goals

- No remote/WAN exposure, ever — loopback only.
- No write commands over the channel in v1 (read-only inspect +
  screenshot); writes arrive with the CommandSpec wave.
- No TLS (loopback needs none).
