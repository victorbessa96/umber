#!/usr/bin/env bash
# recovery-close.sh — the W-GUI-2 outage recovery runbook, executable.
# Encodes docs/loop/queue.md's RECOVERY RUNBOOK so the build host's
# return costs ONE command instead of a careful session. Every step is
# idempotent and re-runnable: it re-checks live state, never assumes a
# prior step's success, and REFUSES to guess past a failure.
#
# Usage: bash scripts/recovery-close.sh [--dry-run]
# Exit codes: 0 = wave closed (battery green, committed, pushed, CI
# watched); 2 = host unreachable; 3 = battery RED (fix-forward, then
# re-run — the script resumes); 4 = a manual-precondition failure.
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
HOST=bessa@192.168.15.64
REPO="$HOME/Projects/umber"
# The manifest's durable home (.dragon/ is gitignored, survives the
# scratch dir's 24h pruning); the scratch copy stays as fallback.
MANIFEST="$REPO/.dragon/wgui2-commit-manifest.txt"
[ -f "$MANIFEST" ] || MANIFEST="$HOME/.hermes/cache/scratch/wgui2-commit-manifest.txt"
BATTERY_LOG="/tmp/wgui2-recovery-battery.log"

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*"; exit "${2:-4}"; }

# --- 0. host liveness ---------------------------------------------------
say "0. build-host liveness"
if ! timeout 8 bash -c "</dev/tcp/192.168.15.64/22" 2>/dev/null; then
    die "centralhost SSH still down — nothing to recover yet. The SSH watcher fires when port 22 answers; re-run this script then." 2
fi
echo "host answers."

# --- 1. preconditions ---------------------------------------------------
say "1. preconditions"
[ -f "$MANIFEST" ] || die "manifest missing: $MANIFEST"
cd "$REPO" || die "repo missing: $REPO"
# The manifest is the add list: every path must exist on disk.
MISS=0
while read -r _hash _path _rest; do
    case "$_hash" in ''|'#'*) continue ;; esac   # header/blank rows
    [ -f "$REPO/$_path" ] || { echo "  MISSING: $_path"; MISS=1; }
done < "$MANIFEST"
[ "$MISS" -eq 0 ] || die "manifest names files not on disk — tree and manifest diverged, reconcile by hand"
# Hash spot-check: 5 random manifest rows vs disk (full hash-diff is the
# commit step's business; this catches wholesale staleness).
STALE=0
i=0
while read -r hash path _rest; do
    case "$hash" in ''|'#'*) continue ;; esac   # header/blank rows
    i=$((i+1))
    [ $((i % 7)) -ne 0 ] && continue
    disk=$(sha256sum "$REPO/$path" 2>/dev/null | cut -c1-16)
    [ "$disk" = "$hash" ] || { echo "  HASH DRIFT: $path (manifest $hash, disk $disk)"; STALE=$((STALE+1)); }
done < "$MANIFEST"
[ "$STALE" -eq 0 ] || echo "  NOTE: $STALE manifest rows drifted (edits after the last manifest refresh) — the commit step commits DISK bytes; verify the drift is yours before proceeding."
grep -q '"approved": true' docs/loop/push-gate.json || die "push gate not approved"
echo "preconditions green."

# --- 2. clean host verify-trees -----------------------------------------
say "2. clean leftover verify trees on the host"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh $HOST 'rm -rf ~/Projects/umber-verify-*'"
else
    ssh -o ConnectTimeout=10 "$HOST" 'rm -rf ~/Projects/umber-verify-*' 2>/dev/null
    echo "  cleaned."
fi

# --- 3. rsync the working tree -------------------------------------------
say "3. rsync tree → host (tracked + uncommitted, NOT git objects)"
RSYNC_CMD="rsync -az --delete --exclude=/target/ --exclude=/.dragon/ --exclude=/Cargo.lock --exclude=/.git/ -e 'ssh -o ConnectTimeout=10' $REPO/ $HOST:Projects/umber/"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: $RSYNC_CMD"
else
    # shellcheck disable=SC2086
    rsync -az --delete --exclude=/target/ --exclude=/.dragon/ --exclude=/Cargo.lock --exclude=/.git/ \
        -e 'ssh -o ConnectTimeout=10' "$REPO/" "$HOST:Projects/umber/" \
        || die "rsync failed — check connectivity" 4
    # Both ends' tips must match:
    local_tip=$(git -C "$REPO" rev-parse HEAD)
    remote_tip=$(ssh -o ConnectTimeout=10 "$HOST" 'cd Projects/umber && git rev-parse HEAD' 2>/dev/null)
    [ "$local_tip" = "$remote_tip" ] || die "host tip $remote_tip != local tip $local_tip — the mirror diverged, reconcile by hand"
    echo "  synced; tips match at $local_tip."
