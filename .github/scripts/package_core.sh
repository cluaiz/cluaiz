#!/usr/bin/env bash
# Packs the core: the cluaiz binary with the llama / ggml libraries, CPU backends and ONNX Runtime
# libraries it loads from its own folder. The ONNX CUDA provider goes to a separate NVIDIA pack; the
# llama GPU backends are built by their own jobs.
#
#   package_core.sh <cargo profile dir> <name>
#   -> dist/<name>/ (unpacked, for the smoke test), dist/<name>.zip (Windows) or .tar.gz,
#      and dist/<name>-onnx-cuda.* when the ONNX CUDA provider was built
set -euo pipefail

src="$1"
name="$2"
out="dist/$name"
nvidia="dist/$name-onnx-cuda"
rm -rf "$out" "$nvidia"
mkdir -p "$out" "$nvidia"

windows=false
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) windows=true ;; esac

if $windows; then cp "$src/cluaiz.exe" "$out/"; else cp "$src/cluaiz" "$out/"; fi

shopt -s nullglob
for file in "$src"/*; do
  base=$(basename "$file")
  case "$base" in *.dll | *.so | *.so.* | *.dylib) ;; *) continue ;; esac
  stem=${base#lib}
  case "$stem" in ggml* | llama* | mtmd* | DirectML* | onnxruntime*) ;; *) continue ;; esac
  # GPU backends ship as their own packs (gpu-* jobs), never inside the core
  case "$stem" in ggml-cuda* | ggml-vulkan* | ggml-hip* | ggml-sycl* | ggml-opencl*) continue ;; esac

  dest="$out"
  case "$stem" in onnxruntime_providers_cuda* | onnxruntime_providers_tensorrt* | onnxruntime_providers_nv_tensorrt*) dest="$nvidia" ;; esac

  if [ -L "$file" ] && [[ "$(readlink "$file")" != /* ]]; then
    cp -P "$file" "$dest/" # libllama.so -> libllama.so.0 stays a link, not a second copy
  else
    cp -L "$file" "$dest/" # ort's links point into its download cache
  fi
done
cp LICENSE THIRD_PARTY_NOTICES.md "$out/"

archive() {
  local dir="$1"
  if $windows && command -v 7z >/dev/null; then
    (cd "$dir" && 7z a -bso0 "../$(basename "$dir").zip" .)
  elif $windows; then
    powershell -NoProfile -Command "Compress-Archive -Path '$(cygpath -w "$dir")\\*' -DestinationPath '$(cygpath -w "$dir").zip' -Force"
  else
    tar -czf "$dir.tar.gz" -C "$dir" .
  fi
}

archive "$out"
if [ -n "$(ls -A "$nvidia")" ]; then archive "$nvidia"; fi
rm -rf "$nvidia"

echo "== $name"
ls -la "$out"
