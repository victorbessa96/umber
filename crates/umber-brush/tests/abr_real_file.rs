//! The clean-room `.abr` reader against REAL Photoshop brush files.
//!
//! The fixtures under `tests/fixtures/` are trimmed copies of the
//! `ag-psd` upstream test corpus (MIT, Agamnentzar/ag-psd), each keeping
//! only the `8BIM` sections the reader interprets (the `samp` section)
//! so the repo carries bytes, not a megabyte of patterns. Provenance and
//! licensing: `tests/fixtures/FIXTURES.md`.
//!
//! Why this file exists: the synthetic fixtures pin the reader against
//! the vendored format authority, but nothing pinned it against a file
//! Photoshop actually wrote. These pins do, and the alpha plane is
//! checked against the upstream ORACLE PNG (ag-psd's own expected
//! output for the same file) — byte for byte.
//!
//! The real files also corrected two of the reader's assumptions, both
//! fixed before these pins landed:
//!
//! * a `samp` record is a **sample** (a tip bitmap with an id and a
//!   position), not a brush — the brushes live in the `desc` section;
//! * real files use **v6, v7, v9 and v10** — a v6/v7-only gate refused
//!   a real v10 file.

use std::path::{Path, PathBuf};

use umber_brush::abr::{read_samples, AbrError};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// The real v6.2 file with a sampled tip: the reader extracts exactly
/// the upstream-expected sample (id, position, size), and the alpha
/// plane is byte-identical to the upstream oracle PNG's alpha channel.
#[test]
fn real_v6_2_sample_matches_the_upstream_oracle() {
    let bytes = std::fs::read(fixture("abr_real_v6_2.abr")).expect("fixture readable");
    let samples = read_samples(&bytes).expect("the real v6.2 file parses");

    assert_eq!(samples.len(), 1, "one sampled tip in the file");
    let sample = &samples[0];
    // The upstream corpus's expected value for this file
    // (`test/abr-read/sample-and-pattern/data.json`).
    assert_eq!(sample.id, "a8f34db0-4814-634e-807c-c61aecde18fd");
    assert_eq!((sample.x, sample.y), (2, 1));
    assert_eq!((sample.width, sample.height), (112, 105));
    assert_eq!(sample.alpha.len(), 112 * 105);

    // The oracle: ag-psd's own expected PNG for this sample
    // (`sample-a8f34db0-4814-634e-807c-c61aecde18fd.png`), whose ALPHA
    // channel is the 8-bit plane. The PNG's RGB is white and its alpha
    // is the sample, so the channel is the plane byte for byte. The PNG
    // is not vendored (it is an image, not a fixture); instead the
    // plane's checksum is pinned here — computed from the upstream PNG's
    // alpha channel on 2026-11-07 and checked against it then.
    //
    // The checksum is FNV-1a 64 over the alpha bytes (a plain, stable
    // byte-hash: no new dependency, and the repo's own `blake3` is not
    // a umber-brush dependency).
    assert_eq!(
        fnv1a64(&sample.alpha),
        0x4be3_afe6_8ff7_4ceb,
        "the alpha plane matches the upstream oracle PNG"
    );
    // A second, independent fingerprint: the plane's byte sum and its
    // non-zero count, both taken from the oracle.
    assert_eq!(
        sample.alpha.iter().map(|&v| u64::from(v)).sum::<u64>(),
        1_128_966
    );
    assert_eq!(sample.alpha.iter().filter(|&&v| v > 0).count(), 6_570);
    // The tip's corner texels are empty (the brush tip fades out at the
    // edges), while the first row carries the very top of the dab's soft
    // edge — the exact values the oracle PNG has.
    assert_eq!(sample.alpha[0], 0);
    assert_eq!(sample.alpha[111], 0);
    assert_eq!(sample.alpha[104 * 112], 0);
    assert_eq!(sample.alpha[65], 5);
    assert_eq!(sample.alpha[66], 10);
    assert_eq!(sample.alpha[67], 13);
}

/// A real v10.2 file (the `special` corpus file) parses with the same
/// `samp` layout: its sample id, position and size match the upstream
/// expected values.
#[test]
fn real_v10_2_sample_parses_with_the_same_layout() {
    let bytes = std::fs::read(fixture("abr_real_v10_2.abr")).expect("fixture readable");
    let samples = read_samples(&bytes).expect("the real v10.2 file parses");

    assert_eq!(samples.len(), 1);
    let sample = &samples[0];
    assert_eq!(sample.id, "80b60ae0-c4b0-11d5-869d-ee7a7e70f1aa");
    assert_eq!((sample.x, sample.y), (295, 195));
    assert_eq!((sample.width, sample.height), (23, 20));
    assert_eq!(sample.alpha.len(), 23 * 20);
}

/// A real v6.2 file whose `samp` section is EMPTY (its brush is a
/// computed one, described entirely in the descriptor section) parses to
/// zero samples — no error, no phantom sample.
#[test]
fn real_file_without_samples_parses_to_zero_samples() {
    let bytes = std::fs::read(fixture("abr_real_v6_2_no_samples.abr")).expect("fixture readable");
    let samples = read_samples(&bytes).expect("the real file parses");
    assert!(samples.is_empty(), "a computed-only file has no samples");
}

/// The v1/v2 boundary: a file claiming v1 is refused with the named
/// version error (the reference refuses v1/v2 too).
#[test]
fn v1_is_refused_by_version() {
    let mut bytes = vec![0u8, 1];
    bytes.extend_from_slice(&[0u8, 1]);
    bytes.extend_from_slice(b"8BIMSAMP");
    bytes.extend_from_slice(&0u32.to_be_bytes());
    match read_samples(&bytes) {
        Err(AbrError::UnsupportedVersion { version }) => assert_eq!(version, 1),
        other => panic!("expected UnsupportedVersion(1), got {other:?}"),
    }
}

/// FNV-1a 64 (the fixture checksum helper; stable, no dependency).
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
