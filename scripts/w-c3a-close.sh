#!/usr/bin/env bash
# w-c3a-close.sh — the freeze-4 (W-C3a) recovery runbook, executable.
#
# Encodes .dragon/runbooks/host-back-recovery.md (the M5 doc runbook)
# so the build host's return costs ONE command instead of a
# minutes-long manual session executed from memory. Built on the
# stencil-wave-close.sh pattern with BOTH of its recorded defects
# pre-fixed: the claim-grep anchors at line start (runbooks quote
# their own sentinel strings mid-line), and the battery-verdict
# reuse gates on a tree-state sentinel (HEAD + porcelain + the dirty
# files' BYTES) so a finished verdict resumes but a fix-forward
# forces a fresh battery.
#
# The fleet (six kills, each with a 16-hex manifest pinned 2026-10-20
# from its kill report's file list):
#   A1 channel-export-resample   P1 idmask-tile-cache
#   G1 picker-sliders-recents    S1 winink-pen-flags
#   C1 snapshot-dir              graph-history
# plus the docs/tooling row (the plan draft, the kill-ledger's
# accumulated worktree rows, this script, its report).
# NOT committed by this script, by design: the display-chain
# eyedropper's dirty viewport.rs/main.rs (its owner's lane — they
# ride the battery compile; a red inside them routes to the owner)
# and everything under .dragon/ (untracked by convention).
#
# Steps (idempotent, re-runnable; refuses to guess past any failure):
#   0. host liveness (port 22) + the note's claim interlock
#   1. preconditions: all SIX manifests fully hash-verified, push gate
#      approved, fleet lock census, the dirty-file ownership map
#   2. host pre-flight: reflog -5 (a sibling's fresh reset = stand
#      down), pgrep -ax cargo/rustc (a live battery = stand down),
#      tip + porcelain
#   3. rsync the tree (target/.git/.dragon/Cargo.lock excluded),
#      then write the battery sentinel (HEAD + porcelain + bytes)
#   4. wave battery on the host: umber-app umber-paint umber-brush
#      umber-gui umber-cli stylus (default features ONLY — never
#      --features gpu inside the recovery window; the wgpu deadlock
#      class, 94469e8, is the one wedge cause under our control).
#      Bounded BATTERY_EXIT poll; an existing verdict is reused only
#      while the sentinel matches the tree.
#   5. commit rows, each by manifest path THROUGH dcommit
#      --path-limited (ledger-attributed, foreign dirt cannot ride),
#      each fleet lock released at its own commit; then the docs row;
#      then the sentinel refresh (commits moved HEAD — a failed push
#      + re-run must reuse the verdict, not re-battery)
#   6. ONE push, explicit run-id resolve from headSha, ci-watch
#      detached
#   7. post-push re-dispatch: packaging (the flatpak chain's verdict
#      gate — PUSH FIRST, the manifest clones master at build time).
#      perf-nightly is NOT dispatched: no W-C3a kill owes it and the
#      hardened nightly runs on schedule.
# The docs pass (post-CI-green, rides the NEXT push per the one-push
# rule) stays manual — printed at the end.
#
# Usage (run from a Hermes session so HERMES_SESSION_ID attributes
# the fleet commits, or export DRAGON_SESSION=<id> first — dcommit
# refuses unattributed commits, fail-closed by design):
#   nohup bash scripts/w-c3a-close.sh > /tmp/w-c3a-close.log 2>&1 &
#   (the battery poll can run ~90 min — run DETACHED, then tail the log)
#   bash scripts/w-c3a-close.sh --dry-run
#   (host-free: verifies everything checkable now — claim state, all
#   manifests, push gate, the ownership map — and prints the plan)
#
# Exit codes: 0 wave closed (pending CI); 2 host down; 3 battery RED
# or poll exhausted; 4 interlock/precondition refusal.
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
HOST=bessa@192.168.15.64
REPO="$HOME/Projects/umber"
NOTE="$REPO/.dragon/COORDINATION-next-wave.md"
MANIFEST_A1="$REPO/.dragon/channel-export-resample-manifest.txt"
MANIFEST_P1="$REPO/.dragon/idmask-tile-cache-manifest.txt"
MANIFEST_G1="$REPO/.dragon/picker-sliders-recents-manifest.txt"
MANIFEST_S1="$REPO/.dragon/winink-pen-flags-manifest.txt"
MANIFEST_C1="$REPO/.dragon/snapshot-dir-manifest.txt"
MANIFEST_GH="$REPO/.dragon/graph-history-manifest.txt"
BATTERY_LOG="/tmp/w-c3a-recovery-battery.log"
SENTINEL="$REPO/.dragon/battery-sentinel"
CRATES="umber-app umber-paint umber-brush umber-gui umber-cli stylus"
BATTERY_POLL_SECONDS=30
BATTERY_POLL_MAX=180    # 180 x 30s = 90 min cap

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*"; exit "${2:-4}"; }
append_note() { printf '\n%s\n' "$*" >> "$NOTE"; }

