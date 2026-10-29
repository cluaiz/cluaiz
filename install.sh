#!/bin/sh
# Installs Cluaiz from its GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | sh
#
# It picks the core for this OS and processor, adds the GPU pack for the GPU it finds (NVIDIA: CUDA,
# other GPUs with a Vulkan driver: Vulkan), checks every file against the release's SHA256SUMS.txt
# and unpacks them into one folder. Nothing asks a question; nothing outside that folder and the
# shell's PATH line is changed.
#
# Environment (all optional):
#   CLUAIZ_VERSION      release to install, e.g. 0.1.0 (default: the newest release)
#   CLUAIZ_INSTALL_DIR  where the files go (default: ~/.cluaiz/bin)
#   CLUAIZ_GPU          auto | cuda | vulkan | cpu (default: auto)
#   CLUAIZ_CUDA         12 | 13 (default: the newest pack the NVIDIA driver and GPU can run, see below)
set -eu

REPO="cluaiz/cluaiz"
INSTALL_DIR="${CLUAIZ_INSTALL_DIR:-$HOME/.cluaiz/bin}"
GPU="${CLUAIZ_GPU:-auto}"

BOLD='\033[1m'; CYAN='\033[0;36m'; GRAY='\033[0;90m'; GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'
step() { printf "  ${GRAY}* %s${NC}\n" "$1"; }
done_step() { printf "  ${GREEN}[DONE]${NC} %s\n" "$1"; }
fail() { printf "\n  ${RED}[ERROR] %s${NC}\n" "$1" >&2; exit 1; }

printf "\n  ${CYAN}${BOLD}cluaiz${NC}\n  ${GRAY}>_ Installing cluaiz...${NC}\n\n"

need() { command -v "$1" >/dev/null 2>&1 || fail "'$1' is needed; install it and run this again"; }
need curl
need tar
if command -v sha256sum >/dev/null 2>&1; then SHA="sha256sum"
elif command -v shasum >/dev/null 2>&1; then SHA="shasum -a 256"
else fail "'sha256sum' or 'shasum' is needed to check the download"; fi

# ── Platform ──────────────────────────────────────────────────────────────────────────────────
case "$(uname -s)" in
  Linux) os="linux" ;;
  Darwin) os="mac" ;;
  *) fail "this installer is for Linux and macOS; on Windows use install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x64" ;;
  aarch64 | arm64) arch="arm64" ;;
  *) fail "processor $(uname -m) is not supported yet" ;;
esac
platform="$os-$arch"

if [ "$os" = "linux" ]; then
  # The release is built on Ubuntu 24.04: it needs glibc 2.38 or newer
  glibc=$(ldd --version 2>/dev/null | head -n1 | grep -oE '[0-9]+\.[0-9]+$' || true)
  if [ -n "$glibc" ]; then
    major=${glibc%.*}; minor=${glibc#*.}
    if [ "$major" -lt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -lt 38 ]; }; then
      fail "glibc $glibc found; Cluaiz needs glibc 2.38 or newer (Ubuntu 24.04, Debian 13, Fedora 39 or newer)"
    fi
  fi
fi

# ── GPU ───────────────────────────────────────────────────────────────────────────────────────
exists() { for path in "$@"; do [ -e "$path" ] && return 0; done; return 1; }
if [ "$GPU" = "auto" ]; then
  GPU="cpu"
  if [ "$os" = "linux" ]; then
    if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi -L >/dev/null 2>&1; then
      GPU="cuda"
    elif exists /usr/lib*/libvulkan.so.1 /usr/lib*/*/libvulkan.so.1 && grep -qs 0x1002 /sys/class/drm/card*/device/vendor; then
      # An AMD GPU with a Vulkan loader. Integrated GPUs are often slower than the CPU, so Intel
      # and others stay on the CPU unless CLUAIZ_GPU=vulkan is set
      GPU="vulkan"
    fi
  fi
fi
[ "$os" = "mac" ] && GPU="cpu" # Metal is inside the macOS core

# ── Release ───────────────────────────────────────────────────────────────────────────────────
step "Finding the release"
if [ -n "${CLUAIZ_VERSION:-}" ]; then
  tag="v${CLUAIZ_VERSION#v}"
else
  # The newest release, pre-releases included
  # sed reads the whole answer (grep -m1 stopped early and curl printed "Failure writing output")
  tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=1" |
    sed -n -E 's/.*"tag_name": *"([^"]+)".*/\1/p' | head -n 1)
  [ -n "$tag" ] || fail "no release found on github.com/$REPO"
