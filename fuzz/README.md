# umber cargo-fuzz wiring — queue row 386, wave-fuzz-corpus (b)

Bounded libFuzzer session over the importer seams, seeded from the row
156-A hostile corpus (`crates/umber-mesh/tests/corpus/`). The root
workspace excludes this directory (`exclude = ["examples", "fuzz"]` in
the root `Cargo.toml`), so `fuzz/` is its own cargo-fuzz workspace and
never builds under the root `cargo test --workspace`.

## Targets

- `mesh_load` — byte 0 selects a lane (the `load` dispatcher once per
  format extension, plus each direct pub importer entry, the same matrix
  as `crates/umber-mesh/tests/hostile_corpus.rs`); the rest of the input
  is written to a temp file with that lane's extension and fed to the
  entry. No `catch_unwind` in the target: a panic in an importer or a
  dependency is the finding and lands as a libFuzzer crash artifact.
- `spp_open` — the input is written to a temp `.spp` file and fed to
  `umber_spp::open` (the HDF5-subset reader seam).

## Seed corpus

`fuzz/corpus/mesh_load/` holds one seed per hostile-corpus file per
relevant lane: a one-byte lane selector prepended to the original bytes
(so the seed survives libFuzzer mutations of byte 0 as well). Regenerate
the source corpus with `python3 crates/umber-mesh/tests/corpus/gen.py`,
then re-derive the seeds by prepending the lane byte for the file's
format subdir (`fbx`→lanes 3+8, `gltf`→0/1+7, `usda`→4+9, `usdc`→5+10,
`usdz`→6+11; see `LANES` in `fuzz_targets/mesh_load.rs`).

`fuzz/corpus/spp_open/` holds the `.spp` fixtures from `crates/spp/tests/`.

## Running (minihost, nightly + cargo-fuzz installed)

```text
export PATH=$HOME/.cargo/bin:$PATH
flock -w 900 /tmp/umber-cargo.lock nice -n 19 \
  cargo +nightly fuzz build mesh_load spp_open
# run one target at a time; artifacts land in fuzz/artifacts/ (uncommitted)
cargo +nightly fuzz run mesh_load -- \
  -max_total_time=3600 -max_len=65536 -rss_limit_mb=3072 -malloc_limit_mb=3072 \
  -ignore_crashes=1 -ignore_ooms=1 -ignore_timeouts=1 \
  -artifact_prefix=fuzz/artifacts/ fuzz/corpus/mesh_load
```

Never `--features gpu`. Cargo runs one at a time under
`/tmp/umber-cargo.lock`. Crash artifacts stay on disk uncommitted; the
kill report (`docs/loop/kill-reports/2026-11-14-w5-fuzz-corpus-b.md`)
records each artifact path with its `cargo +nightly fuzz repro` command
and classification.
