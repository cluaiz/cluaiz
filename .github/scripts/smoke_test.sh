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

windows=false
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) windows=true ;; esac
run_engine() {
  if $windows; then
    powershell.exe -NoProfile -File .github/scripts/run_clean_windows.ps1 "$(cygpath -aw "$exe")" "$@"
  else
    "$exe" "$@"
  fi
}

export CLUAIZ_HOME="${RUNNER_TEMP:-$(mktemp -d)}/cluaiz-smoke-home"
# No terminal in CI: answer the download confirmation with its default
export CLUAIZ_NONINTERACTIVE=1
rm -rf "$CLUAIZ_HOME"

run_engine --version
run_engine status
# `run` takes the registered ID: given the download name of a model that is already installed, it
# only prints that ID (cmd/src/cli/run.rs)
pull_log="${RUNNER_TEMP:-/tmp}/cluaiz-smoke-pull.log"
run_engine pull "$model" | tee "$pull_log"
id=$(awk '/Registered ID:/ { gsub(/\033\[[0-9;]*m|\r/, ""); print $NF }' "$pull_log")
[ -n "$id" ] || { echo "pull did not print the registered ID"; exit 1; }

log="${RUNNER_TEMP:-/tmp}/cluaiz-smoke-run.log"
run_engine -v run "$id" -n 32 "Reply with one short sentence: what is the capital of France?" 2>"$log" | tee "${log%.log}.out"

# The CPU backend was loaded from the folder and the model answered
grep -q "Backends loaded" "$log" || { echo "backends were not loaded"; cat "$log"; exit 1; }
[ -s "${log%.log}.out" ] || { echo "no answer"; cat "$log"; exit 1; }
echo "smoke test passed"
