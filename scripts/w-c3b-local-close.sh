#!/usr/bin/env bash
# w-c3b-local-close.sh — the LOCAL-degraded-mode port of w-c3b-close.sh.
#
# Why this exists: centralhost is unreachable (fragile boot since
# 2026-10-21 ~13:32, memtest86+ owed, checking STOOD DOWN by Bessa's
# order — the host stays untouched). The W-C3a close already ran
# LOCALLY through its own port (96edd1e) under Bessa's 2026-10-22 ~13:1x
# steer ("continue working autonomously cargo lifted, but do it slow to
# not overload the machine"); this port gives the W-C3b sweep the same
# one-command shape. The SLOW THROTTLE ENVELOPE:
#   CARGO_BUILD_JOBS=${W3B_JOBS:-3}   (compile parallelism capped)
#   UMBER_TEST_THREADS=${W3B_TEST_THREADS:-2}  (non-GPU suites)
#   UMBER_GPU_TEST_THREADS=1          (wave-battery default, kept)
#   UMBER_SUITE_TIMEOUT=${W3B_TIMEOUT:-2400}   (throttled builds need room)
#   nice -n ${W3B_NICE:-19}           (the desktop keeps priority)
# One cargo at a time under /tmp/umber-cargo.lock (wave-battery's own
# flock), nothing parallel, and — the 10-22/23 reboot lesson — NO other
# heavy job on the machine while the battery runs: step 2 refuses while
# the kitten-voicesweep bake (or any cargo) is live.
#
# From the host original (w-c3b-close.sh), kept: the W-C3B claim
# interlock (anchored at line start), the W-C3A-must-be-CLOSED gate,
# the four 16-hex manifests FULL verification with the
# truncated-prefix parser, the push gate check, the tree-state battery
# sentinel (verdict reuse only while the bytes match), the commit rows
# through dcommit --path-limited with the exact messages, ONE push +
# explicit run-id ci-watch — now hardened with the workflow filter +
# the job-list assertion (two workflows fire on every push; a headSha
# filter alone can hand the watcher the packaging run).
#
# Two deliberate additions over the original, both born from the
# 2026-10-23 post-close reds:
#   1. The post-rows scorecard regen + --check gate: the W-C3a close
#      pushed with a stale README scorecard and red the layering gate
#      (the fleet crossed the 2,200 test floor); this close verifies
#      the count AFTER its rows commit and lands the regen BEFORE the
#      push, so a battery fix-forward can never push a stale count.
#   2. The post-push packaging re-dispatch: the original deferred the
#      flatpak verdict gate to w-c3a's close, whose dispatch
#      (39908210580) is now CONSUMED-RED at the runner's bwrap/rofiles
#      layer (the ubuntu-26.04 rotation; the job is pinned to
#      ubuntu-24.04, riding this push) — the verdict gate moves here,
#      on the pinned runner, on this push's tip.
#
# The port + its own report are committed by the authoring session
# BEFORE the battery (battery-exempt tooling, rides this push), so this
# script's docs row commits only the host original + its report + the
# brief + the ledger.
#
# Usage (from a Hermes session so HERMES_SESSION_ID attributes the
# commits, or export DRAGON_SESSION=<id> first — dcommit refuses
# unattributed commits, fail-closed by design). The hardened launch
# shape (the oomd + session-death lessons — own cgroup, own shield,
# survives the session):
#   systemd-run --user --unit=w-c3b-local-close --collect \
#     -p MemoryHigh=8G -p MemorySwapMax=2G \
#     --setenv=DRAGON_SESSION=<session-id> \
#     bash "$HOME/Projects/umber/scripts/w-c3b-local-close.sh"
#   (journal: journalctl --user-unit=w-c3b-local-close -f; the battery
#    log the script polls is /tmp/w-c3b-local-battery-r1.log)
#   bash scripts/w-c3b-local-close.sh --dry-run
#   (verifies everything checkable now and prints the plan)
#
# Exit codes: 0 wave closed (pending CI); 3 battery RED or poll
# exhausted; 4 interlock/precondition refusal.
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
REPO="$HOME/Projects/umber"
NOTE="$REPO/.dragon/COORDINATION-next-wave.md"
MANIFEST_USDZ="$REPO/.dragon/usdz-sublayer-walk-manifest.txt"
MANIFEST_BLEND="$REPO/.dragon/blend-replace-opacity-manifest.txt"
MANIFEST_SOLO="$REPO/.dragon/solo-height-manifest.txt"
MANIFEST_TAN="$REPO/.dragon/tangent-frame-manifest.txt"
BATTERY_LOG="/tmp/w-c3b-local-battery-r1.log"
SENTINEL="$REPO/.dragon/battery-sentinel-c3b"
CRATES="umber-mesh umber-core umber-gpu umber-app"
BATTERY_POLL_SECONDS=30
BATTERY_POLL_MAX=360    # 360 x 30s = 3 h cap (throttled builds are slow by design)
JOBS="${W3B_JOBS:-3}"
TEST_THREADS="${W3B_TEST_THREADS:-2}"
SUITE_TIMEOUT="${W3B_TIMEOUT:-2400}"
NICE="${W3B_NICE:-19}"
export PATH="$HOME/.cargo/bin:$PATH"

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*" >&2; exit "${2:-4}"; }
append_note() { printf '\n%s\n' "$*" >> "$NOTE"; }

