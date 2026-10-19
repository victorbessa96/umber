#!/usr/bin/env bash
# stencil-wave-close.sh — the freeze-3 outage recovery runbook, executable.
#
# Encodes .dragon/COORDINATION-stencil-wave.md's "Recovery runbook"
# (steps 1-6) so the build host's return costs ONE command instead of a
# careful minutes-long manual session. This is NOT recovery-close.sh —
# that file carries the W-GUI-2 wave's manifest and strings (the queue's
# SUPERSEDED warning); this one carries the stencil wave's.
#
# Encodes EVERY parked kill's row as of 2026-10-18 15:4x:
#   - the stencil wave (stencil-core/app/viewport, by manifest)
#   - e0063-refscene (its own row, by its manifest)
#   - the agent kills' ONE combined row (agent-brush-radius +
#     agent-export-naming, by their two manifests — agent.rs carries
#     both kills; one file cannot split across commit rows)
#   - stamp-preset-persist (its own row, by its manifest)
#   - flatpak-offline-deps (its own row, by its manifest — no battery
#     scope; the packaging workflow re-dispatch is its verdict gate)
# A kill pinned AFTER this version rides the next session's extension:
# add its manifest to step 1, its paths to a commit row before the push,
# its crates to CRATES, its locks to the release list.
#
# Steps (idempotent, re-runnable; refuses to guess past any failure):
#   0. host liveness (port 22) + the note's claim interlock
#   1. preconditions: ALL SIX manifests fully hash-verified, push gate
#      approved, wave locks present
#   2. host pre-flight: refuses if a cargo/battery is already live on
#      the host (a sibling or Bessa is mid-recovery)
#   3. rsync the tree (target/.git/.dragon/Cargo.lock excluded — the
#      host's git config is host-specific; --delete on .git would
#      clobber it. Divergence from the note's step 3, recorded there),
#      then write the battery sentinel (HEAD + porcelain hash)
#   4. wave battery on the host: umber-app umber-core umber-gpu
#      umber-paint umber-cli (default features ONLY — never
#      --features gpu inside the recovery window; the wgpu deadlock
#      class, 94469e8, is the one wedge cause under our control).
#      Bounded BATTERY_EXIT poll, each probe its own short ssh; an
#      existing verdict is reused only while the sentinel matches the
#      tree (a fix-forward forces a fresh battery).
#   5. commits, by explicit path only: e0063-refscene as its OWN row,
#      then the stencil wave commit-by-manifest (+ both remaining kill
#      reports + queue.md + this script + its report), then the agent
#      kills' combined row, then stamp-preset-persist, then
#      flatpak-offline-deps; every lock releases at its own commit
#   6. ONE push, explicit run-id resolve, ci-watch detached
#   7. post-push re-dispatches: perf-nightly (the e0063 report's duty —
#      the nightly's build step is ref_scene.rs's only compile gate) +
#      packaging (the flatpak kill's verdict gate)
# The step-8 docs pass (delete the consumed notes/manifests, tick
# queue/STATE/CHANGELOG, release this script's own lock) stays manual,
# post-CI-green — printed at the end.
#
# Usage:
#   nohup bash scripts/stencil-wave-close.sh > /tmp/stencil-close.log 2>&1 &
#   (the battery poll can run ~90 min — run DETACHED, then tail the log)
#   bash scripts/stencil-wave-close.sh --dry-run
#   (host-free: verifies everything checkable now — claim state, all
#   manifests, push gate — and prints the plan; changes nothing)
#
# Exit codes: 0 wave closed (pending CI); 2 host down; 3 battery RED or
# poll exhausted; 4 interlock/precondition refusal.
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
HOST=bessa@192.168.15.64
REPO="$HOME/Projects/umber"
NOTE="$REPO/.dragon/COORDINATION-stencil-wave.md"
MANIFEST="$REPO/.dragon/stencil-wave-recovery-manifest.txt"
MANIFEST2="$REPO/.dragon/e0063-refscene-manifest.txt"
MANIFEST3="$REPO/.dragon/agent-brush-radius-manifest.txt"
MANIFEST4="$REPO/.dragon/agent-export-naming-manifest.txt"
MANIFEST5="$REPO/.dragon/stamp-preset-persist-manifest.txt"
MANIFEST6="$REPO/.dragon/flatpak-offline-deps-manifest.txt"
MANIFEST7="$REPO/.dragon/kittest-layers-breadth-manifest.txt"
MANIFEST8="$REPO/.dragon/stencil-hook-design-manifest.txt"
MANIFEST9="$REPO/.dragon/pcpool-design-review-manifest.txt"
MANIFEST10="$REPO/.dragon/dynmat-design-closure-manifest.txt"
MANIFEST11="$REPO/.dragon/masked-bakes-design-closure-manifest.txt"
MANIFEST12="$REPO/.dragon/masked-bakes-ray-half-manifest.txt"
MANIFEST13="$REPO/.dragon/falloff-design-closure-manifest.txt"
BATTERY_LOG="/tmp/stencil-recovery-battery.log"
SENTINEL="$REPO/.dragon/battery-sentinel"
CRATES="umber-app umber-core umber-gpu umber-paint umber-cli umber-bake"
BATTERY_POLL_SECONDS=30
BATTERY_POLL_MAX=180    # 180 x 30s = 90 min cap

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*"; exit "${2:-4}"; }
append_note() { printf '\n%s\n' "$*" >> "$NOTE"; }

