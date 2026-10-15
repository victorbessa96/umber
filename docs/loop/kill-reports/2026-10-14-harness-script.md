# Kill report — harness-script (the agent's snapshot arm)

Date: 2026-10-14. Author: dragon. Lock: `harness-script` over
`crates/umber-cli/src/agent.rs`, `crates/umber-cli/Cargo.toml`,
`Cargo.lock`, `crates/umber-cli/tests/agent_paint_ops.rs`,
`docs/specs/agent-paint-ops-design.md`, this report (the lock was
re-registered once: the test moved into the existing harness file
instead of a new one, and the blake3 dep needed the two manifests).
Source: ROADMAP wave 5 row `wave-harness-script`.

## Built

- `paint_snapshot` step: the whole selected plane to a PNG at `out`,
  plus its `blake3`. A never-written plane (a layer nothing painted;
  Base always exists once paint initializes) writes nothing and replies
  `exists: false`.
- `Session::read_selected_plane`: the plane selection, typed refusals and
  composite flush, moved out of `paint_read` so both steps share one
  implementation. `paint_read`'s behaviour is unchanged (its 6 tests are
  the regression oracle).
- `PlaneImage`: a CLI-local readback shape, so umber-cli does not name
  umber-brush's `RgbaBuffer` (it does not depend on umber-brush).
- `blake3` added to umber-cli (already a workspace dep, used by
  umber-paint's sidecar clean-skip).
- Test `snapshot_hash_tracks_strokes_and_undo_exactly` in
  `agent_paint_ops.rs`, reusing its spawn harness.

## Judgment calls

1. **A protocol step, not a `--script` CLI flag.** A script is a JSONL
   file piped into `umber-cli agent`; the protocol already is the script
   runner. A second runner would duplicate the session loop. The ROADMAP
   row's "--script arm" is this step plus the pipe.
2. **Relational pins, not golden hashes.** Dab-edge coverage is
   implementation-defined across lavapipe / WARP / hardware (the paint-ops
   module's exactness rule), so a hard-coded hex would fail on some
   machine. Pinned instead: file decodes to the hashed bytes,
   determinism, a stroke changes the hash, undo restores it exactly.
3. **Raw PNG (linear, premultiplied as stored).** The hash has to cover
   the plane's own bytes; an sRGB-encoded or un-premultiplied PNG would
   hash something else. It is a regression artifact, not a viewer image.
4. **Test placed in the existing file.** Copying the 120-line spawn
   harness into a new `agent_script.rs` would make two copies to maintain.

## Test fix during Gate 1

The first run failed on the test's own assumption: it expected Base to be
absent before a stroke. Base is allocated (512², zero) when paint
initializes, so it snapshots as `exists: true`. The absent case now uses a
never-painted layer, and the blank Base hash became the undo target:
undoing both strokes must restore it byte-for-byte.

A second fix, same class: the test's second stroke was 2 points
(`[[0.2,0.2],[0.25,0.22]]`) and changed NOTHING — the snapshot hash after
it equalled the hash before. The snapshot did its job (it caught a stroke
that drew nothing). The stroke now uses the same 9-point shape as the
first, moved to UV ~0.18, and lands.

**Open finding (not fixed here):** a short wire stroke can paint nothing.
`paint_stroke` feeds points through the live conditioner (one-euro +
lazy-mouse + spacing — `PaintState::push_event_styled`), which is tuned
for dense pointer streams; a 2-point agent stroke apparently never clears
the stabilizer. Unverified which stage swallows it. An agent stroke is
already-intended geometry, so the likely fix is a conditioner bypass for
wire strokes (or a `smoothing: false` field) — a protocol decision, so it
is named here and in the ROADMAP rather than guessed at.

## Unverified

- Per-platform golden hashes (one set per adapter class) are not built;
  a cross-machine pin needs them.
- The protocol version stays 1: 0.1.0 is untagged, and the Beta-freeze
  rule ("bump on any step change") starts at the tag.

## Deferred

- Golden-hash sets per adapter class.
- A `snapshot_dir` convenience (auto-named files per step index).
