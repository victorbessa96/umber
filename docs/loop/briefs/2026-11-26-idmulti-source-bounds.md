# Kill brief: IdMulti source bounds and effective Replace coverage

- Lock: `razul-idmulti-source-bounds`.
- Base: current audit branch state, with GPU and Windows CI green at `c14d1baf` and docs close-out `ec979743`.
- Trigger: independent IdMulti analysis found a real >8 CPU/GPU divergence and missing Replace predecessor cases. Current `validate_multi` accepts any source count; GPU construction truncates to eight while `hash_source_cpu` iterates all sources.

## Targets

1. Add a named eight-source ceiling constant and a typed `IdBakeError::TooManySources { count, max }`. `validate_multi` rejects more than eight sources before GPU work. CPU and GPU therefore share one contract instead of silent truncation.
2. Add CPU validation regressions proving eight sources are accepted and nine are refused with the typed error.
3. Add GPU Replace coverage for predecessor gating:
   - zero-weight first source does not latch Replace; the next effective source wins;
   - masked-out first source does not latch Replace; a covered black second source wins and a later source cannot overwrite it;
   - all sources rejected by weight or mask still produce transparent zero, never opaque black.
4. Preserve sequential order, existing eight ordinary mask bindings, 32/272-byte ABI, sparse mask padding, and all current green behavior.

## Concurrency and compatibility

One code file plus its report/status docs. The full-project audit remains read-only on the same file. No schema change, dependency, new device feature, release action, or broad refactor. This is an additive typed error variant, not a silent behavior change for valid configurations.

## Verification

RED first for source count and each new effective-Replace case, then GREEN. Run the full IdMulti GPU filter, full `umber-bake --features gpu` suite single-threaded, workspace clippy, rustfmt, static gates, and an independent bounded review. Commit and push through `safe-push.py`; dispatch current-tip CI if the final wave changes code.

## Non-goals

No changes to ground plane, project serialization, public UI, release packaging, upstream PRs, centralhost, or unrelated audit lanes.