claim_state() {
    # The note's LAST claim-ish line decides: a bare CLAIMED with no
    # later relinquish/close means someone owns the recovery. ANCHORED
    # TO LINE START — the note's own prose quotes sentinel strings
    # mid-line ("W-C3A CLOSE SCRIPT CLAIMED ..." inside the events
    # line) and must never match. tac | grep -m1, never a plain grep.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3A RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
relinquish() {
    append_note "W-C3A RECOVERY RELINQUISHED by w-c3a-close.sh at $(date '+%F %T %Z') — $1"
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
    # FULL verification (mismatch = wrote-after-pinning drift;
    # reconcile before anything else). Not a spot-check. NOTE: the
    # house pin convention is a TRUNCATED 16-hex sha256 prefix —
    # `sha256sum -c` cannot consume these files at all ("no properly
    # formatted checksum lines found"); this parser is the correct
    # checker. Proven 2026-10-18: the wrong checker read DRIFT on
    # every healthy manifest.
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

row_files() {  # <manifest> -> the add-list on stdout (paths that exist)
    local m="$1" path
    while read -r _h path _rest; do
        case "$_h" in ''|'#'*) continue ;; esac
        [ -f "$REPO/$path" ] && printf '%s\n' "$path"
    done < "$m"
}

commit_row() {  # <lock-name> <manifest|-> <message> <min-rows> [files...]
    # Commits exactly the manifest's (or the explicit) paths through
    # dcommit --path-limited: the row's files are the lock paths AND
    # the pathspec, so foreign dirt (the eyedropper's viewport.rs /
    # main.rs, the .dragon/ surface) cannot ride, and the commit is
    # ledger-attributed to the running session.
    local lock="$1" m="$2" msg="$3" min="$4"; shift 4
    local tmp; tmp=$(mktemp)
    if [ "$m" = "-" ]; then
        printf '%s\n' "$@" > "$tmp"
    else
        row_files "$m" > "$tmp"
    fi
    mapfile -t files < <(sort -u "$tmp")
    rm -f "$tmp"
    [ "${#files[@]}" -ge "$min" ] || fail_claimed "$lock add-list suspiciously short (${#files[@]} rows)" 4
    git -C "$REPO" add -- "${files[@]}" || fail_claimed "git add ($lock) failed" 4
    # shellcheck disable=SC2086
    python3 "$REPO/.dragon/dcommit.py" --path-limited $(printf -- '--lock %s ' "${files[@]}") -- -m "$msg" \
        || fail_claimed "dcommit row ($lock) failed — read the refusal above; nothing was committed past this row" 4
    bash "$REPO/scripts/kill-locks.sh" remove "$lock" 2>/dev/null
    echo "  $lock committed (${#files[@]} paths); lock released (if present)."
}

# --- 0. host liveness + claim interlock ----------------------------------
say "0. build-host liveness (centralhost 192.168.15.64:22)"
if timeout 8 bash -c '</dev/tcp/192.168.15.64/22' 2>/dev/null; then
    echo "host answers."
else
    if [ "$DRY" -eq 1 ]; then
        echo "host still down — a real run exits 2 here; the notify watcher"
        echo "(.dragon/watch-centralhost-notify.sh) DMs on HOST_RETURNED."
    else
        die "centralhost SSH still down — nothing to recover yet. Re-run this script when the watcher fires (or after a manual probe answers)." 2
    fi
fi

say "0b. claim interlock (.dragon/COORDINATION-next-wave.md, sentinel anchored at line start)"
CLAIMED=$(claim_state)
if [ -n "$CLAIMED" ]; then
    case "$CLAIMED" in
        *CLAIMED*) die "the note's last claim line is '$CLAIMED' with no relinquish — another session owns the recovery; it must append W-C3A RECOVERY RELINQUISHED first" 4 ;;
        *) echo "last claim state: $CLAIMED (clear to proceed)." ;;
    esac
else
    echo "no prior claim — this run claims it."
fi
if [ "$DRY" -eq 0 ]; then
    append_note "W-C3A RECOVERY CLAIMED by w-c3a-close.sh (pid $$) at $(date '+%F %T %Z')"
fi