claim_state() {
    # The note's LAST claim-ish line decides: a bare CLAIMED with no
    # later relinquish/close means someone owns the recovery. Anchored
    # to line start — the runbook's own quoted example ("RECOVERY
    # CLAIMED by <session>") is mid-line documentation, not a claim.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
relinquish() {
    append_note "RECOVERY RELINQUISHED by stencil-wave-close.sh at $(date '+%F %T %Z') — $1"
}
fail_claimed() {  # die, but relinquish first (post-claim exits only)
    relinquish "$1"
    die "$1" "${2:-4}"
}

tree_state() {
    # The identity of the bytes the battery sees: HEAD plus the
    # dirty/untracked set with each dirty file's BYTES hashed — a
    # fix-forward edits already-dirty files, which the path list alone
    # cannot see. .dragon/ and Cargo.lock are excluded to match the
    # rsync surface exactly (coordination notes churn every run and
    # never reach the host).
    local list bytes
    list=$(git -C "$REPO" status --porcelain | grep -v -e '\.dragon/' -e 'Cargo\.lock')
    bytes=$(printf '%s\n' "$list" | awk '{print $2}' | while read -r f; do
                [ -f "$f" ] && sha256sum "$f" | cut -c1-16; done \
            | sha256sum | cut -c1-16)
    printf 'HEAD %s\nPORCELAIN %s\nBYTES %s\n' \
        "$(git -C "$REPO" rev-parse HEAD)" \
        "$(printf '%s\n' "$list" | sha256sum | cut -c1-16)" \
        "$bytes"
}

verify_manifest() {
    # FULL verification (the wave contract: mismatch = wrote-after-
    # pinning drift; reconcile before anything else). Not a spot-check.
    # NOTE: the house pin convention is a TRUNCATED 16-hex sha256
    # prefix — `sha256sum -c` cannot consume these files at all ("no
    # properly formatted checksum lines found"); this parser is the
    # correct checker.
    local m="$1" bad=0 n=0 hash path disk
    [ -f "$m" ] || { echo "  manifest missing: $m"; return 1; }
    while read -r hash path _rest; do
        case "$hash" in ''|'#'*) continue ;; esac
        n=$((n+1))
        disk=$(sha256sum "$REPO/$path" 2>/dev/null | cut -c1-16)
        if [ "$disk" != "$hash" ]; then
            echo "  DRIFT: $path (manifest $hash, disk ${disk:-MISSING})"
            bad=$((bad+1))
        fi
    done < "$m"
    echo "  $m: $n rows, $bad drift"
    [ "$bad" -eq 0 ]
}

# --- 0. host liveness + claim interlock ----------------------------------
say "0. build-host liveness (centralhost 192.168.15.64:22)"
if timeout 8 bash -c '</dev/tcp/192.168.15.64/22' 2>/dev/null; then
    echo "host answers."
