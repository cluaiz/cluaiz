#!/usr/bin/env bash
# Run 66: build the Engine at the bundled HEAD, Linux release, CPU only (--no-default-features: no CUDA
# backend library), in its own target folder so run 63's CUDA build in target/ stays as it is.
set -uo pipefail
cd ~/cluaiz-pf
git -c fetch.recurseSubmodules=false fetch -q ~/r66/engine66.bundle HEAD:run66 && git checkout -q -f run66 || exit 1
git log -1 --oneline
( cd engine/llama/llama.cpp && git log -1 --oneline )
. ~/.cargo/env
export CARGO_TARGET_DIR=$HOME/cluaiz-pf/target-cpu
start=$(date +%s)
cargo build -p cmd --release --no-default-features
echo "== build exit $? after $(( $(date +%s) - start )) s"
ls -la $CARGO_TARGET_DIR/release/cluaiz; ls $CARGO_TARGET_DIR/release/*.so
$CARGO_TARGET_DIR/release/cluaiz --version
df -h / | tail -1
