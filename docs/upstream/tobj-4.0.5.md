# Upstream draft: tobj 4.0.5 (NOT FILED)

- Tracker: https://github.com/Twinklebear/tobj/issues. Verified via the crates.io API `repository` and the crate's `Cargo.toml`.
- Latest release: 4.0.5 (2026-08-02), still the newest on crates.io. `master` @ `7cc3123` (2026-09-18) still has the bug at `src/lib.rs:923` / `:931` (single-index) and `:1126` / `:1167` (multi-index).
- Source: Umber mesh panic hunt, findings #17–#18. Reproducers `bug_obj_relative_texcoord_underflow_panics` and `bug_obj_relative_normal_underflow_panics` in `crates/umber-mesh/tests/panic_hunt_mesh.rs`.
- Duplicates: none. I listed all 33 issues (`gh issue list --state all`) and searched for panic, overflow, negative, relative, fuzz, "out of bounds" and index. The nearest is #18 (closed 2020, "crashes if model references index that's out of bounds"), which was about a positive `v` index. The `v` bounds check now saturates, and `vt` / `vn` were left unchecked.
- Verified 2026-10-15 with the program below against 4.0.5 and git `master` @ `7cc3123`, in debug and release.

---

**Title:** Relative `vt`/`vn` index before the first element overflows in `add_vertex` (panic in debug, silently wrong data in release)

**Body:**

A negative (relative) texcoord or normal index whose magnitude exceeds the number of `vt`/`vn` read so far makes `VertexIndices::parse` compute `(tex_sz as isize + x) as usize`, which wraps to a huge `usize`. `add_vertex` then bounds-checks it with an unchecked `vt * 2 + 1` / `vn * 3 + 2`, which overflows:

| build | single_index | site (4.0.5) | site (`master` @ 7cc3123) | result |
|---|---|---|---|---|
| debug | true | `lib.rs:852` (`vt`), `:860` (`vn`) | `:923`, `:931` | panic `attempt to multiply with overflow` |
| debug | false | `lib.rs:1055`, `:1096` (`add_vertex_multi_index`) | `:1126`, `:1167` | same panic |
| release | either | same | same | `vt * 2 + 1` wraps. For small magnitudes it fails the bound and gives `Err(FaceTexCoordOutOfBounds)`. A chosen index wraps onto a **valid** slot and loads the wrong normal, with no error |

The position index has no such problem: `add_vertex` checks it with `v.saturating_mul(3).saturating_add(2)` (line 843).

Reproducer (`tobj = "=4.0.5"`):

```rust
fn main() {
    let cases = [
        ("relative vt", "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1/-5 2/1 3/1\n"),
        ("relative vn", "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 1//-5 2//1 3//1\n"),
        // Release: 2 + x = 0xAAAA…AAAB as usize, so vn * 3 wraps to 1, a valid offset into `normal`.
        ("aliased vn", "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nvn 7 8 9\nf 1//-6148914691236517207 2//1 3//1\n"),
    ];
    for (name, obj) in cases {
        for single_index in [true, false] {
            let opts = tobj::LoadOptions { triangulate: true, single_index, ..Default::default() };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tobj::load_obj_buf(&mut obj.as_bytes(), &opts, |_| Err(tobj::LoadError::OpenFileFailed))
                    .map(|(models, _)| models[0].mesh.normals.clone())
            }));
            println!("{name} (single_index={single_index}): {result:?}");
        }
    }
}
```

Observed on 4.0.5 and `master`:

- debug: all six runs panic at the sites above.
- release: `relative vt` → `Err(FaceTexCoordOutOfBounds)`, `relative vn` → `Err(FaceNormalOutOfBounds)`, `aliased vn` → `Ok` with normals `[0.0, 1.0, 7.0, …]`. The first vertex's normal is assembled from the wrong floats, and nothing reports an error.

Expected: `Err(LoadError::FaceTexCoordOutOfBounds)` / `Err(FaceNormalOutOfBounds)` in every build.

Suggested fix, either or both:

1. In `VertexIndices::parse`, reject a relative index that points before the start: `let i = (sz as isize).checked_add(x).filter(|i| *i >= 0)?;` → `return None` (the caller then yields `LoadError::FaceParseError`).
2. In `add_vertex` / `add_vertex_multi_index`, bounds-check the same way `v` already is: `vt.saturating_mul(2).saturating_add(1) >= texcoord.len()` and `vn.saturating_mul(3).saturating_add(2) >= normal.len()`. (The `v_color` check at line 868, `v * 3 + 2`, is safe today only because the `v` check above it runs first.)

Found by fuzzing an importer that passes untrusted `.obj` files to this crate.
