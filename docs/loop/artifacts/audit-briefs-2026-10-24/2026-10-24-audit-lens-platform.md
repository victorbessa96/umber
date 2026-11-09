# Audit lens F — platform, distribution, release, security (2026-10-24, live-verified)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app. READ-ONLY TASK: no cargo, no builds, no file edits outside your
report file. Every claim cites file:line from the LIVE tree.

## The lens: everything between the code and a user's machine

### 1. The CI matrix (.github/workflows/)

Every workflow file: its jobs, its triggers, its platforms. The CI
path filter (the Windows leg's platforms job, 908135e). The nightly
workflows (perf-nightly + any others): their schedules, the pinned
24.04 image (ec7ccb8), the compare gate. The docs-only path. The
current red/green state of each workflow (this is a static read of
the workflow FILES, not a live run query).

### 2. The packaging (packaging/ or scripts/ + the flatpak chain)

The Flatpak manifest + the wall-4 arc's landing state (the 26.08
platform/SDK install, --disable-rofiles-fuse, librsvg2-common, the
appstream-compose revert — d5c2a63/4f0c482): what the flatpak job
does TODAY. AppImage. The shell+PS proven path. The icons/atlas
(DPI 2× + the runtime re-raster debt).

### 3. The release gates (docs/specs/release-checklist-0.1.0.md)

Walk the checklist: which gates are MET (cite evidence), which are
partner-gated (the SPEC v1.1 ratification, the release decisions
C2-16/22/23-25/30), which are mechanically open. The release
phases (alpha/beta/RC, 2c19dcd). What 0.1.0 actually needs before a
tag — the honest critical path.

### 4. The security posture (SECURITY.md + the surfaces)

SECURITY.md's rows: current or stale (each row's HEAD anchor).
The WASM consent gate (blake3, plugins.rs:22). The memory caps on
importers. The fuzz harnesses. The crash-reporter state (the core
landed ddb59e0; the frame-state remainder). Any known CVE-class
deps (grep Cargo.toml for the pinned versions of wgpu/eframe/
openusd/ufbx/image — name the versions, no live advisory check
needed, just the pins).

### 5. The licensing + legal walls

The GPL-3.0 + MIT library split. The .spp-import-is-legal ruling
(docs/loop/decisions.md). The .sbsar NEVER. The Adobe-format
boundaries. Any license files' state.

### 6. The i18n + accessibility surface for RELEASE (the partner-
facing polish)

The .ftl catalogs' lock state (the freeze-3 recovery landed — is
the lock lifted? grep docs/loop for the lock's origin + its lift
condition). The pt-BR completeness. The a11y rows (G7's state).

## The output

Write `docs/loop/audit-2026-10-24-lens-platform.md`:
- The workflow table: workflow | trigger | platforms | state.
- The packaging state + what a user can download today.
- The release critical path: met / partner-gated / open.
- The security verdict.
- The .ftl LOCK STATE verdict (this gates a named kill — be
  precise: what locked it, what lifts it, is it lifted NOW).
- "The named gaps" + "The surprises".
