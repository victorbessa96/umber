# Workflow: parallel claw lanes — 2026-11-05 (ratified by Bessa: 2-5 parallel workers OK)

## Lane map (avoids file conflicts; flock serializes cargo, dcommit path-limited prevents cross-commits)

| lane | owns | examples |
|---|---|---|
| mesh | crates/umber-mesh | hostile-input sweep, fuzz battery, mesh import/export |
| app | crates/umber-app/src/{main,toolbar,commands,control,actions,bindings_v2,...}.rs, assets/i18n | row 154 waves, control channel |
| cli | crates/umber-cli | agent steps, MCP catalog, serve/bridge |
| gpu/paint | crates/umber-gpu, umber-paint, umber-bake | UV G-buffer, bake, blend catalog |
| docs/scripts | docs/, scripts/, assets/fonts, STATE.md, queue | briefs, kill reports, queue rows, tooling |

Dragon (Razul) owns: cross-crate wiring, wave-close battery across all touched
crates, the push, and dragon review of every claw commit before it rides a push.

## Claw task template (every dispatch includes these verbatim)

1. Repo + branch + tip; lane; exact file scope ("do NOT touch X").
2. Exact verify env: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_BUILD_JOBS=3 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0` under `flock /tmp/umber-cargo.lock`, `cargo fmt && clippy -p <crate> --all-targets -- -D warnings && cargo test -p <crate>`.
3. Commit: `git add <lane paths>` then `python3 .dragon/dcommit.py --path-limited --lock <paths> -- -m "..."`; NEVER bare git commit; NEVER push.
4. Report: commit SHA, battery totals, gaps found (report, don't fix across lanes).

## Battery discipline

- One-stack rule LIFTED (Bessa, 2026-11-05): Umber cargo no longer waits on
  other territories' cargo. `flock /tmp/umber-cargo.lock` still serializes
  Umber's own cargo runs (they share `target/`). Check `df -h /tmp` before a
  battery: /tmp is a 7.8G RAM tmpfs and filled to 100% on 2026-11-05.
- Claws: targeted crate battery only (fast feedback).
- Wave close (dragon): full multi-crate battery (the 8-crate pattern), fmt --check,
  layers gate, then ONE push for the wave (2026-10-14 alignment).
- Every code turn ends with one ad-hoc `hermes-verify-*` tempfile script run under
  the scratch dir + cleanup, labeled ad-hoc (the verification hook requires it).

## New automation enabled by row 154 w2c (command.list/command.run)

- `scripts/control_call.py` drives the live GUI by id over loopback.
- Pending probe (claw, 2026-11-05): headless xvfb/lavapipe run -> scripted
  end-to-end smoke (methods, command.list, command.run, viewport.screenshot,
  clean SIGTERM). If it works, promote to a CI job and desktop-check 5 goes headless.
- hostile_fuzz (#[ignore], 200k iters) as a separate long/nightly CI job;
  hostile_corpus stays always-on in unit tests.

## Project suggestions (priority order)

1. Row 154 completion (w4 wiring + w5 adapters) — GUI agent-drivable, menus/shortcuts/agent converge.
2. Assets panel + starter content — the roadmap's named moat gap (brief 2026-10-24-rc1-assets-panel.md).
3. UV G-buffer single source of truth (row 156) once the mesh lane frees.
4. Row 155 rmcp swap + cap-std hardening (cli lane).
5. Packaging legs (row 156) — release lane itself stays Bessa-gated (SPEC ratification, signing, alpha tag).
