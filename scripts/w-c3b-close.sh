#!/usr/bin/env bash
# w-c3b-close.sh — the W-C3b (free-file boundary sweep) recovery runbook,
# executable. Runs ONLY AFTER scripts/w-c3a-close.sh has CLOSED its wave
# (step 0c gates on it): serial closes, one cargo flock, one push per wave.
#
# The sweep (FOUR kills, each with a 16-hex manifest pinned 2026-10-20
# by local-4d2e8a71 from its kill report's file list):
#   usdz-sublayer-walk        umber-mesh/usdc.rs (.usdz sublayer stack)
#   blend-replace-opacity     umber-core/layers.rs + umber-gpu/compositor.rs
#                            + umber-app/layers_panel.rs (the mode's bypass)
#   solo-height               umber-gpu shaders/view_mode/renderer (the
#                            Solo/Preview height arm reads binding 12)
#   tangent-frame             umber-mesh tangents.rs NEW + lib.rs (the
#                            per-vertex tangent frames, row 79's
#                            follow-up foundation; the renderer wiring
#                            is the named next-after-recovery kill)
# plus the docs/tooling row (this script + its report + the kill-ledger's
# accumulated W-C3b rows — appended AFTER w-c3a's docs row landed the
# ledger's earlier state, per the single-owner rule).
#
# Built on the w-c3a-close.sh pattern with both recorded defect classes
# pre-fixed: the claim-grep anchors at line start (tac | grep -m1), and
# the battery-verdict reuse gates on a tree-state sentinel (HEAD +
# porcelain + dirty BYTES) so a finished verdict resumes while a
# fix-forward forces a fresh battery.
#
# Cross-wave dirt, named up front: the w-c3a battery (which runs FIRST)
# rsyncs the WHOLE tree, so this sweep's dirty bytes ride its umber-app
# compile. A red inside usdc.rs / layers.rs / compositor.rs / shaders.rs /
# view_mode.rs / renderer.rs / layers_panel.rs during the W-C3a recovery
# routes to THIS wave's owner (local-4d2e8a71), not to the fleet — the
# manifests + kill reports are the routing map. Static verification
# host-free: rustfmt parse-clean x all files, every openusd/wgpu symbol
# grep-verified, python ports agreeing, and a red-risk sweep over every
# existing Replace/usdz/solo pin (all full-opacity or structural).
#
# Steps (idempotent, re-runnable; refuses to guess past any failure):
#   0. host liveness (port 22)
#   0b. the W-C3B claim interlock (line-start anchored)
#   0c. the W-C3A gate: the last W-C3A claim line must read CLOSED
#   1. preconditions: all THREE manifests fully hash-verified, push gate,
#       lock census, the dirty-file ownership map
#   2. host pre-flight: reflog -5, live cargo check, tip + porcelain
#   3. rsync the tree (target/.git/.dragon/Cargo.lock excluded), then
#       write THIS wave's battery sentinel
#   4. wave battery on the host: umber-mesh umber-core umber-gpu
#       umber-app (default features ONLY — never --features gpu inside
#       a recovery window; the 94469e8 wgpu deadlock class). Bounded
#       BATTERY_EXIT poll; an existing verdict is reused only while
#       the sentinel matches the tree.
#   5. commit rows, each by manifest THROUGH dcommit --path-limited
#       (ledger-attributed, foreign dirt cannot ride), each lock
#       released at its own commit; then the docs row; then the
#       sentinel refresh.
#   6. ONE push, explicit run-id resolve from headSha, ci-watch detached.
#       NO packaging re-dispatch here: w-c3a's close owns the flatpak
#       verdict-gate dispatch, and the push trigger itself runs the
#       AppImage+lint legs on this tip.
#
# Exit codes: 0 wave closed (pending CI); 2 host down; 3 battery RED or
# poll exhausted; 4 interlock/precondition refusal.
#
# Usage (from a Hermes session so HERMES_SESSION_ID attributes the
# commits, or export DRAGON_SESSION=<id> — dcommit refuses
# unattributed commits, fail-closed by design):
#   nohup bash scripts/w-c3b-close.sh > /tmp/w-c3b-close.log 2>&1 &
#   (the battery poll can run long — run DETACHED, then tail the log)
#   bash scripts/w-c3b-close.sh --dry-run
set -u

DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1
HOST=bessa@192.168.15.64
REPO="$HOME/Projects/umber"
NOTE="$REPO/.dragon/COORDINATION-next-wave.md"
MANIFEST_USDZ="$REPO/.dragon/usdz-sublayer-walk-manifest.txt"
MANIFEST_BLEND="$REPO/.dragon/blend-replace-opacity-manifest.txt"
MANIFEST_SOLO="$REPO/.dragon/solo-height-manifest.txt"
MANIFEST_TAN="$REPO/.dragon/tangent-frame-manifest.txt"
BATTERY_LOG="/tmp/w-c3b-recovery-battery.log"
SENTINEL="$REPO/.dragon/battery-sentinel-c3b"
CRATES="umber-mesh umber-core umber-gpu umber-app"
BATTERY_POLL_SECONDS=30
BATTERY_POLL_MAX=120    # 120 x 30s = 60 min cap

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'REFUSING: %s\n' "$*"; exit "${2:-4}"; }
append_note() { printf '\n%s\n' "$*" >> "$NOTE"; }

claim_state() {
    # The note's LAST claim-ish line decides, ANCHORED TO LINE START —
    # the note's prose quotes sentinel strings mid-line and must never
    # match. tac | grep -m1, never a plain grep.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3B RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
c3a_state() {
    # Same shape for the W-C3A sentinel: this wave closes strictly
    # after that one.
    tac "$NOTE" 2>/dev/null | grep -m1 -E '^W-C3A RECOVERY (CLAIMED|RELINQUISHED|CLOSED)' | head -1
}
relinquish() {
    append_note "W-C3B RECOVERY RELINQUISHED by w-c3b-close.sh at $(date '+%F %T %Z') — $1"
}
fail_claimed() {  # die, but relinquish first (post-claim exits only)
    relinquish "$1"
    die "$1" "${2:-4}"
}

tree_state() {
    # The identity of the bytes the battery sees: HEAD plus the
    # dirty/untracked set with each dirty file's BYTES hashed.
    # .dragon/ and Cargo.lock excluded to match the rsync surface.
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

# --- 0. host liveness -------------------------------------------------------
say "0. build-host liveness (centralhost 192.168.15.64:22)"
if timeout 8 bash -c '</dev/tcp/192.168.15.64/22' 2>/dev/null; then
    echo "host answers."
else
    if [ "$DRY" -eq 1 ]; then
        echo "host still down — a real run exits 2 here."
    else
        die "centralhost SSH still down — nothing to recover yet. Re-run when the watcher fires." 2
    fi
fi

# --- 0b. the W-C3B claim interlock -----------------------------------------
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

# --- 0c. the W-C3A gate ------------------------------------------------------
say "0c. the W-C3A gate (this wave closes strictly after that one)"
C3A=$(c3a_state)
if [ -z "$C3A" ]; then
    if [ "$DRY" -eq 1 ]; then
        echo "  (dry-run) no W-C3A RECOVERY line yet — a REAL run exits 4 here (the fleet's close runs first)."
        echo "  (dry-run) continuing so this wave's own preconditions still get verified below."
    else
        die "no W-C3A RECOVERY line in the note — run scripts/w-c3a-close.sh FIRST; the nine-kill fleet's close owns the first battery, the first push, and the packaging dispatch" 4
    fi
else
    case "$C3A" in
        *CLOSED*) echo "W-C3A recovery: $C3A — clear to proceed." ;;
        *CLAIMED*)
            if [ "$DRY" -eq 1 ]; then
                echo "  (dry-run) the W-C3A recovery is still CLAIMED — a REAL run exits 4 here; continuing."
            else
                die "the W-C3A recovery is still CLAIMED ('$C3A') — wait for its close (or its relinquish) before running this script" 4
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
    append_note "W-C3B RECOVERY CLAIMED by w-c3b-close.sh (pid $$) at $(date '+%F %T %Z')"
fi

# --- 1. preconditions --------------------------------------------------------
say "1. preconditions (all THREE manifests FULLY hash-verified, push gate, ownership map)"
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

echo "Ownership map — this sweep's parked bytes (a red inside any of them during ANY battery routes here):"
git status --porcelain -- crates/umber-mesh/src/usdc.rs crates/umber-core/src/layers.rs \
    crates/umber-gpu/src/compositor.rs crates/umber-gpu/src/shaders.rs \
    crates/umber-gpu/src/view_mode.rs crates/umber-gpu/src/renderer.rs \
    crates/umber-app/src/layers_panel.rs crates/umber-mesh/src/tangents.rs \
    crates/umber-mesh/src/lib.rs | sed 's/^/  /'

LOCAL_TIP=$(git rev-parse HEAD)
ORIGIN_TIP=$(git rev-parse origin/master 2>/dev/null || echo unknown)
[ "$LOCAL_TIP" = "$ORIGIN_TIP" ] || \
    echo "NOTE: local HEAD $LOCAL_TIP != origin/master $ORIGIN_TIP — the unpushed rows ride this push too."
echo "preconditions green (tree at $LOCAL_TIP)."

# --- 2. host pre-flight ------------------------------------------------------
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

