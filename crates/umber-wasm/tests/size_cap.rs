//! The module-size cap (C2-21): an oversized buffer is refused before
//! wasmtime compiles a byte, with a reason naming the cap.

use umber_wasm::{PluginError, PluginRuntime, MODULE_SIZE_CAP};

#[test]
fn a_module_over_the_cap_is_refused_before_compiling() {
    // Valid wasm magic + version, then padding: the cap must fire on the
    // length alone (a compile attempt would report a different error).
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    bytes.resize(MODULE_SIZE_CAP + 1, 0);
    match PluginRuntime::new().load(&bytes) {
        Err(PluginError::BadModule(reason)) => {
            assert!(reason.contains("MiB cap"), "{reason}");
        }
        other => panic!("expected the size-cap refusal, got {:?}", other.err()),
    }
}

#[test]
fn a_module_at_the_cap_reaches_the_compiler() {
    // Exactly at the cap: not refused for size, so wasmtime's own
    // validation answers (this padding is not a valid module).
    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    bytes.resize(MODULE_SIZE_CAP, 0);
    match PluginRuntime::new().load(&bytes) {
        Err(PluginError::BadModule(reason)) => {
            assert!(
                !reason.contains("MiB cap"),
                "the cap is inclusive: {reason}"
            );
        }
        other => panic!("expected a validation error, got {:?}", other.err()),
    }
}
