#!/usr/bin/env bash
# Runs an unpacked core on its own: it must start, download a model and answer on the CPU, using
# only the files in its folder and a fresh home (CI runners have no GPU and no GPU runtime).
#
#   smoke_test.sh <unpacked core dir> <model id>
set -euo pipefail

dir="$1"
model="$2"
exe="$dir/cluaiz"
[ -f "$dir/cluaiz.exe" ] && exe="$dir/cluaiz.exe"

export CLUAIZ_HOME="${RUNNER_TEMP:-$(mktemp -d)}/cluaiz-smoke-home"
rm -rf "$CLUAIZ_HOME"

"$exe" --version
"$exe" status
"$exe" pull "$model"

log="${RUNNER_TEMP:-/tmp}/cluaiz-smoke-run.log"
"$exe" -v run "$model" -n 32 "Reply with one short sentence: what is the capital of France?" 2>"$log" | tee "${log%.log}.out"

# The CPU backend was loaded from the folder and the model answered
grep -q "Backends loaded" "$log" || { echo "backends were not loaded"; cat "$log"; exit 1; }
[ -s "${log%.log}.out" ] || { echo "no answer"; cat "$log"; exit 1; }
echo "smoke test passed"