# --- 3. rsync the tree --------------------------------------------------------
say "3. rsync tree -> host (target/.git/.dragon/Cargo.lock excluded)"
RSYNC_CMD="rsync -az --delete --exclude=/target/ --exclude=/.git/ --exclude=/.dragon/ --exclude=/Cargo.lock $REPO/ $HOST:Projects/umber/"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: $RSYNC_CMD"
    echo "  (dry-run) then write the battery sentinel (.dragon/battery-sentinel-c3b: HEAD + porcelain + dirty-bytes)"
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

# --- 4. wave battery (default features only) ----------------------------------
say "4. wave battery: $CRATES (default features; NEVER --features gpu in the recovery window)"
if [ "$DRY" -eq 1 ]; then
    echo "  (dry-run) would: ssh + rm -f $BATTERY_LOG + nohup wave-battery.sh $CRATES"
    echo "  (dry-run) then poll $BATTERY_LOG for BATTERY_EXIT (30s probes, 60 min cap),"
    echo "  (dry-run) reusing an existing verdict only while the sentinel matches this tree."
    echo
    echo "DRY-RUN COMPLETE — all host-free preconditions verified. The commit plan:"
    echo "  row 1  usdz-sublayer-walk ($(grep -vc '^#' "$MANIFEST_USDZ") paths) -> dcommit path-limited"
    echo "  row 2  blend-replace-opacity ($(grep -vc '^#' "$MANIFEST_BLEND") paths)"
    echo "  row 3  solo-height ($(grep -vc '^#' "$MANIFEST_SOLO") paths)"
    echo "  row 4  tangent-frame ($(grep -vc '^#' "$MANIFEST_TAN") paths)"
    echo "  row 5  docs/tooling: this script + its report + kill-ledger.md (the W-C3b rows)"
    echo "  then: ONE push + explicit run-id ci-watch. No packaging dispatch (w-c3a's close owns it)."
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
    *) fail_claimed "battery RED ($EXISTING) — fix-forward from $HOST:$BATTERY_LOG (open the per-crate logs in the newest /tmp/wave-battery.* dir; a red inside this sweep's files routes to local-4d2e8a71's reports), re-pin any edited manifest rows, re-run this script (it resumes)." 3 ;;
esac

# --- 5. commit rows (each through dcommit --path-limited) ---------------------
say "5. commit rows (three sweep kills by manifest + the docs row; each lock released at its commit)"

commit_row usdz-sublayer-walk "$MANIFEST_USDZ" \
"feat(usdz): load_usdz walks the sublayer stack — the release checklist's .usdz-beyond-first-layer deferral closed. Root-layer precedence preserved (a root that authors a Mesh never reads a sublayer); then the root's subLayers in authored order (USD opinion strength), depth-first, resolved INSIDE the package through Archive::contains/read; a missing or unreadable sublayer is skipped per USD's layer-registry tolerance (a later one may hold the mesh); the walk is cycle-guarded and depth-bounded (16) — a sublayer graph is untrusted input; escaping paths (.., absolute) are rejected lexically (USDZ forbids leaving the package); a found-but-malformed Mesh is a hard error (the honest first-Mesh contract). 8 pinned tests, fixtures programmatic through usdz::ArchiveWriter (raw .usda texts, stored entries — the panic-hunt usdz_package pattern); the python port of the normalization agrees 12/12. Requirements row 111's boundary closed; parked through freeze-4, battery-green this recovery" 2

commit_row blend-replace-opacity "$MANIFEST_BLEND" \
"feat(blend): Replace's bypass — requirements row 39's first named v1 boundary closed. The mode's documented contract implemented at every coverage gate: the layer opacity AND the source-alpha grading are bypassed (a present texel, any alpha > 0, overwrites in full; only a fully-transparent texel is a no-op — the premultiplied arm's cov == 0 invariant stays load-bearing; the mask still clips, it is coverage not opacity). umber-core's composite_channel returns the blend result unmixed for Replace (even at opacity 0 — the no-effect rule belongs to opacity, and Replace bypasses it); the umber-gpu oracle gates cov = presence x mask x opacity-1 for the mode and the WGSL stack compositor mirrors the same gate AFTER the un-premultiply arm (var sa); the panel's channel-opacity field greys for Replace rows (an enabled editor for a bypassed value would be a lie — no new i18n string, the catalogs sit in a parked lock; the stored value rides along untouched). The variant doc's NOT-implemented-yet clause closed; the equivalence invariant (composite_pixel <-> composite_channel fold) holds by construction, both sides bypass; every surviving Replace pin re-derived (all full-opacity or census-structural — the red-risk sweep is in the report); the python port agrees 4/4. Row 39's second boundary (PerChannel routing) stays design-gated; parked through freeze-4, battery-green this recovery" 4

