#!/usr/bin/env python3
"""Crate layering gate: every workspace crate sits in a declared layer,
and a crate may depend only on crates in strictly LOWER layers (or the
same-layer pairs INTRA_LAYER allows). UI crates (egui, eframe, winit,
rfd, egui_dock) stay out of every layer below the shell, so the engine,
the headless CLI and the agent protocol can never quietly grow a window
dependency.

Pure Python over Cargo.toml files (stdlib tomllib, Python >= 3.11): no
cargo, no build, seconds in CI. Absorbed 2026-10-14 from photocraft's
`cargo xtask layers` (xtask/src/layers.rs), reshaped to Umber's crates.

    python3 scripts/check-layers.py          # exit 0 = clean, 1 = violation
    python3 scripts/check-layers.py --print  # the layer table

A NEW CRATE FAILS this gate until it is registered in LAYERS below:
decide its layer on purpose, in the same commit.
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Layer number per crate. Lower = more foundational. A dependency edge
# must point strictly DOWN (dep layer < crate layer).
LAYERS = {
    # L0 — leaves: no workspace deps; standalone, publishable shapes.
    "umber-color": 0,
    "umber-mesh": 0,
    "umber-graph": 0,
    "umber-spp": 0,
    "umber-plugin-sdk": 0,
    "stylus": 0,
    # L1 — document model + format codecs + the plugin host.
    "umber-core": 1,
    "umber-export": 1,
    "umber-wasm": 1,
    # L2 — the GPU layer (renderer, compute, display callbacks).
    "umber-gpu": 2,
    # L3 — GPU consumers: brush engine, bake engine.
    "umber-brush": 3,
    "umber-bake": 3,
    # L4 — the headless paint engine (brush + bake + persistence).
    "umber-paint": 4,
    # L5 — the front ends: the CLI / agent session, the widget kit.
    "umber-cli": 5,
    "umber-gui": 5,
    # L6 — the shell.
    "umber-app": 6,
}

# Same-layer edges allowed on purpose (dependent, dependency).
INTRA_LAYER = set()

# Crates that may pull UI/windowing crates, with the ones they may pull.
# umber-gpu's egui-wgpu is the paint-callback trait the viewport draws
# through (no windowing); umber-gui is the widget kit; umber-app is the
# shell. Everything else stays UI-free.
UI_CRATES = {"egui", "eframe", "winit", "rfd", "egui_dock", "egui-winit", "egui-wgpu"}
UI_ALLOWED = {
    "umber-gpu": {"egui-wgpu"},
    "umber-gui": {"egui"},
    "umber-app": UI_CRATES,
}


def load_crates():
    """name -> (manifest path, normal deps, dev deps) for every member."""
    crates = {}
    for manifest in sorted(ROOT.glob("crates/*/Cargo.toml")):
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        name = data["package"]["name"]
        deps = set(data.get("dependencies", {}))
        dev = set(data.get("dev-dependencies", {}))
        for target in data.get("target", {}).values():
            deps |= set(target.get("dependencies", {}))
            dev |= set(target.get("dev-dependencies", {}))
        crates[name] = (manifest.relative_to(ROOT), deps, dev)
    return crates


def check(crates):
    """Every violation as a human-readable line (empty = clean)."""
    errors = []
    for name in sorted(set(crates) - set(LAYERS)):
        errors.append(
            f"{crates[name][0]}: crate `{name}` has no layer — register it in "
            "scripts/check-layers.py LAYERS (decide its layer on purpose)"
        )
    for name, (path, deps, dev) in sorted(crates.items()):
        if name not in LAYERS:
            continue
        mine = LAYERS[name]
        for dep in sorted(deps & set(LAYERS)):
            theirs = LAYERS[dep]
            if theirs > mine or (theirs == mine and (name, dep) not in INTRA_LAYER):
                errors.append(
                    f"{path}: `{name}` (L{mine}) depends on `{dep}` (L{theirs}) — "
                    "dependencies must point to a lower layer"
                )
        # Dev-deps may reach sideways/down for test fixtures, never UP into
        # a higher layer (a test that needs the shell lives in the shell).
        for dep in sorted((dev - deps) & set(LAYERS)):
            if LAYERS[dep] > mine:
                errors.append(
                    f"{path}: `{name}` (L{mine}) dev-depends on `{dep}` "
                    f"(L{LAYERS[dep]}) — tests may not reach up a layer"
                )
        allowed = UI_ALLOWED.get(name, set())
        for dep in sorted((deps & UI_CRATES) - allowed):
            errors.append(
                f"{path}: `{name}` depends on UI crate `{dep}` — only "
                f"{sorted(UI_ALLOWED)} may (engine/headless crates stay UI-free)"
            )
    return errors


def main():
    crates = load_crates()
    # A registered-but-absent crate is a warning, not a failure: a branch
    # may register a crate another branch is about to add (the gate must
    # not order merges). Stale entries still print on every run.
    for name in sorted(set(LAYERS) - set(crates)):
        print(f"LAYERS WARN `{name}` is registered but not in this tree")
    if "--print" in sys.argv:
        for layer in sorted(set(LAYERS.values())):
            members = sorted(n for n, l in LAYERS.items() if l == layer)
            print(f"L{layer}: {', '.join(members)}")
        return 0
    errors = check(crates)
    for line in errors:
        print(f"LAYERS FAIL {line}")
    if errors:
        print(f"=== layers: RED ({len(errors)} violation(s)) ===")
        return 1
    print(f"=== layers: GREEN ({len(crates)} crates) ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
