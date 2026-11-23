#!/usr/bin/env bash
# Run 50: sets media.projector_workspace_bytes in the isolated test home to each MiB value given,
# runs a scenario (image_first unless SCENARIO is set) on a fresh server, and restores the saved settings at the end.
# Used only to place the projector on the GPU with the same thin margin run 49 had (free VRAM on
# this laptop changes with the desktop, so the default 1 GiB estimate puts it on the CPU today).
#   CLUAIZ_TEST_HOME=<isolated home> [SCENARIO=five_images] [EXTRA=<json>] bash sweep_workspace.sh <exe> <label> <MiB>...
set -u
HOME_DIR="${CLUAIZ_TEST_HOME:?set CLUAIZ_TEST_HOME to the isolated test home}"
CFG="$HOME_DIR/config/settings.json"
EXE="$1"; LABEL="$2"; shift 2
EXTRA_JSON="${EXTRA:-}"; [ -n "$EXTRA_JSON" ] || EXTRA_JSON="{}"
cp "$CFG" "$CFG.sweep-restore"
for mib in "$@"; do
  python - "$CFG" "$mib" <<'PY'
import json, sys
path, mib = sys.argv[1], int(sys.argv[2])
data = json.load(open(path, encoding="utf-8"))
data["media"]["projector_workspace_bytes"] = mib * 1024 * 1024
json.dump(data, open(path, "w", encoding="utf-8"), indent=2)
PY
  echo "== workspace $mib MiB"
  python vision_vram.py --exe "$EXE" --home "$HOME_DIR" --port 8014 --label "${LABEL}_ws${mib}" --out raw/sweep --only "${SCENARIO:-image_first}" --extra "$EXTRA_JSON"
done
cp "$CFG.sweep-restore" "$CFG" && rm "$CFG.sweep-restore"
