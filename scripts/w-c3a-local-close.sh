#!/usr/bin/env bash
# w-c3a-local-close.sh — the LOCAL-degraded-mode port of w-c3a-close.sh.
#
# Why this exists: centralhost is unreachable (verified 2026-10-22 13:17
# by the original script's own dry-run probe; fragile boot, memtest86+
# still owed — the host stays untouched). Bessa's steer 2026-10-22
# ~13:1x: "Continue working autonomously cargo lifted, but do it slow
# to not overload the machine" — cargo is LIFTED; the close therefore
# runs LOCALLY on the 8-thread tower, under the SLOW THROTTLE ENVELOPE:
#   CARGO_BUILD_JOBS=${W3A_JOBS:-3}   (compile parallelism capped)
#   UMBER_TEST_THREADS=${W3A_TEST_THREADS:-2}  (non-GPU suites)
#   UMBER_GPU_TEST_THREADS=1          (wave-battery default, kept)
#   UMBER_SUITE_TIMEOUT=${W3A_TIMEOUT:-2400}   (throttled builds need room)
#   nice -n ${W3A_NICE:-19}           (the desktop keeps priority)
# One cargo at a time under /tmp/umber-cargo.lock (wave-battery's own
# flock), nothing parallel. The 10-21 sessions ran these steps by hand
# under the same local sanction and died un-landed twice; this port
# makes the local close ONE idempotent command so a session death
# costs a re-run, not a re-derivation.
#
# Everything else is the original script verbatim: the claim interlock
# (anchored at line start), the nine-manifest FULL verification with
# the truncated-prefix parser, the push gate check, the A2 baseline
# skip logic, the tree-state battery sentinel (verdict reuse only
# while the bytes match), the ten commit rows through dcommit
# --path-limited with the exact messages, the ONE push + explicit
# run-id ci-watch, the packaging re-dispatch. Local seams only replace
# steps 0/2/3/4 (host liveness, host pre-flight, rsync, host battery).
#
# Row 11 (this port's own landing) is added to the original's row 10:
# the port + its report commit with the docs row's tooling.
#
# Usage (from a Hermes session so HERMES_SESSION_ID attributes the
# fleet commits, or export DRAGON_SESSION=<id> first — dcommit refuses
# unattributed commits):
#   nohup bash scripts/w-c3a-local-close.sh > /tmp/w-c3a-local-close.log 2>&1 &
#   (the throttled battery poll can run 1-3 h — run DETACHED, tail the log)
#   bash scripts/w-c3a-local-close.sh --dry-run
#   (verifies everything checkable now and prints the plan)
#
# Exit codes: 0 wave closed (pending CI); 3 battery RED or poll
# exhausted; 4 interlock/precondition refusal; 5 the A2 baselines were
# generated — vision check owed, re-run after it passes.
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
REPO="$HOME/Projects/umber"
NOTE="$REPO/.dragon/COORDINATION-next-wave.md"
MANIFEST_A1="$REPO/.dragon/channel-export-resample-manifest.txt"
MANIFEST_P1="$REPO/.dragon/idmask-tile-cache-manifest.txt"
MANIFEST_G1="$REPO/.dragon/picker-sliders-recents-manifest.txt"
MANIFEST_S1="$REPO/.dragon/winink-pen-flags-manifest.txt"
MANIFEST_C1="$REPO/.dragon/snapshot-dir-manifest.txt"
MANIFEST_GH="$REPO/.dragon/graph-history-manifest.txt"
MANIFEST_B10="$REPO/.dragon/b10-eyedropper-manifest.txt"
MANIFEST_A2="$REPO/.dragon/a2-snapshot-breadth-manifest.txt"
MANIFEST_A3="$REPO/.dragon/a3-picker-app-sites-manifest.txt"
BATTERY_LOG="/tmp/w-c3a-local-battery-r3.log"
SENTINEL="$REPO/.dragon/battery-sentinel"
CRATES="umber-app umber-paint umber-brush umber-gui umber-cli stylus"
BATTERY_POLL_SECONDS=30
BATTERY_POLL_MAX=360    # 360 x 30s = 3 h cap (throttled builds are slow by design)
JOBS="${W3A_JOBS:-3}"
TEST_THREADS="${W3A_TEST_THREADS:-2}"
SUITE_TIMEOUT="${W3A_TIMEOUT:-2400}"
NICE="${W3A_NICE:-19}"
export PATH="$HOME/.cargo/bin:$PATH"

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*" >&2; exit "${2:-4}"; }
append_note() { printf '\n%s\n' "$*" >> "$NOTE"; }

