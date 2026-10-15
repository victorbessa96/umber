//! Crash-safe GPU startup (ROADMAP wave 8b, `wave-gpu-safe-start`).
//!
//! A graphics driver that segfaults inside wgpu device creation (or the
//! first presents) takes the process down before any Rust error path can
//! run, and it would do so again on every launch. So the startup is
//! bracketed by a marker file:
//!
//! 1. Before eframe creates the wgpu instance/device, [`prepare`] writes
//!    `gpu-starting.marker` (the planned backend, pid, timestamp) into the
//!    app's state dir and holds an exclusive OS lock on it.
//! 2. eframe's device-descriptor closure calls [`note_adapter`] right
//!    before `request_device`, so the marker also names the adapter (name,
//!    backend, driver) the driver is about to be asked for a device on.
//! 3. The shell calls [`note_frame`] from `App::ui`; on the
//!    [`FRAMES_TO_CONFIRM`]-th call the start is confirmed: the marker is
//!    cleared and, when this launch fell back, the backend that worked is
//!    persisted to `gpu-backend`.
//!
//! The marker also records the STAGE the start reached ([`Stage`]):
//! `planned` (written before the driver is touched), `adapter` (an adapter
//! was chosen; `request_device` is next), `device` (the device exists and
//! the app is constructing). Only a start that died at `planned` or
//! `adapter` died inside the driver. A marker left at `device` means the
//! process ended after the driver survived device creation — killed
//! (SIGTERM, a closed terminal, a headless launch whose window never
//! mapped so frames never came) or an app error — and stepping the
//! backend down for that would punish a working driver. That exact false
//! positive bricked a Wayland desktop on 2026-10-15: a `timeout`-killed
//! headless launch forced GL, which cannot present to a Wayland surface,
//! and the GL failure then kept the marker so every launch retried GL.
//!
//! A launch that finds a marker nobody holds the lock on, at a driver
//! stage, knows the previous start died inside the driver and steps the
//! backend down one rung ([`decide_backend`], the pure, table-tested
//! core):
//!
//! - Linux / other Unix: auto (wgpu's pick, normally Vulkan) / Vulkan -> GL.
//! - Windows: auto / Vulkan -> DX12 -> GL.
//! - macOS: Metal only (wgpu 30 has no other native backend there), so a
//!   crash there exhausts the chain at once.
//!
//! Overrides: a non-empty `WGPU_BACKEND` wins over everything (egui-wgpu's
//! default `instance_descriptor.backends` already reads it via
//! `wgpu::Backends::from_env`, so this module leaves the backends alone);
//! `--safe-gpu` forces the bottom rung for one launch without persisting it.
//!
//! Judgment calls (v1 boundaries):
//!
//! * Marker format is versioned `key=value` lines, not JSON: umber-app has
//!   no non-dev serde dependency and the brief forbids adding one. Hence the
//!   `.marker` extension (a file named `.json` holding non-JSON would lie).
//! * A corrupt/unparseable marker is a stale marker of an unknown rung
//!   ([`Planned::Unknown`]): the launch steps down from the rung it would
//!   otherwise use. Logged, never a panic.
//! * A marker written under `WGPU_BACKEND` ([`Planned::Env`]) never steps
//!   the chain: the user chose those backends, not this module.
//! * Live vs stale: the lock (`File::try_lock`) is held from [`prepare`] to
//!   confirmation, so a second concurrent Umber launch sees `Live` and
//!   skips marker handling instead of treating a healthy sibling as a crash.
//! * Clean exit before confirmation (`run_native` returned Ok) clears the
//!   marker without persisting. An `Err` return ([`finish`],
//!   [`exit_action`]) keeps it only when the error says the driver failed
//!   this backend (a failed device request, stage `adapter`). An `Err` at
//!   stage `device` is the app's own error and clears it. An `Err` at stage
//!   `planned` on a FALLBACK rung means that rung found no adapter able to
//!   present on this display (GL on Wayland): the fallback is undone
//!   (marker cleared, persisted fallback removed) so the next launch
//!   returns to auto instead of retrying a backend that cannot work.
//! * A persisted fallback is never upgraded automatically; deleting
//!   `gpu-backend` in the state dir resets it. No UI for that in v1.
//! * State dir: the parent of the brushes dir (`$XDG_DATA_HOME/umber`,
//!   `~/.local/share/umber`, `%APPDATA%\umber`), the same root the plugins
//!   dir uses; `temp_dir()/umber` when no home is discoverable.
//! * Clean-room reimplementation of the idea in photocraft's
//!   `gpu_startup.rs` (MIT/Apache); no code copied. No CPU/software-raster
//!   rung: Umber's paint pipeline needs a real wgpu device.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use eframe::egui_wgpu::wgpu;

/// The startup marker's file name in the state dir.
pub const MARKER_FILE: &str = "gpu-starting.marker";
/// The persisted backend's file name in the state dir.
pub const PERSISTED_FILE: &str = "gpu-backend";
/// `App::ui` calls that confirm a start. The 3rd `ui` call runs after the
/// 1st and 2nd frames were painted and presented.
pub const FRAMES_TO_CONFIRM: u32 = 3;
/// First line of every marker (format version).
const MARKER_HEADER: &str = "umber-gpu-marker v1";
/// Largest marker read; a real one is a few hundred bytes.
const MARKER_MAX: u64 = 16 << 10;
/// Longest adapter/driver string stored.
const FIELD_MAX: usize = 256;

/// Platform families with distinct backend chains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Mac,
    /// Linux, the BSDs and other Unix.
    Unix,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Unix
        }
    }

    /// The bottom rung of this platform's chain (`--safe-gpu`, exhaustion).
    pub fn safest(self) -> Backend {
        match self {
            Platform::Windows | Platform::Unix => Backend::Gl,
            Platform::Mac => Backend::Metal,
        }
    }
}

/// A backend rung. `Auto` = egui-wgpu's default backend set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Auto,
    Vulkan,
    Dx12,
    Metal,
    Gl,
}

impl Backend {
    pub const ALL: [Backend; 5] = [
        Backend::Auto,
        Backend::Vulkan,
        Backend::Dx12,
        Backend::Metal,
        Backend::Gl,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Vulkan => "vulkan",
            Backend::Dx12 => "dx12",
            Backend::Metal => "metal",
            Backend::Gl => "gl",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Backend::ALL.into_iter().find(|b| b.name() == s.trim())
    }

