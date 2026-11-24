#!/usr/bin/env bash
# pp2048 cause run: build the Engine at the bundled HEAD with the fork of run 63 (96b0cfb2d), plus the
# test-only no-warmup switch (nowarm.patch, inert unless CLUAIZ_TEST_NO_WARMUP is set). Linux release, CUDA for the L4.
set -uo pipefail
cd ~/cluaiz-pf
git -c fetch.recurseSubmodules=false fetch -q ~/ppcause/engine_cause.bundle HEAD:ppcause && git checkout -q -f ppcause || exit 1
git log -1 --oneline
( cd engine/llama/llama.cpp && git checkout -q -f 96b0cfb2d && git log -1 --oneline ) || exit 1
git apply ~/ppcause/nowarm.patch || exit 1
git diff --stat
. ~/.cargo/env
export CUDA_PATH=/usr/local/cuda CUDACXX=/usr/local/cuda/bin/nvcc PATH="/usr/local/cuda/bin:$PATH" CUDA_ARCHITECTURES=89
start=$(date +%s)
cargo build -p cmd --release
rc=$?
echo "== build exit $rc after $(( $(date +%s) - start )) s"
[ $rc = 0 ] && mkdir -p ~/ppcause/bin && cp -a target/release/cluaiz target/release/*.so* ~/ppcause/bin/
git checkout -q -- engine/llama/src/native/core.rs
ls -la ~/ppcause/bin/cluaiz; ~/ppcause/bin/cluaiz --version
df -h / | tail -1
