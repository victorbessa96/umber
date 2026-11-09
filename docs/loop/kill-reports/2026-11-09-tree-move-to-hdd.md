# 2026-11-09 — the Umber tree moves to the HDD (row 296)

- **Author:** dragon (Razul), session
  `local-ee79b5c495f10bbd08339152566beca313239d9318a5ef89447f3a297ceda877`.
- **Kill type:** infrastructure move (no code change). No kill-lock: the
  move touches the tree root, and no code file was edited.

## What moved

| | before | after |
| --- | --- | --- |
| tree | `/home/bessa/Projects/umber` (NVMe) | `/mnt/hdd/data/Projects/umber` (sda1, 869G free) |
| old path | real directory | **symlink** → the new tree |
| `target/` | 76G on NVMe | deleted (see "Cargo artifacts") |
| `tools/icon-raster/target` | 818M on NVMe | deleted (untracked build dir) |

The symlink at the old path is the same pattern Learning-AI already uses
(`/home/bessa/Projects/Learning-AI` → `/mnt/hdd/data/Projects/Learning-AI`).
It keeps every hardcoded reference working without an edit: the Hermes
config's `umber` MCP server (`--read-root /home/bessa/Projects/umber`),
`~/.hermes/scripts/project_watch_state.py` (the 30-minute watch cron),
`scripts/kill-locks.sh`'s repo-relative paths, and the claw briefs' stated
workspace path.

## Why

The NVMe root was at 91% (21G free) with a 76G cargo `target/` in it,
which starved every other project on the box. The rotary sda1 (1TB, 869G
free) takes the tree; the NVMe keeps only the fast, hot artifacts.

## Verification (all read-back, no claim without evidence)

- `rsync -a --checksum --dry-run --delete` over the WHOLE tree including
  `.git/`: **0 differing lines** — byte-identical, nothing missed.
- `git rev-parse HEAD` → `9393c405` before and after.
- `git ls-files` → 1729 tracked files on both sides; `git status
  --porcelain` → the same 13 untracked entries (the sibling's
  `.dragon/briefs` lens files, `l6-review-input.patch`, `runbooks/`, the
  three `watch-centralhost*.sh` scripts, `docs/references/`).
- `git fsck --connectivity-only` on the copy: clean (only dangling trees,
  which are the expected rebase leftovers).
- File counts excluding `.git`: 1864 on both sides.
- `git fetch origin` + `git ls-remote origin master` from the new path:
  `9393c405`, `rev-list --left-right --count origin/master...HEAD` → `0 0`.
  The remote and the working tree agree after the move.

## Cargo artifacts (the disk relief)

- `~/Projects/umber/target` (76G) — deleted. Rebuilds from scratch; the
  warm rebuild on the new location was launched immediately after the
  move (`cargo build --workspace --all-targets` under the repo flock,
  nice 19, `CARGO_BUILD_JOBS=3`).
- `tools/icon-raster/target` (818M) — deleted; it is an untracked,
  gitignored build dir (`tools/icon-raster/.gitignore` = `/target/`).
- `~/.hermes/cache/scratch/ab-target` (2.7G) + `ab-ref-tree` (34M) — the
  A/B perf-gate build trees, deleted; the perf verdict they supported is
  already landed (row 292 / `02b2375d`).
- `/tmp` was at 100% (7.8G) from ~12.9k stale `brain-*` L-AI test dirs;
  cleared the ones older than 120 minutes → 575M used, 8%.

## Carve-outs (named, not done)

- The cargo `target/` now lives on the **rotary** disk. Build wall-clock
  grows; the fast NVMe is no longer the compile target. Accepted: the
  alternative (76G on NVMe) is what caused the pressure. A
  `CARGO_TARGET_DIR` on NVMe + source on HDD is the later option if the
  build-speed tax becomes the binding constraint.
- The git history moved with the tree, so `git gc`/repacks now run on
  sda1.
- `hosttree` (the `ssh://bessa@192.168.15.64/~/Projects/umber` remote) is
  untouched and still points at the host copy; centralhost is still
  STOOD DOWN per Bessa's order, so it was not contacted.

## Process note

The 23-hour-old `scripts/check-layers.py` process (pid 2698998, a dead
session's leftover sitting in `do_wait` on a closed socket) was holding
the old tree's directory inode open through the swap; it was killed after
the copy verified, so the old directory could be removed cleanly.
