//! Row 282/285: the `desc` section against REAL Photoshop `.abr` files.
//!
//! Fixtures (vendored from the ag-psd upstream corpus, MIT, see
//! `tests/fixtures/FIXTURES.md` "Row 282 additions"):
//! - `abr_real_v6_2_full.abr` (upstream `simple/src.abr`, whole 662-byte
//!   file with a real `desc`): upstream `data.json` says 0 samples and one
//!   computed brush "Soft Round" (size 30, spacing 0.25, hardness 0).
//! - `abr_real_v6_2_samp_desc.abr` (upstream `sample-and-pattern/src.abr`
//!   trimmed to `samp`+`desc`): upstream `data.json` says 1 sample
//!   `a8f34db0-...` (112x105 at (2,1)) and 1 sampled brush "Bobbys Brush 743"
//!   (size 112, spacing 0.02) whose `sampledData` is that id.

use umber_brush::abr::{match_sample, read_abr, AbrToolClass};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

use std::path::Path;

#[test]
fn real_full_file_parses_one_computed_brush_matching_upstream() {
    let file = read_abr(&fixture("abr_real_v6_2_full.abr")).expect("real v6.2 file parses");
    assert_eq!(file.samples.len(), 0, "upstream data.json: no samples");
    assert_eq!(file.brushes.len(), 1, "upstream data.json: one brush");
    let b = &file.brushes[0];
    assert_eq!(b.name, "Soft Round");
    assert_eq!(b.tool_class, AbrToolClass::Brush);
    assert_eq!(b.diameter_px, 30.0);
    assert_eq!(b.angle_deg, 0.0);
    assert_eq!(b.roundness, 1.0);
    assert!(b.spacing_on);
    assert_eq!(b.spacing, 0.25);
    assert_eq!(b.hardness, 0.0);
    assert!(
        b.sampled_data_id.is_none(),
        "computed brush has no stamp link"
    );
    assert!(match_sample(b, &file).is_none());
}

#[test]
fn real_samp_desc_file_links_the_sampled_brush_to_its_stamp() {
    let file =
        read_abr(&fixture("abr_real_v6_2_samp_desc.abr")).expect("real samp+desc file parses");
    assert_eq!(file.samples.len(), 1, "upstream data.json: one sample");
    let s = &file.samples[0];
    assert_eq!(s.id, "a8f34db0-4814-634e-807c-c61aecde18fd");
    assert_eq!((s.width, s.height), (112, 105));
    assert_eq!(s.alpha.len(), 112 * 105);

    assert_eq!(file.brushes.len(), 1, "upstream data.json: one brush");
    let b = &file.brushes[0];
    assert_eq!(b.name, "Bobbys Brush 743");
    assert_eq!(b.diameter_px, 112.0);
    assert_eq!(b.spacing, 0.02);
    // upstream data.json omits hardness for this brush -> parse default 1.0
    assert_eq!(b.hardness, 1.0);
    assert_eq!(
        b.sampled_data_id.as_deref(),
        Some("a8f34db0-4814-634e-807c-c61aecde18fd")
    );
    let matched = match_sample(b, &file).expect("sampled brush matches its stamp");
    assert_eq!(matched.id, s.id);
    assert_eq!(
        matched.alpha, s.alpha,
        "the matched stamp plane is the sample"
    );
}

#[test]
fn real_samp_desc_sample_plane_is_identical_to_the_samp_only_trim() {
    // The row-271 fixture `abr_real_v6_2.abr` is the same upstream file trimmed
    // to `samp` only; the samp payload bytes are untouched by that trim, so
    // both fixtures must yield the identical alpha plane.
    let full = read_abr(&fixture("abr_real_v6_2_samp_desc.abr")).unwrap();
    let samp_only = read_abr(&fixture("abr_real_v6_2.abr")).unwrap();
    assert_eq!(full.samples.len(), samp_only.samples.len());
    for (a, b) in full.samples.iter().zip(samp_only.samples.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.alpha, b.alpha, "samp payload identical across trims");
    }
}
