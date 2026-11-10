//! Wave 3.1a: the `patt` section against a REAL Photoshop `.abr` file.
//!
//! Fixture (vendored from the ag-psd upstream corpus, MIT, see
//! `tests/fixtures/FIXTURES.md` "Row 282 additions"):
//! `abr_real_v6_2_full.abr` (upstream `simple/src.abr`, the whole 662-byte
//! file). Its `patt` section is present but EMPTY: `8BIMpatt` at offset 16
//! with a zero declared size, then `8BIMdesc` at offset 28. That is the pin:
//! a real file whose pattern section decodes to nothing must import exactly
//! as it did before wave 3.1a — no patterns, no skip, brush intact.
//!
//! The only real file with a NON-empty `patt` is upstream
//! `sample-and-pattern/src.abr`, whose ~1 MiB pattern was trimmed out of the
//! vendored copies (rows 271/282). Wave 3.1a ships no new binary fixture
//! (partner ruling Q3): the synthetic records in `src/abr_patt.rs` cover the
//! decoder, and an oracle comparison against the untrimmed file is a
//! dragon-side one-off outside the repo.

use umber_brush::abr::{read_abr, AbrToolClass};
use umber_brush::abr_import::import_abr;

fn fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("abr_real_v6_2_full.abr");
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture abr_real_v6_2_full.abr: {e}"))
}

#[test]
fn real_full_file_has_an_empty_pattern_section() {
    let file = read_abr(&fixture()).expect("the real v6.2 full file parses");
    assert!(
        file.patterns.is_empty(),
        "the vendored file's patt section is empty"
    );
    assert_eq!(file.skipped_patterns, 0, "an empty section is not a skip");

    assert_eq!(file.brushes.len(), 1);
    let brush = &file.brushes[0];
    assert_eq!(brush.name, "Soft Round");
    assert_eq!(brush.tool_class, AbrToolClass::Brush);
    assert!(
        brush.texture.is_none(),
        "Soft Round carries no useTexture key"
    );
}

#[test]
fn real_full_file_still_imports_one_untextured_preset() {
    // Byte-compat: the pattern reader must not disturb what this file
    // imported before wave 3.1a.
    let imported = import_abr(&fixture()).expect("the real file imports");
    assert_eq!(imported.entries.len(), 1);
    assert!(imported.patterns.is_empty());
    assert_eq!(imported.orphan_patterns, 0);
    assert_eq!(imported.textures_missing_pattern, 0);
    let entry = &imported.entries[0];
    assert_eq!(entry.preset.name, "Soft Round");
    assert!(entry.texture.is_none());
    assert!(entry.stamp.is_none(), "Soft Round is a computed brush");
    assert_eq!(imported.brushes_without_sample, 1);
}
