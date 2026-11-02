# Brief 2026-11-02 — export-padding dilation: driver side DONE, caller wiring owed

Lock owner: sibling local-4fe45fb (export-dilation-pad). This session implemented
the driver half of queue row 151 in crates/umber-export/src/driver.rs (fmt clean,
check-layers GREEN, 2 pinned tests UNRUN per the no-cargo envelope):

- `pub type DilateHook<'a> = &'a dyn Fn(&mut [u8], u32, u32, u16)` (packed RGBA8, w, h, tile)
- `pub const DEFAULT_PADDING_TEXELS: u32 = 32`
- `run_preset_dilated`, `run_preset_dilated_with_surface`,
  `run_preset_tiled_dilated`, `run_preset_tiled_dilated_with_surface`
  (existing run_preset* delegate with None — no-mesh callers byte-identical)
- write_outputs applies the hook per output after packing.

HARD RULE: do not add umber-core or umber-mesh as deps of umber-export — L1->L1
(or L1->L0 mesh) edges: umber-core is same-layer (RED, verified), and the gate
keeps export mesh-free by design. Callers close over the DilationMap instead.

Caller wiring (export_dialog.rs, ctx.mesh: Option<&umber_mesh::MeshData>):
  let mut cache: HashMap<(u16,u32,u32), umber_core::uv_dilation::DilationMap> = ...;
  let hook = move |packed: &mut [u8], w: u32, h: u32, tile: u16| {
      let map = cache.entry((tile,w,h)).or_insert_with(|| {
          let covered: Vec<bool> = umber_mesh::fill::rasterize_uv_geometry(mesh, tile, w, h)
              .iter().map(|g| g.is_some()).collect();
          umber_core::uv_dilation::DilationMap::new(&covered, w, h, Some(umber_export::DEFAULT_PADDING_TEXELS))
      });
      map.apply(packed, 4);
  };
  ... run_preset_tiled_dilated(&filtered, &pairs, &group_sources, &self.output_dir, mesh.map(|_| &hook as umber_export::DilateHook))
Same shape in umber-cli main.rs export_mesh (mesh: &MeshData in scope).
Then: kill report docs/loop/kill-reports/2026-11-02-export-dilation.md, queue row, dcommit.
