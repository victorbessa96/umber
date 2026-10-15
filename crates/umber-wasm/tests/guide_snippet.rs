//! Guide snippet check (docs/plugins/authoring-guide.md §4): the load
//! path a plugin author tests with compiles and behaves as written.

#[test]
fn the_authoring_guide_load_snippet_works() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugin_vignette.wasm");
    let bytes = std::fs::read(path)?;
    let module = umber_wasm::PluginRuntime::new().load(&bytes)?; // contract + node-def check
    assert_eq!(module.def().name, "vignette");
    assert_eq!(module.def().n_params, 1);
    Ok(())
}
