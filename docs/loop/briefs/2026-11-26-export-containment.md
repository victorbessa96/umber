# Kill brief: export containment and PNG16 wire order

- Lock: `razul-export-containment`.
- Trigger: formats audit findings 1 and 2 at `ec979743` showed that MaterialX/USDA document names can escape `out_dir`, and PNG16 writes native-endian samples instead of PNG's big-endian wire order.
- Scope: `driver.rs`, `usd.rs`, `png.rs`, direct inline tests, and the kill report.

## Required fixes

1. Sanitize only the MaterialX/USDA **filename component** through the existing `path_safe_value` rule. Preserve in-document names and texture references. Test absolute names, parent traversal, dot-only names, platform separators, and containment under an existing output directory; include a symlink escape negative case where supported.
2. Serialize each PNG16 sample with `to_be_bytes`. Test the decoded sample bytes directly against the original values with `u16::from_be_bytes`, including values whose byte order differs. Preserve transfer and alpha behavior.

## Constraints

- No output-template redesign, multi-set USDA work, whole-run transaction work, release, upstream PR, or centralhost.
- Keep document content and all valid existing filenames byte-identical.
- Use existing dependencies and test helpers.

## Verification

- Focused hostile-name and PNG16 tests, then full `umber-export` suite, rustfmt, clippy with `-D warnings`, static gates, and independent review before commit.