    /// The rung an adapter's wgpu backend corresponds to (`None` for the
    /// noop/browser backends, which never back a native window).
    pub fn from_wgpu(b: wgpu::Backend) -> Option<Self> {
        match b {
            wgpu::Backend::Vulkan => Some(Backend::Vulkan),
            wgpu::Backend::Dx12 => Some(Backend::Dx12),
            wgpu::Backend::Metal => Some(Backend::Metal),
            wgpu::Backend::Gl => Some(Backend::Gl),
            wgpu::Backend::Noop | wgpu::Backend::BrowserWebGpu => None,
        }
    }

    /// Whether wgpu 30 offers this rung natively on `platform`.
    pub fn available_on(self, platform: Platform) -> bool {
        match (platform, self) {
            (_, Backend::Auto) => true,
            (Platform::Windows, b) => matches!(b, Backend::Vulkan | Backend::Dx12 | Backend::Gl),
            (Platform::Unix, b) => matches!(b, Backend::Vulkan | Backend::Gl),
            (Platform::Mac, b) => b == Backend::Metal,
        }
    }

    /// The instance backend set for this rung (`None` = leave egui-wgpu's
    /// default, which honours `WGPU_BACKEND`).
    pub fn wgpu_backends(self) -> Option<wgpu::Backends> {
        match self {
            Backend::Auto => None,
            Backend::Vulkan => Some(wgpu::Backends::VULKAN),
            Backend::Dx12 => Some(wgpu::Backends::DX12),
            Backend::Metal => Some(wgpu::Backends::METAL),
            Backend::Gl => Some(wgpu::Backends::GL),
        }
    }
}

/// The next safer rung after a start on `tried` crashed; `None` when
/// `tried` is already the bottom of `platform`'s chain.
pub fn next_safer(tried: Backend, platform: Platform) -> Option<Backend> {
    match (platform, tried) {
        (Platform::Unix, Backend::Gl) | (Platform::Windows, Backend::Gl) => None,
        (Platform::Unix, _) => Some(Backend::Gl),
        (Platform::Windows, Backend::Dx12) => Some(Backend::Gl),
        (Platform::Windows, _) => Some(Backend::Dx12),
        (Platform::Mac, _) => None,
    }
}

/// What the crashed start had planned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Planned {
    Backend(Backend),
    /// Started under `WGPU_BACKEND` (the user's override, never stepped).
    Env,
    /// Unreadable marker: the rung is unknown.
    Unknown,
}

/// How far a start got (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// The marker is written; the driver has not been touched.
    Planned,
    /// An adapter was chosen; `request_device` is next.
    Adapter,
    /// The device exists; the driver survived device creation.
    Device,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Planned => "planned",
            Stage::Adapter => "adapter",
            Stage::Device => "device",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Stage::Planned, Stage::Adapter, Stage::Device]
            .into_iter()
            .find(|st| st.name() == s.trim())
    }

    /// Whether a start that ended at this stage can have died inside the
    /// driver (the only case that steps the backend down).
    pub fn is_driver_stage(self) -> bool {
        self < Stage::Device
    }
}

/// What a start records before touching the driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub planned: Planned,
    /// How far the start got.
    pub stage: Stage,
    /// The adapter about to be asked for a device (once known).
    pub adapter: Option<String>,
    pub adapter_backend: Option<Backend>,
    pub driver: Option<String>,
    /// Seconds since the Unix epoch.
    pub timestamp: u64,
    pub pid: u32,
}

impl Marker {
    pub fn new(planned: Planned) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Marker {
            planned,
            stage: Stage::Planned,
            adapter: None,
            adapter_backend: None,
            driver: None,
            timestamp,
            pid: std::process::id(),
        }
    }

    /// The stand-in for a marker whose content could not be parsed.
    pub fn unknown() -> Self {
        Marker {
            planned: Planned::Unknown,
            stage: Stage::Planned,
            adapter: None,
            adapter_backend: None,
            driver: None,
            timestamp: 0,
            pid: 0,
        }
    }

    pub fn serialize(&self) -> String {
        let planned = match &self.planned {
            Planned::Backend(b) => b.name(),
            Planned::Env => "env",
            Planned::Unknown => "unknown",
        };
        let mut out = format!(
            "{MARKER_HEADER}\nplanned={planned}\nstage={}\n",
            self.stage.name()
        );
        if let Some(a) = &self.adapter {
            out.push_str(&format!("adapter={}\n", clean_field(a)));
        }
        if let Some(b) = self.adapter_backend {
            out.push_str(&format!("adapter_backend={}\n", b.name()));
        }
        if let Some(d) = &self.driver {
            out.push_str(&format!("driver={}\n", clean_field(d)));
        }
        let (timestamp, pid) = (self.timestamp, self.pid);
        out.push_str(&format!("timestamp={timestamp}\npid={pid}\n"));
        out
    }

    /// Strict parse: a wrong header or a missing/unknown `planned` is
    /// [`MarkerError::Corrupt`]; unknown keys are ignored (forward compat).
    pub fn parse(text: &str) -> Result<Self, MarkerError> {
        let corrupt = |detail: &str| MarkerError::Corrupt {
            detail: detail.to_string(),
        };
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(MARKER_HEADER) {
            return Err(corrupt("missing or wrong header line"));
        }
        let mut marker = Marker::unknown();
        let mut planned = None;
        let mut stage = None;
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "planned" => {
                    planned = Some(match value {
                        "env" => Planned::Env,
                        "unknown" => Planned::Unknown,
                        v => Planned::Backend(
                            Backend::parse(v).ok_or_else(|| corrupt("unknown planned backend"))?,
                        ),
                    })
                }
                "stage" => stage = Stage::parse(value),
                "adapter" => marker.adapter = Some(clean_field(value)),
                "adapter_backend" => marker.adapter_backend = Backend::parse(value),
                "driver" => marker.driver = Some(clean_field(value)),
                "timestamp" => marker.timestamp = value.parse().unwrap_or(0),
                "pid" => marker.pid = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        marker.planned = planned.ok_or_else(|| corrupt("no planned= line"))?;
        // Pre-stage markers (no `stage=` line): infer it. A recorded
        // adapter means the start reached `request_device`.
        marker.stage = stage.unwrap_or(if marker.adapter.is_some() {
            Stage::Adapter
        } else {
            Stage::Planned
        });
        Ok(marker)
    }

    /// The rung this marker planned, if it named one.
    pub fn planned_backend(&self) -> Option<Backend> {
        match self.planned {
            Planned::Backend(b) => Some(b),
            _ => None,
        }
    }

    /// The rung the crashed start was on: the adapter's when an `auto`
    /// start got as far as choosing one, else the plan's. `base` stands in
    /// for an unknown plan. `None` for an env-override start.
    fn tried(&self, base: Backend) -> Option<Backend> {
        // Past device creation the driver survived: not a driver crash.
        if !self.stage.is_driver_stage() {
            return None;
        }
        match &self.planned {
            Planned::Env => None,
            Planned::Unknown => Some(self.adapter_backend.unwrap_or(base)),
            Planned::Backend(Backend::Auto) => Some(self.adapter_backend.unwrap_or(Backend::Auto)),
            Planned::Backend(b) => Some(*b),
        }
    }
}