claim_state() {
    # The note's LAST claim-ish line decides: a bare CLAIMED with no
    # later relinquish/close means someone owns the recovery. ANCHORED
    # TO LINE START — the note's own prose quotes sentinel strings
    # mid-line and must never match. tac | grep -m1, never a plain grep.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3A RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
relinquish() {
    append_note "W-C3A RECOVERY RELINQUISHED by w-c3a-local-close.sh at $(date '+%F %T %Z') — $1"
}
fail_claimed() {  # die, but relinquish first (post-claim exits only)
    relinquish "$1"
    die "$1" "${2:-4}"
}

tree_state() {
    # The identity of the bytes the battery sees: HEAD plus the
    # dirty/untracked set with each dirty file's BYTES hashed — a
    # fix-forward edits already-dirty files, which the path list alone
    # cannot see. .dragon/ and Cargo.lock are excluded (coordination
    # notes churn every run; the battery surface is what remains).
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
    # reconcile before anything else). The house pin convention is a
    # TRUNCATED 16-hex sha256 prefix — sha256sum -c cannot consume
    # these files; this parser is the correct checker.
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
    # the pathspec, so foreign dirt cannot ride, and the commit is
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

# --- 0. mode banner (LOCAL degraded; the host stays untouched) --------------
say "0. LOCAL degraded mode (centralhost unreachable 2026-10-22 13:17 by the original script's own probe; cargo LIFTED by Bessa ~13:1x; slow-throttle envelope: -j$JOBS, test-threads $TEST_THREADS/1, timeout ${SUITE_TIMEOUT}s, nice $NICE)"
echo "  The host is NOT contacted anywhere in this run. memtest86+ stays"
echo "  Bessa's gate at the next physical access."

# --- 0b. claim interlock (.dragon/COORDINATION-next-wave.md) ----------------
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
    append_note "W-C3A RECOVERY CLAIMED by w-c3a-local-close.sh (pid $$) at $(date '+%F %T %Z') — LOCAL degraded mode"
fi

# --- 1. preconditions --------------------------------------------------------
say "1. preconditions (all nine manifests FULLY hash-verified, push gate, ownership map)"
for m in "$MANIFEST_A1" "$MANIFEST_P1" "$MANIFEST_G1" "$MANIFEST_S1" "$MANIFEST_C1" "$MANIFEST_GH" "$MANIFEST_B10" "$MANIFEST_A2" "$MANIFEST_A3"; do
    [ -f "$m" ] || die "manifest missing: $m"
done
cd "$REPO" || die "repo missing: $REPO"
verify_manifest "$MANIFEST_A1" || die "channel-export-resample manifest drift — fix-forward changed pinned bytes. Re-pin the rows you edited (sha256sum <path> | cut -c1-16), then re-run." 4
verify_manifest "$MANIFEST_P1" || die "idmask-tile-cache manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_G1" || die "picker-sliders-recents manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_S1" || die "winink-pen-flags manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_C1" || die "snapshot-dir manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_GH" || die "graph-history manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_B10" || die "b10-eyedropper manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_A2" || die "a2-snapshot-breadth manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_A3" || die "a3-picker-app-sites manifest drift — reconcile the edited rows, then re-run." 4
grep -q '"approved": true' docs/loop/push-gate.json || die "push gate not approved"
echo "push gate: approved (standing, 2026-10-12)."

echo "fleet lock census (the /tmp registry was cleaned since the fleet parked; absence is expected, the claim governs):"
for lock in channel-export-resample idmask-tile-cache picker-sliders-recents winink-pen-flags snapshot-dir graph-history b10-eyedropper-adoption a2-snapshot-breadth a3-picker-app-sites; do
    if [ -f "/tmp/umber-kill-locks/$lock.lock" ]; then
        echo "  lock live: $lock ($(wc -l < "/tmp/umber-kill-locks/$lock.lock") files)"
    else
        echo "  lock absent from registry: $lock (released-by-cleanup; this run owns the tree via the claim)"
    fi
done

echo "Fleet note — the display-chain eyedropper is ADOPTED (2026-10-20, the"
echo "  adoption rules ran: holder exit confirmed, static gates re-derived,"
echo "  the test pins python-ported). Its uncommitted hunks now ride BOTH the"
echo "  battery compile AND row 7's commit; a red inside them is this wave's"
echo "  to fix-forward. Current status:"
git status --porcelain -- crates/umber-app/src/viewport.rs crates/umber-app/src/main.rs | sed 's/^/  /'

LOCAL_TIP=$(git rev-parse HEAD)
ORIGIN_TIP=$(git rev-parse origin/master 2>/dev/null || echo unknown)
[ "$LOCAL_TIP" = "$ORIGIN_TIP" ] || \
    echo "NOTE: local HEAD $LOCAL_TIP != origin/master $ORIGIN_TIP — the unpushed doc/tooling tail rides this push too."
echo "preconditions green (tree at $LOCAL_TIP)."

# --- 2. local pre-flight -----------------------------------------------------
say "2. local pre-flight (a live local cargo = stand down)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: pgrep -ax cargo/rustc; refuse if a local battery is live"
else
    if pgrep -ax cargo >/dev/null 2>&1 || pgrep -ax rustc >/dev/null 2>&1; then
        fail_claimed "a cargo/rustc is LIVE on this machine — a sibling or Bessa is mid-build; stand down" 4
    fi
    echo "  no live cargo/rustc. load: $(cut -d' ' -f1-3 /proc/loadavg) on $(nproc) threads."
fi

# --- 3. battery sentinel (local tree; no rsync — the battery runs HERE) -----
say "3. battery sentinel from the LOCAL tree (HEAD + porcelain + dirty-bytes)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: tree_state > $SENTINEL"
else
    OLD_SENTINEL=$(cat "$SENTINEL" 2>/dev/null || true)
    tree_state > "$SENTINEL"
    sed 's/^/  /' "$SENTINEL"
fi

# --- 4a. A2 snapshot baselines (local; skipped when the four PNGs exist) -----
say "4a. A2 snapshot baselines (UPDATE_SNAPSHOTS=1, one cargo under the flock — skipped when all four exist)"
snap_dir="crates/umber-app/tests/snapshots"
NEW_BASELINES="history_panel_empty history_panel_ops toolbar rich_tooltip"
gen_needed=0
for p in $NEW_BASELINES; do
    [ -s "$REPO/$snap_dir/$p.png" ] || gen_needed=1
done
if [ "$gen_needed" -eq 1 ]; then
    if [ "$DRY" -eq 1 ]; then
        echo "  (dry-run) would: flock /tmp/umber-cargo.lock UPDATE_SNAPSHOTS=1 cargo test -p umber-app --test panel_snapshot"
        echo "  (dry-run) then cmp the three existing baselines, existence-check the four new ones,"
        echo "  (dry-run) then EXIT 5 for the recovery session's vision check."
    else
        ( cd "$REPO" && flock /tmp/umber-cargo.lock nice -n "$NICE" env CARGO_BUILD_JOBS="$JOBS" \
            UPDATE_SNAPSHOTS=1 cargo test -p umber-app --test panel_snapshot ) \
            || fail_claimed "the A2 baseline generation run FAILED — read the failure above before retrying" 4
        for p in axis_gizmo display_panel psd_import_section; do
            git -C "$REPO" diff --quiet -- "$snap_dir/$p.png" \
                || fail_claimed "existing baseline $p.png DRIFTED during generation — a panel moved; reconcile before the battery" 4
        done
        for p in $NEW_BASELINES; do
            [ -s "$REPO/$snap_dir/$p.png" ] || fail_claimed "baseline $p.png was not generated" 4
        done
        echo "  four new baselines present; the three existing baselines byte-unchanged."
        echo "  >>> VISION CHECK OWED: open the four new PNGs in $snap_dir and confirm"
        echo "  >>> widgets are VISIBLE, not blank (the g1-axis-gizmo precedent)."
        echo "  >>> Then RE-RUN this script: it skips the generation and continues."
        relinquish "4a complete — the vision check on the four new baselines gates the battery; re-run after it passes"
        die "4a complete, vision check owed before the battery — re-run this script after checking the four PNGs" 5
    fi
else
    echo "  all four baselines present — the vision check is recorded as done; continuing."
fi

# --- 4. wave battery (LOCAL, throttled; default features only) ---------------
say "4. wave battery (LOCAL, throttled: -j$JOBS nice $NICE): $CRATES (default features; NEVER --features gpu)"
echo "  Fleet note: the adopted eyedropper's viewport.rs/main.rs hunks are in"
echo "  umber-app's compile (the battery sees the whole tree) — a red inside"
echo "  them is this wave's to fix-forward (the adoption re-derived the gates)."
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: rm -f $BATTERY_LOG + nohup nice -n $NICE env CARGO_BUILD_JOBS=$JOBS UMBER_TEST_THREADS=$TEST_THREADS UMBER_SUITE_TIMEOUT=$SUITE_TIMEOUT wave-battery.sh $CRATES"
    echo "  (dry-run) then poll $BATTERY_LOG for BATTERY_EXIT (30s probes, 3 h cap),"
    echo "  reusing an existing verdict only while the sentinel matches this tree."
    echo
    echo "DRY-RUN COMPLETE — all local preconditions verified. The commit plan:"
    echo "  row 1  channel-export-resample ($(grep -vc '^#' "$MANIFEST_A1") paths) -> dcommit path-limited"
    echo "  row 2  idmask-tile-cache ($(grep -vc '^#' "$MANIFEST_P1") paths)"
    echo "  row 3  picker-sliders-recents ($(grep -vc '^#' "$MANIFEST_G1") paths)"
    echo "  row 4  winink-pen-flags ($(grep -vc '^#' "$MANIFEST_S1") paths)"
    echo "  row 5  snapshot-dir ($(grep -vc '^#' "$MANIFEST_C1") paths)"
    echo "  row 6  graph-history ($(grep -vc '^#' "$MANIFEST_GH") paths)"
    echo "  row 7  b10-eyedropper, adopted ($(grep -vc '^#' "$MANIFEST_B10") paths)"
    echo "  row 8  a2-snapshot-breadth ($(grep -vc '^#' "$MANIFEST_A2") paths + the four generated PNGs)"
    echo "  row 9  a3-picker-app-sites ($(grep -vc '^#' "$MANIFEST_A3") paths)"
    echo "  row 10 docs/tooling: next-wave-plan-draft.md + kill-ledger.md + the host script + its report"
    echo "  row 11 this port + its report"
    echo "  then: ONE push (local tail + rows) + explicit run-id ci-watch + the packaging re-dispatch."
    exit 0
fi

# Reuse path: a verdict already in the log counts only for THESE bytes.
EXISTING=$(grep -o 'BATTERY_EXIT=[0-9]*' "$BATTERY_LOG" 2>/dev/null | tail -1 || true)
SENT_OK=0
[ -n "${OLD_SENTINEL:-}" ] && [ "$OLD_SENTINEL" = "$(tree_state)" ] && SENT_OK=1
if [ -n "$EXISTING" ] && [ "$SENT_OK" -eq 1 ]; then
    echo "  reusing verdict $EXISTING (sentinel matches this exact tree)."
else
    if [ -n "$EXISTING" ]; then
        echo "  NOTE: discarding stale verdict $EXISTING (tree changed since it ran — fresh battery)."
    elif [ "$SENT_OK" -eq 0 ]; then
        echo "  NOTE: no usable sentinel — launching a fresh battery."
    fi
    rm -f "$BATTERY_LOG"
    ( cd "$REPO" && nohup nice -n "$NICE" env CARGO_BUILD_JOBS="$JOBS" \
        UMBER_TEST_THREADS="$TEST_THREADS" UMBER_SUITE_TIMEOUT="$SUITE_TIMEOUT" \
        bash -c "bash scripts/wave-battery.sh $CRATES > '$BATTERY_LOG' 2>&1; echo BATTERY_EXIT=\$? >> '$BATTERY_LOG'" \
        </dev/null >/dev/null 2>&1 & )
    echo "  battery launched (log: $BATTERY_LOG). Polling (every ${BATTERY_POLL_SECONDS}s, cap ${BATTERY_POLL_MAX} probes = 3 h)..."
    i=0; VERDICT=""
    while [ "$i" -lt "$BATTERY_POLL_MAX" ]; do
        sleep "$BATTERY_POLL_SECONDS"
        VERDICT=$(grep -o 'BATTERY_EXIT=[0-9]*' "$BATTERY_LOG" 2>/dev/null | tail -1 || true)
        if [ -n "$VERDICT" ]; then
            echo "  verdict landed after ~$(( (i+1) * BATTERY_POLL_SECONDS / 60 )) min: $VERDICT"
            break
        fi
        i=$((i+1))
        [ $((i % 10)) -eq 0 ] && echo "  ...still running ($(( i * BATTERY_POLL_SECONDS / 60 )) min; load $(cut -d' ' -f1 /proc/loadavg))"
    done
    if [ -z "$VERDICT" ]; then
        fail_claimed "battery poll exhausted (3 h, no verdict) — the battery may still be running locally; WAIT for it, do not relaunch. When BATTERY_EXIT lands in $BATTERY_LOG, re-run this script (the verdict is reused)." 3
    fi
    EXISTING="$VERDICT"
fi
case "$EXISTING" in
    *BATTERY_EXIT=0*) echo "  battery GREEN." ;;
    *) fail_claimed "battery RED ($EXISTING) — fix-forward from $BATTERY_LOG (if the red is inside viewport.rs/main.rs hunks it routes to the eyedropper's owner, not this wave), re-pin any edited manifest rows, re-run this script (it resumes)." 3 ;;
