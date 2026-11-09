# Audit lens E — engine, architecture, performance (2026-10-24, live-verified)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app (166k+ LOC, 16 crates). READ-ONLY TASK: no cargo, no builds, no
file edits outside your report file. Every claim cites file:line
from the LIVE tree.

## The lens: the machine under the paint

### 1. The crate map + health

Every crate in crates/: its role, its LOC, its test-fn count (grep
`#[test]` per crate), its per-crate test density (tests/kLOC), its
no-test >80-line files (the 10-17 audit named five — verify which
still stand). The dependency layering (the layering gate) — any
upward deps?

### 2. The paint engine's architecture (umber-paint + umber-brush)

The stroke path: pointer → route_tile → the conditioner (one-euro,
lazy-mouse, line constraint) → dab staging → the hooks (layer mask,
quick mask, retouch, symmetry, per-layer pool) → the GPU splat/retouch
writeback. The per-layer pool's plane storage + budgets. The
compositor. The undo history (the journal + the retouch history).
Verify each stage's live site + its tests. The KNOWN architecture
debts (the L2b same-class arms, the unified timeline L6 lane,
split-redo, merge-below-top).

### 3. The GPU stack (umber-gpu)

The wgpu version, the device/queue startup (the safe-start marker +
backend fallback), the shader surfaces (the splat pass, the
compositor, the bakes, the ID bake), the error handling, the
readback paths. The probe/capability model. The wgpu 30 / eframe
0.36 upgrade posture (the named probes).

### 4. Performance

docs/perf/protocol.md: the baseline, the nightly's invocation, the
compare gate, the triage record. The perf floor requirement (60fps
@ 1M-tri + 4K). The known hot paths + the measured state (the
lavapipe viewport mean halving). The pool_perf test. What has NO
measurement (the honest unmeasured list).

### 5. The robustness posture

The never-crash doctrine: the panic hook, crash.rs, the fuzz corpus
(mesh panic hunt, mutated-valid replays), the memory caps, the
safety LINTS (deny unwrap/expect/panic where). SECURITY.md's rows —
stale or current?

## The output

Write `docs/loop/audit-2026-10-24-lens-engine.md`:
- The crate health table: crate | LOC | tests | density | notes.
- The architecture verdict: what is sound, what is debt (each debt
  with its seam file:line).
- The perf state + the unmeasured list.
- The robustness state.
- "The named gaps" + "The surprises" (docs-claims the tree
  refutes).
