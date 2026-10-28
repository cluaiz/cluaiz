#!/usr/bin/env bash
# regression_tests.sh one model at a time, so a Spot VM that is stopped keeps the reports already written;
# calling it again skips the models that have a report.
# Usage: regression_all.sh <cluaiz commit> <CUDA arch> <models file> <name> <model id parts, comma list> [run.py args]
set -uo pipefail
COMMIT="$1"; ARCH="$2"; MODELS="$3"; NAME="$4"; IDS="$5"; shift 5
HERE="$(cd "$(dirname "$0")" && pwd)"
for m in ${IDS//,/ }; do
  [ -f "$HOME/results-regression/RESULTS_${NAME}_$m.md" ] && continue
  "$HERE/regression_tests.sh" "$COMMIT" "$ARCH" "$MODELS" "${NAME}_$m" --only "$m" "$@"
done
echo "== all done $NAME"
