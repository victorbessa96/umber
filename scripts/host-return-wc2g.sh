#!/usr/bin/env bash
# W-C2g host-return runbook: run when 192.168.15.64 answers SSH again.
# Batteries for every PARKED kill (uncommitted, unrun since the 2026-10-15
# outage), each against a clean clone of HEAD + that kill's files. Prints
# PASS/FAIL per kill; commits nothing (the dragon commits per kill after
# reading the failures). Usage: bash scripts/host-return-wc2g.sh
set -uo pipefail
cd "$(dirname "$0")/.."
HOST=bessa@192.168.15.64
REMOTE=Projects/hermes-verify-wc2g

# kill name | crates for the battery | files (the kill's lock)
KILLS=(
  "g5-workspace-persist|umber-app|Cargo.toml Cargo.lock crates/umber-app/Cargo.toml crates/umber-app/src/main.rs crates/umber-app/src/workspaces.rs crates/umber-app/src/shell_menu.rs"
  "g0-kittest-breadth|umber-app|crates/umber-app/src/lib.rs crates/umber-app/src/palette.rs crates/umber-app/tests/support/mod.rs crates/umber-app/tests/panel_kittest.rs"
  "c2-28-export-kittest|umber-app|crates/umber-app/src/export_dialog.rs"
  "mask-compose-perf|umber-app|crates/umber-app/src/document.rs"
  "showlog-no-cmd|umber-app|crates/umber-app/src/logfile.rs"
  "i18n-coverage-test|umber-app|crates/umber-app/src/i18n.rs"
  "sec-import-channel|umber-app|crates/umber-app/src/mesh_map_import.rs"
  "sec-name-traversal|umber-mesh umber-export umber-app umber-cli|crates/umber-mesh/src/mesh_maps.rs crates/umber-export/src/lib.rs"
  "sec-sidecar-channel|umber-paint umber-app|crates/umber-paint/src/persistence.rs"
  "sec-smart-caps|umber-core umber-app|crates/umber-core/src/smart.rs"
  "spp-inflate-bound|spp|crates/spp/src/hdf5.rs"
  "g5-tab-overflow-core|umber-gui|crates/umber-gui/src/widgets/mod.rs crates/umber-gui/src/widgets/tab_overflow.rs"
)
# NOT in the table: win-version-resource. It needs the workspace dep
# `winresource` in the root Cargo.toml + umber-app's build-dep, both
# under g5-workspace-persist's lock, and a Cargo.lock resolved on the host
# (`cargo metadata`). Do it by hand after g5-workspace-persist commits:
# docs/loop/kill-reports/2026-10-15-win-version-resource.md.

ssh -o BatchMode=yes -n "$HOST" "rm -rf ~/$REMOTE && mkdir -p ~/$REMOTE"
git bundle create - HEAD 2>/dev/null | ssh -o BatchMode=yes "$HOST" \
  "cat > ~/$REMOTE.bundle && git clone -q ~/$REMOTE.bundle ~/$REMOTE/base && rm ~/$REMOTE.bundle"

for entry in "${KILLS[@]}"; do
  IFS='|' read -r name crates files <<<"$entry"
  echo "=== $name ($crates)"
  # A fresh copy of HEAD per kill, so one kill's failure never masks another's.
  ssh -o BatchMode=yes -n "$HOST" "rm -rf ~/$REMOTE/$name && cp -a ~/$REMOTE/base ~/$REMOTE/$name"
  # shellcheck disable=SC2086
  tar -c $files | ssh -o BatchMode=yes "$HOST" "tar -xm -C ~/$REMOTE/$name"
  timeout 600 ssh -o BatchMode=yes -n "$HOST" "cd ~/$REMOTE/$name && export PATH=\$HOME/.cargo/bin:\$PATH CARGO_TARGET_DIR=\$HOME/Projects/umber/target && bash scripts/wave-battery.sh $crates > /tmp/wc2g-$name.log 2>&1; echo \"BATTERY_EXIT=\$?\"; grep -E '^(PASS|FAIL)' /tmp/wc2g-$name.log; L=\$(grep -oE '/tmp/wave-battery\.[A-Za-z0-9]+' /tmp/wc2g-$name.log | head -1); [ -n \"\$L\" ] && grep -hE 'panicked at|^error' -A6 \$L/*.log | head -20"
done
echo "=== done; full logs on the host at /tmp/wc2g-<kill>.log"
echo "Then: commit each green kill, run the full wave gate on a clean clone, push W-C2g."
