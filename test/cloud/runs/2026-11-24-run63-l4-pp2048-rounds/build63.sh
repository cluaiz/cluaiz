#!/usr/bin/env bash
# Build the Engine at the bundled HEAD (Linux release, CUDA for the L4), as run 59's build48.sh / build43.sh.
set -uo pipefail
cd ~/cluaiz-pf
git -c fetch.recurseSubmodules=false fetch -q ~/ssdpf/engine62.bundle HEAD:run63 && git checkout -q -f run63 || exit 1
git log -1 --oneline
L=engine/llama/llama.cpp
( cd "$L" && git fetch -q ~/ssdpf/fork62.bundle HEAD:run63 && git checkout -q -f run63 && git log -1 --oneline ) || exit 1
. ~/.cargo/env
export CUDA_PATH=/usr/local/cuda CUDACXX=/usr/local/cuda/bin/nvcc PATH="/usr/local/cuda/bin:$PATH" CUDA_ARCHITECTURES=89
start=$(date +%s)
cargo build -p cmd --release
echo "== build exit $? after $(( $(date +%s) - start )) s"
ls -la target/release/cluaiz; target/release/cluaiz --version
df -h / | tail -1