else
    if [ "$DRY" -eq 1 ]; then
        echo "host still down — a real run exits 2 here; the SSH watcher"
        echo "(/tmp watcher, pattern HOST_BACK) fires when port 22 answers."
    else
        die "centralhost SSH still down — nothing to recover yet. The watcher fires on HOST_BACK; re-run this script then." 2
    fi
fi

say "0b. claim interlock (.dragon/COORDINATION-stencil-wave.md)"
CLAIMED=$(claim_state)
if [ -n "$CLAIMED" ]; then
    case "$CLAIMED" in
        *CLAIMED*) die "the note's last claim line is '$CLAIMED' with no relinquish — another session owns the recovery; it must append RECOVERY RELINQUISHED first" 4 ;;
        *) echo "last claim state: $CLAIMED (clear to proceed)." ;;
    esac
else
    echo "no prior claim — this run claims it."
fi
if [ "$DRY" -eq 0 ]; then
    append_note "RECOVERY CLAIMED by stencil-wave-close.sh (pid $$) at $(date '+%F %T %Z')"
fi

# --- 1. preconditions -----------------------------------------------------
say "1. preconditions (all thirteen manifests FULLY hash-verified, push gate)"
[ -f "$MANIFEST" ] || die "stencil-wave manifest missing: $MANIFEST"
[ -f "$MANIFEST2" ] || die "e0063 manifest missing: $MANIFEST2"
[ -f "$MANIFEST3" ] || die "agent-brush-radius manifest missing: $MANIFEST3"
[ -f "$MANIFEST4" ] || die "agent-export-naming manifest missing: $MANIFEST4"
[ -f "$MANIFEST5" ] || die "stamp-preset-persist manifest missing: $MANIFEST5"
[ -f "$MANIFEST6" ] || die "flatpak-offline-deps manifest missing: $MANIFEST6"
[ -f "$MANIFEST7" ] || die "kittest-layers-breadth manifest missing: $MANIFEST7"
[ -f "$MANIFEST8" ] || die "stencil-hook-design manifest missing: $MANIFEST8"
[ -f "$MANIFEST9" ] || die "pcpool-design-review manifest missing: $MANIFEST9"
[ -f "$MANIFEST10" ] || die "dynmat-design-closure manifest missing: $MANIFEST10"
[ -f "$MANIFEST11" ] || die "masked-bakes-design-closure manifest missing: $MANIFEST11"
[ -f "$MANIFEST12" ] || die "masked-bakes-ray-half manifest missing: $MANIFEST12"
[ -f "$MANIFEST13" ] || die "falloff-design-closure manifest missing: $MANIFEST13"
cd "$REPO" || die "repo missing: $REPO"
verify_manifest "$MANIFEST" || \
    die "stencil-wave manifest drift — fix-forward changed pinned bytes. Re-pin the rows you edited (sha256sum <path> | cut -c1-16), then re-run." 4
verify_manifest "$MANIFEST2" || \
    die "e0063 manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST3" || \
    die "agent-brush-radius manifest drift — reconcile the edited rows (the manifest header documents its re-pin history), then re-run." 4
verify_manifest "$MANIFEST4" || \
    die "agent-export-naming manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST5" || \
    die "stamp-preset-persist manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST6" || \
    die "flatpak-offline-deps manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST7" || \
    die "kittest-layers-breadth manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST8" || \
    die "stencil-hook-design manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST9" || \
    die "pcpool-design-review manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST10" || \
    die "dynmat-design-closure manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST11" || \
    die "masked-bakes-design-closure manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST12" || \
    die "masked-bakes-ray-half manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST13" || \
    die "falloff-design-closure manifest drift — reconcile the edited rows, then re-run." 4
grep -q '"approved": true' docs/loop/push-gate.json || die "push gate not approved"
echo "push gate: approved (standing, 2026-10-12)."
for lock in stencil-core stencil-app stencil-viewport e0063-refscene agent-brush-radius agent-export-naming stamp-preset-persist flatpak-offline-deps kittest-layers-breadth stencil-hook-design pcpool-design-review dynmat-design-closure masked-bakes-design-closure masked-bakes-ray-half falloff-design-closure; do
    if [ -f "/tmp/umber-kill-locks/$lock.lock" ]; then
        echo "lock live: $lock ($(wc -l < "/tmp/umber-kill-locks/$lock.lock") files)"
    else
        echo "NOTE: lock $lock not in the registry (already released?)"
    fi
