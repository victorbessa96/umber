# Writing an Umber node plugin

An Umber plugin is a `.wasm` module that adds one node to the Graph
panel. It takes one image (plus up to four named params) and returns an
image of the same size. It runs sandboxed: no host imports, no file or
network access, a fuel budget per call, a 64 MiB memory cap, and a fresh
instance per evaluation.

This guide covers the v1 contract: how to build a plugin, how a user
installs it, and what stays stable. The byte-level wire is in
`crates/umber-wasm/WIRE.md`, and the design rationale in
`docs/specs/wasm-plugins-design.md`.

## 1. The smallest plugin

```toml
# Cargo.toml
[package]
name = "plugin-invert"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
umber-plugin-sdk = { path = "../umber/crates/umber-plugin-sdk" }

[profile.release]
panic = "abort"     # a panic is a trap; no unwinding in no_std
opt-level = "s"
lto = true
```

```rust
// src/lib.rs
#![no_std]

use umber_plugin_sdk::{umber_node, Image, ImageMut, NodeError, Params};

fn invert(input: &Image<'_>, _p: &Params<'_>, out: &mut ImageMut<'_>) -> Result<(), NodeError> {
    for (o, i) in out.data.chunks_exact_mut(4).zip(input.data.chunks_exact(4)) {
        o.copy_from_slice(&[255 - i[0], 255 - i[1], 255 - i[2], i[3]]);
    }
    Ok(())
}

umber_node!(name: "invert", params: 0, eval: invert);
```

Build it:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/plugin_invert.wasm
```

Three complete examples live in `examples/`: `plugin-blur5` (a 5×5 box
blur), `plugin-vignette` (a `strength` param), and `plugin-infinite`,
which loops forever on purpose so the tests can prove fuel stops it.
`crates/umber-wasm/tests/fixtures/build.sh` builds all three.

## 2. The contract your function sees

| | |
|---|---|
| `input` | RGBA8, row-major, **top row first**, `data.len() == width * height * 4`. Width and height are at least 1. |
| `out` | Pre-sized to the input's dimensions. **Write every byte.** The region is not zeroed for you. |
| colour | 8-bit values exactly as the graph passes them. v1 does not convert colour spaces for you. |
| return | `Ok(())`, or a `NodeError` (`Internal` for your own failures). The node then shows a failed evaluation; nothing crashes. |
| panics | A panic traps the instance and is reported as a plugin failure. It never takes the host down. |

### Params

A node declares how many params it takes (`params: N`, at most 4), and
the graph sends them by name.

- `params.float("strength", 0.5)` and `params.int("radius", 2)` return
  your default when the param is absent, and `Err(NodeError::Internal)`
  when it is present but of the wrong type (or a non-finite float).
- Every other kind (`Vec2`, `Vec3`, `Color`, `Bool`, `Asset`) is read
  with `params.get("name")` and a match on `Param::…`.
- `Color` is linear RGB.

v1's node-def record carries only the param COUNT, not names or types,
so the Graph panel shows a plugin node without per-param widgets yet.
Param values still round-trip through `.mtlx` documents.

### Limits

| limit | value | why |
|---|---|---|
| inputs | exactly 1 image | v1 filter shape |
| params | ≤ 4 | v1 boundary |
| node name | ≤ 64 UTF-8 bytes, unique | a name that collides with a built-in (`blur`, `flood_fill`, …) or an earlier plugin is **refused**; plugins never shadow |
| module size | ≤ 8 MiB | refused before compilation |
| guest memory | 64 MiB | both images live in guest memory, so this bounds the image size to about 2896² |
| fuel | 2·10⁹ per call | roughly a 2048² image for a blur-heavy plugin; a runaway loop ends as `FuelExhausted` |
| imports | none | a module importing anything is refused at load |

## 3. Installing: the consent step

Put the `.wasm` in the user plugins directory:

| OS | directory |
|---|---|
| Linux | `$XDG_DATA_HOME/umber/plugins/`, else `~/.local/share/umber/plugins/` |
| Windows | `%APPDATA%\umber\plugins\` |

On the next launch the plugin does **not** run. It is hashed (blake3),
and the Graph panel lists it as awaiting trust, showing its path and the
first 12 hex digits of the hash. **Trust & load** records the hash in
`plugins-trusted.txt`, beside the plugins directory, and loads it.

Trust binds the exact bytes: rebuilding the plugin, or changing any byte
of it, makes it untrusted again. To revoke trust, delete its line from
`plugins-trusted.txt`.

**For authors:** publish your release's blake3 hash (`b3sum
plugin_x.wasm`) next to the download, so users can check the 12-digit
prefix the panel shows before trusting.

## 4. Testing a plugin without the app

`umber-wasm` loads modules exactly as the app does:

```rust
let bytes = std::fs::read("plugin_invert.wasm")?;
let module = umber_wasm::PluginRuntime::new().load(&bytes)?; // contract + node-def check
assert_eq!(module.def().name, "invert");
```

`crates/umber-wasm/tests/guide_snippet.rs` runs exactly this snippet
against the vignette example. `crates/umber-wasm/tests/fixtures.rs` goes
further: it evaluates the blur5 plugin inside a real graph and checks it
byte-for-byte against the native `blur`, and runs `plugin-infinite` to
prove fuel exhaustion.

(Verified 2026-10-15: §1's Cargo.toml and lib.rs, copied verbatim from
this page, build to a 2.7 KB `.wasm` that this snippet loads as `invert`
with 0 params.)

## 5. What is stable (the v1 ABI contract)

**Stable for every 0.x release:**

- the two exports `umber_node_def() -> i32` and
  `umber_eval(in_ptr, in_len, out_ptr, out_cap) -> i32`;
- the memory export (`umber_mem`, or rustc's `memory`);
- the `MBER` image region layout, the `MDEF` node-def record, the param
  TLV kinds 0-6, and the error codes −1…−4, all in `WIRE.md`;
- the no-imports rule.

A v1 `.wasm` built today keeps loading.

**May change, with a CHANGELOG entry:**

- the fuel budget, the memory cap and the module-size cap. Raising them
  never breaks a plugin; lowering them is a breaking change and will be
  called out as one;
- the SDK's Rust API (`Params` helpers, `umber_node!`'s syntax). The
  wire under it is what is frozen;
- the consent store's file format.

**A v2 wire**, if it comes (named param declarations, more inputs,
host-provided services), uses a new magic, and the host will keep
loading v1 modules beside it.

## 6. Security model, in one paragraph

A plugin cannot touch anything but the two images and its params: no
imports means no syscalls, no clock, no randomness and no I/O. A hostile
plugin can do three things: waste its fuel budget, which ends the call;
return wrong pixels, which only affects the node's output; or trap,
which is reported as a failed node. The consent step exists so that code
someone else dropped into your plugins directory does not run unseen,
not because the sandbox is weak. `SECURITY.md` lists the boundaries.
