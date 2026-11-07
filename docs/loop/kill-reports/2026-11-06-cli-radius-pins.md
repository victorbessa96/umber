# Kill report — cli paint_stroke radius validation pins (row 208 follow-up closed, queue row 253)

- Date: 2026-11-06 ~23:4x -03. Session: local-1625-rz6f2a (Razul). Kill-lock:
  `razul-cli-radius-pins` (crates/umber-cli/src/agent.rs, STATE.md, this
  report).
- What was open: queue row 208 noted "Cli wiring of paint_stroke.radius =
  follow-up once the sibling frees umber-cli". The wiring itself is present
  (agent.rs:1844 `set_brush_radius_texels`, alias handling, reply reports
  the radius in force); what was missing was a session-level pin, and
  umber-cli is free again.

## The change (tests only)

- Pin `paint_stroke_validates_radius_fields_before_any_project_or_mesh`:
  five session lines — radius+radius_texels conflict, negative radius,
  zero radius_texels, zero radius_scale, and a valid radius that passes
  validation and fails only on "no project open" (proving the validation
  order sits before any state access). No GPU/project needed, so the pin
  runs in the plain unit battery.

## Verification

- Battery (flock, nice 19, -j3, /tmp/umber-cliradius-battery.log): fmt OK,
  clippy -p umber-cli --all-targets -D warnings 0, cargo test -p umber-cli
  --all-targets all suites ok 0 failed, pin included.

## Records note

Queue row 253 + COORDINATION line DEFERRED: docs/loop/queue.md and
.dragon/COORDINATION-next-wave.md remain held by the live sibling lock
`razul-bundled-assets` (its dirty files untouched).

## Not done / next

- split_redo RedoPending lift + merge-below-top design now have their
  primitive (peek_redo, row 252) — paint lane, free.
- Row 242 History wiring + installers gap #2 sibling-locked; v0.1.0 tag
  partner-gated.