commit_row solo-height "$MANIFEST_SOLO" \
"feat(view-modes): the Solo/Preview height arm reads the bound height tile — requirements row 79's boundary closed, the view_mode doc's named small separate follow-up. solo_proxy's HEIGHT arm samples binding 12 through the SAME addressing chain and the SAME PAINT_HAS_HEIGHT_TEXTURE flag gate as height_perturbed_normal (solo and Lit can never disagree about which texel a pixel reads); the mid-gray stand-in stays for a mesh with no height channel (the fallback texture is never sampled while the flag is clear); the mesh-map PREVIEW_HEIGHT delegation inherits the real data; binding 12 is layout-static in the shared group 0 so the shader arm alone reaches both paths — zero renderer changes. The GPU golden solo_height_reads_the_bound_height_tile mirrors the height_stroke precedent (unbound ~128 stand-in; the u-ramp's texel (1,1)=85/255 -> ~85; skip-graceful without an adapter). parked through freeze-4, battery-green this recovery" 4

commit_row tangent-frame "$MANIFEST_TAN" \
"feat(mesh): per-vertex tangent frames, the mikkTSpace way — the requirements row 79 named follow-up's foundation. umber's importer output is per-corner EXPANDED (indices always sequential), so textbook index-sharing accumulation would never smooth anything: corners weld into groups by the bit-exact (position, normal, uv) tuple first — the graphics-standard tangent-seam rule (a UV seam legitimately splits groups, keeping per-island frames) — then per-triangle Lengyel UV-derivative tangents accumulate into the weld groups, Gram-Schmidt against the group normal, handedness from sign(dot(cross(N, T), B_sum)) (the compact encoding B = handedness x cross(N, T)). Never panics on untrusted input (out-of-range/short-attribute triangles skipped, % 3 remainder ignored, zero-UV-area triangles contribute nothing, zero-contribution groups take the deterministic least-dominant-axis perpendicular fallback). 7 unit tests; the python port agrees 7/7 (and caught a vacuous seam fixture BEFORE any test ran — u ran along +X on the XZ triangle; the fixture was fixed, not the assertion). The consumer half — the renderer wiring (vertex attribute + pipeline layouts + the TBN height gradient + the lit goldens re-derived over the solo-height pins) — is the named next-after-recovery kill; parked through freeze-4, battery-green this recovery" 3

commit_row wave-docs-and-tooling - \
"docs(wave): the W-C3b close tooling + the ledger's sweep rows — scripts/w-c3b-close.sh (the executable recovery: the W-C3B claim interlock anchored at line start + the W-C3A-must-be-CLOSED gate, three 16-hex manifests full-verified, hardened rsync + this wave's own tree-state sentinel, default-leg battery umber-mesh umber-core umber-gpu umber-app with the bounded verdict-reuse poll, commit rows through dcommit --path-limited so foreign dirt cannot ride, ONE push + explicit run-id ci-watch; no baselines, no packaging dispatch — w-c3a's close owns both) + its kill report + docs/loop/kill-ledger.md (the W-C3b rows appended after the W-C3a docs row landed the ledger's earlier state — the single-owner rule); rides this push" 3 \
    scripts/w-c3b-close.sh \
    docs/loop/kill-reports/2026-10-20-w-c3b-close-script.md \
    docs/loop/briefs/2026-10-20-tangent-wiring.md \
    docs/loop/kill-ledger.md

# Every commit moved HEAD, which stales the battery sentinel written at
# rsync time — refresh it so a failed PUSH + re-run reuses the verdict.
tree_state > "$SENTINEL"

# --- 6. ONE push + ci-watch ----------------------------------------------------
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
echo "  packaging NOT dispatched: w-c3a's close owns the flatpak verdict-gate dispatch;"
echo "  the push trigger itself runs the AppImage+lint legs on this tip."

append_note "W-C3B RECOVERY CLOSED by w-c3b-close.sh (pid $$) at $(date '+%F %T %Z') — battery green, three sweep rows + the docs row pushed, CI run $RUN_ID."
echo
echo "WAVE CLOSED (pending CI run $RUN_ID). The docs pass is the next turn's first kill:"
echo "  - delete the consumed manifests (.dragon/usdz-sublayer-walk-manifest.txt,"
echo "    blend-replace-opacity-manifest.txt, solo-height-manifest.txt,"
echo "    tangent-frame-manifest.txt) + .dragon/battery-sentinel-c3b"
echo "  - release-checklist-0.1.0.md: the .usdz-beyond-first-layer known-deferred row ticks"
echo "  - requirements.md rows 39 (first boundary) / 79 (Solo/Preview height) / 111 (usdz)"
echo "  - the kill-ledger rows: sweep -> LANDED"
echo "  - M1 pass 3 (the standing stale-row sweep) at the wave close, per the standing rule"
exit 0
