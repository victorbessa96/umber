# M1 pass 4 — the flatpak-chain rows + the W-C3b docs remnant (2026-10-24, wake session)

Authorship: dragon (autonomous wake cycle, the Umber loop's M1
recurring stale-row sweep lane, pass 4). No kill lock — docs-only,
battery-exempt (the M1-pass-2 precedent: tracking-docs ticks ride the
wave commit; dcommit's `--lock` flags carried the ledger discipline).
Implements: the standing charter's RECORD step (vault
`projects/umber/loop-charter.md`, cycle step 6) + the STATE block's
open-lane line — "the W-C3b docs pass (consume the four manifests +
sentinel-c3b, release-checklist/requirements rows 39/79/111, ledger
sweep→LANDED, M1 pass)" — of which the manifests, the sentinel, and
the requirements rows already landed at the close (`211195f`); this
pass closes the named remnant: the release-checklist .usdz row, the
ledger sweep→LANDED, M1 pass 4's flatpak-chain rows.

## Built

1. `docs/specs/release-checklist-0.1.0.md` — the .usdz known-deferred
   bullet CLOSED (`d36cfb1`'s load_usdz walks the sublayer stack);
   the sweep-note history preserved (the 10-19 five + this one); the
   known-deferred list is down to Windows signing alone.
2. `docs/loop/kill-ledger.md` — the four W-C3b In-flight rows → a
   LANDED section (battery numbers from the close log: mesh 145 +
   core 126 + gpu 151 + app 850 = 1,272); the flatpak five-wall chain
   rows with their run-id citations; the Waiting packaging-verdict
   row marked CONSUMED (answered by 40040567126); the two
   "(the 10-23 docs-pass commit)" placeholders resolved to `f62d112`
   (runner pin) and `f4eff95` (scorecard regen) — the
   citation-variant rule: verified which commit carried the file via
   `git log`, never the wave summary's enumeration.
3. `docs/loop/queue.md` — the W-C3b + chain push-log row (tip
   `3f1f307`; the five red dispatch ids + the green verdict id).
4. `STATE.md` — status line, Next, and the Blocked/waiting Flatpak row
   rewritten to the post-chain truth; the perf-nightly pointer carries
   the latest red (39923126737).
5. `CHANGELOG.md` — the 2026-10-23 W-C3b + chain section.
6. The coordination STATE block rewritten (the M4 discipline).

## Run-id evidence (every row re-verified live from the run objects this pass)

| run | sha | verdict | what it named |
|---|---|---|---|
| 39908210580 | 96edd1e | failure | the runner-pin red: bwrap/rofiles on ubuntu-26.04, pre-command |
| 40017411324 | 089b1a4 | failure | the close push's ci: E0433 on the gpu-feature leg (fixed 07bcd2b) |
| 40017442313 | 089b1a4 | failure | wall 1: the vendor parser vs cranelift-0.137.0-rc.1 |
| 40023752815 | 01f69a6 | failure | wall 2: rustc 1.97 demanded, the 24.08 runtime ships 1.89 |
| 40025359694 | e3ae21d | failure | wall 3: the 26.08 SDK trio not installed |
| 40026754260 | e76be0c | failure | wall 4: the compose icon read through rofiles-fuse |
| 40035240398 | 334af5b | failure | the falsification: no-rofiles, identical compose death |
| 40020097585 | 07bcd2b | success | the wave's real Gate 2 + the c697bab pin's verdict (segv gone) |
| 40040567126 | 3f1f307 | success | THE CHAIN GREEN: Umber.flatpak 12,635,392 bytes uploaded |

Plus ci green at every chain tip: 40023735402, 40025342767, 40026739440,
40034973269, 40035173125, 40040549515.

## Judgment calls

- The wall-4 row keeps BOTH commits (`3824b41` + `334af5b`) and names
  the falsification run inside the row — a corrected attribution that
  hides its own correction would read as if the first diagnosis never
  happened; the rofiles flag stays recorded as orthogonal hardening,
  not deleted.
- The desktop-checks Result lines ×4 stay OPEN (verified empty this
  pass): they need the live desktop; ticking them without eyes would
  be the lazy-tick smell the M1 lane exists to kill.
- No push: docs commits ride the next wave's push (the two-gate
  standing rule; the runway's first wave carries this commit).

## Unverified

- Nothing code-side: this pass touches tracking docs only, and every
  factual row cites a run object or a commit re-verified live this
  session (gh run view + git log; no claim trusted from the prior
  session's summaries).

## Deferred

- The perf-nightly red (39923126737 on `96edd1e`, the drift gate red
  since 10-21) — its own hunt, the runway's named follower; pin vs
  re-baseline is that hunt's call, not made here.
