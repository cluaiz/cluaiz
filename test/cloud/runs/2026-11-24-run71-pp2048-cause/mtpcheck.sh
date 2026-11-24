#!/usr/bin/env bash
# CLUAIZ_MTP=off / on on an MTP-capable model (Qwen3.5 2B MTP Q4_0): does the log show MTP off / on?
# Isolated home (copy of run 23's home without models, models linked), port 8011.
set -uo pipefail
OUT=~/results-cause/mtp; mkdir -p "$OUT"
H=~/ch-cause/mtp
if [ ! -d "$H" ]; then mkdir -p "$H"; (cd ~/ch-run23 && tar --exclude=./models -cf - .) | tar -xf - -C "$H"; ln -s ~/ch-run23/models "$H/models"; fi
for v in off on; do
  env -i HOME="$HOME" PATH="$PATH" USER="$USER" CLUAIZ_HOME="$H" CLUAIZ_PORT=8011 CLUAIZ_HOST=127.0.0.1 CLUAIZ_NONINTERACTIVE=1 \
    CLUAIZ_MTP=$v ~/ppcause/bin/cluaiz bench qwen3.5-2b-mtp-gguf-Q4_0 -p 0 -n 64 -r 1 -vv > "$OUT/mtp_$v.md" 2> "$OUT/mtp_$v.log"
  echo "== CLUAIZ_MTP=$v rc $?"
  grep -h "MTP\|NextN\|speculative" "$OUT/mtp_$v.log" | sed 's/^.*\(\[MTP\]\)/\1/' | head -8
  cat "$OUT/mtp_$v.md"
done
