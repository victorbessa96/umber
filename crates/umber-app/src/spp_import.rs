//! The .spp import app seam (binding ruling 1: import-only FOREVER;
//! `umber-spp` is the read-only reader crate — this module only
//! PRESENTS what it returns, never writes .spp anything).
//!
//! The UX mirrors the PSD section: drop-scan for `.spp` files, the
//! routed table, an Import button that parses and REPORTS. v1 presents
//! the import report (the reader's first-class artifact: every dataset
//! imported/skipped/not-imported with reasons, the HBO era lines, the
//! version stamp) and the parsed texture sets + channels as the import
//! preview.
//!
//! Real Painter files fill the sets from `texture-cache/` (the flattened
//! channels, research §7a), named by their Painter material. "Extract
//! channels" writes each as `<set>_<channel>.png` into a folder next to
//! the `.spp` and feeds that folder to the mesh-map import, which routes
//! every file to its set and channel as a NEW layer: the same landing
//! path every imported map takes, so nothing is overwritten. The CLI's
//! `extract-spp` writes the same files.

use std::path::{Path, PathBuf};

use umber_spp::{open, SppError, SppProject};

/// The extensions this seam routes (the reader validates the HDF5
/// signature itself — the extension is routing only).
pub const SPP_EXTENSIONS: [&str; 1] = ["spp"];

/// Routing predicate (the shell's drop router + the scan).
pub fn is_spp_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            SPP_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// One parsed .spp waiting to land (the import preview + its report).
#[derive(Debug, Clone)]
pub struct ParsedSpp {
    /// The file as presented.
    pub file: String,
    /// Where it was read from (the extract folder goes beside it).
    pub path: Option<PathBuf>,
    /// The parsed project (the reader's full truth).
    pub project: SppProject,
}

/// A name segment safe as a file-name part: set and channel names come
/// from the file, so nothing path-like passes, and the `_` the mesh-map
/// router splits on never appears inside a segment.
pub fn file_safe(name: &str) -> String {
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

/// Writes every RGBA channel of `project` as `<set>_<channel>.png` under
/// `dir`; returns the written paths. Channels that are not 4-component
/// (H1 rasters from non-Painter files) are skipped and reported by the
/// caller.
pub fn write_channels(project: &SppProject, dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = Vec::new();
    for set in &project.texture_sets {
        for channel in &set.channels {
            if channel.channels != 4 {
                continue;
            }
            let path = dir.join(format!(
                "{}_{}.png",
                file_safe(&set.name),
                file_safe(&channel.name)
            ));
            umber_export::png::write_png(
                &path,
                channel.width,
                channel.height,
                &channel.pixels,
                // Linear, ALL channels, colour included: these PNGs are an
                // internal hand-off to the mesh-map import, which loads
                // bytes into the linear plane as stored. Writing colour
                // sRGB here would land it in the plane sRGB-encoded (the
                // double curve the reader's linearize exists to prevent).
                // The CLI's `extract-spp`, whose files leave Umber, writes
                // colour sRGB instead.
                umber_export::png::Transfer::Linear,
            )
            .map_err(|e| format!("{}: {e}", path.display()))?;
            written.push(path);
        }
    }
    Ok(written)
}

/// The extract folder for `spp`: `<stem>-channels/` beside it.
pub fn extract_dir_for(spp: &Path) -> PathBuf {
    let stem = spp
        .file_stem()
        .and_then(|s| s.to_str())
        .map(file_safe)
        .unwrap_or_else(|| "spp".to_string());
    spp.with_file_name(format!("{stem}-channels"))
}

/// The import state: drops route here, Import parses + reports.
#[derive(Debug, Default)]
pub struct SppImportState {
    /// Dropped .spp files this session, in drop order (name + path).
    pub pending: Vec<(String, Option<PathBuf>)>,
    /// Parsed projects (populated by Import), in parse order.
    pub parsed: Vec<ParsedSpp>,
    /// The status line (the last scan/import outcome).
    pub status: String,
}

impl SppImportState {
    /// egui file drops this frame: `.spp` files route here (everything
    /// else is left for the other importers' scans).
    pub fn poll_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| is_spp_file(p))
                .collect()
        });
        for path in dropped {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            self.pending.push((name, Some(path)));
        }
        if !self.pending.is_empty() {
            self.status = format!("{} .spp file(s) waiting", self.pending.len());
        }
    }

    /// Parses every pending file (the reader's `open`), populating
    /// `parsed`. Failures land in `status` per file and the file stays
    /// pending for a retry; successes clear their slot.
    pub fn import_all(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for (name, path) in pending {
            let Some(path) = path else {
                self.status = format!("{name}: no path to read");
                continue;
            };
            match open(&path) {
                Ok(project) => {
                    self.status = format!(
                        "{}: {} texture set(s), {} datasets reported",
                        name,
                        project.texture_sets.len(),
                        project.datasets.len()
                    );
                    self.parsed.push(ParsedSpp {
                        file: name,
                        path: Some(path),
                        project,
                    });
                }
                Err(SppError::Io(detail)) => {
                    self.status = format!("{name}: read failed ({detail})");
                    self.pending.push((name, Some(path)));
                }
                Err(err) => {
                    // A typed reader error is PRESENTED, not retried —
                    // the file is what it is (the never-crash doctrine:
                    // NotHdf5/Unsupported/Corrupt are report lines).
                    self.status = format!("{name}: {err}");
                }
            }
        }
    }
}