claim_state() {
    # The note's LAST claim-ish line decides, ANCHORED TO LINE START —
    # the note's prose quotes sentinel strings mid-line and must never
    # match. tac | grep -m1, never a plain grep.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3B RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
c3a_state() {
    # Same shape for the W-C3A sentinel: this wave closes strictly
    # after that one (already CLOSED per the 02:52 line — this gate
    # proves it, not assumes it).
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3A RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
relinquish() {
    append_note "W-C3B RECOVERY RELINQUISHED by w-c3b-local-close.sh at $(date '+%F %T %Z') — $1"
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
    # FULL verification (mismatch = wrote-after-pinning drift). The
    # house pin convention is a TRUNCATED 16-hex sha256 prefix —
    # `sha256sum -c` cannot consume these files at all; this parser
    # is the correct checker.
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
say "0. LOCAL degraded mode (centralhost unreachable since 2026-10-21, fragile, memtest86+ owed — the host is NOT contacted; cargo LIFTED by Bessa 2026-10-22, slow envelope: -j$JOBS, test-threads $TEST_THREADS/1, timeout ${SUITE_TIMEOUT}s, nice $NICE, ONE cargo stack machine-wide, no other heavy job)"

# --- 0b. claim interlock (.dragon/COORDINATION-next-wave.md) ----------------
say "0b. claim interlock (W-C3B sentinel, anchored at line start)"
CLAIMED=$(claim_state)
if [ -n "$CLAIMED" ]; then
    case "$CLAIMED" in
        *CLAIMED*) die "the note's last W-C3B claim line is '$CLAIMED' with no relinquish — another session owns this wave's recovery; it must append W-C3B RECOVERY RELINQUISHED first" 4 ;;
        *) echo "last W-C3B claim state: $CLAIMED (clear to proceed)." ;;
    esac
else
    echo "no prior W-C3B claim — this run claims it."
fi

# --- 0c. the W-C3A gate (this wave closes strictly after that one) ----------
say "0c. the W-C3A gate"
C3A=$(c3a_state)
if [ -z "$C3A" ]; then
    if [ "$DRY" -eq 1 ]; then
        echo "  (dry-run) no W-C3A RECOVERY line yet — a REAL run exits 4 here."
        echo "  (dry-run) continuing so this wave's own preconditions still get verified below."
    else
        die "no W-C3A RECOVERY line in the note — run the W-C3a close FIRST" 4
    fi
else
    case "$C3A" in
        *CLOSED*) echo "W-C3A recovery: $C3A — clear to proceed." ;;
        *CLAIMED*)
            if [ "$DRY" -eq 1 ]; then
                echo "  (dry-run) the W-C3A recovery is still CLAIMED — a REAL run exits 4 here; continuing."
            else
                die "the W-C3A recovery is still CLAIMED ('$C3A') — wait for its close before running this script" 4
            fi ;;
        *)
            if [ "$DRY" -eq 1 ]; then
                echo "  (dry-run) the W-C3A recovery's last line is '$C3A' — a REAL run exits 4 here; continuing."
            else
                die "the W-C3A recovery's last line is '$C3A' — treat it as unresolved; wait for its CLOSED line" 4
            fi ;;
    esac