done
LOCAL_TIP=$(git rev-parse HEAD)
ORIGIN_TIP=$(git rev-parse origin/master 2>/dev/null || echo unknown)
[ "$LOCAL_TIP" = "$ORIGIN_TIP" ] || \
    echo "NOTE: local HEAD $LOCAL_TIP != origin/master $ORIGIN_TIP — unpushed commits ride this push too."
echo "preconditions green (tree at $LOCAL_TIP)."

# --- 2. host pre-flight ---------------------------------------------------
say "2. host pre-flight (reflog, live cargo check, porcelain count)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh $HOST 'git reflog -5; pgrep -af cargo; porcelain count'"
else
    pre=$(ssh -o ConnectTimeout=10 "$HOST" \
        'cd Projects/umber && echo "tip: $(git rev-parse HEAD)" && echo "porcelain: $(git status --porcelain | wc -l) rows" && { pgrep -ax cargo || pgrep -ax rustc; } || echo "no cargo running"' 2>/dev/null) \
        || fail_claimed "host pre-flight failed (SSH) — re-run when the host is stable" 4
    echo "$pre" | sed 's/^/  /'
    if ! echo "$pre" | grep -q '^no cargo running$'; then
        fail_claimed "a cargo/battery is LIVE on the host — a sibling or Bessa is mid-recovery; stand down" 4
    fi
    echo "$pre" | grep -q '^tip:' || fail_claimed "host has no Projects/umber checkout — reconcile the mirror by hand" 4
fi

# --- 3. rsync the tree -----------------------------------------------------
say "3. rsync tree -> host (target/.git/.dragon/Cargo.lock excluded)"
RSYNC_CMD="rsync -az --delete --exclude=/target/ --exclude=/.git/ --exclude=/.dragon/ --exclude=/Cargo.lock $REPO/ $HOST:Projects/umber/"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: $RSYNC_CMD"
    echo "  (dry-run) then write the battery sentinel (.dragon/battery-sentinel)"
else
    # shellcheck disable=SC2086
    rsync -az --delete --exclude=/target/ --exclude=/.git/ --exclude=/.dragon/ \
        --exclude=/Cargo.lock -e 'ssh -o ConnectTimeout=10' "$REPO/" "$HOST:Projects/umber/" \
        || fail_claimed "rsync failed — check connectivity; the tree is NOT synced" 4
    OLD_SENTINEL=$(cat "$SENTINEL" 2>/dev/null || true)
    tree_state > "$SENTINEL"
    echo "  synced; battery sentinel written:"
    sed 's/^/  /' "$SENTINEL"
fi

# --- 4. wave battery (default features only) -------------------------------
say "4. wave battery: $CRATES (default features; NEVER --features gpu in the recovery window)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh + rm -f $BATTERY_LOG + nohup wave-battery.sh $CRATES"
    echo "  (dry-run) then poll $BATTERY_LOG for BATTERY_EXIT (30s probes, 90 min cap),"
    echo "  reusing an existing verdict only while the sentinel matches this tree."
    exit 0
fi

# Reuse path: a verdict already in the log counts only for THESE bytes.
EXISTING=$(ssh -o ConnectTimeout=10 "$HOST" \
    "grep -o 'BATTERY_EXIT=[0-9]*' $BATTERY_LOG 2>/dev/null | tail -1" 2>/dev/null || true)
SENT_OK=0
[ -n "${OLD_SENTINEL:-}" ] && [ "$OLD_SENTINEL" = "$(tree_state)" ] && SENT_OK=1
if [ -n "$EXISTING" ] && [ "$SENT_OK" -eq 1 ]; then
    echo "  reusing verdict $EXISTING (sentinel matches this exact tree)."
