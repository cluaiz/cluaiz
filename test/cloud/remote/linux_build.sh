#!/usr/bin/env bash
# On a fresh Ubuntu Deep Learning VM: toolchain, then the release build of the engine.
# Usage: linux_build.sh <source.tgz> <cuda-architectures, e.g. "75;89", or "none" for CPU only>
set -uo pipefail
SRC="$1" ARCHS="$2"
OUT="$HOME/results"; mkdir -p "$OUT"
exec > >(tee -a "$OUT/build.log") 2>&1

echo "== machine"; uname -a; lsb_release -ds; nproc; free -g; nvidia-smi || true
sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
  build-essential cmake pkg-config clang libclang-dev libssl-dev autoconf automake libtool git curl python3 >/dev/null
command -v cargo >/dev/null || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustc --version

# "-" keeps the tree already unpacked (a file changed since can be copied in first)
[ "$SRC" != - ] && rm -rf "$HOME/cluaiz" && mkdir -p "$HOME/cluaiz" && tar -xzf "$SRC" -C "$HOME/cluaiz"
cd "$HOME/cluaiz"
FEATURES=()
if [ "$ARCHS" = none ]; then
  FEATURES=(--no-default-features)
else
  export CUDA_PATH=/usr/local/cuda CUDACXX=/usr/local/cuda/bin/nvcc PATH="/usr/local/cuda/bin:$PATH"
  export CUDA_ARCHITECTURES="$ARCHS"
  nvcc --version | tail -1
fi
start=$(date +%s)
cargo build -p cmd --release "${FEATURES[@]}"
status=$?
echo "== build exit $status after $(( $(date +%s) - start )) s"
ls -la target/release/cluaiz 2>/dev/null && ldd target/release/cluaiz | grep -Ei "cuda|cublas|onnx|not found"
exit $status