# --- 1. preconditions -----------------------------------------------------
say "1. preconditions (all six manifests FULLY hash-verified, push gate, ownership map)"
for m in "$MANIFEST_A1" "$MANIFEST_P1" "$MANIFEST_G1" "$MANIFEST_S1" "$MANIFEST_C1" "$MANIFEST_GH"; do
    [ -f "$m" ] || die "manifest missing: $m"
done
cd "$REPO" || die "repo missing: $REPO"
verify_manifest "$MANIFEST_A1" || die "channel-export-resample manifest drift — fix-forward changed pinned bytes. Re-pin the rows you edited (sha256sum <path> | cut -c1-16), then re-run." 4
verify_manifest "$MANIFEST_P1" || die "idmask-tile-cache manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_G1" || die "picker-sliders-recents manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_S1" || die "winink-pen-flags manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_C1" || die "snapshot-dir manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_GH" || die "graph-history manifest drift — reconcile the edited rows, then re-run." 4
grep -q '"approved": true' docs/loop/push-gate.json || die "push gate not approved"
echo "push gate: approved (standing, 2026-10-12)."

echo "fleet lock census (the /tmp registry was cleaned since the fleet parked; absence is expected, the claim governs):"
for lock in channel-export-resample idmask-tile-cache picker-sliders-recents winink-pen-flags snapshot-dir graph-history; do
    if [ -f "/tmp/umber-kill-locks/$lock.lock" ]; then
        echo "  lock live: $lock ($(wc -l < "/tmp/umber-kill-locks/$lock.lock") files)"
    else
        echo "  lock absent from registry: $lock (released-by-cleanup; this run owns the tree via the claim)"
    fi
done

echo "CAVEAT — the display-chain eyedropper's uncommitted hunks (its owner's lane):"
git status --porcelain -- crates/umber-app/src/viewport.rs crates/umber-app/src/main.rs | sed 's/^/  /'
echo "  They ride the battery compile (the rsync carries them) but are NOT in"
echo "  any commit row — dcommit --path-limited cannot pick them up. A battery"
echo "  red inside their hunks routes to the eyedropper's owner, not this wave."

LOCAL_TIP=$(git rev-parse HEAD)
ORIGIN_TIP=$(git rev-parse origin/master 2>/dev/null || echo unknown)
[ "$LOCAL_TIP" = "$ORIGIN_TIP" ] || \
    echo "NOTE: local HEAD $LOCAL_TIP != origin/master $ORIGIN_TIP — the unpushed doc/tooling tail rides this push too."
echo "preconditions green (tree at $LOCAL_TIP)."

# --- 2. host pre-flight ---------------------------------------------------
say "2. host pre-flight (reflog, live cargo check, tip + porcelain)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh $HOST 'git reflog -5; pgrep -ax cargo; tip; porcelain count'"
else
    pre=$(ssh -o ConnectTimeout=10 "$HOST" \
        'cd Projects/umber && echo "tip: $(git rev-parse HEAD)" && echo "porcelain: $(git status --porcelain | wc -l) rows" && { git reflog -5 | sed "s/^/reflog: /"; } && { pgrep -ax cargo || pgrep -ax rustc; } || echo "no cargo running"' 2>/dev/null) \
        || fail_claimed "host pre-flight failed (SSH) — re-run when the host is stable" 4
    echo "$pre" | sed 's/^/  /'
    if ! echo "$pre" | grep -q '^no cargo running$'; then
        fail_claimed "a cargo/battery is LIVE on the host — a sibling or Bessa is mid-recovery; stand down" 4
    fi
    echo "$pre" | grep -q '^tip:' || fail_claimed "host has no Projects/umber checkout — reconcile the mirror by hand" 4
fi

# --- 3. rsync the tree ------------------------------------------------------
say "3. rsync tree -> host (target/.git/.dragon/Cargo.lock excluded)"
RSYNC_CMD="rsync -az --delete --exclude=/target/ --exclude=/.git/ --exclude=/.dragon/ --exclude=/Cargo.lock $REPO/ $HOST:Projects/umber/"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: $RSYNC_CMD"
    echo "  (dry-run) then write the battery sentinel (.dragon/battery-sentinel: HEAD + porcelain + dirty-bytes)"
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

