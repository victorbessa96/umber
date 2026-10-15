# SPEC ratification brief (C2-23)

For Bessa, 2026-10-15. One page: what ratifying SPEC v1.0 + the v1.1
amendment commits Umber to, and what today's audit found the drafts get
WRONG. Ratify by ticking the block at the end of `SPEC.md`, or mark it up.
Nothing here is in force until you do (SPEC "Sources of truth" rule 1).

## What you are signing (unchanged by today)

- **Goal:** a GPL-3.0-or-later, Rust-native, Substance-Painter-class
  texture painter for Windows + Linux, local-first, no telemetry, no
  subscription.
- **Non-goals:** no modelling, no sculpting, no macOS, no cloud, no
  in-app generative AI, no >4K in-app painting in v0.1. No Adobe format
  execution ever; with amendment A2, `.spp` IMPORT is allowed and `.spp`
  writing never is.
- **7 deliverables:** the app, the crate workspace, the node-graph engine
  (MaterialX documents), the bake engine, the `.umber` git-friendly
  format, the headless CLI + WASM plugins, and two-OS CI releases.
- **15 acceptance criteria**, each with an oracle ([CI], [QA] or [HW]).
  Amendment A4: they are ticked at the RC walkthrough with evidence,
  never pre-ticked.
- **Sustainability, named plainly:** a human bus factor of one, no
  funding. Still true.

## Corrections today's work makes necessary (fold into v1.1 before signing)

| SPEC says | Reality (2026-10-15) | Proposed |
|---|---|---|
| W1: "winit 0.30 has no pen events — we bypass it" | winit 0.30.13 reads `WM_POINTER` pen pressure (`GetPointerPenInfo`) into `Touch { force }`; Umber's viewport uses it (C2-30 trace). winit has no TILT. | "Windows pen pressure rides winit's `WM_POINTER` path; the owned `stylus` crate adds what winit lacks (tilt, eraser, Wintab), via a WndProc subclass" |
| Acceptance: "tag → Windows **MSI** + Flatpak manifest + AppImage" | The installers design dropped MSI. cargo-dist 0.32.0 makes a Windows `.zip` + shell/PowerShell installers; `packaging.yml` makes the AppImage + validates the Flatpak | "tag → Windows zip + PS installer, Linux tar.xz + AppImage, a validated Flatpak manifest" (C2-19 changed the requirements row, not this one) |
| W6: "Windows MSI + Flatpak + AppImage" | same | same edit |
| (silent on signing) | Decided 2026-10-15: alpha unsigned + SHA-256; apply to SignPath Foundation (`docs/CODE_SIGNING.md`) | add to Constraints: "release binaries are built only by CI from a tagged commit; code signing via SignPath when granted" |
| Acceptance [HW] "reference GPU class (RTX 3060 / RX 6600)" | The perf baseline is from an Intel HD 530; the real-GPU row (C2-16) is owed on your RTX 3070 | keep the criterion; record the 3070 as the reference rig (it is above the class, so a pass there is not proof for the class floor; note that honestly) |
| "~32 blend modes" (W4) | `BlendMode` has 32 variants | confirm ✓ |
| W5 dogfood: "≥2 external artists" | Release checklist gate 8 asks for one real asset by you first | keep ≥2 external for 0.1.0 final; the alpha needs only yours |

## What ratifying does NOT do

It does not tick any acceptance box, start a release, or freeze the
roadmap. It makes `SPEC.md` the contract the RC walkthrough checks
against, and makes any later change a recorded decision.

## My recommendation

Ratify v1.0 **with** the v1.1 amendment, whose A6–A10 now carry the
corrections above (folded in 2026-10-15 under your "follow on
everything" approval). Every row is evidence-backed and cited. The alternative,
signing v1.0 as written, would contract to an MSI that will never ship
and to a stylus premise the code already disproves.