/// One line, bounded: newlines would break the line format.
fn clean_field(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(FIELD_MAX)
        .collect()
}

/// Why this launch uses its backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// No history: egui-wgpu's default.
    Preferred,
    /// A fallback an earlier launch confirmed.
    Persisted,
    /// The previous start crashed on `from`; stepped down one rung.
    FellBack {
        from: Backend,
        adapter: Option<String>,
    },
    /// The previous start crashed on the bottom rung; nothing safer exists.
    Exhausted { tried: Backend },
    /// `--safe-gpu` for this launch.
    SafeFlag,
    /// `WGPU_BACKEND` is set; egui-wgpu applies it.
    Env(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub backend: Backend,
    pub reason: Reason,
}

impl Decision {
    /// The instance backends to force (`None` = egui-wgpu's default).
    pub fn wgpu_backends(&self) -> Option<wgpu::Backends> {
        match self.reason {
            Reason::Env(_) => None,
            _ => self.backend.wgpu_backends(),
        }
    }

    /// The backend to persist once this start is confirmed: only a
    /// step-down that worked.
    pub fn persist_on_success(&self) -> Option<Backend> {
        matches!(self.reason, Reason::FellBack { .. }).then_some(self.backend)
    }

    /// What this launch's marker records as its plan.
    pub fn planned(&self) -> Planned {
        match self.reason {
            Reason::Env(_) => Planned::Env,
            _ => Planned::Backend(self.backend),
        }
    }
}

/// Decide this launch's backend. Pure: every input is passed in.
///
/// Precedence: `WGPU_BACKEND` (non-blank) > `--safe-gpu` > a stale marker
/// (step down from what crashed) > the persisted fallback > auto.
pub fn decide_backend(
    marker: Option<&Marker>,
    persisted: Option<Backend>,
    env: Option<&str>,
    safe_flag: bool,
    platform: Platform,
) -> Decision {
    if let Some(v) = env.map(str::trim).filter(|v| !v.is_empty()) {
        return Decision {
            backend: Backend::Auto,
            reason: Reason::Env(v.to_string()),
        };
    }
    if safe_flag {
        return Decision {
            backend: platform.safest(),
            reason: Reason::SafeFlag,
        };
    }
    let base = match persisted.filter(|b| b.available_on(platform)) {
        Some(b) => Decision {
            backend: b,
            reason: Reason::Persisted,
        },
        None => Decision {
            backend: Backend::Auto,
            reason: Reason::Preferred,
        },
    };
    let Some((m, tried)) = marker.and_then(|m| Some((m, m.tried(base.backend)?))) else {
        return base;
    };
    match next_safer(tried, platform) {
        Some(next) => Decision {
            backend: next,
            reason: Reason::FellBack {
                from: tried,
                adapter: m.adapter.clone(),
            },
        },
        None => Decision {
            backend: platform.safest(),
            reason: Reason::Exhausted { tried },
        },
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MarkerError {
    #[error("gpu startup file {path}: {err}")]
    Io {
        path: PathBuf,
        #[source]
        err: std::io::Error,
    },
    #[error("corrupt gpu startup marker: {detail}")]
    Corrupt { detail: String },
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> MarkerError + '_ {
    move |err| MarkerError::Io {
        path: path.to_path_buf(),
        err,
    }
}

/// The marker file, locked by this process until cleared or dropped. A
/// drop without [`MarkerGuard::clear`] leaves the content in place —
/// exactly what a crash leaves behind.
#[derive(Debug)]
pub struct MarkerGuard {
    file: File,
    path: PathBuf,
}

/// The outcome of [`acquire_marker`].
#[derive(Debug)]
pub enum Acquired {
    /// Another running process holds the marker (a concurrent launch).
    Live,
    /// The lock is ours; `previous` is what an earlier start left
    /// (`None`: no marker; `Some(Err)`: an unreadable one).
    Free {
        guard: MarkerGuard,
        previous: Option<Result<Marker, MarkerError>>,
    },
}

/// Open (creating) and lock `dir/MARKER_FILE`, reading any stale content.
pub fn acquire_marker(dir: &Path) -> Result<Acquired, MarkerError> {
    let path = dir.join(MARKER_FILE);
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(io_err(&path))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(Acquired::Live),
        Err(TryLockError::Error(err)) => return Err(MarkerError::Io { path, err }),
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MARKER_MAX)
        .read_to_end(&mut bytes)
        .map_err(io_err(&path))?;
    let previous = if bytes.iter().all(u8::is_ascii_whitespace) {
        None
    } else {
        Some(match std::str::from_utf8(&bytes) {
            Ok(text) => Marker::parse(text),
            Err(_) => Err(MarkerError::Corrupt {
                detail: "not UTF-8".to_string(),
            }),
        })
    };
    Ok(Acquired::Free {
        guard: MarkerGuard { file, path },
        previous,
    })
}

impl MarkerGuard {
    /// Replace the marker's content (flushed to disk before the driver
    /// is touched — a crash right after must find it).
    pub fn write(&mut self, marker: &Marker) -> Result<(), MarkerError> {
        let text = marker.serialize();
        let path = self.path.clone();
        self.file.set_len(0).map_err(io_err(&path))?;
        self.file.seek(SeekFrom::Start(0)).map_err(io_err(&path))?;
        self.file
            .write_all(text.as_bytes())
            .map_err(io_err(&path))?;
        self.file.sync_data().map_err(io_err(&path))
    }

    /// The start is confirmed: empty the file while still locked (so a
    /// racing launch never reads stale content), unlock, then remove it.
    pub fn clear(self) -> Result<(), MarkerError> {
        let MarkerGuard { file, path } = self;
        file.set_len(0).map_err(io_err(&path))?;
        drop(file);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(MarkerError::Io { path, err }),
        }
    }
}

