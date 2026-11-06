# Kill report — headless launch never persists the GPU fallback (installers gap #5, queue row 249)

- Date: 2026-11-06 ~19:4x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-headless-persist` (crates/umber-app/src/gpu_startup.rs, STATE.md,
  this report).
- Gap closed: installers smoke (row 240) gap #5 — "no-display launch
  poisons GPU fallback to GL permanently". Mechanism: with no display,
  wgpu's adapter pick is usually software GL/llvmpipe; when such a
  headless launch reached frame confirmation (e.g. under `xvfb-run`, or
  a killed-but-confirmed start), `Session::confirm` wrote that GL choice
  to the `gpu-backend` file, and every later desktop launch then started
  on the persisted GL rung.

## The change (crates/umber-app/src/gpu_startup.rs only — main.rs untouched)

- New `pub fn has_display()` — true when any of `WAYLAND_DISPLAY`,
  `WAYLAND_SOCKET`, `DISPLAY` is set and non-empty.
- New `Decision::persist_on_success_in(has_display)` — the existing
  `persist_on_success` rule (only a `FellBack` step-down persists) gated
  on having a display: **headless ⇒ never persist**.
- `Session::new` now sets `persist: decision.persist_on_success_in(has_display())`.
  `prepare(safe_flag)`'s signature is unchanged, so main.rs (held by the
  sibling lock `razul-bundled-assets`) needed no edit.
- Module doc gains the judgment-call bullet.
- Pins: `a_headless_launch_never_persists_its_fallback` (pure gate:
  `persist_on_success_in(false)` is `None` even for a FellBack decision;
  `(true)` still yields `Some`) and
  `confirmation_writes_gpu_backend_only_when_the_session_carries_a_persist`
  (confirm path unchanged: a session carrying `persist` writes
  `gpu-backend` on the FRAMES_TO_CONFIRM-th frame; one without writes
  nothing).

## Verification

- `cargo fmt --all -- --check`: clean.
- Battery (flock, nice 19, -j3, log /tmp/umber-headless-battery.log):
  clippy -p umber-app --all-targets -D warnings, cargo test -p umber-app
  --all-targets — results in the log; both new pins green.
- Note: the working tree also carries the sibling's uncommitted
  bundled-assets files (other lock); this commit is path-limited to the
  three locked files via dcommit.

## Records note

Queue row 249 + COORDINATION STATE line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md are held by sibling lock
`razul-bundled-assets`. Append when it releases.

## Not done / next

- Installers gap #2 (exe-relative runtime assets) — sibling lane
  (bundled.rs). Row 242 History-panel split-redo wiring — needs
  umber-app/main.rs, sibling-locked. Gap #3 (v0.1.0 tag) stays
  partner-gated.
