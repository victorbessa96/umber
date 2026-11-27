# Kill brief: D3-D5 WARP worker crash isolation

- Lock: `razul-d3d5-warp-crash`.
- Trigger: exact filtered run `45999725905` crashes with `0xc0000409` on Windows in `d3_d5_maps_execute_through_app_worker`; mapped-ray, direct multi-map GroundPlane, and direct cavity opacity all pass on Windows.

## Required diagnostic coverage

Add minimal real-worker regressions, each with one decoded output:

1. GroundPlane only.
2. Manual Opacity only.
3. IdMulti with an IdMap source only.

Run them independently on Windows. If one fails, root-cause that branch; if all pass, test their composition. Do not skip the crashing test, weaken the existing D3-D5 oracle, or accept a platform-specific timeout as a fix.

## Verification and constraints

Local full app suite and clippy first, then filtered Windows runs and independent review. A production fix requires RED before and GREEN after, with the whole original D3-D5 worker oracle restored to green. No release, tag, upstream PR, or centralhost work.
