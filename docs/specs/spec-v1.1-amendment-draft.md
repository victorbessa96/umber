# SPEC v1.1 — proposed amendment (DRAFT, for ratification at the 0.1.0 RC)

Not in force. SPEC.md changes only by Bessa's explicit decision (its own
"Sources of truth" rule 1). This file is the diff to ratify alongside v1.0,
so the contract describes the project that exists. Each item cites the
ruling or artifact it reconciles.

## A1. Banner: replace "no implementation work beyond Wave-0"

The v1.0 banner gates all implementation on ratification. Every wave since
ran under Bessa's standing directive (2026-10-12, "do NOT wait on me…
work autonomously", docs/loop/loop-prompt.md §0). Proposed banner:

> Ratified scope contract. Execution order lives in docs/ROADMAP.md (the
> plan of record); the Waves table below is the scope mapping, not the
> schedule.

## A2. Non-goals: the .spp line

v1.0: ".spp is undocumented HDF5" (no exception). Ruling 1 of 2026-10-12
(docs/loop/decisions.md, BINDING) ratified IMPORT; it landed in wave 3
(`30b966e` + `0a5c20d`). Proposed:

> **No Adobe format execution or writing, ever.** .sbsar never runs.
> .spp is never written; user-provided .spp files may be IMPORTED
> (clean-room, from published community HDF5-layout docs; per-project
> import report; geometry stays FBX/USD).

Constraints line "No Adobe SDKs, no .sbsar/.spp execution ever" stays as
written ("execution" already excludes reading).

## A3. Waves table: mark it the scope mapping

Add under the table: "Status per wave lives in docs/ROADMAP.md." Two rows
moved since v1.0, named so ratification sees them:

- W1 stylus: v1.0 names the raw Windows Ink path as the first backend.
  Today the shipped, verified path is Linux XI2 + raw pressure (ROADMAP
  scoreboard); a Windows Ink module exists with its live-HWND
  integration still an open item, and the Wintab half of the dual path
  is open (ROADMAP wave 5, `wave-stylus-win32`). To confirm at the RC:
  which Windows path is actually live.
- W4 per-layer pixels: delivered by the per-layer pool arc (stages 1a–5,
  2026-10-14), not the v1 destructive-publish path.

## A4. Acceptance criteria: walk them at the RC, don't pre-tick

All 15 boxes are unchecked in v1.0. The RC walkthrough
(docs/specs/release-checklist-0.1.0.md) ticks each with its evidence
(a CI run id, a golden, a protocol log). Evidence already known:

- Workspace build + tests + clippy, both OSes: CI run 38628118144
  (2026-10-14, green).
- Stylus input-to-photon: stage-3 baseline p95 10.19 ms (budget 20 ms;
  local lavapipe — the [QA] reference-rig run is still owed).

Everything else is verified at the walkthrough, not asserted here.

## A6. W1 stylus premise: winit reads Windows pen pressure

v1.0 W1: "winit 0.30 has no pen events — we bypass it". winit 0.30.13
handles `WM_POINTER` and reads `GetPointerPenInfo` pressure into
`WindowEvent::Touch { force }` (`platform_impl/windows/event_loop.rs`
:1985, :2087, :994); egui-winit forwards it and Umber's viewport uses it
(`docs/loop/kill-reports/2026-10-15-c2-30-winink.md`). Proposed W1 text:

> Windows pen pressure rides winit's `WM_POINTER` path. The owned
> `stylus` crate adds what winit lacks: tilt, eraser and barrel button
> through a WndProc subclass, and Wintab for tablets that only speak it.

## A7. Release artifacts: no MSI

v1.0 acceptance + W6: "Windows MSI + Flatpak manifest + AppImage". The
installers design dropped MSI; cargo-dist 0.32.0 produces a Windows
`.zip` + PowerShell installer and a Linux `.tar.xz` + shell installer;
`packaging.yml` builds the AppImage and validates the Flatpak manifest.
Proposed criterion:

> [CI] Release: tag → CI produces the Windows zip + PowerShell
> installer, the Linux tar.xz + shell installer and AppImage, and a
> validated Flatpak manifest, with SHA-256 sums for each.

## A8. Constraints: release provenance and signing

New constraint line:

> Release binaries are built only by CI from a tagged commit of the
> public repository, never on a personal machine. Windows binaries are
> code-signed through SignPath Foundation once granted
> (`docs/CODE_SIGNING.md`); until then releases ship SHA-256 sums.

## A9. The reference GPU, honestly

The [HW] criteria name "RTX 3060 / RX 6600 class". The only real-GPU rig
available is Bessa's RTX 3070, ABOVE the class. Proposed note under the
oracle legend:

> The recorded reference rig is an RTX 3070. A pass there shows the
> budget is reachable; it does not prove the class floor. A 3060-class
> run is owed before the criterion is called met for the class.

## A10. Dogfood gate, split by release

W5: "≥2 external artists each complete a full asset". Proposed:

> The alpha needs one complete asset by the maintainer (release
> checklist gate 8). 0.1.0 final keeps ≥2 external artists.

## A5. Unchanged (confirm only)

One-sentence goal, the rest of the non-goals, deliverables 1–7,
constraints (plus A8), risks, sustainability. The ~32 blend modes (W4)
are confirmed: `BlendMode` has 32 variants. The sustainability section's "human
bus factor of one" is still true.
