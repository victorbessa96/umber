//! `extract-spp <file.spp> <out-dir>`: write every channel the `.spp`
//! reader imports as `<set>_<channel>.png` (binding ruling 1: read-only;
//! nothing is ever written back to the `.spp`).
//!
//! Real Painter projects yield their flattened `texture-cache/` channels
//! (research doc §7a). The names follow the mesh-map import convention
//! (`TextureSetName_map`), so the folder drops straight into the app's
//! Assets ▸ mesh-map import, which routes every file to its set and
//! channel as a new layer.
//!
//! - The reader hands every channel over LINEAR (Umber's plane space).
//!   Colour channels (`baseColor`, `emissive`) are written with the sRGB
//!   curve (`Transfer::Srgb`), the convention every texture tool expects
//!   and the one Umber's own export uses. Data channels are written as
//!   stored.
//! - Names come from the reader: `baseColor`, `normal`, and `flag_<n>`
//!   for channels whose Painter name is not established.
//! - A summary line per file, then the reader's report lines (the
//!   per-dataset truth), go to stdout.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// `extract-spp <file.spp> <out-dir>`.
pub fn extract_spp_cmd(args: &[String]) -> Result<()> {
    let (Some(spp), Some(out)) = (args.first(), args.get(1)) else {
        anyhow::bail!("usage: umber-cli extract-spp <file.spp> <out-dir>");
    };
    let written = extract(Path::new(spp), Path::new(out))?;
    for path in &written {
        println!("wrote {}", path.display());
    }
    println!("{} channel(s) written", written.len());
    Ok(())
}

/// A name segment safe as a file-name part on every platform: the set and
/// channel names come from the file, so nothing path-like passes.
fn file_safe(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

/// Opens `spp`, writes each imported channel under `out`, returns the
/// written paths in report order.
pub fn extract(spp: &Path, out: &Path) -> Result<Vec<PathBuf>> {
    let project = umber_spp::open(spp).with_context(|| format!("reading {}", spp.display()))?;
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut written = Vec::new();
    for set in &project.texture_sets {
        for channel in &set.channels {
            if channel.channels != 4 {
                // H1 rasters (grey / 2–3 component) are not texture-cache
                // output; widen them so every PNG is RGBA.
                log::info!(
                    "{}: {}-component channel widened to RGBA",
                    channel.path,
                    channel.channels
                );
            }
            let rgba = to_rgba(channel);
            let transfer = transfer_for(&channel.name);
            let name = format!("{}_{}.png", file_safe(&set.name), file_safe(&channel.name));
            let path = out.join(name);
            umber_export::png::write_png(&path, channel.width, channel.height, &rgba, transfer)
                .with_context(|| format!("writing {}", path.display()))?;
            written.push(path);
        }
    }
    let v = project.version;
    println!(
        "{}: Painter {}.{}, {} texture set(s)",
        spp.display(),
        v.major,
        v.minor,
        project.texture_sets.len()
    );
    for note in &project.notes {
        println!("note: {note}");
    }
    for dataset in &project.datasets {
        println!("{}", dataset.summary());
    }
    Ok(written)
}

/// The PNG transfer for a channel: sRGB for colour, linear for data
/// (`umber_export::driver::output_transfer`'s rule).
pub(crate) fn transfer_for(channel: &str) -> umber_export::png::Transfer {
    if channel.eq_ignore_ascii_case("basecolor") || channel.eq_ignore_ascii_case("emissive") {
        umber_export::png::Transfer::Srgb
    } else {
        umber_export::png::Transfer::Linear
    }
}

/// Any reader channel as RGBA8 (the texture-cache path is RGBA already).
fn to_rgba(channel: &umber_spp::ChannelImage) -> Vec<u8> {
    let c = usize::from(channel.channels.max(1));
    if c == 4 {
        return channel.pixels.clone();
    }
    channel
        .pixels
        .chunks_exact(c)
        .flat_map(|t| match c {
            1 => [t[0], t[0], t[0], 255],
            2 => [t[0], t[0], t[0], t[1]],
            _ => [t[0], t[1], t[2], 255],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_the_file_cannot_escape_the_folder() {
        assert_eq!(
            file_safe("cache-1423614033083633785"),
            "cache-1423614033083633785"
        );
        assert_eq!(file_safe("../../etc"), "------etc");
        assert_eq!(file_safe("C:\\x"), "C--x");
        assert_eq!(file_safe(""), "unnamed");
        // The `_` separator the importer splits on never appears inside.
        assert_eq!(file_safe("flag_128"), "flag-128");
    }

    #[test]
    fn grey_and_two_component_channels_widen_to_rgba() {
        let mut ch = umber_spp::ChannelImage {
            width: 2,
            height: 1,
            channels: 1,
            pixels: vec![10, 20],
            ..Default::default()
        };
        assert_eq!(to_rgba(&ch), vec![10, 10, 10, 255, 20, 20, 20, 255]);
        ch.channels = 2;
        ch.pixels = vec![10, 99, 20, 0];
        assert_eq!(to_rgba(&ch), vec![10, 10, 10, 99, 20, 20, 20, 0]);
    }

    #[test]
    fn colour_channels_are_written_srgb_and_data_linear() {
        use umber_export::png::Transfer;
        assert_eq!(transfer_for("baseColor"), Transfer::Srgb);
        assert_eq!(transfer_for("emissive"), Transfer::Srgb);
        for data in ["roughness", "metallic", "height", "normal", "flag_8"] {
            assert_eq!(transfer_for(data), Transfer::Linear, "{data}");
        }
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let out = std::env::temp_dir().join(format!("umber-spp-x-{}", std::process::id()));
        assert!(extract(Path::new("/nonexistent/none.spp"), &out).is_err());
        let _ = std::fs::remove_dir_all(&out);
    }
}