fi

if [ "$DRY" -eq 0 ]; then
    append_note "W-C3B RECOVERY CLAIMED by w-c3b-local-close.sh (pid $$) at $(date '+%F %T %Z') — LOCAL degraded mode (slow envelope, one heavy job)"
fi

# --- 1. preconditions --------------------------------------------------------
say "1. preconditions (all FOUR manifests FULLY hash-verified, push gate, ownership map)"
for m in "$MANIFEST_USDZ" "$MANIFEST_BLEND" "$MANIFEST_SOLO" "$MANIFEST_TAN"; do
    [ -f "$m" ] || die "manifest missing: $m"
done
cd "$REPO" || die "repo missing: $REPO"
verify_manifest "$MANIFEST_USDZ" || die "usdz-sublayer-walk manifest drift — fix-forward changed pinned bytes. Re-pin the rows you edited (sha256sum <path> | cut -c1-16), then re-run." 4
verify_manifest "$MANIFEST_BLEND" || die "blend-replace-opacity manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_SOLO" || die "solo-height manifest drift — reconcile the edited rows, then re-run." 4
verify_manifest "$MANIFEST_TAN" || die "tangent-frame manifest drift — reconcile the edited rows, then re-run." 4
grep -q '"approved": true' docs/loop/push-gate.json || die "push gate not approved"
echo "push gate: approved (standing, 2026-10-12)."

echo "sweep lock census (a lock held = its kill is still parked; released at its commit row):"
for lock in usdz-sublayer-walk blend-replace-opacity solo-height tangent-frame; do
    if [ -f "/tmp/umber-kill-locks/$lock.lock" ]; then
        echo "  lock live: $lock ($(wc -l < "/tmp/umber-kill-locks/$lock.lock") files)"
    else
        echo "  lock absent from registry: $lock (released-by-cleanup; this run owns the rows via the claim)"
    fi
done

echo "Ownership map — this sweep's parked bytes (a red inside any of them during the battery routes here):"
git status --porcelain -- crates/umber-mesh/src/usdc.rs crates/umber-core/src/layers.rs \
    crates/umber-gpu/src/compositor.rs crates/umber-gpu/src/shaders.rs \
    crates/umber-gpu/src/view_mode.rs crates/umber-gpu/src/renderer.rs \
    crates/umber-app/src/layers_panel.rs crates/umber-mesh/src/tangents.rs \
    crates/umber-mesh/src/lib.rs | sed 's/^/  /'

LOCAL_TIP=$(git rev-parse HEAD)
ORIGIN_TIP=$(git rev-parse origin/master 2>/dev/null || echo unknown)
[ "$LOCAL_TIP" = "$ORIGIN_TIP" ] || \
    echo "NOTE: local HEAD $LOCAL_TIP != origin/master $ORIGIN_TIP — the unpushed rows (the segv pin, the flatpak pin, the docs pass, this port) ride this push too."
echo "preconditions green (tree at $LOCAL_TIP)."

# --- 2. local pre-flight -----------------------------------------------------
say "2. local pre-flight (live local cargo = stand down; one heavy job machine-wide)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: pgrep -ax cargo/rustc + the kitten-voicesweep unit check; refuse if either is live"
else
    if pgrep -ax cargo >/dev/null 2>&1 || pgrep -ax rustc >/dev/null 2>&1; then
        fail_claimed "a cargo/rustc is LIVE on this machine — a sibling or Bessa is mid-build; stand down" 4
    fi
    echo "  no live cargo/rustc."
    SWEEP=$(systemctl --user is-active kitten-voicesweep 2>/dev/null || true)
    if [ "$SWEEP" = "active" ]; then
        fail_claimed "the kitten-voicesweep bake is still RUNNING (unit active) — heavy jobs serialize one at a time (the 10-22/23 reboot lesson); re-run this script when the sweep completes (SWEEP_DONE marker present)" 4
    fi
    echo "  voicesweep not running (${SWEEP:-no unit}) — the heavy-job line is clear."
    echo "  load: $(cut -d' ' -f1-3 /proc/loadavg) on $(nproc) threads."
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

