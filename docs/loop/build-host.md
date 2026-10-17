# The build host (centralhost, 192.168.15.64)

Umber's every cargo command runs here (Bessa, 2026-10-15: "use cargo and
tests and builds on the remote machine"). When it is down, nothing builds
and kills park uncommitted. This page is the check-and-recover list.

## The 2026-10-16 outage — root cause: hardware, not software

Forensics from the dead boot's journal (`journalctl -b -1`), run
2026-10-17 ~05:30 after the box answered again:

- **10:15:40** — the kernel logged a machine check:
  `[Hardware Error]: System Fatal error.` CPU:1 (19:21:0),
  MC14_STATUS `[Over|UE|MiscV|AddrV|PCC|SyndV|UECC|Deferred|-|Scrub]`,
  bank 14 (reserved), cache level RESV, tx: INSN. **UECC = an
  uncorrectable ECC error**; `Deferred` marks the scrub pending.
- **18:32:16** — the last journal line of that boot. After it: no
  shutdown record, no panic, no OOM, no thermal trip, nothing. The box
  froze silently mid-flight, ~8h16m after the MCE.
- The freeze signature matches the MCE class: NIC answering ARP while
  ICMP and sshd die = a wedged kernel, not a lost LAN.
- `last -x` shows a **Sep-30 "crash"** marker too — this box has now
  hard-frozen twice in ~2.5 weeks.
- The kernel cmdline runs `nowatchdog` — a hard hang can never
  self-recover; only a physical power-cycle brings it back.

**What this means:** the 9-hour outage was a hardware fault (likely
RAM or IMC — the bank-14/UECC signature), not a software wedge. The
2026-10-15 outage (see below) may share the cause.

**Recommended (for Bessa, next physical access):**

1. `memtest86+` a full pass (or several) — UECC points at RAM first.
2. If the board supports it, install `rasdaemon` (absent on the box
   today) + `ras-mc-ctl --summary` for accumulated error history.
3. Watch for recurrence: any new `Hardware Error` line in
   `journalctl -k` = the same fault class coming back. One MCE +
   one freeze is a warning; two is a pattern.
4. Kernel cmdline `nowatchdog`: consider dropping it so a hang at
   least panics + reboots instead of sitting dead (it was added for
   a reason — check what reason before removing).

## The 2026-10-15 outage

- **~18:43 -03:** mid-battery, SSH refused (port 22 closed) while the
  machine still answered ping.
- **Minutes later:** no ping at all; ARP still showed the MAC
  (`18:c0:4d:97:a7:41`) as REACHABLE from the dev machine at first, which
  points at the OS going down rather than the network.
- A watcher polled every 15 s for an hour.

## When it comes back (Bessa, at the box or over SSH)

```bash
# why it went down: the previous boot's last minutes
journalctl -b -1 -n 200 --no-pager
journalctl -b -1 -p err --no-pager | tail -50
# an OOM kill or a thermal trip say so plainly:
journalctl -b -1 --no-pager | grep -iE "out of memory|oom|thermal|critical temperature|watchdog|panic" | tail
last -x | head          # shutdown/reboot/crash markers
uptime
```

What the patterns mean:

- `Out of memory: Killed process … cargo/rustc`: parallel test binaries
  plus a release build exceeded RAM. Fix: lower the battery's
  `--test-threads`, or add swap.
- thermal / `critical temperature`: sustained all-core builds. Fix:
  clean the cooling; cap the build parallelism (`CARGO_BUILD_JOBS`).
- **`[Hardware Error]` / UECC / Machine check**: hardware fault — the
  2026-10-16 root cause (section above). Memtest next physical
  access; treat any recurrence as the same pattern.
- Nothing logged, a hard cut: power. Fix below.

## Auto power-on (approved by Bessa 2026-10-15)

In the BIOS/UEFI: **Restore on AC power loss = Power On** (also called
"AC Back" or "After Power Failure"). A power blip then restarts the host
by itself; sshd is enabled at boot.

## After any outage

- `ls ~/Projects | grep hermes-verify`: remove only the dragon's
  `hermes-verify-*` copies, by exact name.
- The shared target dir (`~/Projects/umber/target`) survives; the next
  battery is incremental.
- `/tmp/umber-cargo.lock`: a stale flock file is harmless (flock locks
  the open file, not its existence).

## The .spp corpus (armed 2026-10-15)

`crates/spp/tests/corpus.rs` reads `~/corpora/umber/spp/` ON THE MACHINE
RUNNING THE TEST. The armed corpus (3 Painter projects, 1.3 GB, plus
`manifest.json`) lives on the dev machine only. On the host the
directory is absent, so the test skips, unless it is copied over once:

    rsync -a --partial ~/corpora/umber/spp/ bessa@192.168.15.64:corpora/umber/spp/

These are Bessa's private projects: LAN copy only, never committed or
uploaded (`MANIFEST-README.md`). CI never sees them: the test skips
there by design.
