# Kill brief: auto-cage triangle-index ABI

- Lock: `razul-autocage-index-abi`.
- Trigger: engine audit found Rust uploads `Vec<[u32; 3]>` (12 bytes) while WGSL declares `array<vec3<u32>>` with a 16-byte storage stride. Every triangle after the first therefore reads shifted indices and can sample the wrong cage offsets.

## Target

- Pack low-triangle vertex indices as `[u32; 4]` with a zero fourth word, matching the WGSL record's 16-byte stride, and pin the Rust size assertion.
- Preserve the existing transfer shader and cage lerp behavior for all valid inputs.
- Add a two-triangle, nonuniform-cage GPU parity regression through `bake_transfer_mesh`, where the second triangle's indices and offset differ from the first. Compare GPU output with the CPU reference computed from the same cage offsets. The old upload must fail this regression.
- Keep unrelated D3–D5 worker-routing edits separate.

## Verification

RED first on the old 12-byte upload, then GREEN after padding. Run the transfer GPU filter, full `umber-bake --features gpu` suite single-threaded, workspace clippy, rustfmt, static gates, and an independent bounded review. Commit/push via `safe-push.py` and dispatch current-tip CI.

## Non-goals

No changes to the direct bake algorithms repaired earlier, no new cage feature, no release packaging, no upstream PR, no centralhost.