fi
version=${tag#v}
base="https://github.com/$REPO/releases/download/$tag"
if [ "$os" = "mac" ]; then gpu_text="Metal, inside the core"; else gpu_text="$GPU"; fi
done_step "Release $tag ($platform, GPU: $gpu_text)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$base/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt" || fail "cannot download SHA256SUMS.txt of $tag"

files="cluaiz-$version-$platform.tar.gz"
case "$GPU" in
  cuda)
    cuda="${CLUAIZ_CUDA:-}"
    if [ -z "$cuda" ]; then
      # A pack runs on a GPU it has ready-made code for, given a driver of the same CUDA major version.
      # For other GPUs (e.g. Turing, RTX 20) the driver compiles the pack's code on first use, and only a
      # driver at least as new as the pack's toolkit can: nvidia-smi prints the newest CUDA version the
      # driver supports. Toolkits and ready-made GPUs of cluaize-llama-driver.yml (linux-cuda, llama.cpp's
      # default architectures): CUDA 13.4 for 8.6 8.9 12.0 12.1, needs 7.5+; CUDA 12.8 for 8.6 8.9 12.0
      drv=$(nvidia-smi 2>/dev/null | grep -oE 'CUDA (UMD )?Version: *[0-9]+\.[0-9]+' | head -n1 | grep -oE '[0-9]+\.[0-9]+$')
      drv_num=0
      [ -n "$drv" ] && drv_num=$(( ${drv%.*} * 100 + ${drv#*.} ))
      caps=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>/dev/null | tr -d '.\r')
      # runs <major> <toolkit as 1304> <lowest GPU> <ready-made GPUs>: true when every GPU can run it
      runs() {
        [ "$drv_num" -ge $(( $1 * 100 )) ] && [ -n "$caps" ] || return 1
        for c in $caps; do
          [ "$c" -ge "$3" ] 2>/dev/null || return 1
          [ "$drv_num" -ge "$2" ] && continue
          case " $4 " in *" $c "*) ;; *) return 1 ;; esac
        done
      }
      if runs 13 1304 75 "86 89 120 121"; then cuda=13
      elif runs 12 1208 50 "86 89 120"; then cuda=12
      else
        printf "  ${GRAY}- The NVIDIA driver (CUDA %s) is too old for the CUDA packs; update it for CUDA. Using Vulkan meanwhile${NC}\n" "${drv:-unknown}"
        GPU="vulkan"
      fi
    fi
    if [ "$GPU" = cuda ]; then
      files="$files cluaiz-$version-$platform-llama-cuda$cuda.tar.gz cluaiz-$version-$platform-cudart$cuda.tar.gz cluaiz-$version-$platform-onnx-cuda$cuda.tar.gz"
      GPU="cuda$cuda"
    fi
    ;;
esac
[ "$GPU" = vulkan ] && files="$files cluaiz-$version-$platform-llama-vulkan.tar.gz"

# ── Download, check, unpack ───────────────────────────────────────────────────────────────────
for file in $files; do
  if ! grep -q " \*\{0,1\}$file\$" "$tmp/SHA256SUMS.txt"; then
    case "$file" in
      "cluaiz-$version-$platform.tar.gz") fail "release $tag has no build for $platform" ;;
      *) printf "  ${GRAY}- %s is not in this release; models run on the CPU instead${NC}\n" "$file"; continue ;;
    esac
  fi
  step "Downloading $file"
  curl -fL --progress-bar "$base/$file" -o "$tmp/$file" || fail "download of $file failed"
  expected=$(grep " \*\{0,1\}$file\$" "$tmp/SHA256SUMS.txt" | cut -d' ' -f1)
  actual=$(cd "$tmp" && $SHA "$file" | cut -d' ' -f1)
  [ "$expected" = "$actual" ] || fail "$file does not match SHA256SUMS.txt (expected $expected, got $actual)"
  done_step "$file (checksum OK)"
done

step "Installing into $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
for file in $files; do
  [ -f "$tmp/$file" ] && tar -xzf "$tmp/$file" -C "$INSTALL_DIR"
done
chmod +x "$INSTALL_DIR/cluaiz"
done_step "Installed into $INSTALL_DIR"

# ── PATH ──────────────────────────────────────────────────────────────────────────────────────
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    line="export PATH=\"$INSTALL_DIR:\$PATH\""
    for rc in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
      if [ -f "$rc" ] && ! grep -qF "$INSTALL_DIR" "$rc"; then
        printf '\n# Cluaiz\n%s\n' "$line" >>"$rc"
      fi
    done
    printf "  ${GRAY}Open a new terminal (or run: %s) to use 'cluaiz'${NC}\n" "$line"
    ;;
esac

"$INSTALL_DIR/cluaiz" --version
printf "\n  ${GREEN}[DONE] Cluaiz is installed.${NC}\n"
printf "  ${GRAY}Next: cluaiz pull <model>   then   cluaiz run <model>${NC}\n\n"
