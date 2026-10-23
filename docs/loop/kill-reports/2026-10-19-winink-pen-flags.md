# Kill report: winink pen flags — eraser end + barrel edge mapping (W-C3a lane S1)

Date: 2026-10-19. Author: opencode claw (claw: opencode/deepseek-v4.1-flash-free),
for the dragon. Lock: `winink-pen-flags` over
`crates/stylus/src/pen_flags.rs` (NEW), `crates/stylus/src/lib.rs`,
`crates/stylus/src/winink.rs`, and this report.

Design source: `docs/loop/next-wave-plan-draft.md` §1 lane S1
(`winink-pen-flags`) + `docs/loop/briefs/2026-10-19-winink-pen-flags.md`.

## Context

Windows pen pressure already reaches the app through winit's Touch path, but
three pen facts never did: the ERASER END, the BARREL BUTTON and TILT.
`winink.rs` was a passive decode with no hook, and `Contact`/`Motion` in the
frozen vocabulary carry no flag payload — so `winink.rs` only ever called
`GetPointerPenInfo` for `WM_POINTERUPDATE` and ignored `penFlags` entirely
(`LANDING_NOTES.md` even recorded "barrel-button mapping is a natural Wave 2
addition"). This kill builds the platform-free half of the fix: the flag →
event mapping, ungated so it unit-tests on Linux. The live `WndProc` hook is
a named later kill and NOT in this surface.

## Built

**`crates/stylus/src/pen_flags.rs` (NEW, ungated, platform-free)**

- The three vendored constant mirrors with the citation on each doc block:
  `PEN_FLAG_BARREL = 1` (`windowsandmessaging_mod.rs:5477`),
  `PEN_FLAG_ERASER = 4` (`:5478`), `PEN_FLAG_INVERTED = 2` (`:5479`); the
  module docs also cite `POINTER_PEN_INFO::penFlags: u32`
  (`win32_pointer/mod.rs:305`). Values cross-checked against those lines by
  a python diff run this session (see "Static verification").
- `ERASER_FLAGS = PEN_FLAG_INVERTED | PEN_FLAG_ERASER` (private): the bits
  that select the eraser end. `PEN_FLAG_BARREL` is deliberately NOT in it.
- `PenFlagTracker { last_flags: u32 }` (`#[derive(Debug, Default)]`), the
  per-pointer flag memory, with:
  - `is_eraser(pen_flags) -> bool` — the eraser-end predicate.
  - `contact(&self, down, pen_flags, time_ns) -> TabletEvent` — `Eraser
    { active: down }` when flagged, else `Contact { down }`. Stateless:
    this sample's flags alone decide.
  - `barrel_edge(&mut self, pen_flags, time_ns) -> Option<TabletEvent>` —
    `Some(Button { button: 0, down })` exactly on a barrel-bit transition,
    else `None`; the remembered word is replaced every call.
- Module docs carry every judgment call and the v1 boundary (see below).

**`crates/stylus/src/lib.rs`**

- `mod pen_flags;` (private, ungated) + the rule-13 registration:
  `pub use pen_flags::{PenFlagTracker, PEN_FLAG_BARREL, PEN_FLAG_ERASER,
  PEN_FLAG_INVERTED};` — every `pub` item of the private module is
  re-exported, so `unreachable_pub` (warn-by-default, denied by clippy `-D
  warnings`) cannot fire and the tail kill/app gets the crate-root names.
  Re-exporting ALL of them is deliberate: a `pub mod` would have been the
  sibling pattern, but the brief/plan spell the module private, so the
  reachability has to come from the re-export.
- Two crate-doc lines naming the shared platform-free decode.

**`crates/stylus/src/winink.rs`** (Windows-only; the delta)

- `use crate::{PenFlagTracker, TabletEvent};` and `PenState` gains
  `flags: PenFlagTracker` (the derive `Default` still holds).
- `decode`: the non-update arm now resolves the sample's flag word and
  routes the tip pair through the tracker —
  `let flags = pen_flags(pointer_id, msg); map_simple_message(msg, flags,
  &mut self.flags, time_ns)`. The `WM_POINTERUPDATE` arm is UNCHANGED.
- New private `pen_flags(pointer_id, msg) -> u32`: fetches
  `GetPointerPenInfo` for `WM_POINTERDOWN`/`WM_POINTERUP` only (a second
  `windows::` call site reusing the already-imported symbol), `0` for
  enter/leave and on a failed fetch.
- `map_simple_message` gains `(flags: u32, tracker: &mut PenFlagTracker)`
  and its down/up arms became `tracker.contact(..)` — one mapping site, so
  the eraser rule cannot diverge between callers. Its doc says why.
- Tests: the existing `simple_messages_map_to_proximity_and_contact`
  updated to the new signature (same pinned expectations), plus one new
  wiring pin (below).

## Tests added (7; 6 ungated in `pen_flags.rs`, 1 `cfg(windows)` in `winink.rs`)

`crates/stylus/src/pen_flags.rs` `#[cfg(test)] mod tests` — these run on
LINUX in the battery:

1. `eraser_flags_emit_eraser_instead_of_contact` — `INVERTED|ERASER`,
   `INVERTED` alone and `ERASER` alone all give exactly
   `Eraser { active: true, time_ns }`; the asserted value is what pins
   "not `Contact`" (a `Contact` would fail the comparison).
2. `eraser_release_clears_the_hold_and_a_plain_tip_flips_back` — the up
   edge with the pen still flipped is `Eraser { active: false }` (the
   variant that releases the app's hold); flags `0` flip back to plain
   `Contact { down: true }` / `{ down: false }`.
3. `plain_tip_flags_emit_contact` — no flags ⇒ `Contact` both edges, and a
   held `PEN_FLAG_BARREL` does NOT turn the tip into an eraser.
4. `barrel_press_and_release_are_one_edge_each` — `0→BARREL` yields exactly
   one `Button { button: 0, down: true }`, the repeat sample yields `None`,
   `BARREL→0` yields exactly one `down: false`.
5. `steady_barrel_flags_never_report_twice` — a first sample with the
   barrel already held reports ONE press (the fresh-memory judgment); the
   other bits churning (`|INVERTED`, `|ERASER`) under steady barrel flags
   report nothing; the release still reports exactly once.
6. `contact_mapping_does_not_disturb_the_barrel_memory` — one sample
   carrying eraser tip + barrel drives both halves from one tracker, and
   the following identical sample reports no second button (the two halves
   share the tracker, not the memory).

`crates/stylus/src/winink.rs` (Windows CI only, `cargo test -p stylus
--features winink`):

7. `an_eraser_flagged_tip_maps_to_eraser_through_the_tracker` — the wiring
   pin: `map_simple_message(WM_POINTERDOWN, PEN_FLAG_INVERTED, ..)`
   yields `Eraser { active: true }`, so the decode delta really consults
   the tracker.

## Static verification (no cargo)

Python port of the mapping + edge detector, run this session, reproducing
each test's exact sequence:

```
t1 eraser-down: ('Eraser',True) ('Eraser',True) ('Eraser',True)
t2 eraser-up  : ('Eraser',True) ('Eraser',False) ('Contact',True) ('Contact',False)
t3 plain tip  : ('Contact',True) ('Contact',False) ('Contact',True)
t4 barrel     : [None, ('Button',0,True,52), None, ('Button',0,False,54), None]
t5 steady     : [('Button',0,True,61), None, None, None, ('Button',0,False,65), None]
t6 shared     : ('Eraser',True) [('Button',0,True,71), None]
```

Every tuple matches the assertion written in the corresponding test (an
exact-value pin, not a range).

Vendored cross-check (python, this session): the three cited lines in
`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs` are
`PEN_FLAG_BARREL: u32 = 1u32` (`:5477`), `PEN_FLAG_ERASER: u32 = 4u32`
(`:5478`), `PEN_FLAG_INVERTED: u32 = 2u32` (`:5479`), all equal to the
mirrors; `win32_pointer/mod.rs:305` is `pub penFlags: u32`. The vendored
`GetPointerPenInfo(pointerid: u32, peninfo: *mut POINTER_PEN_INFO) ->
windows_core::Result<()>` (`:95`) is the same shape the existing
`GetPointerInfo` call already used, so `.is_ok()` is valid; **no new
`windows::` symbol was imported** — the delta reuses
`GetPointerPenInfo`/`POINTER_PEN_INFO`/`WM_POINTERDOWN`/`WM_POINTERUP`,
all already imported and vendored.

`bash scripts/verify-kill.sh crates/stylus/src/pen_flags.rs
crates/stylus/src/lib.rs crates/stylus/src/winink.rs` → braces 41/41, 24/24,
49/49; `PASS: fmt clean` (no in-place rewrite was needed); matches!-arity 0
suspicious; doc-attachment clean on all three. The only WARNs are the two
pre-existing `SourcedEvent.source` field lines in `lib.rs` — a false
positive (not a thiserror error type), unchanged by this kill.

**cargo UNRUN** on this host by contract (no cargo/rustc/build/test, per
the brief). The dragon's host gates fmt + clippy `-D warnings` + the full
suites; Windows CI is the only place the `winink.rs` delta compiles.

## Judgment calls

1. **Barrel edges are detected in `pen_flags.rs`, but `decode` cannot
   deliver them (yet).** `decode`'s contract is `Option<TabletEvent>` — one
   event per message — and Windows reports a barrel transition on the SAME
   `WM_POINTERUPDATE` that is also a motion sample. Rather than silently
   drop one of the two, or invent a precedence rule that could lose a
   stroke start, I kept the signature (the brief's "minimum lines") and
   consume `contact` only; `barrel_edge`'s consumers today are tests 4–6
   and the tail kill's hook, which owns the event queue and can push both
   a `Button` and the message's own event. This is the rule-6 case the plan
   named in advance ("pen_flags ships consumed only by its tests, a rule-6
   note in the report naming the tail kill as its consumer") — the code is
   NOT deleted or `#[allow]`ed, and the module docs + `winink.rs`'s module
   docs both state it.
2. **`contact` takes `&self` (no memory).** ALTERNATIVE REJECTED: having
   `contact` record the flag word too. That would make the barrel edge
   depend on call ORDER — `decode` (or a test) calling `contact` before
   `barrel_edge` for the same sample would erase the transition. Test 6
   pins the order independence. The tracker still "holds last flags" for
   the detector, as the contract says.
3. **Either eraser bit means eraser.** `INVERTED|ERASER` select the eraser
   end together; `BARREL` is excluded (test 3). Some digitizers report a
   dedicated eraser tip without physically inverting.
4. **`button: 0`.** Windows Ink reports the barrel only as a flag, so the
   frozen vocabulary's `u8` has no Windows button id to carry; 0 is the
   crate's first button slot (XI2 uses `detail`). Recorded in the module
   docs.
5. **A fresh tracker's memory is `0`, so a held barrel reports one press on
   the first sample.** ALTERNATIVE REJECTED: `Option<u32>` memory that
   swallows the first sample's state — that hides a barrel the user is
   already holding until they release and re-press. `Button` is an
   idempotent state report in the app today (`stylus_input.rs` ignores
   `Button`), so one extra event is cheaper than a missed state. Pinned by
   test 5.
6. **`map_simple_message` was widened rather than bypassed.** ALTERNATIVE
   REJECTED: keeping its old signature and intercepting down/up in
   `decode` before the call — that would leave two places deciding
   `Contact`-vs-`Eraser` and make the helper's down/up arms dead. The
   signature gain (`flags`, `&mut tracker`) keeps ONE mapping site and
   keeps the existing test meaningful (updated to the new call shape, same
   expectations).
7. **On a failed `GetPointerPenInfo`, flags are `0` (plain tip), not
   `None`.** The brief's region is the flag mapping; a missing flag word
   must never cost a stroke edge, so `decode` treats it as an untilt-style
   silent default rather than propagating the existing "drop the event"
   risk any further. Documented on `decode` and `pen_flags`.
8. **`mod pen_flags;` private + re-export (rule 13)** rather than
   `pub mod` — the brief and the plan both spell it `mod pen_flags;`. All
   `pub` items are re-exported so nothing is `unreachable_pub`.
9. **Tilt is untouched.** `PEN_MASK_TILT_X`/`_Y` gating in `map_pen_message`
   already maps tilt into `TabletEvent::Motion.tilt`, and the app carries
   but does not consume it (the known C1 boundary). Adding a tilt flag
   path here would duplicate that.

## Unverified

- **The `winink.rs` delta does not compile on this host** — `windows` is
  target-gated and `winink` is not a default feature; there is no
  `x86_64-pc-windows-msvc` run here (and no cargo at all by contract). Its
  first compile is Windows CI's
  `.github/workflows/ci.yml:122-124` ("Stylus winink feature tests
  (Windows only)", `cargo test -p stylus --features winink`) — the
  battery's, not mine. Read twice: every symbol in the delta
  (`GetPointerPenInfo`, `POINTER_PEN_INFO`, `WM_POINTERDOWN`,
  `WM_POINTERUP`) was already imported and is vendored-verified above.
- The new Windows-gated test (7) has never executed; it is reasoned from
  the same mapping the Linux tests pin.
- No real pen hardware was touched (no tilted/eraser/barrel pen on this
  box); the flag word's real-world bit combinations (a digitizer setting
  both `INVERTED` and `ERASER`, or a barrel flag surviving a tip-up) are
  taken from the vendored constants, not from a device.
- API shapes confirmed by reading, not compiling: `GetPointerPenInfo`'s
  `Result<()>` return (vendored `:95-97`), `PenFlagTracker` deriving
  `Debug + Default` into `PenState`'s derive, and `2 | 4 == 6` as the
  `ERASER_FLAGS` mask (python-verified above).

## Deferred

- **The live `WndProc` hook** (which pointer ids to track, when to
  synthesize a missed up/leave, and where the two-event `Button` +
  motion delivery lands) is the named later kill; `lib.rs`'s
  `open_system_stream` still returns `None` off Linux.
- **Tilt**: unchanged, still `map_pen_message`'s `penMask` gate; the app's
  tilt-consuming brush path is a separate open gate.
- **The real-pen hardware check**: eraser-end, barrel and (later) tilt
  behavior on an actual Windows tablet remains unverified end-to-end.
- **A `button` id for barrel presses** (Windows exposes only the flag) —
  the tail kill decides whether `0` is enough.
- `LANDING_NOTES.md` is now partially stale (its "`GetPointerPenInfo` is
  only called for `WM_POINTERUPDATE`" decision, `:30-36`, is superseded).
  It is outside this kill's file surface, so it was left untouched rather
  than edited — flagging it for the dragon.

## Process note

Only the four owned files were written. Reads (`docs/claw-artifacts/...`,
`ci.yml`, the plan, `git`-free greps), `python3` (the two checks above) and
`bash scripts/verify-kill.sh` (rustfmt + brace/trap greps + the lock
audit's read-only listing) were used. No cargo/rustc/build/test, no git
mutations, no `kill-locks.sh`, no writes to `/tmp` or outside the four
files — `main.rs`, `viewport.rs`, `.dragon/*` and the sibling kills'
reports were not touched (the verify-kill lock audit lists them under
their own locks).

## One-screen summary

Files changed: `crates/stylus/src/pen_flags.rs` (new),
`crates/stylus/src/lib.rs` (`mod pen_flags;` + rule-13 re-export + 2 crate-doc
lines), `crates/stylus/src/winink.rs` (`PenState.flags`, the `decode`
down/up route, new `pen_flags` fetch, `map_simple_message` widened, 1 test
updated + 1 added), this report.

Tests (7) and what each pins:

| # | Test | Pins |
|---|---|---|
| 1 | `eraser_flags_emit_eraser_instead_of_contact` | `INVERTED|ERASER`/`INVERTED`/`ERASER` ⇒ `Eraser{active:true}`, never `Contact` |
| 2 | `eraser_release_clears_the_hold_and_a_plain_tip_flips_back` | flagged up edge ⇒ `Eraser{active:false}`; flags `0` ⇒ plain `Contact` |
| 3 | `plain_tip_flags_emit_contact` | no flags ⇒ `Contact`; `BARREL` alone never flips the tip |
| 4 | `barrel_press_and_release_are_one_edge_each` | exactly one `Button{0,down:true}` on press, one `{down:false}` on release |
| 5 | `steady_barrel_flags_never_report_twice` | steady barrel bit ⇒ no repeats (battle the other bits churn); first-sample press edge |
| 6 | `contact_mapping_does_not_disturb_the_barrel_memory` | one tracker, both halves, call-order independent |
| 7 | `an_eraser_flagged_tip_maps_to_eraser_through_the_tracker` (Windows CI) | the decode delta really routes the tip through the tracker |

Gates: verify-kill PASS (braces 41/41, 24/24, 49/49; fmt clean; no new
`windows::` symbol; 2 pre-existing false-positive WARNs), cargo UNRUN by
contract — Windows CI is this delta's only compile gate.
