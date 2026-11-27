# Export containment and PNG16 wire order

- Lock: `razul-export-containment`.
- Brief: `a19bf074277c` (`docs(export): containment and PNG16 brief`).
- Outcome: MaterialX/USDA document writes are contained, and PNG16 uses the required big-endian wire order on every host.

## Findings and fixes

1. **Document names could escape `out_dir`.** `document_output_path` now applies the existing `path_safe_value` rule to the filename only, preserving in-document names and texture references. A pre-existing symlink or other non-regular output is refused instead of followed. MaterialX and USDA share the helper.
2. **PNG16 used native endianness.** Every `u16` sample is now serialized with `to_be_bytes`; transfer and alpha behavior are unchanged.

## RED/GREEN evidence

- Before the fix, hostile MaterialX and USDA names escaped the output directory, and the first PNG16 samples decoded as little-endian bytes (`16,39,32,78`) instead of the required big-endian sequence (`39,16,78,32`).
- After the fix, absolute, traversal, dot-only, and separator-containing names stay under `out_dir`; pre-existing output symlinks fail without changing their targets.
- The first PNG16 texel now decodes to the exact expected big-endian bytes, and `u16::from_be_bytes` recovers the original values.
- Test expectation corrections were made only for the actual existing sanitizer's `.._escape` output and to clean the escaped artifact created by the RED run; no production behavior was weakened.

## Verification

- Focused MaterialX containment, USDA containment, and PNG16 tests: all green after RED.
- Full `umber-export`: **105 lib + 7 integration passed, 0 failed**.
- Strict `cargo clippy -p umber-export --all-targets -- -D warnings`: clean.
- Layers, parity, rustfmt, and `git diff --check`: clean. Generated exact scorecard is current at 3,313 after the two new tests.
- Independent bounded review `deleg_ddf7e4d1`: **SHIP**, zero scoped findings. Non-blocking residual notes cover pre-existing directory output rejection, broader end-to-end coverage, Windows symlink execution, and ordinary image-output TOCTOU outside this kill's scope.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No multi-set USDA redesign, whole-run transaction work, new dependency, release, upstream PR, or centralhost work.