else
    if [ -n "$EXISTING" ]; then
        echo "  NOTE: discarding stale verdict $EXISTING (tree changed since it ran — fresh battery)."
    elif [ "$SENT_OK" -eq 0 ]; then
        echo "  NOTE: no usable sentinel (fresh sync expected) — launching a fresh battery."
    fi
    ssh -o ConnectTimeout=10 "$HOST" \
        "cd Projects/umber && export PATH=\$HOME/.cargo/bin:\$PATH && rm -f $BATTERY_LOG && nohup bash -c 'bash scripts/wave-battery.sh $CRATES > $BATTERY_LOG 2>&1; echo BATTERY_EXIT=\$? >> $BATTERY_LOG' </dev/null >/dev/null 2>&1 &" \
        || fail_claimed "battery launch failed (SSH)" 4
    echo "  battery launched (remote log: $BATTERY_LOG). Polling (every ${BATTERY_POLL_SECONDS}s, cap ${BATTERY_POLL_MAX} probes)..."
    i=0; VERDICT=""
    while [ "$i" -lt "$BATTERY_POLL_MAX" ]; do
        sleep "$BATTERY_POLL_SECONDS"
        VERDICT=$(ssh -o ConnectTimeout=10 "$HOST" \
            "grep -o 'BATTERY_EXIT=[0-9]*' $BATTERY_LOG 2>/dev/null | tail -1" 2>/dev/null || true)
        if [ -n "$VERDICT" ]; then
            echo "  verdict landed after ~$(( (i+1) * BATTERY_POLL_SECONDS / 60 )) min: $VERDICT"
            break
        fi
        i=$((i+1))
        [ $((i % 10)) -eq 0 ] && echo "  ...still running ($(( i * BATTERY_POLL_SECONDS / 60 )) min)"
    done
    if [ -z "$VERDICT" ]; then
        fail_claimed "battery poll exhausted (~$(( BATTERY_POLL_MAX * BATTERY_POLL_SECONDS / 60 )) min, no verdict) — the battery may still be running on the host; WAIT for it, do not relaunch. When BATTERY_EXIT lands in $BATTERY_LOG, re-run this script (the verdict is reused)." 3
    fi
    EXISTING="$VERDICT"
fi
case "$EXISTING" in
    *BATTERY_EXIT=0*) echo "  battery GREEN." ;;
    *) fail_claimed "battery RED ($EXISTING) — fix-forward from $HOST:$BATTERY_LOG, re-pin any edited manifest rows, re-run this script (it resumes)." 3 ;;
esac

# --- 5. commits (e0063, wave, agent row, stamp row, flatpak row) -------------
say "5. commits (e0063 own row; stencil wave by manifest; agent combined row; stamp row; flatpak row)"
# 5a. e0063 — its own commit row, per its manifest.
: > /tmp/stencil-e0063-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-e0063-add.txt
done < "$MANIFEST2"
mapfile -t e_files < <(sort -u /tmp/stencil-e0063-add.txt)
[ "${#e_files[@]}" -ge 1 ] || fail_claimed "e0063 add-list is empty" 4
git -C "$REPO" add -- "${e_files[@]}" || fail_claimed "git add (e0063) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "fix(gpu): the perf-harness E0063 — cube_grid's MeshData literal gains the a23bcac triangle_materials default (empty = whole-mesh, the a394876 migration rule; only the nightly-only perf-harness feature builds the literal, so master's CI read green while every nightly died at the harness build step) — closes the 39097675689 red; the post-push perf-nightly re-dispatch becomes Wave 3's real discriminator" \
    || fail_claimed "commit (e0063) failed" 4
bash "$REPO/scripts/kill-locks.sh" remove e0063-refscene 2>/dev/null
echo "  e0063 committed; lock released."

