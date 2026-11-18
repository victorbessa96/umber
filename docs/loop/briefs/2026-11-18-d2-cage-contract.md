# Design brief: D2 cage contract alignment (cross-review MEDs, 2026-11-18)

Kill: `razul-d2-cage-contract`. Resolves the three D2 MED findings and one LOW from
`docs/loop/reviews/2026-11-17-d2-d5-cross-review.md` §D2 (next-steps B1).

## Contract decision (the deliberate resolution)

The D2 brief's ray recipe (brief line 51) and its DoD line 72 disagreed:

- Recipe: per low vertex, ray origin = `low_pos + normal * front`, direction
  `-normal`; on hit, **offset = hit − low_pos**; on miss, offset = `front * normal`.
- DoD line 72 claimed: on flat geometry (high == low), Inflate offsets =
  `front * normal`, byte-equal to Manual.

Both cannot hold: on flat, the ray from `low_pos + front*n` along `-n` hits the
surface at `t = front`, i.e. hit point = `low_pos`, so offset = 0. The
implementation (`Cage::from_high_low`, bake_support.rs) follows the recipe, and the
test asserts offset = 0 for Inflate vs `front*n` for Manual.

**Resolved contract**: the cage offset is defined relative to `low_pos`, and the
effective transfer ray origin is `low_pos + normal*front + offset` (brief line 58,
shader `cs_main`). On flat geometry Inflate (offset 0) and Manual (offset
`front*n`) therefore produce the *same effective ray origin* — which is what
"byte-equal to Manual" must mean, since that is what the transfer output depends
on. The DoD line is corrected to say exactly this; the implementation is unchanged.
The test is strengthened, not weakened: it keeps the per-method offset assertions
and adds the effective-ray-origin equality assertion across methods.

## Scope

1. `docs/loop/briefs/2026-11-16-d2-auto-cage.md`: correct DoD line 72 to the resolved
   contract (offset semantics + effective ray origin equality on flat), with a
   2026-11-18 resolution note. No implementation behavior change.
2. `crates/umber-mesh/src/bake_support.rs` tests:
   - Extend `cage_from_high_low_inflate_matches_manual_on_flat` with the
     effective-ray-origin equality assertion
     (`low_pos + n*front + inflate_off == low_pos + n*front + manual_off`, eps).
   - Add named test `transfer_shader_cage_lerp_matches_cpu`: `Cage::lerp_at` with
     weights `(w,u,v)` is bit-exact (`==` per f32 component) against the manual
     `a*w + b*u + c*v` per-component expansion, mirroring WGSL `lerp3_cage`
     (`bake_shaders.rs:1442`: `a*bary.x + b*bary.y + c*bary.z`) — same op order as
     CPU `uv_gbuffer::lerp3`, so bit-identical.
3. `crates/umber-bake/src/transfer.rs`: extract the inline
   `Vec<[f32;3]> → Vec<[f32;4]>` cage packing into a pure `fn pack_cage_vec4` and add
   a unit test asserting 16-byte alignment contract: output len == input len, w ==
   0.0, xyz preserved, and `bytemuck::cast_slice(&v).len() % 16 == 0` (the
   `GpuTriangle` vec4-precedent alignment the brief requires).
4. `crates/umber-app/src/bakes_panel.rs` + `assets/i18n/en-US.ftl` +
   `assets/i18n/pt-BR.ftl`: add `.on_hover_text(tr("bakes.cage-method-hint"))` to the
   cage-method ComboBox and the new `bakes.cage-method-hint` key in both catalogs
   (.ftl lock lifted 2026-11-12; both catalogs edited in the same kill).

No production behavior changes except the extracted pure packing fn (same output).
No format changes. No version bump.

## Acceptance

- rustfmt clean on touched .rs; check-layers GREEN; scorecard current.
- clippy `-p umber-mesh -p umber-bake -p umber-app --all-targets -- -D warnings`: 0 errors.
- `cargo test -p umber-mesh -p umber-bake -p umber-app`: all green, including the two
  new/extended tests.
- safe-push green; ls-remote verified; kill lock released; queue row appended.
