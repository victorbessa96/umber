---
name: Bug report
about: Something misbehaves
labels: bug
---

**Build** — Umber version or commit (`git rev-parse --short HEAD`), OS,
GPU + driver (or "lavapipe"/"WARP" for software rendering):

**What happened** vs **what you expected**:

**Reproduction** — the smallest mesh + steps that show it. Attach the
mesh (or say which bundled one), and the `.umber` project if the bug
needs your layers/paint:

**Logs** — run with `RUST_LOG=info` and paste the tail (a panic message
and backtrace with `RUST_BACKTRACE=1` if it crashed):

**Screenshots** (UI or viewport bugs):
