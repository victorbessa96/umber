//! `umber-cli bench [--size N] [--rays N] [--json PATH]`: a comparable
//! timing of the headless paths on a fixed reference mesh (ROADMAP
//! `wave-benchmark-public`: "the bake/export/paint paths timed on a
//! reference mesh, so community perf reports are comparable").
//!
//! - **Mesh:** a generated cube grid (6 faces × n² quads, pure arithmetic,
//!   identical bytes on every machine), so no asset ships and two reports
//!   always measure the same geometry. [`BENCH_SUBDIVISIONS`] = 64 →
//!   49,152 triangles: large enough to load the bakers, small enough for
//!   a laptop iGPU. (The 1M-triangle viewport scene stays behind
//!   umber-gpu's `perf` feature; `docs/perf/protocol.md`.)
//! - **Timed:** mesh generation, GPU context creation, each of the six
//!   bakers (bake + UV padding, as `bake-all` runs them), and the PNG
//!   encode of each map, wall-clock per stage via `Instant`.
//! - **Output:** a human table, plus one `bench json:` line (the stage
//!   times, size, rays, triangles, adapter name, device type, backend,
//!   umber version and commit) that reports can paste whole. `--json
//!   PATH` also writes it to a file.
//! - Nothing is written except the optional JSON and a scratch dir for
//!   the PNG encodes, which is removed afterwards.
//!
//! Paint is not timed here: the headless paint path is the agent
//! session's, and its numbers live in the `pool_perf` / input-to-photon
//! benches (`perf` feature). This command needs no feature flag.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};

/// Cube-grid subdivisions per face edge for the bench mesh.
pub const BENCH_SUBDIVISIONS: u32 = 64;

/// The bench's reference mesh: a cube of six `n × n` quad grids, with
/// each face's UVs filling [0, 1]² (overlapping UVs: every face bakes into
/// the same square, which is what loads the bakers evenly). The same
/// generator shape as umber-gpu's `ref_scene::cube_grid`, kept here so
/// the CLI needs no `perf` feature.
pub fn bench_mesh(subdivisions: u32) -> umber_mesh::MeshData {
    let n = subdivisions.max(1) as usize;
    let row = n + 1;
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let mut mesh = umber_mesh::MeshData {
        positions: Vec::with_capacity(6 * row * row),
        normals: Vec::with_capacity(6 * row * row),
        uvs: Vec::with_capacity(6 * row * row),
        indices: Vec::with_capacity(6 * n * n * 6),
        material_names: vec!["BenchCube".to_string()],
    };
    for (face, (normal, u, v)) in FACES.iter().enumerate() {
        for j in 0..row {
            let t = j as f32 / n as f32;
            for i in 0..row {
                let s = i as f32 / n as f32;
                let (a, b) = (2.0 * s - 1.0, 2.0 * t - 1.0);
                mesh.positions
                    .push(std::array::from_fn(|k| normal[k] + a * u[k] + b * v[k]));
                mesh.normals.push(*normal);
                mesh.uvs.push([s, t]);
            }
        }
        let base = (face * row * row) as u32;
        for j in 0..n {
            for i in 0..n {
                let a = base + (j * row + i) as u32;
                let b = a + 1;
                let c = a + 1 + row as u32;
                let d = a + row as u32;
                mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
    }
    mesh
}

/// One timed stage.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Stage {
    /// Stage name (`mesh`, `gpu-context`, `bake:<map>`, `encode:<map>`).
    pub name: String,
    /// Wall-clock milliseconds.
    pub ms: f64,
}

/// The whole report: what ran, where, and how long each stage took.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchReport {
    /// `umber-cli` version.
    pub version: &'static str,
    /// Map size in texels (square).
    pub size: u32,
    /// AO / thickness rays per texel.
    pub rays: u32,
    /// Reference-mesh triangles.
    pub triangles: usize,
    /// GPU adapter name, device type and backend.
    pub adapter: String,
    /// Per-stage timings, in run order.
    pub stages: Vec<Stage>,
    /// Sum of the bake stages, ms (the headline number).
    pub bake_total_ms: f64,
}

/// Parses `--size N` / `--rays N` / `--json PATH`.
fn parse(args: &[String]) -> Result<(u32, u32, Option<PathBuf>)> {
    let (mut size, mut rays, mut json) = (1024u32, 32u32, None);
    let mut i = 0;
    while i < args.len() {
        let value = || {
            args.get(i + 1)
                .ok_or_else(|| anyhow::anyhow!("{} needs a value", args[i]))
        };
        match args[i].as_str() {
            "--size" => size = value()?.parse().context("--size")?,
            "--rays" => rays = value()?.parse().context("--rays")?,
            "--json" => json = Some(PathBuf::from(value()?)),
            other => anyhow::bail!(
                "unknown bench option {other}\nusage: umber-cli bench [--size N] [--rays N] [--json PATH]"
            ),
        }
        i += 2;
    }
    anyhow::ensure!(
        (64..=8192).contains(&size) && size.is_power_of_two(),
        "--size must be a power of two in 64..=8192"
    );
    anyhow::ensure!((1..=1024).contains(&rays), "--rays must be in 1..=1024");
    Ok((size, rays, json))
}

