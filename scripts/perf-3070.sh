#!/usr/bin/env bash
# The real-GPU perf row (C2-16): run on Bessa's RTX 3070 host, NOT CI.
# Produces the three JSONs the baselines table needs and prints the row.
# Each bench prints its own P95 verdict; nothing here fails on budget.
#
#   bash scripts/perf-3070.sh            # 1000 ref-scene frames, 5000 i2p events
#   FRAMES=200 EVENTS=1000 bash scripts/perf-3070.sh   # a quick look
#
# Needs: the NVIDIA proprietary driver (Vulkan), a checkout at the commit
# to record, cargo. Takes ~10-20 min (release build + three benches).
set -euo pipefail
cd "$(dirname "$0")/.."
FRAMES="${FRAMES:-1000}"
EVENTS="${EVENTS:-5000}"
OUT="${OUT:-perf-3070-$(git rev-parse --short HEAD)}"
mkdir -p "$OUT"

echo "== adapter"
vulkaninfo --summary 2>/dev/null | grep -E "deviceName|driverVersion|apiVersion" | head -6 \
  || echo "(vulkaninfo missing: the benches print the adapter they ran on)"

echo "== viewport: 1M tris + 4K set, ${FRAMES} frames"
UMBER_REF_SCENE_FRAMES="$FRAMES" UMBER_REF_SCENE_JSON="$OUT/ref-scene.json" \
  cargo test -p umber-gpu --features perf --release ref_scene -- --nocapture 2>&1 \
  | grep -E "ref-scene (baseline|json)|test result" | tee "$OUT/ref-scene.log"

echo "== input-to-photon, ${EVENTS} events"
UMBER_I2P_EVENTS="$EVENTS" UMBER_I2P_JSON="$OUT/i2p.json" \
  cargo test -p umber-app --features perf --release input_photon -- --nocapture 2>&1 \
  | grep -E "input-to-photon|baseline|test result" | tee "$OUT/i2p.log"

echo "== composite-on-read: opacity drag on a masked 5-channel 4K material"
cargo test -p umber-app --features perf --release --test pool_perf pool_drag -- --nocapture 2>&1 \
  | grep -E "pool-drag (baseline|json)|test result" | tee "$OUT/pool-drag.log"

echo
echo "== the row for docs/perf/protocol.md (fill the numbers from the JSONs):"
echo "| $(date +%F) | $(git rev-parse --short HEAD) | <i2p p95> | — | <ref-scene p95> | — | RTX 3070, <driver>, Vulkan. Ref scene: 1,002,252 tris + 4K set (resident, unsampled). pool_drag p95 <n> ms. |"
echo "JSONs in $OUT/: send them back and the dragon writes the row + the baseline reseed."
