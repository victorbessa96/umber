# Brief: 2026-10-19 stencil-wave recovery close — the host returned

Mandate: Bessa, 2026-10-19 09:31 (-03): "The remote machine is back
online. Work on the Umber project. I want you to resume it and work
autonomously."

Trigger: watch-centralhost.sh HOST_BACK probe=1188 at 08:42:55 -03
(boot ~08:43). Boot journal CLEAN this session: zero Hardware Error/MCE
lines (the only grep hit is the kernel's boot-time "MCE decoding
enabled" banner). Uptime 51 min at 09:34, load 0.16, 24G free.
memtest86+ still owed at next physical access; any recurrence = the
same UECC/bank-14 class, treat as hardware, not software.

## Scope

Close the parked stencil wave + the 10-18 kill fleet through
scripts/stencil-wave-close.sh (the executable runbook, 2026-10-18).
13 manifests, 51 pinned rows, ZERO drift (script dry-run verified
2026-10-19 09:5x). Push gate: standing (2026-10-12) + today's
explicit resume order.

## Execution order (the script encodes it; this is the map)

1. Claim the recovery (interlock = last `^RECOVERY` line in
   .dragon/COORDINATION-stencil-wave.md).
2. rsync tree -> host (target/.git/.dragon/Cargo.lock excluded);
   battery sentinel written (HEAD + porcelain sha).
3. Wave battery on the host: umber-app umber-core umber-gpu
   umber-paint umber-cli umber-bake — DEFAULT FEATURES ONLY. Never
   `--features gpu` inside the recovery window (the 94469e8 wgpu
   deadlock class is the one wedge cause under our control, and the
   host's UECC history makes a mid-battery wedge the named risk).
4. Commits by explicit path, per kill row: e0063-refscene first, then
   the stencil wave (core/app/viewport + reports + queue.md + the
   script itself), then the agent kills' combined row, then
   stamp-preset-persist, then flatpak-offline-deps, then the 10-18
   docs-closure rows per the script's step-5 list. Every kill lock
   releases at its own commit.
5. ONE push (Gate 2), explicit run-id resolve, ci-watch detached.
6. Post-push re-dispatches: perf-nightly (e0063's compile gate) +
   packaging workflow (the flatpak kill's verdict gate).

## Discipline

- The battery verdict is ground truth before any commit. RED = stop,
  fix-forward, fresh battery (the sentinel forces it on byte change).
- Host wedges mid-battery: stop, re-park, log the wedge window. The
  tree is sha-pinned; the script is idempotent and re-runnable.
- Step 8 (docs pass: delete consumed notes/manifests, tick
  queue/STATE/CHANGELOG, release the script's own lock) stays manual,
  POST-CI-green.
- Wake sessions: never end a turn expecting the CI ping; the NEXT
  session reads `gh run list` live. Verdict = all jobs; the Windows
  leg is the long pole.

Owner: Razul main session (foreground, Bessa in-seat), 2026-10-19.
Coordination stays additive in .dragon/COORDINATION-stencil-wave.md.