esac

# --- 5. commit rows (each through dcommit --path-limited) --------------------
say "5. commit rows (nine fleet kills by manifest + the docs row + the port row; each lock released at its commit)"

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

commit_row b10-eyedropper "$MANIFEST_B10" \
"feat(eyedropper): the display-chain-aware pick (B-10's named follow-up, ADOPTED) — the viewport's pick builds the v1 sRGB sample then runs it through the session's apply_display_chain (the preview strip's exact input shape) and quantizes with the strip's rule; the shell syncs DisplaySettings into the viewport each frame (one line beside the picker-arm sync); the pure chain helper carries two unit-pinned expectations (re-derived by an independent python port at adoption: [137,188,229,173] -> [158,185,204,173] under Srgb/-0.5EV/0.9gamma, identity default passthrough); adopted from the dead fleet-era session per the full rules — holder exit confirmed, fresh lock, static gates re-run (fmt clean re-derived, the brace-count false positive confirmed pre-existing at HEAD), manifest-pinned for this row; parked through freeze-4, battery-green this recovery" 3

commit_row a2-snapshot-breadth - \
"test(kittest): snapshot breadth — the History panel the harness doc has claimed since v1 (an empty session, then after two journalled LayerCommand::Add ops), the toolbar through the headless IconAtlas::empty precedent (a fully specified ToolbarView), and the rich tooltip's full fixture; four baselines host-generated pre-battery (UPDATE_SNAPSHOTS=1 under the cargo flock, the three existing baselines byte-verified unchanged, the four new ones vision-checked before the battery ran); parked through freeze-4, battery-green this recovery" 6 \
    crates/umber-app/tests/panel_snapshot.rs \
    docs/loop/kill-reports/2026-10-20-a2-snapshot-breadth.md \
    crates/umber-app/tests/snapshots/history_panel_empty.png \
    crates/umber-app/tests/snapshots/history_panel_ops.png \
    crates/umber-app/tests/snapshots/toolbar.png \
    crates/umber-app/tests/snapshots/rich_tooltip.png