# --- 4. wave battery (default features only) --------------------------------
say "4. wave battery: $CRATES (default features; NEVER --features gpu in the recovery window)"
echo "  CAVEAT again: the eyedropper's dirty viewport.rs/main.rs hunks are in"
echo "  umber-app's compile (the rsync carries the whole tree). A red inside"
echo "  their hunks routes to the owner — do not fix-forward them here."
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh + rm -f $BATTERY_LOG + nohup wave-battery.sh $CRATES"
    echo "  (dry-run) then poll $BATTERY_LOG for BATTERY_EXIT (30s probes, 90 min cap),"
    echo "  reusing an existing verdict only while the sentinel matches this tree."
    echo
    echo "DRY-RUN COMPLETE — all host-free preconditions verified. The commit plan:"
    echo "  row 1  channel-export-resample ($(grep -vc '^#' "$MANIFEST_A1") paths) -> dcommit path-limited"
    echo "  row 2  idmask-tile-cache ($(grep -vc '^#' "$MANIFEST_P1") paths)"
    echo "  row 3  picker-sliders-recents ($(grep -vc '^#' "$MANIFEST_G1") paths)"
    echo "  row 4  winink-pen-flags ($(grep -vc '^#' "$MANIFEST_S1") paths)"
    echo "  row 5  snapshot-dir ($(grep -vc '^#' "$MANIFEST_C1") paths)"
    echo "  row 6  graph-history ($(grep -vc '^#' "$MANIFEST_GH") paths)"
    echo "  row 7  docs/tooling: next-wave-plan-draft.md + kill-ledger.md + this script + its report"
    echo "  then: ONE push (local tail + rows) + explicit run-id ci-watch + the packaging re-dispatch."
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
    *) fail_claimed "battery RED ($EXISTING) — fix-forward from $HOST:$BATTERY_LOG (if the red is inside viewport.rs/main.rs hunks it routes to the eyedropper's owner, not this wave), re-pin any edited manifest rows, re-run this script (it resumes)." 3 ;;
esac

# --- 5. commit rows (each through dcommit --path-limited) --------------------
say "5. commit rows (six fleet kills by manifest + the docs row; each lock released at its commit)"

commit_row channel-export-resample "$MANIFEST_A1" \
"feat(export): channel-export-resample (A1) — both painted readbacks (painted_channel_tile + painted_base_color_tile) fit to the export size through material_layer::resample_image_rgba8 on a dims mismatch instead of silently exporting flat/baked when the paint size differs from the dialog's export size; the decision is the pure fit_channel_to_export (identity passes bytes through exactly); the painted_at_size decline re-pinned deliberately — a session exists = the new contract, the behaviour change named in the report; the CLI/agent export path grep-audited (bakes only, no painted drop); bilinear downsampling aliases at large ratios — box-filtering the named deferral; parked through freeze-4, battery-green this recovery" 4

commit_row idmask-tile-cache "$MANIFEST_P1" \
"feat(idmask): idmask-tile-cache (P1) — the multi-tile bake (filter_mesh_for_tile per present tile, the click UV rebased by the same integer tile offset so the right texel is picked; single-tile mesh byte-identical to v1) + the single-entry content-keyed CachedIdBake cache (std DefaultHasher over indices + uv bit patterns + positions.len + IdFlavor inside paint_state; a hit skips the GPU bake, a resize misses naturally; the O(tris) hash cost vs the 16M-texel bake stated in the module docs; single-entry last-clicked is the named judgment call); the refusal-first contract unchanged, the cache never touched on an error path; parked through freeze-4, battery-green this recovery" 4

commit_row picker-sliders-recents "$MANIFEST_G1" \
"feat(gui): picker-sliders-recents (G1) — HSV and RGB slider rows under the hex field, each track tinted across its own channel range (an egui::Mesh gradient on the SV-square precedent, no texture); RGB rows edit sRGB-encoded 0..255 matching hex, HSV rows edit the kept HSV (hue survives greys); the new pure set_srgb_component setter; PickerLabels passed in so umber-gui stays tr-free; the additive color_button_with beside the UNCHANGED color_button (umber-app compiles at every point of the wave); recents pushed on popup close only when the colour differs from previous, the SwatchGrid of recents shown in the popup; 8 tests incl. the kittest drag + AccessKit roles; the inline eyedropper + Linear/Perceptual toggle the named deferrals; parked through freeze-4, battery-green this recovery" 6

commit_row winink-pen-flags "$MANIFEST_S1" \
"feat(stylus): winink-pen-flags (S1) — the pure platform-free PenFlagTracker mapping (down, pen_flags, time_ns) to contact events: Eraser on INVERTED|ERASER matching the XI2 emit-Eraser-instead-of-Contact precedent, a barrel edge detector yielding Button transitions only, no repeat on steady flags; the vendored constant mirrors carry their citation lines (PEN_FLAG_BARREL/INVERTED/ERASER from the vendored windowsandmessaging block); winink.rs fetches POINTER_PEN_INFO for DOWN/UP and routes through the tracker — a Windows-only delta, Windows CI its sole compile gate (the tracker's tests run ungated on Linux in the battery); parked through freeze-4, battery-green this recovery" 5

