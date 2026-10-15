# Kill report: the CLI transfer bake's origin starts at the cage

Date: 2026-10-15. Author: dragon. Locks: `cli-transfer-offset` and
`cli-transfer-offset-main` over `crates/umber-cli/src/{batch,main}.rs` and
this report. Found while building the panel's High poly group (C2-29).

## Bug

The transfer ray runs inward from `surface + n * offset`. The CLI's
default was `offset = 0.0`, which leaves every HIGH hit outside the LOW
surface behind the ray origin. A default `bake_transfer` recipe therefore
baked only the detail that sits inside the low mesh, and `front` did
nothing. The panel kill's GPU test pinned this behavior: with offset 0,
the bake misses (alpha 0).

## Fix

- `TransferSettings::default().offset` is now `0.1`, equal to the
  default `front`.
- A batch/agent `bake_transfer` step without `offset` uses ITS OWN
  `front` (`Option<f32>`, `unwrap_or(front)`), so a recipe that widens
  the cage moves the origin with it.
- An explicit `offset` still wins.
- The recipe parse test pins the absent → `None` → front rule.

This changes the output of existing recipes that relied on the old
default. They gain the outward detail they were silently missing.

Gate 1: umber-cli 68. fmt and clippy clean.
