# IdMulti source bounds and effective Replace coverage

- Lock: `razul-idmulti-source-bounds`.
- Brief: `b20680e3d35ded59738846d51396ccef50aef438`.
- Implementation: `07675dd9f4664ea37bd6aeb923ef352c1b5a3153`.
- Reviewed implementation-delta SHA-256: `dce889d1500c1f2f8951e2849fdbdf4d94caf46da2c30c615dbdb0ddc4f3deb1`.

## Findings and fixes

`IdMultiParams.sources` documented a maximum of eight, but `validate_multi` accepted more. GPU construction then truncated to eight while `hash_source_cpu` continued over every source, producing divergent results above the documented ceiling.

The implementation now exposes `MAX_ID_MULTI_SOURCES = 8` and typed `IdBakeError::TooManySources { count, max }`. `validate_multi` rejects more than eight sources before GPU work, and the host's source upload/count uses the same named constant. Valid configurations retain their behavior.

Three acceptance gaps are closed:

- Zero-weight predecessors do not latch Replace; the next effective source wins.
- A masked-out predecessor does not latch Replace; a covered black second source wins and a later source cannot overwrite it. If every source is rejected, output remains transparent zero rather than opaque black.
- Eight sources validate; nine return `TooManySources { count: 9, max: 8 }`.

The nine-source test was RED before the fix (`is_err` assertion failed), then GREEN with the exact typed error. The other cases are coverage additions for semantics the earlier repair already established; they did not require a production behavior change.

## Verification

- Nine-source typed validation regression: **RED then GREEN**.
- Replace family including existing regressions: **5 passed, 0 failed**.
- Full `cargo test -p umber-bake --features gpu -- --test-threads=1`: **209 passed, 0 failed, 0 ignored**.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- Rustfmt, check-layers, scorecard, parity, and `git diff --check`: clean.
- Independent bounded review `deleg_bc78ff7b`: **SHIP, zero findings**.
- Final CI `45851497794` at `0d83e5fcea97667cd3b7279cac8a808e7d1fbf0c`: **SUCCESS**. Ubuntu `140018843097` passed all steps including GPU and E2E; Windows `140018842822` passed tests, WinInk, and release smoke. Origin/master read-back is the same SHA.

## Non-goals

No changes to ground plane, serialization, UI, release packaging, upstream PRs, centralhost, or unrelated audit lanes.
