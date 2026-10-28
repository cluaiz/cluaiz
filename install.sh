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
#   CLUAIZ_CUDA         12 | 13 (default: 13 with NVIDIA driver 580+ on an RTX 20 series or newer, else 12)
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
  tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=1" |
    grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
  [ -n "$tag" ] || fail "no release found on github.com/$REPO"
fi
version=${tag#v}
base="https://github.com/$REPO/releases/download/$tag"
done_step "Release $tag ($platform, GPU: $GPU)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$base/SHA256SUMS.txt" -o "$tmp/SHA256SUMS.txt" || fail "cannot download SHA256SUMS.txt of $tag"

files="cluaiz-$version-$platform.tar.gz"
case "$GPU" in
  cuda)
    cuda="${CLUAIZ_CUDA:-}"
    if [ -z "$cuda" ]; then
      # CUDA 13 needs driver 580+ and compute capability 7.5+ (Turing); older drivers and GPUs use CUDA 12
      driver=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -n1 | cut -d. -f1)
      cap=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>/dev/null | tr -d . | sort -n | head -n1)
      if [ "${driver:-0}" -ge 580 ] 2>/dev/null && [ "${cap:-0}" -ge 75 ] 2>/dev/null; then cuda=13; else cuda=12; fi
    fi
    files="$files cluaiz-$version-$platform-llama-cuda$cuda.tar.gz cluaiz-$version-$platform-cudart$cuda.tar.gz cluaiz-$version-$platform-onnx-cuda$cuda.tar.gz"
    GPU="cuda$cuda"
    ;;
  vulkan) files="$files cluaiz-$version-$platform-llama-vulkan.tar.gz" ;;
esac

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
