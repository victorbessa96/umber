//! The corpus pattern for importers (queue row 16): pinned, never-committed
//! oracle files + feature-gated tests that light up the moment the corpus
//! directory exists, and SKIP (with a loud note) when it doesn't.
//!
//! ## Layout (the absorbed photocraft pattern, adapted)
//!
//! ```text
//! ~/corpora/umber/spp/          — real .spp oracle files (NEVER committed)
//!     manifest.json             — [{ "file": "<name>.spp",
//!                                  "sha256": "<hex>",
//!                                  "notes": "<tree shape, HBO era, ...>" }]
//! ```
//!
//! `UMBER_SPP_CORPUS` overrides the default path (CI can mount a corpus).
//! A file whose sha256 doesn't match the manifest is a HARD failure — a
//! drifted oracle is worse than no oracle (the pinned-bytes doctrine).
//!
//! ## Why feature-gated, not env-gated
//!
//! The corpus contains real artwork files that must never enter the repo
//! or CI caches. The test compiles everywhere but runs only where the
//! directory exists; the manifest itself may be committed (it's just
//! hashes + notes) so the pin survives even though the bytes never do.

use std::path::PathBuf;

/// The corpus root: `$UMBER_SPP_CORPUS` or `~/corpora/umber/spp`.
pub fn corpus_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("UMBER_SPP_CORPUS") {
        return PathBuf::from(dir);
    }
    let mut dir = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    dir.push("corpora");
    dir.push("umber");
    dir.push("spp");
    dir
}

/// One manifest row.
#[derive(serde::Deserialize)]
pub struct CorpusEntry {
    pub file: String,
    pub sha256: String,
    #[serde(default)]
    pub notes: String,
}

/// The manifest, or `None` when the corpus directory doesn't exist.
pub fn manifest() -> Option<Vec<CorpusEntry>> {
    let dir = corpus_dir();
    let path = dir.join("manifest.json");
    let bytes = std::fs::read(path).ok()?;
    Some(serde_json::from_slice(&bytes).expect("manifest.json parses"))
}

/// Sha256 hex of `data` (lowercase) — the pin the manifest carries.
pub fn sha256_hex(data: &[u8]) -> String {
    // A tiny pure-Rust sha256 (no ring/digest dep): the FIPS 180-4 rounds.
    // Small enough to inline here; the corpus test is the only consumer.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vectors() {
        // The pin primitive must be correct before any oracle rides it:
        // the three standard vectors (FIPS 180-4 + the empty string).
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn corpus_runs_only_when_present() {
        // The corpus gate: absent directory = the test skips with a loud
        // note (never a failure — no samples exist yet, row 16 §8);
        // present directory = every manifest entry must exist, hash-match,
        // and open through the reader without a panic (the fuzz contract
        // at real-world bytes).
        let Some(entries) = manifest() else {
            eprintln!(
                "corpus: absent at {} — skip (drop real .spp files + manifest.json there to arm)",
                corpus_dir().display()
            );
            return;
        };
        assert!(
            !entries.is_empty(),
            "corpus present but manifest is empty — remove the dir or pin files"
        );
        let dir = corpus_dir();
        for entry in &entries {
            let path = dir.join(&entry.file);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|err| panic!("corpus file missing {}: {err}", entry.file));
            assert_eq!(
                sha256_hex(&bytes),
                entry.sha256,
                "corpus drift: {} no longer matches its manifest pin",
                entry.file
            );
            // Real Painter files hold their channels in texture-cache/
            // (research §7a): every pinned file must yield at least one
            // set with a baseColor, and every channel must be RGBA of
            // its declared size.
            let project = umber_spp::open(&path)
                .unwrap_or_else(|err| panic!("corpus file {} did not open: {err}", entry.file));
            assert!(
                !project.texture_sets.is_empty(),
                "{}: no texture sets imported",
                entry.file
            );
            let mut base_colours = 0;
            for set in &project.texture_sets {
                for ch in &set.channels {
                    assert_eq!(
                        ch.pixels.len(),
                        ch.width as usize * ch.height as usize * usize::from(ch.channels),
                        "{}: {} / {} has a short pixel buffer",
                        entry.file,
                        set.name,
                        ch.name
                    );
                    base_colours += usize::from(ch.name == "baseColor");
                    if ch.name == "height" {
                        // Signed height, ±1 → 0..255 around Umber's flat
                        // 128. The median depends on the sculpt (one corpus
                        // set sits at 185), so the check is on clipping:
                        // under 1% of texels may land at 0 or 255 (the
                        // corpus maximum is 0.02%).
                        let total = (ch.pixels.len() / 4).max(1);
                        let clipped = ch
                            .pixels
                            .chunks_exact(4)
                            .filter(|t| t[0] == 0 || t[0] == 255)
                            .count();
                        assert!(
                            clipped * 100 < total,
                            "{}: {} height clips {clipped} of {total} texels",
                            entry.file,
                            set.name
                        );
                    }
                }
            }
            assert!(base_colours > 0, "{}: no baseColor channel", entry.file);
            eprintln!(
                "corpus: {} -> {} set(s): {}",
                entry.file,
                project.texture_sets.len(),
                project
                    .texture_sets
                    .iter()
                    .map(|s| format!("{} ({} ch)", s.name, s.channels.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}
