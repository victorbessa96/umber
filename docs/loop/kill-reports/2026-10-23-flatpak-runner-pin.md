# Kill report — 2026-10-23 flatpak-runner-pin (the packaging job's bwrap/rofiles red)

**Authorship:** dragon (wake session, 2026-10-23). **Kill lock:**
`flatpak-runner-pin` — `.github/workflows/packaging.yml`, this report.
**Implements:** the wave-tip packaging dispatch's root-cause fix (run
39908210580, job `build Flatpak bundle (x86_64)`, FAILURE) — the second
runner-rotation red of the day, found while gating the W-C3b close's push
(the first being the ci.yml SIGSEGV, pinned c697bab).

## The evidence chain

1. **The red.** The W-C3a close's post-push packaging re-dispatch
   (39908210580, wave tip 96edd1e, dispatched 02:52 local 10-23) fails at
   the umber module's build setup: all 621 vendored crate sources
   download and extract clean (the archive-type fix's proof — the old
   wall never appears), the module clones master, then
   `Running: python3 flatpak/vendor-checksums.py` is followed by
   `bwrap: Can't find source path
   /home/runner/work/umber/umber/.flatpak-builder/rofiles/rofiles-0GyG0n/files:
   Permission denied` → `Error: module umber: Child process exited with
   code 1`. The command never runs; the sandbox never forms.
2. **The flip is environmental, not content.** The prior dispatch of the
   same job (39306564270, 10-19) got PAST the sandbox and failed on
   content instead — `vendor-checksums: unparseable vendor dir '.cargo'`
   (the bug fixed by 4d69808, now pushed). Between the two dispatches:
   `git log .github/workflows/packaging.yml` names `1b0616c` (the
   offline-deps kill, pre-10-19) as the last workflow change, and the
   manifest bytes changed only by the strip-components fix — nothing
   that touches the bwrap/rofiles layer.
3. **The runner image rotated in exactly that window** (the same rotation
   the ci SIGSEGV hunt convicted): the 10-19 dispatch's Flatpak job ran
   `Image: ubuntu-24.04`; the 10-23 dispatch ran
   `Image: ubuntu-26.04` (version 20261012.163.1). `ubuntu-latest` moved
   LTS major on 2026-10-21; resolute's bwrap/flatpak-builder stack fails
   the rofiles bind that noble's passed.
4. **The push-trigger packaging runs say nothing about this job** — by
   trigger design they skip the Flatpak job (lint+AppImage only), so the
   10-23 docs-tip packaging run 39903281330's green is not evidence
   against this red.

## Built

- `.github/workflows/packaging.yml:154` — the Flatpak job's
  `runs-on: ubuntu-latest` → `ubuntu-24.04`, with the evidence comment
  inline. The AppImage and lint jobs are untouched (green on the wave
  tip, on their own images).

## Judgment calls

- **Pin the image, not the bwrap version.** The green evidence is the
  whole 24.04 image; GitHub-hosted runners do not let a job pin
  apt-package versions ahead of the image anyway. Same ruling as the
  ci.yml check-matrix pin.
- **Pin only the Flatpak job.** The lint job (`ubuntu-latest`, python
  only) and the AppImage job (its own runner) are green on 26.04-era
  images; leaving them on latest keeps the early-warning property the
  segv hunt named.
- **The verdict gate moves to the W-C3b push's re-dispatch.** The W-C3a
  close's dispatch is consumed (ran, red, root-caused here); the flatpak
  chain has never been green end to end and this pin un-gates its next
  attempt, which the W-C3b local close's post-push step dispatches on
  the pinned tip.
- **perf-nightly is NOT pinned here either.** Same reasoning as the segv
  hunt: the drift-gate red is that hunt's call (pin vs re-baseline), with
  the rotation as its opening hypothesis.

## Unverified

- The pinned run itself: this commit rides the W-C3b close's push (one
  push per wave); the post-push packaging re-dispatch on the pinned tip
  is this kill's verdict gate. Until it runs, the pin is evidence-based,
  not run-verified.
- The 26.04 bwrap/rofiles failure's exact mechanism (rofiles-fuse
  vs /dev/fuse vs a flatpak-builder behavior change) is not diagnosed —
  the pin is the fix, the mechanism is the named follow-up if 26.04
  must ever carry this job again.

## Deferred

- Diagnosing resolute's bwrap stack: not our lane while 24.04 runs the
  job; revisit under the pin's revisit conditions.

## Process note

- Found while pre-flighting the W-C3b close's push surface: the
  coordination block's recent-events named the docs-tip CI red (the
  segv) but NOT the wave-tip packaging dispatch's red — this hunt's
  census caught it because the close's push would have inherited an
  unexplained red verdict gate. No cargo ran in this kill; the evidence
  is run objects + logs + a clean-tip worktree probe.
