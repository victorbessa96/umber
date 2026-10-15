# The build host (centralhost, 192.168.15.64)

Umber's every cargo command runs here (Bessa, 2026-10-15: "use cargo and
tests and builds on the remote machine"). When it is down, nothing builds
and kills park uncommitted. This page is the check-and-recover list.

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
