# Upstream draft: gltf / gltf-json 1.4.1 (NOT FILED)

- Tracker: https://github.com/gltf-rs/gltf/issues. Verified via the crates.io API (`repository` for both `gltf` and `gltf-json`) and the crate's own `Cargo.toml`.
- Latest release: 1.4.1 (2024-05-10), still the newest on crates.io. `main` at `50d6522` (2026-05-11, PR #471 "Fix panics") is unreleased. #465 asks for a release.
- Source: Umber mesh panic hunt, findings #9–#16 (`docs/loop/kill-reports/2026-10-14-mesh-panic-hunt.md`), reproducers `bug_gltf_*` in `crates/umber-mesh/tests/panic_hunt_mesh.rs`.
- Verified 2026-10-15 with a standalone binary (code below) against crates.io 1.4.1 and git `main` @ `50d6522`, in debug and release.

## Status per finding

| # | input | 1.4.1 site | 1.4.1 debug / release | `main` @ 50d6522 | status |
|---|---|---|---|---|---|
| 9 | accessor `count: 0` | `accessor/util.rs:384` `count - 1` | panic / `Some([])` | no panic | fixed on `main` by #471 ("avoid underflows with zero counts"), unreleased |
| 10 | accessor `count: 2^62` | `accessor/util.rs:384` `stride * (count - 1)` | panic / `Some([])` | panic (`util.rs:419`) | same expression as open **#479** (that one is a large `byteOffset`). Draft comment on #479 below |
| 11 | `byteStride: 4` on a VEC3 f32 view | `util.rs:376` debug_assert; release `util.rs:266` `assert!` | panic / **panic** | panic (`util.rs:408` / `:286`) | **new issue: Draft A** |
| 12 | POSITION as f32 VEC2 or u8 SCALAR | `util.rs:371` `debug_assert_eq` | panic / wrong-size read | panic (`util.rs:403`) | same root cause as open **#478** (which covers TEXCOORD VEC3 at the same assert). Draft comment on #478 |
| 13 | NORMAL on a u32 SCALAR accessor | `util.rs:371` | panic / `None` | panic (`util.rs:403`) | same as #12: comment on #478 |
| 14 | indices componentType FLOAT / BYTE | `mesh/mod.rs:396` `unreachable!()` | panic / panic | panic | **already reported: https://github.com/gltf-rs/gltf/issues/478** (FLOAT case; BYTE hits the same arm) |
| 15 | TEXCOORD_0 on a u32 accessor | `mesh/mod.rs:430` `unreachable!()` | panic / panic | panic | same root cause as #478 (its TEXCOORD case is VEC3, not u32). Comment on #478 |
| 16 | `"POSITION": 7` with 2 accessors (gltf-json) | `gltf-json/src/mesh.rs:151` `root.accessors[i]` | panic / panic | `Err(Index out of bounds)` | fixed on `main` by #471 ("fix crash if accessor is not available"), unreleased |

Searches run (`gh search issues --repo gltf-rs/gltf`): panic, unreachable, overflow, stride, count, "index out of bounds", fuzz. Related but not duplicates: #460 (add `try_*` APIs), #359 (closed, "Don't panic on any malformed glTF input"), #313 (sparse iteration).

## Shared reproducer (`gltf = { version = "=1.4.1", features = ["import"] }`)

A one-triangle buffer: 36 bytes of f32 VEC3 positions, then 12 bytes of u32 indices `[0, 1, 2]`. Run with `cargo run -- <case>`.

```rust
const BUFFER: &str = "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAAAAAAAAEAAAACAAAA";

fn gltf(extra_attr: &str, stride: &str, pos: &str, index_component: u32) -> String {
    format!(
        r#"{{"asset":{{"version":"2.0"}},
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0{extra_attr}}},"indices":1}}]}}],
            "buffers":[{{"byteLength":48,"uri":"{BUFFER}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36{stride}}},
                           {{"buffer":0,"byteOffset":36,"byteLength":12}}],
            "accessors":[{{"bufferView":0,{pos},"min":[0,0,0],"max":[1,1,0]}},
                         {{"bufferView":1,"componentType":{index_component},"count":3,"type":"SCALAR"}}]}}"#
    )
}

fn main() {
    const POS: &str = r#""componentType":5126,"count":3,"type":"VEC3""#;
    let case = std::env::args().nth(1).expect("stride | count | position | normal | texcoord | index-byte");
    let json = match case.as_str() {
        "stride" => gltf("", r#","byteStride":4"#, POS, 5125),
        "count" => gltf("", "", r#""componentType":5126,"count":4611686018427387904,"type":"VEC3""#, 5125),
        "position" => gltf("", "", r#""componentType":5126,"count":3,"type":"VEC2""#, 5125),
        "normal" => gltf(r#","NORMAL":1"#, "", POS, 5125),
        "texcoord" => gltf(r#","TEXCOORD_0":1"#, "", POS, 5125),
        "index-byte" => gltf("", "", POS, 5120),
        other => panic!("unknown case {other}"),
    };
    let (document, buffers, _) = gltf::import_slice(json.as_bytes()).expect("validation passes");
    let primitive = document.meshes().next().unwrap().primitives().next().unwrap();
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    println!("positions {:?}", reader.read_positions().map(|p| p.collect::<Vec<_>>()));
    println!("normals {:?}", reader.read_normals().map(|n| n.collect::<Vec<_>>()));
    println!("texcoords {:?}", reader.read_tex_coords(0).map(|t| t.into_f32().collect::<Vec<_>>()));
    println!("indices {:?}", reader.read_indices().map(|i| i.into_u32().collect::<Vec<_>>()));
}
```

`import_slice` returns `Ok` for every case. Validation accepts them all.

---

## Draft A (new issue): `Reader` panics when `byteStride` is smaller than the element size (release builds too)

**Title:** `byteStride` smaller than the accessor element size passes validation, then `read_positions()` panics (debug and release)

**Body:**

`gltf::import_slice` accepts a bufferView with `"byteStride": 4` (the schema minimum) used by a VEC3 f32 accessor (12-byte elements). `read_positions()` then panics:

- debug: `Mismatch in stride, expected at least 12 stride but found 4` (`src/accessor/util.rs:376` in 1.4.1, `:408` on `main` @ 50d6522)
- release: `assertion failed: slice.len() >= 3 * mem::size_of::<T>()` (`src/accessor/util.rs:266` in 1.4.1, `:286` on `main`), from `<[T; 3] as Item>::from_slice` when the last strided element runs off the slice.

Reproduced on 1.4.1 and on `main` @ 50d6522: `cargo run -- stride` with the program above.

Expected: an `Err` from validation, or `None` from the reader, not a panic. The reader's own `debug_assert!` states the invariant (`stride >= size_of::<T>()`), but nothing enforces it for input.

Suggested fix: in `accessor::Iter::new` (and the sparse branch), return `None` when `stride < mem::size_of::<T>()` instead of `debug_assert!`. Or have validation reject a `byteStride` smaller than the element size of any accessor that uses the view.

Found by fuzzing an importer that passes untrusted glTF to this crate.

---

## Draft comment on #479 (finding #10)

> The same expression also overflows through `count` alone, with a normal `byteOffset`: `"count": 4611686018427387904` on the POSITION accessor panics with `attempt to multiply with overflow` at `src/accessor/util.rs:384` (1.4.1) / `:419` (`main` @ 50d6522), in `stride * (accessor.count() - 1)`. In release builds it wraps, and `read_positions()` returns an empty iterator with no error. The `checked_mul` / `checked_add` fix proposed here covers it. Validation could also check `count` against the bufferView length (§3.6.2.4). Repro: change `"count":3` to `"count":4611686018427387904` in this issue's program.

## Draft comment on #478 (findings #12, #13, #15)

> More layouts in the same class, all accepted by validation, on 1.4.1 and `main` @ 50d6522:
>
> | case | call | debug | release |
> |---|---|---|---|
> | `POSITION` as f32 `VEC2` (or u8 `SCALAR`) | `read_positions()` | `assertion left == right failed` (12 vs 8), `accessor/util.rs:371` / `:403` | reads the data as VEC3 f32, no error |
> | `NORMAL` on a u32 `SCALAR` accessor | `read_normals()` | same assert (12 vs 4) | `None`, no error |
> | `TEXCOORD_0` on a u32 `SCALAR` accessor | `read_tex_coords(0)` | `unreachable!()`, `mesh/mod.rs:430` | same panic |
> | indices `BYTE` (5120) | `read_indices()` | `unreachable!()`, `mesh/mod.rs:396` (the FLOAT arm already listed here) | same panic |
>
> Spec §3.7.2.1 requires POSITION and NORMAL to be VEC3 float. The fix proposed here (check the attribute table in validation, or return `None` instead of `unreachable!()` / `debug_assert_eq!`) covers all four. I can give the full repro program if useful.

## Expected behaviour (all findings)

Malformed input returns `Err` from `import*` / `Gltf::from_slice`, or `None` from a reader. It never panics.
