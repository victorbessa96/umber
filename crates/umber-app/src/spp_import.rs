//! The .spp import app seam (binding ruling 1: import-only FOREVER;
//! `umber-spp` is the read-only reader crate — this module only
//! PRESENTS what it returns, never writes .spp anything).
//!
//! The UX mirrors the PSD section: drop-scan for `.spp` files, the
//! routed table, an Import button that parses and REPORTS. v1 presents
//! the import report (the reader's first-class artifact: every dataset
//! imported/skipped/not-imported with reasons, the HBO era lines, the
//! version stamp) and the parsed texture sets + channels as the import
//! preview. The pixels-to-paint-target landing is the same named
//! follow-up as the psd v2 (the layer/texture-set commit path both
//! share).

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
    /// The parsed project (the reader's full truth).
    pub project: SppProject,
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
/// dataset line).
pub fn show_section(ui: &mut egui::Ui, state: &mut SppImportState) {
    use crate::i18n::tr;
    ui.heading(tr("spp-import.title"));
    ui.label(tr("spp-import.hint"));
    ui.add_space(4.0);
    if state.pending.is_empty() && state.parsed.is_empty() {
        ui.label(tr("spp-import.no-files"));
        return;
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
}
