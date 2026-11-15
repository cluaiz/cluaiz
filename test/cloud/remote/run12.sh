#!/usr/bin/env bash
# Run 12 on the L4 VM: the v0.1.0 rebuild with the two fixes run 11 asked for (answers without tools
# read by llama.cpp's parser; client tool calls kept in the prompt) and the pull fix. Only what those
# fixes touch is run again; the rest of run 11 stands. Each step is skipped when its output exists.
#   1 install the release; an untagged pull of the gpt-oss repo (must take the model, not the draft)
#   2 gpt-oss-20b all four placements; Qwen3.5-9B auto and gpu (an answer parser that also reads it now)
#   3 features (tools, thinking, second turn, vision, embeddings, speech)
#   4 gpt-oss-20b through Cluaiz on the compare prompt (Ollama and LM Studio ran in run 11)
# Usage: run12.sh <cluaiz commit of the release>
set -uo pipefail
COMMIT="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HOME/results-run12"; mkdir -p "$OUT"
BIN="$HOME/.cluaiz/bin/cluaiz"
export CLUAIZ_HOME="$HOME/ch-reg" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1
step() { echo "== $(date +%H:%M:%S) $*" | tee -a "$OUT/steps.log"; }

step "1 install"
if [ ! -f "$OUT/installed" ]; then
  curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | sh > "$OUT/install.log" 2>&1 && touch "$OUT/installed"
fi
"$BIN" --version | tee -a "$OUT/steps.log"
if [ ! -f "$OUT/pull_untagged.log" ]; then
  # a home of its own, so the test models stay as they are
  CLUAIZ_HOME="$HOME/ch-pull" "$BIN" pull ggml-org/gpt-oss-20b-GGUF </dev/null > "$OUT/pull_untagged.log" 2>&1
  echo "untagged pull exit $?" | tee -a "$OUT/steps.log"
  find "$HOME/ch-pull/models" -name '*.gguf' -printf '%f %s\n' | tee -a "$OUT/steps.log"
  [ -d "$HOME/ch-pull" ] && [ ! -L "$HOME/ch-pull" ] && rm -rf "$HOME/ch-pull"
fi

step "2 placements"
"$HERE/regression_all.sh" "$COMMIT" 89 "$HERE/models_run11.txt" l4r12 gpt-oss-20b --modes auto,gpu,split,cpu --runs 2 \
  > "$OUT/placements_gptoss.log" 2>&1
"$HERE/regression_all.sh" "$COMMIT" 89 "$HERE/models_run11.txt" l4r12q qwen3.5-9b --modes auto,gpu --runs 2 \
  > "$OUT/placements_qwen.log" 2>&1

step "3 features"
if [ ! -f "$OUT/features/results.json" ]; then
  "$BIN" serve > "$OUT/server_features.log" 2>&1 & SERVER=$!
  for i in $(seq 1 120); do curl -s -m 2 http://127.0.0.1:8011/health >/dev/null && break; sleep 1; done
  python3 -u "$HERE/features11.py" http://127.0.0.1:8011 "$CLUAIZ_HOME" "$OUT/features" > "$OUT/features.log" 2>&1
  kill $SERVER; wait $SERVER 2>/dev/null
fi

step "4 compare (Cluaiz, gpt-oss-20b)"
if [ ! -f "$OUT/compare/results.md" ]; then
  TOOLS=cluaiz python3 -u "$HERE/compare3.py" "$OUT/compare" 16384 gpt-oss-20b-gguf-FP4 > "$OUT/compare.log" 2>&1
fi
step "all done"