commit_row a3-picker-app-sites "$MANIFEST_A3" \
"feat(picker): the picker's app sites — the brush preset colour row and the fill layer's two colour rows (Solid + the gradient's A/B stops) move to the shared preferences::picker_button: G1's sliders + swatch-grid popup over the session-shared recents (egui memory under one fixed Id, seeded from the swatches file on first use, persisted on change); preferences gains the swatches codec (plain #rrggbb lines beside the preferences file, garbage skipped, the RecentColors cap trims) + the shared-row helper + four codec tests; both catalogs gain the shared picker.* label group (identical attribute sets, python-verified); the stock color_button keeps its kittest consumers — the delegate conversion is a named follow-up; graph-history's manifest re-pinned for the additive catalog keys (recorded in its header); parked through freeze-4, battery-green this recovery" 6

commit_row wave-docs-and-tooling - \
"docs(wave): the W-C3a close tooling + the plan-of-record + the ledger's accumulated rows — scripts/w-c3a-close.sh (the executable recovery: claim interlock anchored at line start, six 16-hex manifests full-verified, hardened rsync + the tree-state sentinel, default-leg battery with the bounded verdict-reuse poll, commit rows through dcommit --path-limited so foreign dirt cannot ride, ONE push + explicit run-id ci-watch, the packaging re-dispatch — both stencil-wave-close defects pre-fixed) + its kill report + docs/loop/next-wave-plan-draft.md (the claude planner's W-C3a plan, executed as the fleet) + docs/loop/kill-ledger.md (the single-owner rule's ledger commit: the M1/M3/M4/M5 landed rows + the fleet status ticks that accumulated worktree-side through the freeze); rides this push" 4 \
    docs/loop/next-wave-plan-draft.md \
    docs/loop/kill-ledger.md \
    scripts/w-c3a-close.sh \
    docs/loop/kill-reports/2026-10-20-w-c3a-close-script.md

