# Auto-cage triangle-index ABI fix

- Lock: `razul-autocage-index-abi`.
- Brief: `7617105fe6230dc0811ebf3bb8eb631260ec50e4`.
- Outcome: **confirmed GPU indexing bug fixed and regression pinned.**

## Root cause

`transfer.rs` uploaded raw `[u32; 3]` low-triangle records while the shader declared `array<vec3<u32>>`. Naga's layout calculation gives a three-component `u32` vector 16-byte alignment; the array stride rounds its 12-byte payload up to 16 bytes. Every triangle after the first therefore read shifted indices.

The upload now emits `[u32; 4]` records with a zero padding lane. No shader or transfer algorithm change was needed.

## Test correction and RED/GREEN evidence

The initial regression was not valid: its two-record indices could alias under the wrong stride, and both selected texels actually landed on triangle 0. Independent review `deleg_1b693124` rejected the resulting false-positive disposition.

The final fixture samples one texel from each real triangle and appends a degenerate third index record beginning with `1`, eliminating the cyclic-index alias without changing UV coverage. Against the old 12-byte upload:

- Triangle 0 matched the CPU oracle at byte 149.
- Triangle 1 was **RED: actual 180, expected 169**.

After padding the records, the same test was **GREEN: triangle 0 = 149/149, triangle 1 = 169/169**. The oracle is the real CPU `UvGBuffer` plus `Cage::lerp_at`, with distinct nonuniform offsets.

## Verification

- Focused regression: RED before the fix, GREEN after it.
- Final `cargo test -p umber-bake --features gpu -- --test-threads=1`: **210 passed, 0 failed, 0 ignored**.
- Independent fix-delta review `deleg_240b61e8-3` (Mistral Large): **SHIP, zero findings**, including layout math, true second-triangle coverage, RED/GREEN arithmetic, and test honesty.
- Desktop-requested ad-hoc verifier: **8/8 focused regressions passed**, including this ABI test; temporary `hermes-verify-` script deleted. This is separate from the suite evidence above.
- Strict clippy across all touched crates and targets: clean. Layers, scorecard, parity, rustfmt, and `git diff --check`: clean.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No transfer feature expansion, no release packaging, no upstream PR, no centralhost work.