commit_row snapshot-dir "$MANIFEST_C1" \
"feat(cli): snapshot-dir (C1) — paint_snapshot takes exactly one of out | out_dir (both or neither = typed refusal before any GPU work); out_dir writes <out_dir>/<step:05>.png named from the session's 0-based step index; deny_unknown_fields keeps old scripts valid, PROTOCOL_VERSION stays 1 pre-tag; the report's own deferral from the harness-script judgment closed; parked through freeze-4, battery-green this recovery" 5

commit_row graph-history "$MANIFEST_GH" \
"feat(graph): the procedural history panel k1 (wave-procedural-history) — graph_panel.rs + the new graph_panel/history.rs: per-node procedural history with journal-integrated commands; the cross-review BLOCKER (AddNode carrying no redo id — an undo would strand the node) found and fixed BEFORE the battery, nemotron cross-reviewed; 13 tests, fmt-clean, both locales' keys landed; the unified timeline remains kill 2 (named follow-up); parked through freeze-4, battery-green this recovery" 5

commit_row wave-docs-and-tooling - \
"docs(wave): the W-C3a close tooling + the plan-of-record + the ledger's accumulated rows — scripts/w-c3a-close.sh (the executable recovery: claim interlock anchored at line start, six 16-hex manifests full-verified, hardened rsync + the tree-state sentinel, default-leg battery with the bounded verdict-reuse poll, commit rows through dcommit --path-limited so foreign dirt cannot ride, ONE push + explicit run-id ci-watch, the packaging re-dispatch — both stencil-wave-close defects pre-fixed) + its kill report + docs/loop/next-wave-plan-draft.md (the claude planner's W-C3a plan, executed as the fleet) + docs/loop/kill-ledger.md (the single-owner rule's ledger commit: the M1/M3/M4/M5 landed rows + the fleet status ticks that accumulated worktree-side through the freeze); rides this push" 4 \
    docs/loop/next-wave-plan-draft.md \
    docs/loop/kill-ledger.md \
    scripts/w-c3a-close.sh \
    docs/loop/kill-reports/2026-10-20-w-c3a-close-script.md

# Every commit moved HEAD, which stales the battery sentinel written at
# rsync time — refresh it so a failed PUSH + re-run reuses the verdict
# instead of re-batterying (the stencil cross-review's resume-gap
# finding, both halves: the commit guards above + this refresh).
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

# --- 7. post-push re-dispatch (the flatpak chain's verdict gate) --------------
say "7. post-push re-dispatch: packaging (PUSH FIRST — the manifest clones master at build time)"
if gh workflow run packaging.yml --repo "$SLUG" --ref master 2>/dev/null; then
    echo "  packaging dispatched on master — the flatpak chain's verdict gate (the"
    echo "  archive-type + strip-components fixes get their first full run on a tip"
    echo "  that carries them)."
else
    echo "  NOTE: packaging dispatch failed — re-run: gh workflow run packaging.yml --repo $SLUG --ref master"
fi
echo "  perf-nightly NOT dispatched: no W-C3a kill owes it; the hardened nightly"
echo "  runs on schedule and the drift gate is live on every pushed tip."

append_note "W-C3A RECOVERY CLOSED by w-c3a-close.sh (pid $$) at $(date '+%F %T %Z') — battery green, six fleet rows + the docs row pushed, CI run $RUN_ID, packaging re-dispatched. The eyedropper's hunks remain its owner's."
echo
echo "WAVE CLOSED (pending CI run $RUN_ID). The docs pass is the next turn's first kill:"
echo "  - delete the consumed manifests (.dragon/*-manifest.txt) + .dragon/battery-sentinel"
echo "  - tick queue.md (the C2 rows the fleet closes: C2-12 sliders/swatches, row 15's snapshot-dir residual),"
echo "    STATE.md, CHANGELOG.md; the desktop-checks Result lines stay open until a desktop session runs them"
echo "  - the kill-ledger rows: fleet -> LANDED, the waiting rows -> closed"
echo "  - the eyedropper: its owner commits viewport.rs/main.rs + its report, or the next session adopts it"
echo "    (holder-exit confirmed, fresh lock, full gate sequence — the adoption rules)"
echo "  - memtest86+ stays OWED at the next physical access: boot straight into it, skip the OS"
exit 0
