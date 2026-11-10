# Kill report: 4.2a community templates (authoring lane)

- Authorship: opencode claw (proc_bbc6726bafd); lock `razul-templates-authoring` over assets/templates/ + project.rs test module; dragon battery green first pass (fmt/layers/score/clippy/tests 0, every_shipped_template_loads ok, all template JSON valid).
- Implements: docs/loop/briefs/2026-11-10-templates-authoring.md

## Built

- 10 curated loadable template projects under `assets/templates/<slug>.umber/`: pbr-surface-kit, sci-fi-panel, terrain-height, character-skin, stylized-toon, ui-icons, grunge-overlay, water-normals, emissive-sign, weathering-kit. Each is a real v1 directory project (metadata + layer stacks, no pixel planes) that `load_from_dir` accepts.
- `assets/templates/TEMPLATES.md`: index table + authoring rules (rule 10 = keep in sync with the loader test).
- Test `project::tests::every_shipped_template_loads`: resolves assets/templates via bundled data_path, loads every `*.umber` dir, asserts Ok + count == 10; skips only when the dir is absent (installed binary).

## Judgment calls

- Templates stay at project version 1 (metadata only, no pixel planes — v2 is for live layer-owned planes), so they double as format examples.
- Loader/menu wiring deliberately NOT in this kill (4.2b), held for the fable UI/UX audit so the entry point lands once, in the right place.

## Unverified

- Templates are validated by load, not by eye — no rendered preview of any template yet; 4.2b's live-render check should screenshot at least the PBR kit.

## Deferred

- 4.2b: New-from-template menu entry + shelf surfacing (after fable audit ~15:37).