/// The persisted fallback (`Ok(None)` when absent).
pub fn read_persisted(dir: &Path) -> Result<Option<Backend>, MarkerError> {
    let path = dir.join(PERSISTED_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => Backend::parse(&text)
            .map(Some)
            .ok_or_else(|| MarkerError::Corrupt {
                detail: format!("{} holds no backend name", path.display()),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(MarkerError::Io { path, err }),
    }
}

pub fn write_persisted(dir: &Path, backend: Backend) -> Result<(), MarkerError> {
    let path = dir.join(PERSISTED_FILE);
    std::fs::write(&path, format!("{}\n", backend.name())).map_err(io_err(&path))
}

/// The state dir: the parent of the per-user brushes dir, else
/// `temp_dir()/umber` (see module docs).
pub fn state_dir() -> PathBuf {
    umber_brush::preset_library::user_preset_dir()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| std::env::temp_dir().join("umber"))
}

/// One launch's marker bookkeeping (behind [`SESSION`] in the app; a
/// plain value in tests).
#[derive(Debug)]
pub struct Session {
    dir: PathBuf,
    guard: Option<MarkerGuard>,
    marker: Marker,
    persist: Option<Backend>,
    frames: u32,
    /// This launch runs on a fallback rung (stepped down or remembered),
    /// not the platform's preferred backend.
    fallback: bool,
}

impl Session {
    pub fn new(dir: PathBuf, guard: Option<MarkerGuard>, decision: &Decision) -> Self {
        Session {
            dir,
            guard,
            marker: Marker::new(decision.planned()),
            persist: decision.persist_on_success(),
            frames: 0,
            fallback: matches!(
                decision.reason,
                Reason::FellBack { .. } | Reason::Persisted | Reason::Exhausted { .. }
            ),
        }
    }

    /// Write the marker (before device creation).
    pub fn write_marker(&mut self) -> Result<(), MarkerError> {
        match self.guard.as_mut() {
            Some(g) => g.write(&self.marker),
            None => Ok(()),
        }
    }

    /// Record the adapter about to get a device request.
    pub fn note_adapter(
        &mut self,
        name: &str,
        backend: wgpu::Backend,
        driver: &str,
    ) -> Result<(), MarkerError> {
        self.marker.adapter = Some(clean_field(name));
        self.marker.adapter_backend = Backend::from_wgpu(backend);
        self.marker.driver = Some(clean_field(driver));
        self.marker.stage = Stage::Adapter;
        self.write_marker()
    }

    /// The device exists (the app is being constructed): the driver
    /// survived device creation, so a death from here on is not a
    /// driver crash (see the module docs).
    pub fn note_device(&mut self) -> Result<(), MarkerError> {
        if self.marker.stage >= Stage::Device {
            return Ok(());
        }
        self.marker.stage = Stage::Device;
        self.write_marker()
    }

    /// Count a frame; `Ok(true)` on the confirming one (marker cleared,
    /// fallback persisted). Later calls are no-ops.
    pub fn note_frame(&mut self) -> Result<bool, MarkerError> {
        if self.frames >= FRAMES_TO_CONFIRM {
            return Ok(false);
        }
        self.frames += 1;
        if self.frames < FRAMES_TO_CONFIRM {
            return Ok(false);
        }
        self.confirm()?;
        Ok(true)
    }

    fn confirm(&mut self) -> Result<(), MarkerError> {
        if let Some(b) = self.persist.take() {
            write_persisted(&self.dir, b)?;
            log::warn!(
                "gpu startup: the {} fallback worked; remembered in {}",
                b.name(),
                self.dir.join(PERSISTED_FILE).display()
            );
        }
        match self.guard.take() {
            Some(g) => g.clear(),
            None => Ok(()),
        }
    }

    /// The process is exiting; see [`exit_action`] for the rule.
    pub fn finish(&mut self, clean: bool) -> Result<(), MarkerError> {
        let action = exit_action(clean, self.marker.stage, self.fallback);
        if action == ExitAction::KeepMarker {
            return Ok(());
        }
        self.persist = None;
        if action == ExitAction::UndoFallback {
            log::error!(
                "gpu startup: the {} fallback found no adapter that can present here; \
                 undoing it so the next launch uses auto again",
                self.marker
                    .planned_backend()
                    .map(Backend::name)
                    .unwrap_or("fallback")
            );
            remove_persisted(&self.dir)?;
        }
        match self.guard.take() {
            Some(g) => g.clear(),
            None => Ok(()),
        }
    }
}

/// What [`Session::finish`] does with an unconfirmed marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitAction {
    /// Clear it: nothing about this exit blames the backend.
    Clear,
    /// Keep it: the driver failed this backend, so the next launch steps
    /// down.
    KeepMarker,
    /// Clear it AND remove the persisted fallback: a fallback rung found
    /// no adapter able to present here, so falling back made things
    /// worse.
    UndoFallback,
}

/// The exit rule (pure, table-tested).
///
/// - A clean exit clears.
/// - An error at stage `device` is the app's own: clear.
/// - An error at stage `adapter` is a failed device request: keep (step
///   down next launch).
/// - An error at stage `planned` means no adapter was found. On the
///   preferred backend, keep it (a broken default stack: try the safer
///   one). On a fallback rung, undo it (the fallback cannot present here,
///   e.g. GL on Wayland): retrying the same dead end every launch is the
///   failure mode this exists to prevent.
pub fn exit_action(clean: bool, stage: Stage, fallback: bool) -> ExitAction {
    match (clean, stage, fallback) {
        (true, _, _) => ExitAction::Clear,
        (false, Stage::Device, _) => ExitAction::Clear,
        (false, Stage::Adapter, _) => ExitAction::KeepMarker,
        (false, Stage::Planned, false) => ExitAction::KeepMarker,
        (false, Stage::Planned, true) => ExitAction::UndoFallback,
    }
}

/// Remove the persisted fallback (absent is fine).
pub fn remove_persisted(dir: &Path) -> Result<(), MarkerError> {
    let path = dir.join(PERSISTED_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(MarkerError::Io { path, err }),
    }
}

/// The app's one session (set by [`prepare`]).
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn with_session(f: impl FnOnce(&mut Session) -> Result<(), MarkerError>) {
    let mut slot = SESSION.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(s) = slot.as_mut() {
        if let Err(e) = f(s) {
            log::warn!("gpu startup: {e}");
        }
    }
}

/// Decide the backend, log why, write the marker and keep it locked.
/// Never fails: every IO problem degrades to a log line and the default
/// path (a broken marker must not stop the app from starting).
pub fn prepare(safe_flag: bool) -> Decision {
    let platform = Platform::current();
    let env = std::env::var("WGPU_BACKEND").ok();
    let dir = state_dir();

    let (guard, previous) = match acquire_marker(&dir) {
        Ok(Acquired::Free { guard, previous }) => (Some(guard), previous),
        Ok(Acquired::Live) => {
            log::info!("gpu startup: another Umber is starting; skipping the crash marker");
            (None, None)
        }
        Err(e) => {
            log::warn!("gpu startup: marker unavailable ({e}); no crash recovery this launch");
            (None, None)
        }
    };
    let stale = match previous {
        None => None,
        Some(Ok(m)) => Some(m),
        Some(Err(e)) => {
            log::warn!("gpu startup: {e}; treating it as a crashed start");
            Some(Marker::unknown())
        }
    };
    let persisted = match read_persisted(&dir) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("gpu startup: ignoring the remembered backend ({e})");
            None
        }
    };

    let decision = decide_backend(
        stale.as_ref(),
        persisted,
        env.as_deref(),
        safe_flag,
        platform,
    );
    log_decision(&decision, stale.as_ref());

    let mut session = Session::new(dir, guard, &decision);
    if let Err(e) = session.write_marker() {
        log::warn!("gpu startup: could not write the marker ({e})");
    }
    *SESSION.lock().unwrap_or_else(|p| p.into_inner()) = Some(session);
    decision
}

