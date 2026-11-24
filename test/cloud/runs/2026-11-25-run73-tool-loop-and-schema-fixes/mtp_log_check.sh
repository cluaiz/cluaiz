#!/bin/sh
# MTP plan log line with CLUAIZ_MTP unset / off / on, before and after binaries (cluaiz bench, isolated home)
# usage: sh mtp_log_check.sh <work dir with before/ and t3after/ and home/> <out dir>
W="$1"; OUT="$2"; mkdir -p "$OUT"
M=qwen3.5-2b-mtp-gguf-Q4_K_M
for v in before t3after; do
  for mtp in unset off on; do
    if [ "$mtp" = unset ]; then
      CLUAIZ_HOME="$W/home" "$W/$v/cluaiz.exe" bench $M -p 0 -n 16 -r 1 -vv > "$OUT/${v}_$mtp.log" 2>&1
    else
      CLUAIZ_MTP=$mtp CLUAIZ_HOME="$W/home" "$W/$v/cluaiz.exe" bench $M -p 0 -n 16 -r 1 -vv > "$OUT/${v}_$mtp.log" 2>&1
    fi
    echo "$v $mtp exit=$? | $(grep -a '\[MTP\]' "$OUT/${v}_$mtp.log" | sed 's/.*\[MTP\]/[MTP]/' | tr '\n' ';') | nextn: $(grep -a -c 'nextn' "$OUT/${v}_$mtp.log")"
  done
done
