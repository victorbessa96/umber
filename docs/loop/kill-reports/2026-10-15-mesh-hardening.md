# Kill report — mesh-hardening (fix the mesh panic-hunt's findings)

Date: 2026-10-15. Author: dragon. Lock: `mesh-hardening` over
`crates/umber-mesh/src/{usdc.rs, usd.rs, gltf.rs, lib.rs}`,
`crates/umber-mesh/tests/panic_hunt_mesh.rs`, this report. Source: the
2026-10-14 mesh-panic-hunt kill's 24 findings
(`docs/loop/kill-reports/2026-10-14-mesh-panic-hunt.md`); each fix is
proven by un-ignoring that finding's `bug_*` reproducer.

## Built

| Finding | Fix | Reproducer now live |
|---|---|---|
| #1–#3 usdc: index past points, negative index, negative face count | `build_mesh_from_spec` validates every count (≥ 3) and every index (`0..point_count`) BEFORE the Newell closure indexes `points` | `bug_usdc_index_out_of_range_panics`, `…_negative_index_…`, `…_negative_count_…` |
| #4–#5 usda: stack-overflow ABORT on 20k nested `[` / 5k nested `def` | `MAX_NESTING = 256` across both recursive descents (`parse_prim` → `parse_body`, `parse_sequence` → `parse_value`), typed error past it | `bug_usda_deep_nesting_overflows_stack`, `bug_usda_deep_prims_overflow_stack` (child-process tests) |
| #6–#7 glTF: `i + base` overflow; out-of-range index accepted | per-primitive index range check + `checked_add`; `base`/`vertex_count` via `u32::try_from` | `bug_gltf_index_plus_base_overflows`, `bug_gltf_index_out_of_range_is_accepted` |
| #8 usdz never loaded | `load_usdz` opens `openusd::usdz::Archive` and reads its first layer (it called the bare-crate `usdc::read_file` on the zip) | `bug_usdz_valid_package_does_not_load` |
| #9–#15 gltf 1.4.1 reader panics | `validate_accessors` pre-checks every accessor a primitive reads: count in `1..=2^28`, `byteStride ≥` element size, POSITION/NORMAL = f32 Vec3, TEXCOORD = Vec2 f32/u8/u16, indices = Scalar u8/u16/u32 | the seven `bug_gltf_*` reader reproducers |
| #16–#18, #20–#24 dependency panics (gltf-json, tobj, openusd) | `umber_mesh::load` runs the dispatch inside `catch_unwind` → `ImportError::Panicked` | `bug_gltf_position_dangling_accessor_panics`, both `bug_obj_relative_*`, the six catchable `bug_openusd_*` |

## Still open (named, not fixed)

- **#19 openusd allocation ABORT** (`vec![…; count]` sized from an unchecked
  crate count; 1 TB–305 PB requests). An abort is not a panic:
  `catch_unwind` cannot stop it. Needs an upstream fix in openusd 0.7.0's
  `ReadExt::read_vec` / `read_compressed`, or importing `.usdc` in a child
  process. `bug_usdc_huge_count_allocation_aborts` stays ignored.
- The dependency panics are CAUGHT, not fixed: the bugs remain in gltf,
  tobj and openusd. Upstream reports are the follow-up.

- **The boundary is on `umber_mesh::load` only.** The per-format public
  functions (`load_gltf`, `usdc::load_usdc`, …) stay raw, and the fuzz
  loops call them directly — which is why the re-run still tallies 17
  usdc + 1 gltf dependency panics as known-bug (caught by the test
  harness, pinned to their reproducers). Every production caller (the
  app's File → Open, every umber-cli path) goes through `load`; verified
  by grep at this commit.

Re-run tallies (`cargo test -p umber-mesh --test panic_hunt_mesh`):
fbx 47 ok / 354 err; gltf 5 / 726 (1 known-bug); usda 98 / 548; usdc
25 / 395 (17 known-bug, 25 abort-skipped); usdz 9 / 645 (0 known-bug,
15 abort-skipped — down from 12 known-bug, since usdz now goes through
the zip reader); obj 112 / 403. 0 unclassified panics, 0 flagged.

## Judgment calls

1. **Validate in umber, catch the rest.** Umber's own parsers validate
   before they index (no reliance on the boundary). The boundary is for
   code Umber does not own, where pre-validation would mean
   re-implementing the dependency's parser.
2. **A cap of 256** for USD nesting (the MaterialX `MAX_ELEM_DEPTH`
   precedent): far deeper than any real scene graph, far shallower than
   the measured overflow.
3. **`2^28` accessor-count cap**: 268M elements is past any real mesh and
   keeps `stride * count` far from overflow.
4. **TEXCOORD accepts f32/u8/u16** (the glTF spec's normalized forms);
   only u32 hit the `unreachable!()`.

## Unverified

- Release builds: several dependency findings are `debug_assert`s or
  overflow checks that only fire in debug. In release they wrap instead
  of panicking; the pre-validation covers the glTF ones regardless.
- The usdz fix is proven on an openusd-written package; Pixar's
  reference `.usdz` assets are not in the corpus.
