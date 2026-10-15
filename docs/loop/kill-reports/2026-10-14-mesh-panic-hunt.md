# Kill report: mesh-panic-hunt (2026-10-14)

- Author: claw (subagent). The importers were not edited: this kill only finds and pins bugs.
- Lock `mesh-panic-hunt`: `crates/umber-mesh/tests/panic_hunt_mesh.rs`, this report.
- Source: ROADMAP wave 8, row `wave-fuzz-corpus`, item (a). Pattern copied from `crates/umber-app/tests/panic_hunt.rs`.

## FINDINGS (25 `#[ignore = "BUG: …"]` reproducers, every one confirmed failing)

The panic sites come from RUST_BACKTRACE=1 runs. Severity scale: ABORT means the process dies and `catch_unwind` cannot save it. PANIC can be caught. LOGIC means a wrong result with no crash.

### Umber's own code (fix in umber-mesh)

| # | importer | input shape | site | sev |
|---|---|---|---|---|
| 1 | usdc | `faceVertexIndices = [0, 1, 99]` (index past points) | `usdc.rs:220` Newell closure `points[a]`; the range check at :253 runs after it | PANIC |
| 2 | usdc | `faceVertexIndices = [0, 1, -1]` | `usdc.rs:219` (and :218) `face[i] as usize * 3` overflow | PANIC |
| 3 | usdc | `faceVertexCounts = [5, -2]`, indices len 3 (sum passes) | `usdc.rs:239` `indices[corner..corner+count]` | PANIC |
| 4 | usda | 20 000 nested `[` in a value | `usd.rs` `parse_value`↔`parse_sequence` recursion: stack overflow | ABORT |
| 5 | usda | 5 000 well-formed nested `def X "A" { … }` | `usd.rs` `parse_prim` recursion: stack overflow (overflows at depths between 1k and 5k on an 8 MiB stack) | ABORT |
| 6 | glTF | two primitives, second has index `u32::MAX` | `gltf.rs:70` `i + base` overflow (wraps silently in release) | PANIC |
| 7 | glTF | index 7777 on a 3-vertex primitive | `gltf.rs:70`: indices are never range-checked, so this returns `Ok` and the first consumer crashes later | LOGIC |
| 8 | usdz | a VALID package (openusd `ArchiveWriter`, one `scene.usdc`) | `usdc.rs:41` calls `usdc::read_file` (the raw crate reader) on the zip → `Err("bootstrap section corrupt")`. `.usdz` import cannot work at all; `openusd::usdz::Archive::open(..).read_first_layer()` is the API it should call | LOGIC |

Findings 1–3 are also reachable through `load_usdz`, because it reads a bare crate.

### Dependencies (umber-mesh must guard before the call, or catch the panic around it)

| # | dep | input shape | site | sev |
|---|---|---|---|---|
| 9 | gltf 1.4.1 | accessor `count: 0` | `accessor/util.rs:384` `count - 1` underflow | PANIC |
| 10 | gltf 1.4.1 | accessor `count: 2^62` | `accessor/util.rs:384` `stride * (count-1)` overflow | PANIC |
| 11 | gltf 1.4.1 | `byteStride: 4` on a VEC3 f32 view | `accessor/util.rs:376` debug_assert | PANIC (debug) |
| 12 | gltf 1.4.1 | POSITION accessor is u8 SCALAR or f32 VEC2 | `accessor/util.rs:371` debug_assert_eq | PANIC (debug) |
| 13 | gltf 1.4.1 | NORMAL attribute on a u32 SCALAR accessor | `accessor/util.rs:371` | PANIC (debug) |
| 14 | gltf 1.4.1 | index accessor componentType FLOAT (5126) / BYTE (5120) | `mesh/mod.rs:396` `unreachable!()` | PANIC |
| 15 | gltf 1.4.1 | TEXCOORD_0 attribute on a u32 accessor | `mesh/mod.rs:430` `unreachable!()` | PANIC |
| 16 | gltf-json 1.4.1 | `"POSITION": 7` with only 2 accessors | `mesh.rs:151` `root.accessors[i]` inside validation | PANIC |
| 17 | tobj 4.0.5 | `f 1/-5 …` (relative vt before the first) | `lib.rs:852` `vt * 2` overflow | PANIC |
| 18 | tobj 4.0.5 | `f 1//-5 …` (relative vn) | `lib.rs:860` `vn * 3` overflow | PANIC |
| 19 | openusd 0.7.0 | crate with a corrupt array/token count (single-byte flips at offsets 36, 104, 188, 389 of the 869-byte fixture) | `usdc/reader.rs` `ReadExt::read_vec` / `read_compressed`: `vec![T::default(); count]` with no bound. Allocations of 1 TB to 305 PB fail and abort | ABORT |
| 20 | openusd 0.7.0 | same count fields, larger values | the same sites → `capacity overflow` | PANIC |
| 21 | openusd 0.7.0 | flip @351 | `usdc/coding.rs:52` `&data[offset..]` | PANIC |
| 22 | openusd 0.7.0 | flip @358 | `usdc/coding.rs:22` `count * 2` overflow | PANIC |
| 23 | openusd 0.7.0 | flip @200 (lz4 chunk count ≠ 0) | `usdc/reader.rs:1562` `todo!()` | PANIC |
| 24 | openusd 0.7.0 | flip @408 | `usdc/reader.rs:656` `debug_assert!(!is_compressed)` | PANIC (debug) |