fn log_decision(d: &Decision, stale: Option<&Marker>) {
    let name = d.backend.name();
    match &d.reason {
        Reason::Preferred => log::info!("gpu startup: backend auto"),
        Reason::Persisted => log::info!("gpu startup: backend {name} (remembered fallback)"),
        Reason::FellBack { from, adapter } => log::error!(
            "gpu startup: the previous launch died inside the GPU driver on {}{}; \
             falling back to {name}",
            from.name(),
            adapter
                .as_deref()
                .map(|a| format!(" ({a})"))
                .unwrap_or_default()
        ),
        Reason::Exhausted { tried } => log::error!(
            "gpu startup: the previous launch died inside the GPU driver on {}, the safest \
             backend; no safer one exists — retrying {name}. Try updating the GPU driver, \
             or set WGPU_BACKEND",
            tried.name()
        ),
        Reason::SafeFlag => log::warn!("gpu startup: --safe-gpu: backend {name} for this launch"),
        Reason::Env(v) => {
            log::info!("gpu startup: WGPU_BACKEND={v} overrides the backend chain");
            if stale.is_some() {
                log::warn!("gpu startup: a previous start did not finish (chain not stepped)");
            }
        }
    }
}

/// From the device-descriptor closure, just before `request_device`.
pub fn note_adapter(info: &wgpu::AdapterInfo) {
    with_session(|s| s.note_adapter(&info.name, info.backend, &info.driver));
}

/// From the app constructor: the device exists (see [`Stage::Device`]).
pub fn note_device() {
    with_session(|s| s.note_device());
}

/// From `App::ui`, every frame (cheap after confirmation).
pub fn note_frame() {
    with_session(|s| s.note_frame().map(|_| ()));
}