fi

# --- 4. wave battery ------------------------------------------------------
say "4. wave battery (umber-app + umber-gui + spp + umber-core)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh + nohup bash scripts/wave-battery.sh umber-app umber-gui spp umber-core"
    echo "  then poll the sentinel for BATTERY_EXIT="
else
    # Detach ON THE REMOTE per the wave-ops rules (all three streams
    # redirected, stdin included). The battery takes the cargo lock
    # itself — NEVER wrap it in an outer flock.
    ssh -o ConnectTimeout=10 "$HOST" \
        "cd Projects/umber && export PATH=\$HOME/.cargo/bin:\$PATH && rm -f $BATTERY_LOG && nohup bash -c 'bash scripts/wave-battery.sh umber-app umber-gui spp umber-core > $BATTERY_LOG 2>&1; echo BATTERY_EXIT=\$? >> $BATTERY_LOG' </dev/null >/dev/null 2>&1 &" \
        || die "battery launch failed"
    echo "  battery launched (remote log: $BATTERY_LOG)."
    echo "  Poll with: ssh $HOST 'tail -3 $BATTERY_LOG'"
    echo "  The BATTERY_EXIT sentinel decides: 0 → green → step 5; else RED → fix-forward from the log, re-run this script."
fi

# --- 5. commit + push (MANUAL GATE: only after battery green) ------------
say "5. commit-by-path + push (runs ONLY after BATTERY_EXIT=0)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: git add <manifest paths> && git commit && push + ci-watch"
    echo "  Re-run without --dry-run after BATTERY_EXIT=0 to execute."
    exit 0
fi
# Only reachable on a real run; the operator confirms the battery green.
echo "  Confirm the battery: ssh $HOST 'grep BATTERY_EXIT $BATTERY_LOG'"
echo -n "  Battery green? [BATTERY_EXIT=0] y/N: "
read -r ans
[ "$ans" = "y" ] || die "battery not confirmed green — run step 4 first (this script re-runs cleanly)"
# Commit the manifest's paths, by explicit path only.
: > /tmp/wgui2-add-list.txt
while read -r _hash path _rest; do
    case "$_hash" in ''|'#'*) continue ;; esac   # header/blank rows
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/wgui2-add-list.txt
done < "$MANIFEST"
# Untracked kill reports + briefs ride too (the manifest carries them).
mapfile -t files < /tmp/wgui2-add-list.txt
git -C "$REPO" add -- "${files[@]}" || die "git add failed"
git -C "$REPO" commit -m "feat(gui): the W-GUI-2 wave — 13 kills through the host outage (gestures, context rail, first-run, tab overflow, overlays, a11y sweep, density, polish widgets, held-key tools, spp f16, assetstore-gc) + the cross-review fixes" \
    || die "commit failed"
# Release every wave lock:
bash "$REPO/scripts/kill-locks.sh" remove wgui2-umber-gui-widgets 2>/dev/null
bash "$REPO/scripts/kill-locks.sh" remove wgui2-gesture-integration 2>/dev/null
bash "$REPO/scripts/kill-locks.sh" remove wgui2-held-key-tools 2>/dev/null
bash "$REPO/scripts/kill-locks.sh" remove sppv2-r1 2>/dev/null
bash "$REPO/scripts/kill-locks.sh" remove assetstore-gc 2>/dev/null
# Push + resolve the run id OURSELVES:
git -C "$REPO" push origin master || die "push failed"
sleep 10
HEADSHA=$(git -C "$REPO" rev-parse HEAD)
RUN_ID=$(gh run list --repo "$(git -C "$REPO" remote get-url origin | sed 's/.*github.com[:/]//;s/.git$//')" \
    --limit 5 --json databaseId,headSha -q ".[] | select(.headSha == \"$HEADSHA\") | .databaseId" | head -1)
[ -n "$RUN_ID" ] || die "could not resolve the CI run id for $HEADSHA — watch manually"
echo "  pushed; CI run $RUN_ID. Watcher:"
nohup "$REPO/scripts/ci-watch.sh" "$RUN_ID" >/dev/null 2>&1 &
echo "  ci-watch launched on run $RUN_ID."
echo
echo "WAVE CLOSED (pending CI). Docs tick = the next turn's first kill."
