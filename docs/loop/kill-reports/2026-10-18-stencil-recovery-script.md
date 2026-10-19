# Kill report: stencil-recovery-close (wave tooling — the audit's "automated recovery script")

- Contract: `.dragon/COORDINATION-stencil-wave.md` "Recovery runbook"
  steps 1-6, the audit's ranked improvement
  (docs/loop/audit-2026-10-17-full-project.md: "automated recovery
  script"), and `scripts/recovery-close.sh` as the structural precedent.
  NOT a replacement for recovery-close.sh (W-GUI-2 strings; untouched).
- Kill-lock `stencil-recovery-close`: `scripts/stencil-wave-close.sh`
  only. Releases at the wave close's docs pass.
- HEAD at build: `ff9ac02` (tree unchanged by this kill except its own
  file + this report + the queue row).

## Headline

The freeze-3 recovery runbook's named hazard is the MINUTES-wide alive
window after a power-cycle, against a seven-step manual sequence. This
kill makes the close ONE command:

    nohup bash scripts/stencil-wave-close.sh > /tmp/stencil-close.log 2>&1 &

The first session to see HOST_BACK fires it; the script claims the
recovery on the coordination note itself, refuses every double-fire
interlock, and walks claim → preconditions → host pre-flight → rsync →
battery → commits → push → ci-watch → perf-nightly re-dispatch without
a human in the loop.

## Built

`scripts/stencil-wave-close.sh` (~250 lines), the runbook encoded:

- **Step 0/0b**: host liveness (port 22, exit 2 while down) + a claim
  interlock on the coordination note — the note's LAST line-start
  `RECOVERY CLAIMED|RELINQUISHED|CLOSED` decides; a live foreign claim
  refuses (exit 4), a real run appends its own claim, every post-claim
  failure appends a RELINQUISH line (idempotent re-runs).
- **Step 1**: BOTH manifests (`stencil-wave-recovery-manifest.txt` 16
  rows, `e0063-refscene-manifest.txt` 2 rows) FULLY sha-verified —
  drift is a hard exit 4 with the re-pin instruction (the wave's
  "reconcile before anything else" contract). Push gate checked. The
  four wave locks censused. HEAD vs origin/master divergence reported.
- **Step 2**: host pre-flight over SSH — refuses (exit 4) if a
  cargo/battery is already LIVE on the host (a sibling or Bessa
  mid-recovery), prints tip/porcelain otherwise.
- **Step 3**: `rsync -az --delete` with `/target/ /.git/ /.dragon/
  /Cargo.lock` excluded, then a battery sentinel
  (`.dragon/battery-sentinel`: HEAD + porcelain sha) — the identity of
  the bytes the battery sees.
- **Step 4**: `wave-battery.sh umber-app umber-core umber-gpu`
  (default features ONLY — the header names the 94469e8 wgpu deadlock
  as the one wedge cause under our control and forbids `--features
  gpu` in the window). Detached remote nohup with the BATTERY_EXIT
  sentinel; bounded poll (30 s × 180 = 90 min cap, each probe its own
  short SSH); an EXISTING verdict is reused only while the sentinel
  matches the current tree — a fix-forward forces a fresh battery, a
  mid-poll script death resumes the finished verdict instead of
  double-launching. RED → relinquish + exit 3 with the fix-forward
  instruction.
- **Step 5**: TWO commits — e0063 as its own row (its manifest's 2
  paths, lock released after), then the stencil wave commit-by-manifest
  (16 manifest paths + the two remaining kill reports + queue.md +
  this script + this report, sort -u'd; the three stencil locks
  released after). One push (the one-push rule holds), explicit run-id
  resolve from headSha, ci-watch detached (CI_VERDICT= pattern).
- **Step 7**: `gh workflow run perf-nightly.yml --ref master` — the
  e0063 report's post-push duty (the nightly's build step is
  ref_scene.rs's only compile gate).
- Closing print: the docs-pass checklist (delete consumed
  notes/manifests/sentinel, tick queue/STATE/CHANGELOG, release this
  kill's own lock, the gate_plane paint-hook as the named next kill).

## Judgment calls

1. **The rsync diverges from the note's step 3** (adds W-GUI-2's
   /.git/ /.dragon/ /Cargo.lock exclusions): `--delete` on .git would
   clobber the host's host-specific git config and objects for zero
   benefit (the battery reads the tree, not git). The host's .git
   stays behind; recorded in the coordination note 13:2x. The tip-match
   assertion from recovery-close.sh is DROPPED for the same reason
   (host .git is stale by design post-sync).
2. **The claim interlock is line-start anchored** — the runbook's own
   quoted example ("RECOVERY CLAIMED by <session>") is mid-line
   documentation; the first grep draft would have refused every run
   on the note's own text. Found by unit-testing the grep before the
   dry-run (3/3 cases below).
3. **Sentinel-gated verdict reuse** over recovery-close.sh's
   unconditional re-launch: a poll-exhausted run whose battery later
   finishes must NOT re-battery (the bytes did not change), but a
   RED-fix-forward re-run MUST (they did). HEAD + porcelain-sha of the
   dirty set distinguishes the two without touching git plumbing.
4. **Battery scope is the three manifest crates** (umber-app,
   umber-core, umber-gpu). The sibling's agent-brush-radius kill
   (umber-paint + umber-cli) is in NO manifest yet — when it pins and
   announces, the recovery session extends the scope; the script
   leaves CRATES as a single named variable for that edit.
5. **No `set -e`** (recovery-close.sh precedent): every failure is an
   explicit die/fail_claimed with the interlocks intact; a trapless
   script with explicit relinquish calls keeps the note's claim state
   honest on every exit path.
6. **Exit codes match the template**: 0 closed-pending-CI, 2 host
   down, 3 battery RED/poll-exhausted, 4 interlock/precondition.
7. **Docs pass stays manual** (step 8 print): deleting consumed
   coordination state and ticking STATE/CHANGELOG mechanically from a
   script would launder the record — the sessions' note discipline
   owns that pass.

## Unverified

- **The real run is host-gated by design** — nothing past step 1 has
  executed against a live host: no rsync, no battery launch/poll, no
  commit/push/ci-watch/perf-nightly leg. The dry-run covers steps
  0-4's decision logic and prints 5-7's plan.
- Step 2/3/4's SSH commands are adapted from recovery-close.sh's
  proven shapes (same host, same nohup pattern, same log-sentinel
  convention) but were never fired.
- `mapfile`/process-substitution shapes are bash-only (the repo runs
  everything through bash; `bash -n` passed).
- No shellcheck on this machine (checked); `bash -n` + the dry-run +
  the grep unit cases are the static gates that ran.

## Gates run

- `bash -n`: **PASS**.
- **Dry-run ×2: byte-identical outputs, exit 0 both** (idempotent —
  no state mutation on the dry path).
- The script's own `verify_manifest` ran live in the dry-run:
  **18/18 rows, 0 drift** across both manifests (also an independent
  re-verification of the parked bytes, matching the coordination
  note's twice-verified claim).
- Claim interlock grep, unit-tested 3/3 against a scratch COPY of the
  note (never a fake claim in the real note): live CLAIMED detected →
  RELINQUISHED clears → the real note (runbook quote present) reads
  as no-claim.
- Push gate read live: approved (standing).
- Mode 755 aligned to recovery-close.sh via chmod --reference.
- Lock registered: `stencil-recovery-close` → 1 file, no conflicts.

## Addendum (2026-10-18 14:2x, wakeup #2 — the agent-row extension)

The sibling session pinned and announced TWO more parked kills
(agent-brush-radius + agent-export-naming — one combined recovery row
per its Reconciliation: agent.rs carries both kills, and one file
cannot split across commit rows). This script now encodes them so the
host's return stays ONE command:

- step 1 verifies ALL FOUR manifests (16 + 2 + 5 + 3 = 26 rows);
- CRATES grew to umber-app umber-core umber-gpu umber-paint umber-cli
  (one battery naming every crate any parked kill touches);
- new step 5c commits the agent kills' combined row (the union of
  their two manifests' paths, 8 unique) and releases both agent locks
  at that commit;
- the extension pattern for future pinned kills is documented in the
  script header: add the manifest to step 1, the paths to a commit row
  before the push, the crates to CRATES, the locks to the release list.

Judgment call 4 above ("battery scope is the three manifest crates")
is superseded by this addendum. Gates re-run after the extension:
bash -n, dry-run ×2 byte-identical, all four manifests verified live
(26/26 rows, 0 drift), claim-grep unit cases unchanged and passing.

## Addendum 2 (2026-10-18 14:5x, wakeup #3 — the stamp row + the checker pitfall)

- v3: stamp-preset-persist encoded — MANIFEST5 verified in step 1
  (28 rows total across the five manifests), its own commit row 5d
  with the stamp lock released there. The recovery remains ONE
  command for all five parked kills.
- verify_manifest gained the pitfall doc: the house pin convention is
  a TRUNCATED 16-hex sha256 prefix, invisible to `sha256sum -c` (it
  format-rejects the whole file — "no properly formatted checksum
  lines found"); a silent or DRIFT-reporting sweep from that tool is a
  format artifact, not drift. This parser is the correct checker. The
  false-DRIFT sweep that surfaced it is recorded in the coordination
  note 14:4x and the umber-wave-ops skill (Build-host outage section).
- Gates re-run after this extension: bash -n, dry-run ×2
  byte-identical, all five manifests verified live (28/28 rows,
  0 drift), claim-grep unit cases unchanged and passing.

## Post-landing duties (the recovery session's checklist, unchanged)

1. HOST_BACK → `nohup bash scripts/stencil-wave-close.sh > /tmp/stencil-close.log 2>&1 &`,
   tail the log; the script self-documents every refusal.
2. CI green → the docs pass (step 8 print): delete the consumed
   COORDINATION-stencil-wave.md + COORDINATION-wgui2-close.md + both
   wave manifests + the battery sentinel; tick queue.md (this section
   → CLOSED), STATE.md, CHANGELOG.md; release this kill's lock.
3. The gate_plane paint-hook kill (umber-paint) follows, once
   agent-brush-radius releases paint_state.rs.

## Addendum 3 (2026-10-18 15:4x, wakeup #5 — the flatpak row closes the OWE)

v4: MANIFEST6 verified in step 1 (35 rows total across six manifests), the
flatpak-offline-deps commit row 5e with its lock release there, and the
post-push packaging workflow re-dispatch (its verdict gate) beside
perf-nightly. The recovery is ONE command for all EIGHT parked kills.
Gates re-run: bash -n, dry-run x2 byte-identical, all six manifests
verified live (35/35 rows, 0 drift).

## Addendum 4 (2026-10-18 16:1x, wakeup #12 — the kittest row)

v5: the ninth parked kill (kittest-layers-breadth, the sibling's) is
encoded — MANIFEST7 verified in step 1 (37 rows across seven
manifests), own commit row 5f with its lock release. Recovery = ONE
command for all NINE parked kills. Gates: bash -n, dry-run x2
byte-identical, 37/37 rows 0 drift live.

## Addendum 5 (2026-10-18 16:3x, wakeup #15 — the cross-review's finding consumed)

The sibling session's 16:2x cross-review (all nine rows, verdict
ENCODING VERIFIED) found one low-severity gap, and the fix is in: the
commit-RESUME gap. Every commit row is now guarded with
`git diff --cached --quiet ||` (an already-committed row is skipped
cleanly instead of dying at "commit failed"), and the battery
sentinel is refreshed after the last commit so a failed PUSH + re-run
reuses the verdict instead of re-batterying. The script's "re-run
resumes" promise now holds end to end: battery half (sentinel), commit
half (guards), push half (idempotent git push). Gates re-run: bash -n,
6 guards verified, dry-run x2 byte-identical, 37/37 manifest rows.

## Addendum 6 (2026-10-18 17:0x — the lock-release finding consumed)

The sibling 16:5x finding (self-corrected: the design-doc half retracted,
the lock-release half standing) is fixed: the 5b release loop now releases
stencil-hook-design at the wave commit, and the lesser point taken too —
MANIFEST8 gated in step 1 (eight manifests, 41 rows). Gates: bash -n,
dry-run x2 byte-identical, 8/8 manifests 0 drift, release-loop + census
verified by the 10-check harness.

## Addendum 7 (2026-10-19 01:4x — the overnight rows)

The sibling landed three kills overnight (per-set-stacks per its lane, the
masked-bakes-ray-half CODE kill, the falloff doc closure); the two owed rows
are encoded: the ray-half gets its OWN guarded 5g row (battery scope
umber-app + umber-bake — umber-bake JOINS the battery CRATES this extension,
its GPU occlusion test runs at recovery) and the falloff closure rides the
wave commit's 5b extras. THIRTEEN manifests, 53 rows, zero drift, verified
live; dry-run x2 idempotent; both new locks censused + released at their
rows. Re-pinned.

## Addendum 2026-10-19 (recovery session, Razul main)

Step-2 host pre-flight had a deterministic false positive: the probe was a
compound ssh command whose own bash -c wrapper contains the literal
'cargo', so pgrep -af cargo matched the wrapper and read a live battery
that did not exist — first live fire (10:03) refused on a clean idle host.
Fix: pgrep -ax cargo || pgrep -ax rustc (exact comm match; the wrapper's
comm is bash). Probe live-tested on the idle host before re-fire: prints
'no cargo running' clean. Claim lifecycle proven: CLAIMED 10:03:59,
RELINQUISHED 10:04:00 — interlock clean.

Addendum 2 (2026-10-19, same session): the verdict-reuse path was vacuous —
step 3 rewrote the sentinel from the current tree, then step 4 compared the
same tree against it (always a match), so a consumed RED verdict was reused
forever; and the state hash (HEAD + porcelain path list) was byte-blind to
fix-forwards that edit already-dirty files. Fix: capture the launch-time
sentinel before the overwrite, and tree_state now hashes every dirty file's
bytes over the exact rsync surface (.dragon/ and Cargo.lock excluded).
Harness-verified 5/5 on the shipped function bytes: stability, byte-sensitivity
on an already-dirty file, .dragon exclusion, byte-exact revert, syntax.
