#!/usr/bin/env bash
# Packs the core: the cluaiz binary with the llama / ggml libraries, CPU backends and the ONNX Runtime
# files it loads from its own folder (DirectML on Windows). GPU drivers are not part of the core:
# cluaize-llama-driver.yml and cluaize-onnx-driver.yml build them.
#
#   package_core.sh <cargo profile dir> <name>
#   -> dist/<name>/ (unpacked, for the smoke test) and dist/<name>.zip (Windows) or .tar.gz
set -euo pipefail

src="$1"
name="$2"
out="dist/$name"
rm -rf "$out"
mkdir -p "$out"

windows=false
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) windows=true ;; esac

if $windows; then cp "$src/cluaiz.exe" "$out/"; else cp "$src/cluaiz" "$out/"; fi

shopt -s nullglob
for file in "$src"/*; do
  base=$(basename "$file")
  case "$base" in *.dll | *.so | *.so.* | *.dylib) ;; *) continue ;; esac
  stem=${base#lib}
  case "$stem" in ggml* | llama* | mtmd* | DirectML* | onnxruntime*) ;; *) continue ;; esac
  # GPU drivers ship as their own packs, never inside the core
  case "$stem" in ggml-cuda* | ggml-vulkan* | ggml-hip* | ggml-sycl* | ggml-opencl* | onnxruntime_providers_*) continue ;; esac

  if [ -L "$file" ] && [[ "$(readlink "$file")" != /* ]]; then
    cp -P "$file" "$out/" # libllama.so -> libllama.so.0 stays a link, not a second copy
  else
    cp -L "$file" "$out/" # ort's links point into its download cache
  fi
done
cp LICENSE THIRD_PARTY_NOTICES.md "$out/"

if $windows && command -v 7z >/dev/null; then
  (cd "$out" && 7z a -bso0 "../$name.zip" .)
elif $windows; then
  powershell -NoProfile -Command "Compress-Archive -Path '$(cygpath -w "$out")\\*' -DestinationPath '$(cygpath -w "$out").zip' -Force"
else
  tar -czf "$out.tar.gz" -C "$out" .
fi

echo "== $name"
ls -la "$out"