/// The Assets panel's .spp section: the drop hint, the pending table,
/// the Import button, and every parsed project's report (the reader's
/// first-class artifact — version stamp, texture sets + channels, every
/// dataset line). Returns the folder an "Extract channels" click wrote,
/// for the caller to hand to the mesh-map import.
pub fn show_section(ui: &mut egui::Ui, state: &mut SppImportState) -> Option<PathBuf> {
    let mut extracted = None;
    // Set inside the report loop, which borrows `state.parsed`; assigned
    // to `state.status` after it.
    let mut status: Option<String> = None;
    use crate::i18n::tr;
    ui.heading(tr("spp-import.title"));
    ui.label(tr("spp-import.hint"));
    ui.add_space(4.0);
    if state.pending.is_empty() && state.parsed.is_empty() {
        ui.label(tr("spp-import.no-files"));
        return None;
    }
    if !state.pending.is_empty() {
        for (name, _) in &state.pending {
            ui.label(tr("spp-import.row-pending").replace("{file}", name));
        }
        if ui.button(tr("spp-import.apply")).clicked() {
            state.import_all();
        }
    }
    if !state.status.is_empty() {
        ui.label(&state.status);
    }
    for parsed in &state.parsed {
        ui.add_space(4.0);
        ui.label(
            tr("spp-import.report-title")
                .replace("{file}", &parsed.file)
                .replace(
                    "{version}",
                    &format!(
                        "{}.{}",
                        parsed.project.version.major, parsed.project.version.minor
                    ),
                ),
        );
        let channel_count: usize = parsed
            .project
            .texture_sets
            .iter()
            .map(|s| s.channels.len())
            .sum();
        if channel_count > 0 {
            if let Some(path) = parsed.path.as_deref() {
                // Labels are English until the catalogs' lock frees
                // (`spp-hint-honest` in the queue adds the keys).
                if ui
                    .button(format!("Extract {channel_count} channel(s) as layers"))
                    .clicked()
                {
                    let dir = extract_dir_for(path);
                    status = Some(match write_channels(&parsed.project, &dir) {
                        Ok(written) => {
                            let line = format!(
                                "{}: {} PNG(s) in {}",
                                parsed.file,
                                written.len(),
                                dir.display()
                            );
                            extracted = Some(dir);
                            line
                        }
                        Err(err) => format!("{}: extract failed ({err})", parsed.file),
                    });
                }
            }
        }
        for set in &parsed.project.texture_sets {
            ui.label(
                tr("spp-import.set-row")
                    .replace("{set}", &set.name)
                    .replace("{count}", &set.channels.len().to_string()),
            );
            for channel in &set.channels {
                ui.label(
                    tr("spp-import.channel-row")
                        .replace("{channel}", &channel.name)
                        .replace("{w}", &channel.width.to_string())
                        .replace("{h}", &channel.height.to_string())
                        .replace("{c}", &channel.channels.to_string()),
                );
            }
        }
        for note in &parsed.project.notes {
            ui.label(tr("spp-import.note-row").replace("{note}", note));
        }
        for dataset in &parsed.project.datasets {
            ui.label(dataset.summary());
        }
    }
    if let Some(line) = status {
        state.status = line;
    }
    extracted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracted_names_route_through_the_mesh_map_import() {
        let mut project = SppProject::default();
        project.texture_sets.push(umber_spp::TextureSet {
            name: "Eye Clear Sphere".into(),
            channels: vec![
                umber_spp::ChannelImage {
                    name: "baseColor".into(),
                    width: 1,
                    height: 1,
                    channels: 4,
                    pixels: vec![1, 2, 3, 4],
                    ..Default::default()
                },
                umber_spp::ChannelImage {
                    name: "flag_128".into(),
                    width: 1,
                    height: 1,
                    channels: 4,
                    pixels: vec![9, 9, 9, 255],
                    ..Default::default()
                },
                // Not RGBA: skipped, never a short PNG.
                umber_spp::ChannelImage {
                    name: "grey".into(),
                    width: 1,
                    height: 1,
                    channels: 1,
                    pixels: vec![5],
                    ..Default::default()
                },
            ],
        });
        let dir = std::env::temp_dir().join(format!("umber-spp-extract-{}", std::process::id()));
        let written = write_channels(&project, &dir).unwrap();
        let names: Vec<String> = written
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "Eye-Clear-Sphere_baseColor.png",
                "Eye-Clear-Sphere_flag-128.png"
            ]
        );
        // The mesh-map router reads them as <set>_<channel> user channels.
        for name in &names {
            match crate::mesh_map_import::classify_file_name(name) {
                crate::mesh_map_import::FileRoute::UserChannel { set, .. } => {
                    assert_eq!(set, "Eye-Clear-Sphere");
                }
                other => panic!("{name} routed as {other:?}"),
            }
        }
        // Every name the reader emits lands in the channel of that name.
        // `height` matches a bake suffix first (stage 1), and the bake
        // route's channel is `height` too.
        use crate::mesh_map_import::{classify_file_name, FileRoute};
        for channel in [
            "baseColor",
            "height",
            "roughness",
            "metallic",
            "normal",
            "emissive",
            "opacity",
        ] {
            let route = classify_file_name(&format!("Body_{channel}.png"));
            let (set, routed) = match route {
                FileRoute::UserChannel { set, channel } => (set, channel),
                FileRoute::MeshMap { set, channel, .. } => (set, channel.to_string()),
                other => panic!("{channel} routed as {other:?}"),
            };
            assert_eq!((set.as_str(), routed.as_str()), ("Body", channel));
        }
        let (w, h, rgba) = umber_export::png::read_png_rgba8(&written[0]).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgba, vec![1, 2, 3, 4]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_extract_folder_sits_beside_the_spp() {
        let dir = extract_dir_for(Path::new("/proj/Bessa Synth.spp"));
        assert_eq!(dir, Path::new("/proj/Bessa-Synth-channels"));
        assert_eq!(file_safe("../x"), "---x");
    }
}
