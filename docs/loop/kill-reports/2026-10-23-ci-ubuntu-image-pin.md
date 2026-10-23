# CI ubuntu image pin — the runner-rotation SIGSEGV

**Authorship:** dragon (wake session local-33ac9db9, 2026-10-23).
**Kill lock:** ci-ubuntu-image-pin — `.github/workflows/ci.yml`, this report.
**Implements:** the coordination note's post-close first kill (THE SEGV HUNT
lane); hunt row d26491a1. Charter: vault `projects/umber/loop-charter.md`.

## Built

- `.github/workflows/ci.yml` — the `check` job's matrix pinned
  `os: [ubuntu-latest, windows-latest]` → `os: [ubuntu-24.04, windows-latest]`
  (ci.yml:59-68), with the evidence comment inline.

## The evidence chain (inline; the scratch CI logs are the raw source)

1. **The red.** Both 10-23 ubuntu legs (docs tip e610ed7, run 39903281308,
   job 120249451291; wave tip 96edd1e, run 39908194347) die with
   `process didn't exit successfully: target/debug/deps/umber-962ed9b619a5dbb9
   (signal: 11, SIGSEGV: invalid memory reference)` mid-suite.
2. **The flip is environmental, not code.** Green through 10-20, red from
   10-21, on zero code change (the recorded nightly pattern). The runner
   image rotated in exactly that window:
   - green 10-19 leg: `Image: ubuntu-24.04`, apt installs
     `mesa-vulkan-drivers 25.2.8-0ubuntu0.24.04.4` (noble-updates);
   - both red 10-23 legs: `Image: ubuntu-26.04`, apt installs
     `mesa-vulkan-drivers 26.0.8-1ubuntu0.3` (resolute-updates, amd64v3).
   `ubuntu-latest` moved LTS major (24.04 → 26.04) and the lavapipe stack
   moved a full mesa major (25.2 → 26.0).
3. **Not reproducible on the known-good stack.** The full local lavapipe run
   (attempt 3, this session): `cargo test -p umber-app --bin umber --
   --test-threads=1` with `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`,
   flock + nice 19 + -j1 envelope:
   `test result: ok. 635 passed; 0 failed; 0 ignored; finished in 8.59s`.
   The tower itself runs `mesa-vulkan-drivers 25.2.8-0ubuntu0.24.04.2` —
   the SAME 25.2.8 family the green leg installed (25.2.8-0ubuntu0.24.04.4);
   the local repro exercised the known-good stack, not a third variant.
   (635 vs the pushed tip's 611: the current worktree is the W-C3b superset;
   every test ran — zero skipped.)
4. **The crash WANDERS.** Last passing tests before the segv:
   `brush_panel::tests::*` in 39903281308; `bakes_panel::tests::*` in
   39908194347. A non-deterministic crash site inside the same binary is the
   corruption/broken-runtime shape, not a single buggy test — which is why
   no local test "names the crash": there is no code-side crash to name.

## Judgment calls

- **Pin the image, do not apt-pin mesa inside 26.04.** The green evidence is
  the whole 24.04 image + its repo's mesa; pinning `runs-on` reproduces it
  exactly. Installing noble's mesa on resolute is cross-release surgery —
  fragile and unproven. Rejected.
- **Pin only the `check` job.** `gates` (layering) and `changes` (classifier)
  also ride ubuntu-latest and both ran GREEN on 26.04 (python/git only).
  Leaving them on latest keeps them tracking the current image as early
  warning for the next rotation. The red is exclusively the vulkan-bearing
  job.
- **windows-latest untouched.** No evidence of a windows problem; the segv is
  the Linux lavapipe stack.
- **perf-nightly NOT pinned here.** `perf-nightly.yml:30` also rides
  `ubuntu-latest`, and the drift-gate flip on 10-21 (the separate PERF red)
  is plausibly the SAME rotation — but whether its fix is a pin or a
  re-baseline is that hunt's call with its own evidence. Named here as its
  opening hypothesis.
- **Revisit conditions.** Un-pin when 26.04's lavapipe stack is fixed
  upstream, or a local repro of the 26.04 stack makes a code-side fix
  possible, or ubuntu-24.04 approaches runner retirement (22.04's retirement
  is the precedent — the pin buys years, not forever).

## Unverified

- The pinned-image CI run itself. This commit rides the W-C3b close's push
  (one push per wave); until that CI runs, the pin is evidence-based, not
  run-verified. The W-C3b wave CI's ubuntu leg is this kill's verdict gate.

## Deferred

- Clean-tree (96edd1e) local control: skipped deliberately. A cold -j1
  compile of the whole dep tree costs hours for marginal value — the 10-21
  zero-code-change flip already proves environment-causation, and the local
  dirty-tree run is a test superset.
- Upstream identification of the mesa 26.0.8 defect: not our lane; if the
  wander-shape needs narrowing later, the runner's coredump route is the
  follow-up.
- A runner-env guard step (probe the lavapipe version, fail with a named
  message): rejected — more moving parts than the pin for the same
  protection.

## Process note

- The local repro took three attempts. Attempts 1+2 died mid-compile to
  systemd-oomd — the re-fire recipe ran default-j with the shield applied
  post-launch, both envelope violations (attempt 2: journal 08:44:03,
  "systemd-oomd killed 9 process(es) in this unit", result oom-kill). Attempt
  3 ran DETACHED as transient units — supervisor `umber-segv-repro` launching
  inner `umber-segv-tryN` units each carrying their own 8G/2G shield at
  launch, nice 19, -j1, under the cargo flock — and the oomd-kill class
  became a non-event: try 1 completed compile+run in 2m51s,
  `SEGV_REPRO_VERDICT=PASS_ALL`. The inner-unit-isolation + supervisor-retry
  shape is the reusable pattern for any oomd-prone long run on this box
  (scripts: /tmp/umber-segv-{try,supervisor}.sh, captured also in the
  coordination note).