# 5b. the stencil wave — manifest paths + the two remaining kill reports +
# queue.md + this script and its own report.
: > /tmp/stencil-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-add.txt
done < "$MANIFEST"
{
    echo "docs/loop/kill-reports/2026-10-17-stencil-app.md"
    echo "docs/loop/kill-reports/2026-10-17-stencil-viewport.md"
    echo "docs/loop/kill-reports/2026-10-18-stencil-recovery-script.md"
    echo "docs/loop/kill-reports/2026-10-18-stencil-hook-design.md"
    echo "docs/specs/stencil-engine-design.md"
    echo "docs/specs/per-channel-tile-pool-design.md"
    echo "docs/loop/kill-reports/2026-10-18-pcpool-design-closure.md"
    echo "docs/specs/dynamic-materials-design.md"
    echo "docs/loop/kill-reports/2026-10-18-dynmat-design-closure.md"
    echo "docs/specs/per-set-masked-bakes.md"
    echo "docs/loop/kill-reports/2026-10-18-masked-bakes-design-closure.md"
    echo "docs/specs/falloff-custom-design.md"
    echo "docs/loop/kill-reports/2026-10-18-falloff-design-closure.md"
    echo "docs/loop/queue.md"
    echo "scripts/stencil-wave-close.sh"
} >> /tmp/stencil-add.txt
mapfile -t w_files < <(sort -u /tmp/stencil-add.txt)
[ "${#w_files[@]}" -ge 10 ] || fail_claimed "wave add-list suspiciously short (${#w_files[@]} rows)" 4
git -C "$REPO" add -- "${w_files[@]}" || fail_claimed "git add (wave) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "feat(stencil): the stencil engine wave — umber-core's pure transform+rasterize (port-verified: 5 contract cases, 3 mutations caught), umber-app's state/PNG-load/panel rows (cross-reviewed; the honest gate seam + on-screen pending notice; 12 i18n keys both locales), the viewport quad math — parked through freeze-3, statically verified, battery-green this recovery; the paint hook (gate_plane composition, umber-paint) is the named follow-up + the wave docs (briefs, cross-review artifact, kill reports, queue continuity) and the executable recovery tooling" \
    || fail_claimed "commit (wave) failed" 4
for lock in stencil-core stencil-app stencil-viewport stencil-hook-design pcpool-design-review dynmat-design-closure masked-bakes-design-closure falloff-design-closure; do
    bash "$REPO/scripts/kill-locks.sh" remove "$lock" 2>/dev/null
done
echo "  wave committed (${#w_files[@]} paths); three stencil locks + the hook-design + pcpool + dynmat + masked-bakes-design + falloff locks released."