/// `umber-cli bench`.
pub fn bench_cmd(args: &[String]) -> Result<()> {
    let (size, rays, json_path) = parse(args)?;
    let report = run(size, rays)?;
    println!(
        "umber bench  {}  size {}  rays {}  {} tris",
        report.adapter, size, rays, report.triangles
    );
    for stage in &report.stages {
        println!("  {:<24} {:>10.1} ms", stage.name, stage.ms);
    }
    println!("  {:<24} {:>10.1} ms", "bake total", report.bake_total_ms);
    let json = serde_json::to_string(&report)?;
    println!("bench json: {json}");
    if let Some(path) = json_path {
        std::fs::write(&path, &json).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Runs every stage and returns the report.
pub fn run(size: u32, rays: u32) -> Result<BenchReport> {
    let mut stages = Vec::new();
    let time = |name: String, start: Instant, stages: &mut Vec<Stage>| {
        stages.push(Stage {
            name,
            ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    };
    let t = Instant::now();
    let mesh = bench_mesh(BENCH_SUBDIVISIONS);
    time("mesh".into(), t, &mut stages);
    let t = Instant::now();
    let (ctx, adapter) = crate::bake_context_with_info()?;
    time("gpu-context".into(), t, &mut stages);
    let flags = crate::BakeFlags {
        size,
        rays,
        ..crate::BakeFlags::default()
    };
    let scratch = std::env::temp_dir().join(format!("umber-bench-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    let result = (|| -> Result<f64> {
        let seams = crate::margin_seams(&mesh, &flags);
        let mut bake_total = 0.0;
        for map in crate::BakeMap::ALL {
            let name = map.kind().suffix();
            let t = Instant::now();
            let data = crate::bake_map(&ctx, &mesh, map, &flags)?;
            let data = crate::pad_map(&ctx, &mesh, seams.as_ref(), data, &flags)?;
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            bake_total += ms;
            stages.push(Stage {
                name: format!("bake:{name}"),
                ms,
            });
            let t = Instant::now();
            let path: &Path = &scratch.join(format!("{name}.png"));
            umber_export::png::write_png(path, size, size, &data, map.transfer())?;
            time(format!("encode:{name}"), t, &mut stages);
        }
        Ok(bake_total)
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    let bake_total_ms = result?;
    Ok(BenchReport {
        version: env!("CARGO_PKG_VERSION"),
        size,
        rays,
        triangles: mesh.indices.len() / 3,
        adapter,
        stages,
        bake_total_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_mesh_is_fixed_and_well_formed() {
        let mesh = bench_mesh(BENCH_SUBDIVISIONS);
        assert_eq!(mesh.indices.len() / 3, 6 * 64 * 64 * 2, "49,152 triangles");
        assert_eq!(mesh.positions.len(), 6 * 65 * 65);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.positions.len()));
        assert!(mesh
            .uvs
            .iter()
            .all(|uv| (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1])));
        // Deterministic: two generations are identical, so two machines
        // bake the same geometry.
        let again = bench_mesh(BENCH_SUBDIVISIONS);
        assert_eq!(mesh.positions, again.positions);
        assert_eq!(mesh.indices, again.indices);
        // A spot value pins the layout: +X face, first vertex.
        assert_eq!(mesh.positions[0], [1.0, -1.0, -1.0]);
    }

    #[test]
    fn options_are_validated_before_any_gpu_work() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse(&a(&[])).unwrap().0, 1024);
        let (size, rays, json) =
            parse(&a(&["--size", "512", "--rays", "8", "--json", "b.json"])).unwrap();
        assert_eq!((size, rays), (512, 8));
        assert_eq!(json, Some(PathBuf::from("b.json")));
        assert!(
            parse(&a(&["--size", "1000"])).is_err(),
            "not a power of two"
        );
        assert!(parse(&a(&["--size", "16384"])).is_err(), "over the cap");
        assert!(parse(&a(&["--rays", "0"])).is_err());
        assert!(parse(&a(&["--size"])).is_err(), "missing value");
        assert!(parse(&a(&["--fast"])).is_err(), "unknown option");
    }
}