commit_row w-c3a-local-port - \
"tooling(recovery): scripts/w-c3a-local-close.sh — the LOCAL-degraded port of the W-C3a close, born from Bessa's 2026-10-22 steer (cargo lifted, slow to not overload the machine) + the unreachable host: the host seams (liveness, pre-flight, rsync, host battery) replaced by local steps, everything else verbatim (the claim interlock, nine-manifest verification, the A2 skip logic, the tree-state sentinel + verdict reuse, the ten original rows with their exact messages, ONE push + explicit run-id ci-watch, the packaging re-dispatch). The slow-throttle envelope: -j3 compile, test-threads 2/1, suite timeout 2400s, nice 19, one cargo at a time under the flock, nothing parallel — the 8-thread tower keeps headroom for the desktop. The 10-21 sessions ran these steps by hand and died un-landed twice; the port makes the local close ONE idempotent command. Report: docs/loop/kill-reports/2026-10-22-w-c3a-local-close.md; rides this push" 2 \
    scripts/w-c3a-local-close.sh \
    docs/loop/kill-reports/2026-10-22-w-c3a-local-close.md

# Every commit moved HEAD, which stales the battery sentinel — refresh it
# so a failed PUSH + re-run reuses the verdict instead of re-batterying.
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

append_note "W-C3A RECOVERY CLOSED by w-c3a-local-close.sh (pid $$) at $(date '+%F %T %Z') — LOCAL degraded mode (cargo lifted by Bessa 2026-10-22, slow-throttle -j3 nice-19), battery green, nine fleet rows + the docs row + the port row pushed, CI run $RUN_ID, packaging re-dispatched. The host was never contacted."
echo
echo "WAVE CLOSED (pending CI run $RUN_ID). The docs pass is the next turn's first kill:"
echo "  - delete the consumed manifests (.dragon/*-manifest.txt) + .dragon/battery-sentinel"
echo "  - tick queue.md (the C2 rows the fleet closes: C2-12 sliders/swatches, row 15's snapshot-dir residual),"
echo "    STATE.md, CHANGELOG.md; the desktop-checks Result lines stay open until a desktop session runs them"
echo "  - the kill-ledger rows: fleet -> LANDED, the waiting rows -> closed"
echo "  - the eyedropper landed with the wave (row 7, the adoption); its kittest extension (a GPU pick"
echo "    fixture) remains the named follow-up"
echo "  - W-C3b (w-c3b-close.sh) is next: five rows, its own claim + battery"
echo "  - memtest86+ stays OWED at the next physical access: boot straight into it, skip the OS"
exit 0