# --- 4. wave battery (LOCAL, throttled; default features only) ---------------
say "4. wave battery (LOCAL, throttled: -j$JOBS nice $NICE): $CRATES (default features; NEVER --features gpu inside a recovery window)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: rm -f $BATTERY_LOG + nohup nice -n $NICE env CARGO_BUILD_JOBS=$JOBS UMBER_TEST_THREADS=$TEST_THREADS UMBER_SUITE_TIMEOUT=$SUITE_TIMEOUT wave-battery.sh $CRATES"
    echo "  (dry-run) then poll $BATTERY_LOG for BATTERY_EXIT (30s probes, 3 h cap),"
    echo "  (dry-run) reusing an existing verdict only while the sentinel matches this tree."
    echo
    echo "DRY-RUN COMPLETE — all local preconditions verified. The commit plan:"
    echo "  row 1  usdz-sublayer-walk ($(grep -vc '^#' "$MANIFEST_USDZ") paths) -> dcommit path-limited"
    echo "  row 2  blend-replace-opacity ($(grep -vc '^#' "$MANIFEST_BLEND") paths)"
    echo "  row 3  solo-height ($(grep -vc '^#' "$MANIFEST_SOLO") paths)"
    echo "  row 4  tangent-frame ($(grep -vc '^#' "$MANIFEST_TAN") paths)"
    echo "  row 5  docs/tooling: the host original + its report + the tangent-wiring brief + the ledger"
    echo "  row 5b scorecard: regen + --check gate; commits README.md if the rows moved the count"
    echo "  then: ONE push + explicit run-id ci-watch (workflow-filtered, job-list asserted)"
    echo "        + the post-push packaging re-dispatch (the flatpak verdict gate, pinned runner)."
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
    *) fail_claimed "battery RED ($EXISTING) — fix-forward from $BATTERY_LOG (open the per-crate logs in the newest /tmp/wave-battery.* dir; a red inside this sweep's files routes to the W-C3b reports), re-pin any edited manifest rows, re-run this script (it resumes)." 3 ;;
esac

# --- 5. commit rows (each through dcommit --path-limited) ---------------------
say "5. commit rows (four sweep kills by manifest + the docs row; each lock released at its commit)"

commit_row usdz-sublayer-walk "$MANIFEST_USDZ" \
"feat(usdz): load_usdz walks the sublayer stack — the release checklist's .usdz-beyond-first-layer deferral closed. Root-layer precedence preserved (a root that authors a Mesh never reads a sublayer); then the root's subLayers in authored order (USD opinion strength), depth-first, resolved INSIDE the package through Archive::contains/read; a missing or unreadable sublayer is skipped per USD's layer-registry tolerance (a later one may hold the mesh); the walk is cycle-guarded and depth-bounded (16) — a sublayer graph is untrusted input; escaping paths (.., absolute) are rejected lexically (USDZ forbids leaving the package); a found-but-malformed Mesh is a hard error (the honest first-Mesh contract). 8 pinned tests, fixtures programmatic through usdz::ArchiveWriter (raw .usda texts, stored entries — the panic-hunt usdz_package pattern); the python port of the normalization agrees 12/12. Requirements row 111's boundary closed; parked through freeze-4, battery-green this recovery" 2

commit_row blend-replace-opacity "$MANIFEST_BLEND" \
"feat(blend): Replace's bypass — requirements row 39's first named v1 boundary closed. The mode's documented contract implemented at every coverage gate: the layer opacity AND the source-alpha grading are bypassed (a present texel, any alpha > 0, overwrites in full; only a fully-transparent texel is a no-op — the premultiplied arm's cov == 0 invariant stays load-bearing; the mask still clips, it is coverage not opacity). umber-core's composite_channel returns the blend result unmixed for Replace (even at opacity 0 — the no-effect rule belongs to opacity, and Replace bypasses it); the umber-gpu oracle gates cov = presence x mask x opacity-1 for the mode and the WGSL stack compositor mirrors the same gate AFTER the un-premultiply arm (var sa); the panel's channel-opacity field greys for Replace rows (an enabled editor for a bypassed value would be a lie — no new i18n string, the catalogs sit in a parked lock; the stored value rides along untouched). The variant doc's NOT-implemented-yet clause closed; the equivalence invariant (composite_pixel <-> composite_channel fold) holds by construction, both sides bypass; every surviving Replace pin re-derived (all full-opacity or census-structural — the red-risk sweep is in the report); the python port agrees 4/4. Row 39's second boundary (PerChannel routing) stays design-gated; parked through freeze-4, battery-green this recovery" 4

