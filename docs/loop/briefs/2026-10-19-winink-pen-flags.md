# Brief: 2026-10-19 winink-pen-flags (W-C3a lane S1)

Mandate: Bessa, 2026-10-19: "Continue working on the umber long horizon
project in all the steps, do not stop." Kill plan:
`docs/loop/next-wave-plan-draft.md` §1 lane S1 (verified anchors).

## What you are closing

Windows pen pressure already works through winit's Touch path
(`2026-10-15-c2-30-winink.md`), but the ERASER END, BARREL BUTTON and
TILT never reach the app — `winink.rs` is a passive decode with no
hook, and Touch carries none of them. This kill builds the
platform-free flag-mapping half (Linux-testable in the battery); the
live WndProc hook is a named later kill (NOT yours).

## Contract

1. NEW `crates/stylus/src/pen_flags.rs` — ungated, platform-free:
   - A pure `PenFlagTracker` (holds last flags) that maps
     `(down: bool, pen_flags: u32, time_ns)` to a contact event:
     `Eraser { active }` when INVERTED|ERASER, else `Contact` —
     matching the XI2 precedent of emitting Eraser INSTEAD of Contact
     (`crates/stylus/src/lib.rs:37-44`; read it first).
   - A barrel edge detector that yields `Button { button: 0, down }`
     ONLY on a transition (press and release edges; no repeat on
     steady flags).
   - Local constant mirrors carrying a doc line citing the vendored
     values: `PEN_FLAG_BARREL = 1`, `PEN_FLAG_INVERTED = 2`,
     `PEN_FLAG_ERASER = 4`
     (`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs:5477-5479`)
     and `penFlags: u32`
     (`docs/claw-artifacts/stylus/win32_pointer/mod.rs:305`). READ
     those two vendored files at the cited lines before writing.
2. `crates/stylus/src/lib.rs`: `mod pen_flags;` + the re-export, same
   kill (rule 13 — read how existing modules re-export and copy the
   pattern).
3. `crates/stylus/src/winink.rs`: `decode` fetches `POINTER_PEN_INFO`
   for DOWN/UP too and routes through the tracker. This change is
   Windows-only and compiles ONLY on Windows (the CI gate:
   `.github/workflows/ci.yml:122-124` — "Stylus winink feature tests
   (Windows only)", `cargo test -p stylus --features winink`). Keep
   the delta to the minimum lines. EVERY `windows::` symbol you touch
   must be grep-verified present in the vendored
   `docs/claw-artifacts/stylus/win32_pointer/` files — read your
   delta twice before finishing.

## Files you own (the ONLY files you may write)

- `crates/stylus/src/pen_flags.rs` (NEW)
- `crates/stylus/src/lib.rs`
- `crates/stylus/src/winink.rs`
- `docs/loop/kill-reports/2026-10-19-winink-pen-flags.md`

## Tests you write (they run on LINUX — but do NOT run them; the dragon gates)

- eraser-down (INVERTED|ERASER flags -> Eraser, not Contact)
- eraser-up (transition back to plain tip)
- plain tip (no flags -> Contact)
- barrel press/release edges (exactly one Button per transition)
- no repeat on steady flags (same flags twice -> no second event)

## Discipline (binding)

- The tree is SHARED. Dirty or untracked files you did not create
  (`main.rs`, `viewport.rs`, `.dragon/*`, other kills' reports) belong
  to other live sessions: never touch, restore, checkout, or commit
  anything. `git log/diff/grep` reads are fine; NO git mutations.
- NO cargo, rustc, build, or test runs of any kind. You may grep/read
  files and run `python3`. Match the files' existing formatting
  style; the dragon runs rustfmt at review.
- NO lock operations (`scripts/kill-locks.sh` any subcommand,
  `/tmp/umber-kill-locks/*`), never write to `/tmp` or anywhere
  outside the four files above.
- If any anchor drifted, re-derive from live source. Never invent an
  API — grep its definition first, and for `windows::` symbols use
  ONLY what the vendored files prove.

## Report shape

Write `docs/loop/kill-reports/2026-10-19-winink-pen-flags.md`:
header naming authorship (claw: opencode/deepseek-v4.1-flash-free),
kill-lock `winink-pen-flags` + its files, design source
(`docs/loop/next-wave-plan-draft.md` §1 lane S1 + this brief); then
**Built**, **Judgment calls**, **Unverified** (the Windows-only
compile is the battery's, not yours — name it), **Deferred** (the
live WndProc hook, tilt, the real-pen hardware check), **Process
note**. End with a one-screen summary: files changed + every test
name + what it pins.
