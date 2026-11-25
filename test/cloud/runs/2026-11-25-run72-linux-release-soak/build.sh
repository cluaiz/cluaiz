#!/usr/bin/env bash
# Run 72: release build of local main on the L4 VM, then the release layout.
# The steps follow test/cloud/remote/linux_build.sh (toolchain already on the VM) and release.yml:
# cargo build -p cmd --release --locked, .github/scripts/package_core.sh for the core, then the CUDA
# driver library and the CUDA runtime beside it, as the linux-x64-llama-cuda + cudart packs install it.
# Difference to CI: the CUDA backend comes from the default `cuda` feature of the same build (same
# llama.cpp submodule commit) with CUDA_ARCHITECTURES=89 (L4 only), not llama.cpp's default arch set,
# and the toolkit is the VM's CUDA 13.2, not the 12.8.1 / 13.4.1 containers.
# Usage: build.sh <repo dir> <out dir>
set -uo pipefail
REPO="$1" OUT="$2"; mkdir -p "$OUT"
exec > >(tee -a "$OUT/build.log") 2>&1
. "$HOME/.cargo/env"
cd "$REPO"
echo "== commit $(git rev-parse HEAD)  submodule $(git -C engine/llama/llama.cpp rev-parse HEAD)"
uname -a; nproc; free -g; nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
rustc --version; cmake --version | head -1; gcc --version | head -1
export CUDA_PATH=/usr/local/cuda CUDACXX=/usr/local/cuda/bin/nvcc PATH="/usr/local/cuda/bin:$PATH"
export CUDA_ARCHITECTURES=89
nvcc --version | tail -1
start=$(date +%s)
cargo build -p cmd --release --locked
status=$?
echo "== build exit $status after $(( $(date +%s) - start )) s"
[ $status = 0 ] || exit $status
ls -la target/release/ | grep -E "cluaiz$|\.so"
ldd target/release/cluaiz | grep -Ei "cuda|cublas|onnx|ggml|llama|mtmd|not found"

# Release layout: core package + CUDA driver + CUDA runtime in one folder
VERSION=$(awk '/^\[workspace.package\]/{w=1; next} /^\[/{w=0} w && /^version *=/{gsub(/[" ]/, "", $0); sub(/^version=/, "", $0); print; exit}' Cargo.toml)
name="cluaiz-${VERSION}-linux-x64"
bash .github/scripts/package_core.sh target/release "$name"
pkg="$REPO/dist/$name"
cp -L target/release/libggml-cuda.so "$pkg/"
major=$(nvcc --version | sed -n 's/.*release \([0-9]*\)\..*/\1/p')
for lib in libcudart libcublas libcublasLt; do cp -L "/usr/local/cuda/lib64/$lib.so.$major" "$pkg/"; done
ls -la "$pkg"; du -sh "$pkg" "$REPO/dist/$name.tar.gz"
sha256sum "$pkg/cluaiz" "$pkg/libggml-cuda.so"
echo "== ldd of the packaged binary with LD_LIBRARY_PATH unset"
env -u LD_LIBRARY_PATH ldd "$pkg/cluaiz" | grep -Ei "not found|ggml|llama|mtmd|gomp|onnx"
echo "== done"