commit_row solo-height "$MANIFEST_SOLO" \
"feat(view-modes): the Solo/Preview height arm reads the bound height tile — requirements row 79's boundary closed, the view_mode doc's named small separate follow-up. solo_proxy's HEIGHT arm samples binding 12 through the SAME addressing chain and the SAME PAINT_HAS_HEIGHT_TEXTURE flag gate as height_perturbed_normal (solo and Lit can never disagree about which texel a pixel reads); the mid-gray stand-in stays for a mesh with no height channel (the fallback texture is never sampled while the flag is clear); the mesh-map PREVIEW_HEIGHT delegation inherits the real data; binding 12 is layout-static in the shared group 0 so the shader arm alone reaches both paths — zero renderer changes. The GPU golden solo_height_reads_the_bound_height_tile mirrors the height_stroke precedent (unbound ~128 stand-in; the u-ramp's texel (1,1)=85/255 -> ~85; skip-graceful without an adapter). parked through freeze-4, battery-green this recovery" 4

commit_row tangent-frame "$MANIFEST_TAN" \
"feat(mesh): per-vertex tangent frames, the mikkTSpace way — the requirements row 79 named follow-up's foundation. umber's importer output is per-corner EXPANDED (indices always sequential), so textbook index-sharing accumulation would never smooth anything: corners weld into groups by the bit-exact (position, normal, uv) tuple first — the graphics-standard tangent-seam rule (a UV seam legitimately splits groups, keeping per-island frames) — then per-triangle Lengyel UV-derivative tangents accumulate into the weld groups, Gram-Schmidt against the group normal, handedness from sign(dot(cross(N, T), B_sum)) (the compact encoding B = handedness x cross(N, T)). Never panics on untrusted input (out-of-range/short-attribute triangles skipped, % 3 remainder ignored, zero-UV-area triangles contribute nothing, zero-contribution groups take the deterministic least-dominant-axis perpendicular fallback). 7 unit tests; the python port agrees 7/7 (and caught a vacuous seam fixture BEFORE any test ran — u ran along +X on the XZ triangle; the fixture was fixed, not the assertion). The consumer half — the renderer wiring (vertex attribute + pipeline layouts + the TBN height gradient + the lit goldens re-derived over the solo-height pins) — is the named next-after-recovery kill; parked through freeze-4, battery-green this recovery" 3

commit_row wave-docs-and-tooling - \
"docs(wave): the W-C3b close tooling + the sweep's docs rows — scripts/w-c3b-close.sh (the host recovery original: the W-C3B claim interlock anchored at line start + the W-C3A-must-be-CLOSED gate, four 16-hex manifests full-verified, hardened rsync + this wave's own tree-state sentinel, default-leg battery umber-mesh umber-core umber-gpu umber-app with the bounded verdict-reuse poll, commit rows through dcommit --path-limited so foreign dirt cannot ride, ONE push + explicit run-id ci-watch) + its kill report + the tangent-wiring brief + the ledger as it stands at close time. The close itself ran through the LOCAL port (scripts/w-c3b-local-close.sh + its report, committed ahead of the battery), which adds the post-rows scorecard gate and the post-push packaging re-dispatch — the w-c3a close's dispatch (39908210580) is consumed-red at the runner's bwrap/rofiles layer and the flatpak verdict gate moves to this push's re-dispatch on the pinned ubuntu-24.04 runner; rides this push" 3 \
    scripts/w-c3b-close.sh \
    docs/loop/kill-reports/2026-10-20-w-c3b-close-script.md \
    docs/loop/briefs/2026-10-20-tangent-wiring.md \
    docs/loop/kill-ledger.md

# --- 5b. scorecard gate (the W-C3a close's miss; the layering gate's tooth) --
say "5b. scorecard regen + --check (post-rows: the pushed tip's count must match the tree)"
python3 scripts/scorecard.py || fail_claimed "scorecard regen failed" 4
if ! python3 scripts/scorecard.py --check; then
    fail_claimed "scorecard --check RED after regen — the count moved past the span rewrite; diagnose before the push" 4
