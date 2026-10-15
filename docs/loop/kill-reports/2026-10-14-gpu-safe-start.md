# Kill report — gpu-safe-start (2026-10-14)

- Author: subagent (code claw), dispatched by the dragon.
- Lock: `crates/umber-app/src/gpu_startup.rs`, `crates/umber-app/src/main.rs`,
  `crates/umber-app/src/lib.rs` (untouched), this report.
- Implements: ROADMAP wave 8b row `wave-gpu-safe-start`.
- Source: photocraft `apps/photocraft/src/gpu_startup.rs` (MIT/Apache), read
  for the idea only (marker before device creation, OS lock to tell live from
  stale, step down a per-OS chain, `WGPU_BACKEND` / `--safe-gpu` overrides).
  Clean-room reimplementation: no code copied, different types, file format
  and decision shape. Photocraft's CPU-composite rung, Intel/DX12 adapter
  ranking and DXC pinning were not carried over.

## Built

- `crates/umber-app/src/gpu_startup.rs` (new, bin-only module):
  - Pure core: `decide_backend(marker, persisted, env, safe_flag, platform) -> Decision`,
    `next_safer`, `Backend`/`Platform`/`Planned`/`Reason`/`Decision`.
  - Marker IO: `acquire_marker` (create + `File::try_lock`; `Live` when
    another process holds it), `MarkerGuard::{write, clear}`,
    `read_persisted` / `write_persisted`, typed `MarkerError::{Io, Corrupt}`.
  - `Session` (frame counter, confirm on frame 3, persist a fallback,
    `finish(clean)`) behind a process `Mutex`; thin wrappers `prepare`,
    `note_adapter`, `note_frame`, `finish` for main.rs.
  - 21 tests: the decision table (no marker, persisted, platform-invalid
    persisted, stale-marker table across Linux/Windows incl. the
    auto-picked-DX12 row, Mac exhaustion, step-from-what-crashed,
    unknown/corrupt marker, env-start marker, env override, `--safe-gpu`),
    an exhaustive 1536-row invariant sweep, chain termination, marker text
    round trip + field sanitising (newline injection), corrupt text/file
    (garbage, non-UTF-8, wrong header, unknown backend), live-vs-stale lock,
    persisted round trip + corruption, session adapter record, frame-3
    confirmation + persist, preferred start persists nothing, finish
    clean/error. Every temp dir is unique (pid + counter + tag).
- `crates/umber-app/src/main.rs` (localized, +25/−5): `mod gpu_startup;`;
  in `fn main` parse `--safe-gpu`, call `gpu_startup::prepare`, set
  `create_new.instance_descriptor.backends` when the decision forces one,
  call `note_adapter(&adapter.get_info())` inside the existing
  device-descriptor closure, call `finish(result.is_ok())` after
  `run_native`; `gpu_startup::note_frame()` at the top of `App::ui`.
- `lib.rs`: unchanged — nothing in `tests/` needs the module (the rest of
  the shell modules are bin-only too).

## Judgment calls

- Backend chain, from the vendored source:
  - egui-wgpu 0.36.2 `setup.rs` `WgpuSetupCreateNew::without_display_handle`
    sets `instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(PRIMARY | GL)`,
    and `lib.rs` enumerates adapters with exactly those backends — so
    overriding that one field selects the backend; `Auto` leaves it alone.
  - egui-wgpu pulls `wgpu/default`, whose wgpu 30.0.1 default features are
    `dx12, metal, gles, vulkan` → chains: Linux `auto/Vulkan → GL`; Windows
    `auto/Vulkan → DX12 → GL`; macOS Metal only (crash = exhausted at once).
  - `wgpu::Backends::from_comma_list` (wgpu-types 30.0.1 `backend.rs`) is
    what parses `WGPU_BACKEND`; egui-wgpu already applies it, so a non-blank
    `WGPU_BACKEND` makes the decision `Env` and leaves the descriptor as-is.
  - Required feature `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` is
    advertised by the vulkan, dx12 and gles hal adapters (wgpu-hal 30.0.1),
    so the GL rung does not trivially fail the device request.
- No CPU/software rung (photocraft has one): Umber's paint pipeline needs a
  real device; llvmpipe arrives through GL anyway on Linux.
- Exhausted chain → retry the safest rung + `log::error!` with the advice
  to update the driver / set `WGPU_BACKEND` (no UI, per brief).
- An `auto` start that recorded its adapter's backend steps from THAT
  (Windows auto that picked DX12 → GL, not DX12 again).
- Marker format: versioned `key=value` lines, file `gpu-starting.marker`
  (not `.json`): umber-app has `serde_json` only as a dev-dependency and the
  brief forbids new deps. Same information (backend, adapter, driver,
  timestamp, pid).
- Corrupt marker = `Planned::Unknown` = crashed start on the rung this
  launch would use; logged at warn, never a panic.
- A marker written under `WGPU_BACKEND` never steps the chain.
- Lock-based staleness (`std::fs::File::try_lock`, stable since 1.89;
  toolchain 1.97 — probed `WouldBlock` on a second handle before use). A
  concurrent launch sees `Live` and skips the marker.
- Confirmation = the 3rd `App::ui` call (frames 1–2 painted + presented).
  A clean exit before that clears without persisting; an `Err` from
  `run_native` keeps the marker so the next launch steps down.
- Persist only a step-down that reached confirmation; `--safe-gpu` and env
  are never persisted; a persisted backend the platform lacks is ignored.
  No auto-upgrade; delete `<state>/gpu-backend` to reset.
- State dir: parent of `umber_brush::preset_library::user_preset_dir()`
  (`$XDG_DATA_HOME/umber`, `~/.local/share/umber`, `%APPDATA%\umber`) —
  the existing helper the plugins dir already reuses; `temp_dir()/umber`
  fallback when no home is discoverable.
- Tests re-read the marker only through a fresh lock: Windows locks are
  mandatory, so reading via a second handle while held would fail there.

## Unverified

- A real driver crash inside `request_device` / first present cannot be
  reproduced in tests; the crash path is covered by dropping a guard
  without `clear` (what a dead process leaves) and by the decision table.
- `instance_descriptor.backends` override and the `note_adapter` call in
  the device-descriptor closure compile and match the egui-wgpu source, but
  no GPU run exercised them (no app launch in this kill).
- The Windows chain and `try_lock` behaviour on Windows are covered only by
  pure tests on Linux; CI's windows leg runs the same tests.
- Whether a GL-only adapter satisfies egui-wgpu's 8192 texture limit on
  every driver is not checked; a failure there is an `Err`, which keeps the
  marker and exhausts next launch with a loud log.

## Deferred

- Preferences UI to view/reset the remembered backend (would need i18n).
- Adapter-level ranking (e.g. photocraft's Intel-on-Windows → DX12) and DXC
  compiler pinning.
- `--safe-gpu` documentation in a user-facing CLI help (the app has no
  `--help` today).

## Process note

- `cargo fmt --all` ran per the brief; `cargo fmt --all --check` afterwards
  reports a diff only in `crates/umber-mesh/tests/panic_hunt_mesh.rs`, a
  concurrent claw's untracked in-flight file that appeared after my fmt run.
  Not touched (outside this lock); `gpu_startup.rs` passes `rustfmt --check`.
