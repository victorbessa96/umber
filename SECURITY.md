# Security Policy

## Supported versions

Umber is pre-release: no tagged release exists yet, so no version
receives security fixes on a branch of its own — fixes land on `master`.
The first supported tag will be `v0.1.0`.

## Reporting a vulnerability

Please report vulnerabilities **privately** using [GitHub's private
vulnerability reporting](https://github.com/victorbessa96/umber/security/advisories/new)
— do not open a public issue.

Include: affected component, the input that triggers it (attach the
file if it's a file-format bug), reproduction steps, and impact. We aim
to acknowledge within 72 hours and will coordinate disclosure with you.

## Threat surface

Umber is a local-first desktop app with no network service. Its attack
surface is the **untrusted files it opens** and the **plugins it runs**:

| Surface | Where | Current defences |
|---|---|---|
| Meshes (FBX, glTF, OBJ, USDA, USDC, USDZ) | `umber-mesh` | own parsers validate topology before indexing; USD nesting capped at 256; glTF accessors pre-validated; `load()` catches dependency panics as a typed error. openusd 0.8.0 (since 2026-10-15) closes the crate-reader allocation ABORTS a panic boundary cannot catch — the 40 abort-class fuzz cases now return typed errors, pinned by the panic hunt on frozen 0.7.0 bytes. In the mesh panic-hunt (`crates/umber-mesh/tests/panic_hunt_mesh.rs`, 5,400+ cases) |
| PSD import | `umber-app` | typed errors; in the panic-hunt |
| Images (PNG/EXR/…) | `image` crate via `umber-export` | upstream decoder hardening |
| `.spp` (Substance project) import | `crates/spp` | read-only, never-crash lint gate; in the panic-hunt |
| MaterialX documents | `umber-graph` | element-depth cap (256); in the panic-hunt |
| `.umber` project files | `umber-app` | corrupt-file arms covered by the panic-hunt |
| WASM plugins | `umber-wasm`, `umber-app/src/plugins.rs` | consent first: a user-dir `.wasm` loads only once its blake3 is in `plugins-trusted.txt` (Graph panel ▸ Trust & load; a changed byte is untrusted again); 8 MiB module-size cap before compile; wasmtime sandbox: no host imports, fuel budget per call, 64 MiB guest-memory cap, fresh store per call |
| Agent protocol (`umber agent`) | `umber-cli` | stdin/stdout only — no socket; `deny_unknown_fields` on wire structs |

`crates/umber-app/tests/panic_hunt.rs` feeds deterministic garbage and
mutated-valid bytes to every public parse seam and asserts typed errors,
never a panic. A crash, hang, or out-of-bounds read on a crafted file is
a vulnerability — please report it the private way above.