fi
if ! git diff --quiet -- README.md; then
    commit_row scorecard-current - \
"docs(scorecard): README headline regenerated at the W-C3b close — the rows' test count moved the floor; the post-rows --check gate (the W-C3a close's miss, red as the wave tip's layering gate) now refuses any push with a stale span" 1 \
    README.md
else
    echo "  README.md already current — no scorecard row needed."
fi

# Every commit moved HEAD, which stales the battery sentinel written
# before them — refresh it so a failed PUSH + re-run reuses the verdict.
tree_state > "$SENTINEL"

# --- 6. ONE push + ci-watch (workflow-filtered, job-list asserted) -----------
say "6. ONE push + explicit run-id resolve + ci-watch"
git -C "$REPO" push origin master || fail_claimed "push failed" 4
sleep 10
HEADSHA=$(git -C "$REPO" rev-parse HEAD)
SLUG=$(git -C "$REPO" remote get-url origin | sed 's/.*github.com[:/]//;s/.git$//')
# TWO workflows fire on every push (ci + packaging) — filter by workflow
# name, then assert the job list, so the watcher cannot latch the
# packaging run (the stencil-wave-close defect class).
RUN_ID=$(gh run list --repo "$SLUG" --workflow ci --limit 5 --json databaseId,headSha \
    -q ".[] | select(.headSha == \"$HEADSHA\") | .databaseId" | head -1)
[ -n "$RUN_ID" ] || fail_claimed "could not resolve the ci run id for $HEADSHA — watch manually" 4
gh run view "$RUN_ID" --repo "$SLUG" --json jobs -q '.jobs[].name' 2>/dev/null \
    | grep -q 'fmt + clippy + test' \
    || fail_claimed "run $RUN_ID is not the ci workflow's check matrix (job list mismatch) — resolve the right run before arming ci-watch" 4
echo "  pushed $HEADSHA; CI run $RUN_ID (job list asserted)."
nohup "$REPO/scripts/ci-watch.sh" "$RUN_ID" >/dev/null 2>&1 &
echo "  ci-watch detached on run $RUN_ID (pattern: CI_VERDICT=)."

# --- 7. post-push packaging re-dispatch (the flatpak chain's verdict gate) ----
say "7. post-push re-dispatch: packaging (PUSH FIRST — the manifest clones master at build time; the flatpak job now runs on the pinned ubuntu-24.04 runner)"
if gh workflow run packaging.yml --repo "$SLUG" --ref master 2>/dev/null; then
    echo "  packaging dispatched on master — the flatpak chain's verdict gate (the archive-type"
    echo "  + strip-components + offline-deps fixes + the runner pin get their first full"
    echo "  pinned-image run on a tip that carries all four)."
else
    echo "  NOTE: packaging dispatch failed — re-run: gh workflow run packaging.yml --repo $SLUG --ref master"
fi

append_note "W-C3B RECOVERY CLOSED by w-c3b-local-close.sh (pid $$) at $(date '+%F %T %Z') — LOCAL degraded mode (slow envelope -j$JOBS nice-$NICE, one heavy job), battery green, four sweep rows + the docs row + the scorecard row pushed, CI run $RUN_ID (workflow-filtered, job-list asserted), packaging re-dispatched. The host was never contacted."
echo
echo "WAVE CLOSED (pending CI run $RUN_ID + the packaging dispatch). The docs pass is the next turn's first kill:"
echo "  - delete the consumed manifests (.dragon/usdz-sublayer-walk-manifest.txt,"
echo "    blend-replace-opacity-manifest.txt, solo-height-manifest.txt,"
echo "    tangent-frame-manifest.txt) + .dragon/battery-sentinel-c3b"
echo "  - release-checklist-0.1.0.md: the .usdz-beyond-first-layer known-deferred row ticks"
echo "  - requirements.md rows 39 (first boundary) / 79 (Solo/Preview height) / 111 (usdz)"
echo "  - the kill-ledger rows: sweep -> LANDED"
echo "  - M1 pass 4 (the standing stale-row sweep) at the wave close, per the standing rule"
echo "  - the pinned-image CI verdict + the pinned-runner flatpak verdict = this wave's gates"
exit 0