# 5c. the agent kills — ONE combined row (agent.rs carries both kills;
# one file cannot split across commit rows — the sibling's Reconciliation).
: > /tmp/stencil-agent-add.txt
for m in "$MANIFEST3" "$MANIFEST4"; do
    while read -r _h path _rest; do
        case "$_h" in ''|'#'*) continue ;; esac
        [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-agent-add.txt
    done < "$m"
done
mapfile -t a_files < <(sort -u /tmp/stencil-agent-add.txt)
[ "${#a_files[@]}" -ge 5 ] || fail_claimed "agent add-list suspiciously short (${#a_files[@]} rows)" 4
git -C "$REPO" add -- "${a_files[@]}" || fail_claimed "git add (agent row) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "feat(agent): the paint-ops follow-ups as ONE combined row — the absolute-radius wire field (PaintState::set_brush_radius_texels seam over the size multiplier; paint_stroke.radius: typed refusal before any GPU work, session-brush inheritance, clamped-radius reply) and export's naming unified to bake's v0 rule (the active set names the outputs; ExportAsk.set_override; the first CLI export integration test) — both kills statically verified + math-ported host-free through the outage, battery-green this recovery" \
    || fail_claimed "commit (agent row) failed" 4
for lock in agent-brush-radius agent-export-naming; do
    bash "$REPO/scripts/kill-locks.sh" remove "$lock" 2>/dev/null
done
echo "  agent row committed (${#a_files[@]} paths); both agent locks released."

# 5d. stamp-preset-persist — its own row, per its manifest.
: > /tmp/stencil-stamp-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-stamp-add.txt
done < "$MANIFEST5"
mapfile -t s_files < <(sort -u /tmp/stencil-stamp-add.txt)
[ "${#s_files[@]}" -ge 1 ] || fail_claimed "stamp add-list is empty" 4
git -C "$REPO" add -- "${s_files[@]}" || fail_claimed "git add (stamp row) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "feat(brush): the stamp sidecar's save half — a hand-picked stamp is retained (PickedStamp, kept only when set_stamp accepts it) and an explicit Save / Save As marks params.stamped + writes the <stem>.stamp.png beside the preset through one shared write_stamp_sidecar helper (the .abr format, so the existing load path restores it unchanged); preset switches clear the pick, Reset never touches the flag or sidecar, a failed sidecar write reports and keeps dirty — the C2-11 open tail closed, parked through freeze-3, battery-green this recovery" \
    || fail_claimed "commit (stamp row) failed" 4
bash "$REPO/scripts/kill-locks.sh" remove stamp-preset-persist 2>/dev/null
echo "  stamp row committed (${#s_files[@]} paths); stamp lock released."

# 5e. flatpak-offline-deps — its own row, per its manifest (the lane
# owner's kill: no battery scope; the packaging re-dispatch is the gate).
: > /tmp/stencil-flatpak-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-flatpak-add.txt
done < "$MANIFEST6"
mapfile -t f_files < <(sort -u /tmp/stencil-flatpak-add.txt)
[ "${#f_files[@]}" -ge 5 ] || fail_claimed "flatpak add-list suspiciously short (${#f_files[@]} rows)" 4
git -C "$REPO" add -- "${f_files[@]}" || fail_claimed "git add (flatpak row) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "build(flatpak): the offline-deps kill — the manifest renders hermetic from Cargo.lock through the committed template + generator (621 crate sources, sha256 straight from the lock's checksums, CDN-verified against the live tarballs), cargo's source replacement consumes the staged vendor tree with the cargo-vendor-shaped .cargo-checksum.json prep, the build runs --offline --locked, and CI lint gains the regenerate+diff drift gate — closes the packaging Flatpak red 38883955783 (cargo's crates.io fetch died inside flatpak-builder's no-network sandbox); the packaging workflow re-dispatch on this push is the verdict gate" \
    || fail_claimed "commit (flatpak row) failed" 4
bash "$REPO/scripts/kill-locks.sh" remove flatpak-offline-deps 2>/dev/null
echo "  flatpak row committed (${#f_files[@]} paths); flatpak lock released."

# 5f. kittest-layers-breadth — its own row, per its manifest (the sibling's
# ninth kill: three kittests + a shared draw closure, insert-only; the
# kittest legs run in the umber-app battery already in CRATES).
: > /tmp/stencil-kittest-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-kittest-add.txt
done < "$MANIFEST7"
mapfile -t k_files < <(sort -u /tmp/stencil-kittest-add.txt)
[ "${#k_files[@]}" -ge 2 ] || fail_claimed "kittest add-list suspiciously short (${#k_files[@]} rows)" 4
git -C "$REPO" add -- "${k_files[@]}" || fail_claimed "git add (kittest row) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "test(kittest): the layers panel's breadth — the duplicate button (copies the row, one copy, same kind), the selection-gated properties rows (opacity/blend/rename render for the selected layer, hidden for Base), and the blend combo (the channel-picker idiom, one History undo restores Normal), three kittests + one shared draw closure, insert-only, zero new i18n keys — parked through freeze-3, battery-green this recovery" \
    || fail_claimed "commit (kittest row) failed" 4
bash "$REPO/scripts/kill-locks.sh" remove kittest-layers-breadth 2>/dev/null
echo "  kittest row committed (${#k_files[@]} paths); kittest lock released."

# 5g. masked-bakes-ray-half — the sibling's CODE kill (the with_occluder ray
# half), its own row per its manifest; battery scope umber-app + umber-bake
# (umber-bake joined CRATES this same extension — its GPU occlusion test runs
# in the battery this recovery).
: > /tmp/stencil-rayhalf-add.txt
while read -r _h path _rest; do
    case "$_h" in ''|'#'*) continue ;; esac
    [ -f "$REPO/$path" ] && echo "$path" >> /tmp/stencil-rayhalf-add.txt
done < "$MANIFEST12"
mapfile -t rh_files < <(sort -u /tmp/stencil-rayhalf-add.txt)
[ "${#rh_files[@]}" -ge 4 ] || fail_claimed "ray-half add-list suspiciously short (${#rh_files[@]} rows)" 4
git -C "$REPO" add -- "${rh_files[@]}" || fail_claimed "git add (ray-half row) failed" 4
git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -m "feat(bakes): the masked-bakes ray half — AO/thickness/bent gain the occluder split (the raster submesh drives UV coverage, rays keep the whole mesh as occluders), the bake_extra_map occluder param, three mapped arms + margins, two bake_sources helpers, and the GPU occlusion pin test + control — the per-set checklist's ray maps stop writing the shared whole-mesh bake under every set's name; brief + report in the row, parked through freeze-3, battery-green this recovery (umber-app + umber-bake)" \
    || fail_claimed "commit (ray-half row) failed" 4
bash "$REPO/scripts/kill-locks.sh" remove masked-bakes-ray-half 2>/dev/null
echo "  ray-half row committed (${#rh_files[@]} paths); ray-half lock released."

# Every commit moved HEAD, which stales the battery sentinel written at
# rsync time — refresh it so a failed PUSH + re-run reuses the verdict
# instead of re-batterying (the cross-review's resume-gap finding, both
# halves: the commit guards above + this refresh).
tree_state > "$SENTINEL"

# --- 6. ONE push + ci-watch ---------------------------------------------------
say "6. ONE push + explicit run-id resolve + ci-watch"
git -C "$REPO" push origin master || fail_claimed "push failed" 4
sleep 10
HEADSHA=$(git -C "$REPO" rev-parse HEAD)
SLUG=$(git -C "$REPO" remote get-url origin | sed 's/.*github.com[:/]//;s/.git$//')
RUN_ID=$(gh run list --repo "$SLUG" --limit 5 --json databaseId,headSha \
    -q ".[] | select(.headSha == \"$HEADSHA\") | .databaseId" | head -1)
[ -n "$RUN_ID" ] || fail_claimed "could not resolve the CI run id for $HEADSHA — watch manually" 4
echo "  pushed $HEADSHA; CI run $RUN_ID."
nohup "$REPO/scripts/ci-watch.sh" "$RUN_ID" >/dev/null 2>&1 &
echo "  ci-watch detached on run $RUN_ID (pattern: CI_VERDICT=)."

# --- 7. post-push re-dispatches (the two parked kills' verdict gates) ---------
say "7. post-push re-dispatches (perf-nightly + packaging — the parked kills' verdict gates)"
if gh workflow run perf-nightly.yml --repo "$SLUG" --ref master 2>/dev/null; then
    echo "  perf-nightly dispatched on master — its verdict is Wave 3's discriminator (green: close the 5e0d89e red as stale; red on real numbers: the live-regression hunt is named)."
else
    echo "  NOTE: perf-nightly dispatch failed — re-run: gh workflow run perf-nightly.yml --repo $SLUG --ref master"
fi
if gh workflow run packaging.yml --repo "$SLUG" --ref master 2>/dev/null; then
    echo "  packaging dispatched on master — the flatpak-offline-deps kill's verdict gate (the Flatpak job builds the hermetic manifest on this tip)."
else
    echo "  NOTE: packaging dispatch failed — re-run: gh workflow run packaging.yml --repo $SLUG --ref master"
fi

append_note "RECOVERY CLOSED by stencil-wave-close.sh (pid $$) at $(date '+%F %T %Z') — battery green, e0063 + wave + agent + stamp + flatpak rows pushed, CI run $RUN_ID, perf-nightly + packaging re-dispatched."
echo
echo "WAVE CLOSED (pending CI run $RUN_ID). The docs pass is the next turn's first kill:"
echo "  - delete the consumed notes (.dragon/COORDINATION-stencil-wave.md, COORDINATION-wgui2-close.md) + all wave manifests + .dragon/battery-sentinel"
echo "  - tick queue.md's stencil section (CLOSED), STATE.md, CHANGELOG.md"
echo "  - release this script's lock: bash scripts/kill-locks.sh remove stencil-recovery-close"
echo "  - the named next kill: the gate_plane paint hook (umber-paint) — its file lock is released at this recovery"
exit 0