/// After `run_native` returns (`clean` = it returned `Ok`).
pub fn finish(clean: bool) {
    with_session(|s| s.finish(clean));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const PLATFORMS: [Platform; 3] = [Platform::Unix, Platform::Windows, Platform::Mac];

    fn crashed(planned: Planned, adapter_backend: Option<Backend>) -> Marker {
        Marker {
            adapter: adapter_backend.map(|_| "Test GPU".to_string()),
            adapter_backend,
            stage: if adapter_backend.is_some() {
                Stage::Adapter
            } else {
                Stage::Planned
            },
            ..Marker {
                planned,
                ..Marker::unknown()
            }
        }
    }

    #[test]
    fn a_start_killed_after_device_creation_never_steps_down() {
        // The 2026-10-15 regression: a launch killed by `timeout` before
        // its 3rd frame (headless: the window never mapped) left a marker.
        // The driver had survived device creation, so the next launch must
        // stay on auto, not force GL.
        for p in PLATFORMS {
            let mut m = crashed(Planned::Backend(Backend::Auto), Some(Backend::Vulkan));
            m.stage = Stage::Device;
            let d = decide_backend(Some(&m), None, None, false, p);
            assert_eq!(
                (d.backend, d.reason),
                (Backend::Auto, Reason::Preferred),
                "{p:?}"
            );
        }
        // With a remembered fallback, it is kept (not stepped further).
        let mut m = crashed(Planned::Backend(Backend::Dx12), Some(Backend::Dx12));
        m.stage = Stage::Device;
        let d = decide_backend(
            Some(&m),
            Some(Backend::Dx12),
            None,
            false,
            Platform::Windows,
        );
        assert_eq!((d.backend, d.reason), (Backend::Dx12, Reason::Persisted));
    }

    #[test]
    fn exit_rule_blames_the_backend_only_for_driver_failures() {
        use ExitAction::*;
        let rows = [
            // (clean, stage, fallback) -> action
            (true, Stage::Planned, false, Clear),
            (true, Stage::Device, true, Clear),
            (false, Stage::Device, false, Clear),
            (false, Stage::Device, true, Clear),
            (false, Stage::Adapter, false, KeepMarker),
            (false, Stage::Adapter, true, KeepMarker),
            (false, Stage::Planned, false, KeepMarker),
            // GL on Wayland: the fallback found no presentable adapter.
            (false, Stage::Planned, true, UndoFallback),
        ];
        for (clean, stage, fallback, want) in rows {
            assert_eq!(
                exit_action(clean, stage, fallback),
                want,
                "{clean} {stage:?} {fallback}"
            );
        }
    }

    #[test]
    fn a_dead_fallback_is_undone_so_the_next_launch_is_auto() {
        // The exact remote failure: previous launch "crashed" on Vulkan,
        // this one fell back to GL, GL found no adapter that can present
        // (Wayland): run_native returns Err at stage planned.
        let dir = fresh_dir("undo-fallback");
        write_persisted(&dir, Backend::Gl).expect("seed a remembered GL");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let d = fell_back(Backend::Vulkan, Backend::Gl, None);
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        s.finish(false).expect("finish");
        drop(s);
        assert!(!dir.join(MARKER_FILE).exists(), "marker cleared");
        assert_eq!(read_persisted(&dir).expect("read"), None, "fallback undone");
        let (guard, previous) = free(acquire_marker(&dir).expect("next launch"));
        assert!(previous.is_none());
        let next = decide_backend(None, None, None, false, Platform::Unix);
        assert_eq!(
            (next.backend, next.reason),
            (Backend::Auto, Reason::Preferred)
        );
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_round_trips_and_old_markers_infer_it() {
        let mut m = Marker::new(Planned::Backend(Backend::Auto));
        for stage in [Stage::Planned, Stage::Adapter, Stage::Device] {
            m.stage = stage;
            assert_eq!(Marker::parse(&m.serialize()).expect("parse").stage, stage);
        }
        // A v1 marker written before `stage=` existed.
        let old = "umber-gpu-marker v1\nplanned=auto\ntimestamp=1\npid=2\n";
        assert_eq!(Marker::parse(old).expect("old").stage, Stage::Planned);
        let old_adapter = "umber-gpu-marker v1\nplanned=auto\nadapter=GPU\n";
        assert_eq!(
            Marker::parse(old_adapter).expect("old").stage,
            Stage::Adapter
        );
    }

    fn fell_back(from: Backend, to: Backend, adapter: Option<&str>) -> Decision {
        Decision {
            backend: to,
            reason: Reason::FellBack {
                from,
                adapter: adapter.map(str::to_string),
            },
        }
    }

    fn exhausted(tried: Backend, to: Backend) -> Decision {
        Decision {
            backend: to,
            reason: Reason::Exhausted { tried },
        }
    }

    #[test]
    fn no_marker_uses_auto_or_the_persisted_fallback() {
        for p in PLATFORMS {
            let d = decide_backend(None, None, None, false, p);
            assert_eq!(d.backend, Backend::Auto);
            assert_eq!(d.reason, Reason::Preferred);
            assert_eq!(d.wgpu_backends(), None);
            assert_eq!(d.persist_on_success(), None);
        }
        let d = decide_backend(None, Some(Backend::Gl), None, false, Platform::Unix);
        assert_eq!((d.backend, &d.reason), (Backend::Gl, &Reason::Persisted));
        assert_eq!(d.wgpu_backends(), Some(wgpu::Backends::GL));
        assert_eq!(d.persist_on_success(), None, "already persisted");
        let d = decide_backend(None, Some(Backend::Dx12), None, false, Platform::Windows);
        assert_eq!((d.backend, d.reason), (Backend::Dx12, Reason::Persisted));
    }

    #[test]
    fn a_persisted_backend_the_platform_lacks_is_ignored() {
        // A config copied from Windows to Linux, or Linux to a Mac.
        let d = decide_backend(None, Some(Backend::Dx12), None, false, Platform::Unix);
        assert_eq!((d.backend, d.reason), (Backend::Auto, Reason::Preferred));
        let d = decide_backend(None, Some(Backend::Gl), None, false, Platform::Mac);
        assert_eq!((d.backend, d.reason), (Backend::Auto, Reason::Preferred));
        let d = decide_backend(None, Some(Backend::Metal), None, false, Platform::Windows);
        assert_eq!((d.backend, d.reason), (Backend::Auto, Reason::Preferred));
    }

    #[test]
    fn a_stale_marker_steps_one_rung_down_the_platform_chain() {
        use Backend::*;
        // (platform, planned, adapter backend, expected decision)
        let rows: [(Platform, Backend, Option<Backend>, Decision); 9] = [
            // Linux: auto/Vulkan -> GL -> exhausted.
            (Platform::Unix, Auto, None, fell_back(Auto, Gl, None)),
            (
                Platform::Unix,
                Auto,
                Some(Vulkan),
                fell_back(Vulkan, Gl, Some("Test GPU")),
            ),
            (Platform::Unix, Vulkan, None, fell_back(Vulkan, Gl, None)),
            (Platform::Unix, Gl, None, exhausted(Gl, Gl)),
            // Windows: auto/Vulkan -> DX12 -> GL -> exhausted.
            (Platform::Windows, Auto, None, fell_back(Auto, Dx12, None)),
            (
                Platform::Windows,
                Auto,
                Some(Vulkan),
                fell_back(Vulkan, Dx12, Some("Test GPU")),
            ),
            // auto picked DX12 and crashed: skip to GL, not "DX12 again".
            (
                Platform::Windows,
                Auto,
                Some(Dx12),
                fell_back(Dx12, Gl, Some("Test GPU")),
            ),
            (Platform::Windows, Dx12, None, fell_back(Dx12, Gl, None)),
            (Platform::Windows, Gl, None, exhausted(Gl, Gl)),
        ];
        for (platform, planned, adapter_backend, want) in rows {
            let m = crashed(Planned::Backend(planned), adapter_backend);
            let got = decide_backend(Some(&m), None, None, false, platform);
            assert_eq!(
                got, want,
                "{platform:?} planned={planned:?} adapter={adapter_backend:?}"
            );
        }
    }

    #[test]
    fn mac_has_one_rung_so_any_crash_exhausts_the_chain() {
        for planned in [Backend::Auto, Backend::Metal] {
            let m = crashed(Planned::Backend(planned), None);
            let d = decide_backend(Some(&m), None, None, false, Platform::Mac);
            assert_eq!(d, exhausted(planned, Backend::Metal));
            assert_eq!(d.persist_on_success(), None);
        }
    }

    #[test]
    fn a_marker_steps_from_what_crashed_not_from_the_persisted_rung() {
        // Persisted DX12; the crashed start was on DX12 -> GL.
        let m = crashed(Planned::Backend(Backend::Dx12), None);
        let d = decide_backend(
            Some(&m),
            Some(Backend::Dx12),
            None,
            false,
            Platform::Windows,
        );
        assert_eq!(d, fell_back(Backend::Dx12, Backend::Gl, None));
        assert_eq!(d.persist_on_success(), Some(Backend::Gl));
        assert_eq!(d.wgpu_backends(), Some(wgpu::Backends::GL));
    }

    #[test]
    fn an_unknown_marker_steps_from_the_rung_this_launch_would_use() {
        let m = Marker::unknown();
        // No history: base auto.
        let d = decide_backend(Some(&m), None, None, false, Platform::Unix);
        assert_eq!(d, fell_back(Backend::Auto, Backend::Gl, None));
        let d = decide_backend(Some(&m), None, None, false, Platform::Windows);
        assert_eq!(d, fell_back(Backend::Auto, Backend::Dx12, None));
        // Persisted DX12 on Windows: the unknown start was on DX12 -> GL.
        let d = decide_backend(
            Some(&m),
            Some(Backend::Dx12),
            None,
            false,
            Platform::Windows,
        );
        assert_eq!(d, fell_back(Backend::Dx12, Backend::Gl, None));
        // Persisted GL already at the bottom.
        let d = decide_backend(Some(&m), Some(Backend::Gl), None, false, Platform::Unix);
        assert_eq!(d, exhausted(Backend::Gl, Backend::Gl));
    }

    #[test]
    fn a_marker_from_an_env_override_start_never_steps_the_chain() {
        let m = crashed(Planned::Env, Some(Backend::Vulkan));
        let d = decide_backend(Some(&m), None, None, false, Platform::Unix);
        assert_eq!((d.backend, d.reason), (Backend::Auto, Reason::Preferred));
        let d = decide_backend(Some(&m), Some(Backend::Gl), None, false, Platform::Unix);
        assert_eq!((d.backend, d.reason), (Backend::Gl, Reason::Persisted));
    }

    #[test]
    fn wgpu_backend_env_wins_over_everything() {
        let m = crashed(Planned::Backend(Backend::Vulkan), None);
        for p in PLATFORMS {
            let d = decide_backend(Some(&m), Some(Backend::Gl), Some(" vulkan "), true, p);
            assert_eq!(d.backend, Backend::Auto);
            assert_eq!(d.reason, Reason::Env("vulkan".to_string()));
            assert_eq!(
                d.wgpu_backends(),
                None,
                "egui-wgpu applies WGPU_BACKEND itself"
            );
            assert_eq!(d.persist_on_success(), None);
            assert_eq!(d.planned(), Planned::Env);
        }
        // Blank env is not an override.
        for blank in ["", "   "] {
            let d = decide_backend(None, None, Some(blank), false, Platform::Unix);
            assert_eq!(d.reason, Reason::Preferred);
        }
    }

    #[test]
    fn safe_flag_forces_the_bottom_rung_for_one_launch() {
        let m = crashed(Planned::Backend(Backend::Auto), None);
        for (p, want) in [
            (Platform::Unix, Backend::Gl),
            (Platform::Windows, Backend::Gl),
            (Platform::Mac, Backend::Metal),
        ] {
            for marker in [None, Some(&m)] {
                let d = decide_backend(marker, Some(Backend::Vulkan), None, true, p);
                assert_eq!((d.backend, &d.reason), (want, &Reason::SafeFlag), "{p:?}");
                assert_eq!(
                    d.persist_on_success(),
                    None,
                    "--safe-gpu is never remembered"
                );
                assert_eq!(d.planned(), Planned::Backend(want));
            }
        }
    }

    /// Exhaustive sweep over every input combination: invariants that
    /// must hold for each row the hand table above does not name.
    #[test]
    fn every_combination_yields_a_backend_the_platform_has() {
        let planned = [
            None,
            Some(Planned::Env),
            Some(Planned::Unknown),
            Some(Planned::Backend(Backend::Auto)),
            Some(Planned::Backend(Backend::Vulkan)),
            Some(Planned::Backend(Backend::Dx12)),
            Some(Planned::Backend(Backend::Metal)),
            Some(Planned::Backend(Backend::Gl)),
        ];
        let adapter = [
            None,
            Some(Backend::Vulkan),
            Some(Backend::Dx12),
            Some(Backend::Gl),
        ];
        let persisted = [
            None,
            Some(Backend::Vulkan),
            Some(Backend::Dx12),
            Some(Backend::Gl),
        ];
        let mut rows = 0;
        for p in PLATFORMS {
            for pl in &planned {
                for ab in adapter {
                    let m = pl.clone().map(|pl| crashed(pl, ab));
                    for pe in persisted {
                        for env in [None, Some("gl")] {
                            for safe in [false, true] {
                                rows += 1;
                                let d = decide_backend(m.as_ref(), pe, env, safe, p);
                                assert!(d.backend.available_on(p), "{d:?} on {p:?}");
                                if env.is_some() {
                                    assert!(matches!(d.reason, Reason::Env(_)));
                                } else if safe {
                                    assert_eq!(d.backend, p.safest());
                                }
                                if let Reason::FellBack { from, .. } = d.reason {
                                    assert_ne!(from, d.backend, "a fallback must change rung");
                                    assert_eq!(next_safer(from, p), Some(d.backend));
                                }
                                if let Reason::Exhausted { tried } = d.reason {
                                    assert_eq!(next_safer(tried, p), None);
                                    assert_eq!(d.backend, p.safest());
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(rows, 3 * 8 * 4 * 4 * 2 * 2);
    }

    #[test]
    fn the_chain_bottoms_out_at_safest_and_every_rung_reaches_it() {
        for p in PLATFORMS {
            assert_eq!(next_safer(p.safest(), p), None, "{p:?}");
            for start in Backend::ALL.into_iter().filter(|b| b.available_on(p)) {
                let mut b = start;
                let mut steps = 0;
                while let Some(n) = next_safer(b, p) {
                    assert!(n.available_on(p));
                    b = n;
                    steps += 1;
                    assert!(steps <= 3, "chain loops on {p:?}");
                }
                // The walk ends on safest, or on Mac's auto (which IS Metal
                // there: one native backend); a crash on that end rung
                // retries safest.
                let end = crashed(Planned::Backend(b), None);
                let d = decide_backend(Some(&end), None, None, false, p);
                assert_eq!(d, exhausted(b, p.safest()), "{p:?} from {start:?}");
                if p != Platform::Mac {
                    assert_eq!(b, p.safest(), "{p:?} from {start:?}");
                }
            }
        }
    }

    #[test]
    fn marker_text_round_trips_and_sanitizes_fields() {
        let m = Marker {
            planned: Planned::Backend(Backend::Vulkan),
            stage: Stage::Adapter,
            adapter: Some("GPU\nplanned=gl".to_string()),
            adapter_backend: Some(Backend::Vulkan),
            driver: Some("drv 1.2".to_string()),
            timestamp: 1_760_000_000,
            pid: 4242,
        };
        let text = m.serialize();
        let back = Marker::parse(&text).expect("round trip");
        // The newline in the adapter could not inject a planned= line.
        assert_eq!(back.planned, Planned::Backend(Backend::Vulkan));
        assert_eq!(back.adapter.as_deref(), Some("GPU planned=gl"));
        assert_eq!(back.adapter_backend, Some(Backend::Vulkan));
        assert_eq!(back.driver.as_deref(), Some("drv 1.2"));
        assert_eq!((back.timestamp, back.pid), (1_760_000_000, 4242));
        for planned in [
            Planned::Env,
            Planned::Unknown,
            Planned::Backend(Backend::Gl),
        ] {
            let m = Marker {
                planned: planned.clone(),
                ..Marker::unknown()
            };
            assert_eq!(Marker::parse(&m.serialize()).expect("parse"), m);
        }
        let long = "x".repeat(1000);
        assert_eq!(clean_field(&long).len(), FIELD_MAX);
    }

    #[test]
    fn corrupt_marker_text_is_a_typed_error() {
        for bad in [
            "",
            "{\"backend\":\"vulkan\"}",
            "umber-gpu-marker v9\nplanned=gl\n",
            "umber-gpu-marker v1\n",
            "umber-gpu-marker v1\nplanned=quantum\n",
        ] {
            assert!(
                matches!(Marker::parse(bad), Err(MarkerError::Corrupt { .. })),
                "{bad:?}"
            );
        }
        // Unknown keys and junk lines are tolerated.
        let ok = "umber-gpu-marker v1\nfuture=1\njunk\nplanned=dx12\npid=abc\n";
        let m = Marker::parse(ok).expect("tolerant");
        assert_eq!(m.planned, Planned::Backend(Backend::Dx12));
        assert_eq!(m.pid, 0);
    }

    static DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn fresh_dir(tag: &str) -> PathBuf {
        let id = DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "umber-gpu-startup-{}-{id}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmpdir");
        dir
    }

    fn free(a: Acquired) -> (MarkerGuard, Option<Result<Marker, MarkerError>>) {
        match a {
            Acquired::Free { guard, previous } => (guard, previous),
            Acquired::Live => panic!("expected a free marker"),
        }
    }

    #[test]
    fn marker_file_round_trip_crash_and_clear() {
        let dir = fresh_dir("roundtrip");
        let (mut guard, previous) = free(acquire_marker(&dir).expect("acquire"));
        assert!(previous.is_none(), "fresh dir has no marker");
        let mut m = Marker::new(Planned::Backend(Backend::Auto));
        m.adapter = Some("Test GPU".to_string());
        m.adapter_backend = Some(Backend::Vulkan);
        guard.write(&m).expect("write");
        // A crash: the guard goes away without clear().
        drop(guard);
        let (guard, previous) = free(acquire_marker(&dir).expect("reacquire"));
        let stale = previous.expect("stale marker").expect("parses");
        assert_eq!(stale, m);
        let d = decide_backend(Some(&stale), None, None, false, Platform::Unix);
        assert_eq!(d, fell_back(Backend::Vulkan, Backend::Gl, Some("Test GPU")));
        guard.clear().expect("clear");
        assert!(!dir.join(MARKER_FILE).exists());
        let (guard, previous) = free(acquire_marker(&dir).expect("third"));
        assert!(previous.is_none(), "cleared marker is gone");
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_marker_is_live_not_stale() {
        let dir = fresh_dir("live");
        let (mut guard, _) = free(acquire_marker(&dir).expect("acquire"));
        guard
            .write(&Marker::new(Planned::Backend(Backend::Auto)))
            .expect("write");
        assert!(matches!(acquire_marker(&dir), Ok(Acquired::Live)));
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_marker_file_is_an_error_not_a_panic() {
        let dir = fresh_dir("corrupt");
        for bytes in [&b"garbage\n"[..], &[0xff, 0xfe, 0x00, 0x41][..]] {
            std::fs::write(dir.join(MARKER_FILE), bytes).expect("seed");
            let (guard, previous) = free(acquire_marker(&dir).expect("acquire"));
            assert!(
                matches!(previous, Some(Err(MarkerError::Corrupt { .. }))),
                "{previous:?}"
            );
            guard.clear().expect("clear");
        }
        // Whitespace only = an empty marker = none.
        std::fs::write(dir.join(MARKER_FILE), b"  \n").expect("seed");
        let (guard, previous) = free(acquire_marker(&dir).expect("acquire"));
        assert!(previous.is_none());
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persisted_backend_round_trip_and_corruption() {
        let dir = fresh_dir("persisted");
        assert_eq!(read_persisted(&dir).expect("absent"), None);
        write_persisted(&dir, Backend::Dx12).expect("write");
        assert_eq!(read_persisted(&dir).expect("read"), Some(Backend::Dx12));
        std::fs::write(dir.join(PERSISTED_FILE), "quantum").expect("seed");
        assert!(matches!(
            read_persisted(&dir),
            Err(MarkerError::Corrupt { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Reads the marker back through a fresh lock: Windows file locks are
    /// mandatory, so a second handle cannot read while the session holds it.
    fn reread(dir: &Path) -> (MarkerGuard, Marker) {
        let (guard, previous) = free(acquire_marker(dir).expect("reacquire"));
        (guard, previous.expect("marker present").expect("parses"))
    }

    #[test]
    fn session_records_the_adapter_before_the_device_request() {
        let dir = fresh_dir("adapter");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let d = fell_back(Backend::Vulkan, Backend::Gl, None);
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        s.note_adapter("llvmpipe", wgpu::Backend::Gl, "mesa")
            .expect("adapter");
        drop(s); // the driver crashed inside request_device
        let (guard, on_disk) = reread(&dir);
        assert_eq!(on_disk.planned, Planned::Backend(Backend::Gl));
        assert_eq!(on_disk.adapter.as_deref(), Some("llvmpipe"));
        assert_eq!(on_disk.adapter_backend, Some(Backend::Gl));
        assert_eq!(on_disk.driver.as_deref(), Some("mesa"));
        assert_eq!(on_disk.pid, std::process::id());
        // GL was the bottom rung: the next launch is exhausted, not looping.
        let next = decide_backend(Some(&on_disk), None, None, false, Platform::Unix);
        assert_eq!(next, exhausted(Backend::Gl, Backend::Gl));
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_confirms_on_the_third_frame_and_persists_a_fallback() {
        let dir = fresh_dir("session");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let d = fell_back(Backend::Vulkan, Backend::Gl, None);
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        assert!(!s.note_frame().expect("1"));
        assert!(!s.note_frame().expect("2"));
        assert!(
            dir.join(MARKER_FILE).exists(),
            "not confirmed before frame 3"
        );
        assert_eq!(read_persisted(&dir).expect("read"), None);
        assert!(s.note_frame().expect("3"));
        assert!(!dir.join(MARKER_FILE).exists());
        assert_eq!(read_persisted(&dir).expect("read"), Some(Backend::Gl));
        assert!(!s.note_frame().expect("4"), "confirm fires once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_confirmed_preferred_start_persists_nothing() {
        let dir = fresh_dir("session-pref");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let d = decide_backend(None, None, None, false, Platform::Unix);
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        for _ in 0..FRAMES_TO_CONFIRM {
            s.note_frame().expect("frame");
        }
        assert!(!dir.join(MARKER_FILE).exists());
        assert_eq!(read_persisted(&dir).expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_finish_clears_on_clean_exit_and_keeps_on_error() {
        let d = Decision {
            backend: Backend::Auto,
            reason: Reason::Preferred,
        };
        let dir = fresh_dir("finish-ok");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        s.finish(true).expect("finish");
        assert!(!dir.join(MARKER_FILE).exists());
        assert_eq!(read_persisted(&dir).expect("read"), None);
        let _ = std::fs::remove_dir_all(&dir);

        let dir = fresh_dir("finish-err");
        let (guard, _) = free(acquire_marker(&dir).expect("acquire"));
        let mut s = Session::new(dir.clone(), Some(guard), &d);
        s.write_marker().expect("write");
        s.finish(false).expect("finish");
        drop(s);
        let (guard, stale) = reread(&dir);
        assert_eq!(stale.planned, Planned::Backend(Backend::Auto));
        guard.clear().expect("clear");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