Item 19 is pinned by one reproducer that covers four offsets. Items 20 and 21–24 have one reproducer each, plus a usdz duplicate. That gives 25 reproducers in total (with #1–#3 and #11–#13 split as listed).

openusd's `safe = true` mode (`CrateData::open(_, true)`) does not protect against items 19–24. The usdc.rs module doc says "a corrupt file never panics", and that claim is false.

Exhaustive single-byte-flip map of the usdc fixture (869 bytes): 510 Err, 226 Ok, 66 ABORT, 67 PANIC. About 15% of one-byte corruptions crash the process outright.

FBX (ufbx) and USDA's non-recursive paths had no findings: 401 FBX cases and 646 usda cases all returned typed results.

## Built

`crates/umber-mesh/tests/panic_hunt_mesh.rs`: 7 active tests, 25 `bug_*` reproducers, and 1 ignored triage tool. The active suite runs in about 1 s in debug.

| test | cases | ok / err | known-bug (caught, pinned) | abort-skipped |
|---|---|---|---|---|
| noise (all 7 exts + magic-prefixed noise, empty/missing/no-ext/.stl) | 2404 | 0 / 2404 | 0 | 0 |
| fbx (cube_b7400 truncations, strided flips, zeroed ranges, 150 random) | 401 | 47 / 354 | 0 | 0 |
| gltf (JSON + GLB byte mutations, 17 structural edits) | 732 | 5 / 726 | 1 | 0 |
| usda (byte mutations, numeric/huge-count/negative/NaN edits, depth ≤512) | 646 | 98 / 548 | 0 | 0 |
| usdc (byte mutations of an openusd-written crate, 5 topology variants) | 462 | 25 / 393 | 19 | 25 |
| usdz (package mutations + crate-as-usdz mutations) | 669 | 23 / 619 | 12 | 15 |
| obj (byte mutations, numeric + relative-index + mtllib edits) | 515 | 112 / 403 | 0 | 0 |

No success on noise was FLAGGED, and no case exceeded the 5 s budget.

## Judgment calls

- Known-bug triage matches on the panic site, not the message. A chained panic hook records `file:line` for each panic that happens inside a case. For panics inside `alloc` it also records the dependency frame that drove the allocation. A caught panic at a pinned site is tallied as `known-bug` and printed. A panic at any other site fails the class. This keeps the loops able to find new sites. The alternative was steering inputs away from the bug classes, which would have thrown away most of the usdc corpus.
- Cases that abort cannot run in the loop. `USDC_ABORTS` / `USDZ_ABORTS` list them by case name (25 and 15 names). Each skip is counted in the output, and the class is pinned in a child process by `bug_usdc_huge_count_allocation_aborts`. The abort lists were found by repeated runs with `UMBER_FUZZ_TRACE=1` (the last traced case before the abort names the killer). `UMBER_FUZZ_SKIP=a|b` adds names at runtime for future triage.
- The ABORT reproducers (#4, #5, #19) re-run the test binary as a child on `triage_one_file` and assert a clean typed Err. Running them in-process would kill every other test once they are un-ignored.
- usdc reproducers #20–24 are single-byte flips of the openusd-written fixture. A shrunk standalone byte string would be shorter, but the crate format is sectioned with offsets, so a flip of the generated fixture is the honest minimal form. `usdc_flip` pins the fixture length at 869 bytes, so an openusd writer change fails loudly instead of moving the offsets silently.
- The gltf structural edits skip the count / stride / type-swap edits. Those panic on every case and have their own reproducers. The OBJ edits drop `1 → -1`, which always lands in the vt/vn slots (#17/#18).
- A success on mutated-valid input is not flagged, because a mutation can stay valid (for example a flipped float). Only pure noise is held to "must be Err".

## Unverified

- Release behaviour. Findings marked "(debug)", and the arithmetic overflows (#2, #6, #9, #10, #17, #18, #22), panic only when debug assertions / overflow checks are on. In release they wrap. #2 then still indexes out of bounds; #6 gives a silently wrong index; the others are not traced. Debug is where CI runs.
- Whether umber-app wraps the mesh import in `catch_unwind`. If it does not, every PANIC above crashes the app on File→Open. Either way, ABORT findings (#4, #5, #19) crash the app.
- The stack-overflow depth is host/stack dependent. The reproducers use depths well past the threshold measured on an 8 MiB stack.

## Deferred

- All fixes (outside the lock). Suggested shape: depth caps in `usd.rs` parse_value/parse_prim; moving the usdc.rs index check before the Newell closure; rejecting negative counts; bounds-checking glTF indices against `vertex_count` and using `checked_add` on the base; pre-validating glTF accessor component type, dimensions, count ≥ 1 and stride before calling the reader; routing `load_usdz` through `openusd::usdz::Archive`. For dependency panics, either upstream fixes or a `catch_unwind` boundary in `umber_mesh::load`. Rule 11 (never crash) does not cover umber-mesh today.
- An OBJ `mtllib` path traversal / absolute path probe (tobj opens whatever path `mtllib` names). It is filesystem-reaching, not a panic, so it was left out of scope.
- A shared-dir race audit of the existing `usdc.rs` unit tests: they write to the fixed `temp_dir()/umber-usdc-tests`.

## Gates run

- `cargo test -p umber-mesh --test panic_hunt_mesh`: ok, 7 passed, 26 ignored. Each of the 25 `bug_*` tests was run with `--ignored` and FAILED as intended.
- `cargo clippy -p umber-mesh --all-targets -- -D warnings`: pass.
- `cargo fmt --all` + `--check`: pass.
- `scripts/verify-kill.sh crates/umber-mesh/tests/panic_hunt_mesh.rs`: balanced, fmt-clean, trap-scanned, locked.

Process note: `cargo fmt --all` runs across the whole shared tree, but `--check` was clean before and after, so no other agent's file changed.
