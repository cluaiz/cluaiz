#!/usr/bin/env bash
# Run 72: scrub a results folder on the VM before it is copied off: the user name, the session token of each
# test home, the VM's internal and external IPv4 and the cloud project id (read from the metadata server, never
# printed) are replaced in every text file. Then greps for them again and prints only counts.
# Usage: scrub.sh <dir> <test home>...
set -uo pipefail
DIR="$1"; shift
md() { curl -s -m 3 -H "Metadata-Flavor: Google" "http://metadata.google.internal/computeMetadata/v1/$1"; }
PROJECT=$(md project/project-id); INT_IP=$(md instance/network-interfaces/0/ip)
EXT_IP=$(md instance/network-interfaces/0/access-configs/0/external-ip)
pairs=()
for h in "$@"; do [ -f "$h/session.token" ] && pairs+=("$(cat "$h/session.token")" "<session-token>"); done
[ -n "$PROJECT" ] && pairs+=("$PROJECT" "<project>")
[ -n "$EXT_IP" ] && pairs+=("$EXT_IP" "<vps>")
[ -n "$INT_IP" ] && pairs+=("$INT_IP" "<vps-internal>")
pairs+=("$USER" "<user>")
find "$DIR" -type f | while read -r f; do
  grep -Iq . "$f" 2>/dev/null || continue
  for ((i = 0; i < ${#pairs[@]}; i += 2)); do
    a="${pairs[i]}" b="${pairs[i+1]}"
    [ -n "$a" ] && A="$a" B="$b" perl -0pi -e 's/\Q$ENV{A}\E/$ENV{B}/g' "$f"
  done
done
left=0
for ((i = 0; i < ${#pairs[@]}; i += 2)); do
  n=$(grep -rIlF -- "${pairs[i]}" "$DIR" 2>/dev/null | wc -l); left=$((left + n))
  echo "${pairs[i+1]}: $n files still contain it"
done
echo "scrub left $left"
