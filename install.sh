#!/bin/sh
# Installs Cluaiz from its GitHub release:
#
#   curl -fsSL https://cluaiz.com/install.sh | bash
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

BOLD='\033[1m'; WHITE='\033[1;37m'; CYAN='\033[0;36m'; GRAY='\033[0;90m'; GREEN='\033[0;32m'; RED='\033[0;31m'; NC='\033[0m'
install_step=0
interactive=0
if [ -t 1 ] && [ "${TERM:-dumb}" != dumb ]; then interactive=1; fi
step() { install_step=$((install_step + 1)); printf "\n  ${CYAN}[%s/7]${NC} %s\n" "$install_step" "$1"; }
done_step() { printf "  ${GREEN}[DONE]${NC} %s\n" "$1"; }
fail() { printf "\n  ${RED}[ERROR] %s${NC}\n" "$1" >&2; exit 1; }

# Bootstrap equivalent of cmd/src/ui/spinner.rs: no installed CLI is available yet.
transfer_view() {
  awk -v got="$1" -v total="$2" -v elapsed="$3" -v complete="$4" -v tick="$5" '
    function bytes(n, u) {
      u=1; while(n>=1024 && u<5){n/=1024;u++}
      return sprintf("%.1f %s",n,units[u])
    }
    BEGIN {
      split("B KiB MiB GiB TiB",units," ")
      split("⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏",frames," ")
      frame=complete ? " " : frames[tick%10+1]
      rate=elapsed>0 ? got/elapsed : 0
      if(complete && total<=0)total=got
      if(total>0){
        percent=int(100*got/total);if(percent>100)percent=100
        filled=int(percent/5);bar=""
        for(i=0;i<20;i++)bar=bar (i<filled ? "#" : "-")
        left=total-got;if(left<0)left=0
        eta=rate>0 ? sprintf("%.0fs",int(left/rate+0.999999)) : "--"
        printf "  %s [%s] %3d%% | %s / %s\n",frame,bar,percent,bytes(got),bytes(total)
        printf "  Left %s | %s/s | File ETA ~%s\n",bytes(left),bytes(rate),eta
      }else{
        printf "  %s [--------------------] --%% | %s / unknown\n",frames[tick%10+1],bytes(got)
        printf "  Left unknown | %s/s | File ETA --\n",bytes(rate)
      }
    }'
}

download_file() {
  dl_url=$1; dl_out=$2; dl_name=$3
  dl_headers="$dl_out.headers"; dl_errors="$dl_out.stderr"
  dl_start=$(date +%s); dl_tick=0; dl_drawn=0; dl_next_log=0
  printf '  %s\n' "$dl_name"
  curl -fLsS --connect-timeout 30 --speed-limit 1 --speed-time 120 \
    --dump-header "$dl_headers" --output "$dl_out" --url "$dl_url" 2>"$dl_errors" &
  download_pid=$!
  while kill -0 "$download_pid" 2>/dev/null; do
    draw_download 0
    dl_tick=$((dl_tick + 1))
    sleep 0.25
  done
  dl_code=0
  wait "$download_pid" || dl_code=$?
  download_pid=''
  if [ "$dl_code" -ne 0 ]; then
    cat "$dl_errors" >&2
    fail "download of $dl_name failed (curl $dl_code)"
  fi
  draw_download 1
  rm -f "$dl_headers" "$dl_errors"
}

draw_download() {
  dl_finished=$1
  dl_now=$(date +%s); dl_elapsed=$((dl_now - dl_start))
  if [ "$interactive" -eq 0 ] && [ "$dl_elapsed" -lt "$dl_next_log" ] && [ "$dl_finished" -eq 0 ]; then return; fi
  dl_got=0
  if [ -f "$dl_out" ]; then dl_got=$(wc -c <"$dl_out" | tr -d ' '); fi
  dl_total=0
  if [ -f "$dl_headers" ]; then
    # Reset at every response: a redirect length must not become the final total.
    dl_total=$(awk 'BEGIN{n=0} /^HTTP\//{n=0} tolower($1)=="content-length:"{n=$2;gsub("\r","",n)} END{print n}' "$dl_headers")
  fi
  if [ "$interactive" -eq 1 ] && [ "$dl_drawn" -eq 1 ]; then printf '\033[2A'; fi
  if [ "$interactive" -eq 1 ]; then
    transfer_view "$dl_got" "$dl_total" "$dl_elapsed" "$dl_finished" "$dl_tick" | while IFS= read -r dl_line; do printf '\r\033[2K%s\n' "$dl_line"; done
  else
    transfer_view "$dl_got" "$dl_total" "$dl_elapsed" "$dl_finished" "$dl_tick"
  fi
  dl_drawn=1; dl_next_log=$((dl_elapsed + 5))
}

cleanup() {
  if [ -n "${download_pid:-}" ]; then kill "$download_pid" 2>/dev/null || true; wait "$download_pid" 2>/dev/null || true; fi
  if [ -n "${tmp:-}" ] && [ "${tmp%/*}" = "${scratch_parent:-}" ]; then
    case "$tmp" in "$scratch_parent"/cluaiz-install.*) rm -rf -- "$tmp" ;; esac
  fi
}

# ── Mascot (High-Detail Braille) ─────────────────────────────────────────────────────────────
# Keep artwork gaps as U+2800 (not ASCII spaces), in sync with install.ps1.
printf "\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣠⣶⣾⣾⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣦⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣺⣿⣿⣿⡿⠛⠋⠈⠀⠀⠀⠀⠈⠉⠋⠀⠀⠀⠀⠀⠈⠋⠻⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⡟⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⣿⡏⠀⠀⠀⠀⠀⣠⣠⡀⠀⠀⠀⠀⠀⠀⠀⣀⣠⣀⠀⠀⠀⠀⠀⠸⣿⣿⡆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠠⣾⣿⣿⡂⠀⠀⠀⠀⣺⣿⣿⣷⡂⠀⠀⠀⠀⠀⠀⣾⣿⣿⡆⠀⠀⠀⠀⠀⣻⣿⣿⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⡏⠀⠀⠀⠀⠀⢺⣿⣿⣿⡂⠀⠀⠀⠀⠀⠈⣿⣿⣿⡏⠀⠀⠀⠀⠀⣺⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⣿⡯⠀⠀⠀⠀⠀⠈⠻⠻⠋⢠⣶⣶⣦⣶⣶⡆⠘⠻⠛⠂⠀⠀⠀⠀⠀⣺⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣺⣿⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⡿⣿⡿⠛⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣿⣿⣿⣿⣧⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣢⣠⣀⣀⡀⡀⡀⡀⡀⣀⣀⣀⣠⣠⣦⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠸⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⡿⠿⡿⠛⠋⢻⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠛⠻⡿⠿⡿⠿⠋⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠋⠀⠀⠈⠙⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢿⣿⣿⣿⣿⣿⣿⡿⠂⠀⠀⠀⠀⠀⠈⠻⣿⣿⣿⣿⣿⣿⣿⡋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "  ${WHITE}⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⠻⠛⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⠻⠛⠋⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀${NC}\n"
printf "\n"
printf "  ${WHITE}         ██████╗██╗     ██╗   ██╗ █████╗ ██╗███████╗${NC}\n"
printf "  ${WHITE}        ██╔════╝██║     ██║   ██║██╔══██╗██║╚══███╔╝${NC}\n"
printf "  ${WHITE}        ██║     ██║     ██║   ██║███████║██║  ███╔╝ ${NC}\n"
printf "  ${WHITE}        ██║     ██║     ██║   ██║██╔══██║██║ ███╔╝  ${NC}\n"
printf "  ${WHITE}        ╚██████╗███████╗╚██████╔╝██║  ██║██║███████╗${NC}\n"
printf "  ${WHITE}         ╚═════╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝${NC}\n"
printf "\n  ${GRAY}Installing cluaiz...${NC}\n\n"

need() { command -v "$1" >/dev/null 2>&1 || fail "'$1' is needed; install it and run this again"; }
step 'Checking this system'
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
done_step "Platform $platform, requested GPU: $GPU"
scratch_parent=$(cd "${TMPDIR:-/tmp}" && pwd -P)
tmp=$(mktemp -d "$scratch_parent/cluaiz-install.XXXXXXXX")
download_pid=''
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ── Release ───────────────────────────────────────────────────────────────────────────────────
step "Finding the release"
if [ -n "${CLUAIZ_VERSION:-}" ]; then
  tag="v${CLUAIZ_VERSION#v}"
else
  # The newest release, pre-releases included
  # sed reads the whole answer (grep -m1 stopped early and curl printed "Failure writing output")
  download_file "https://api.github.com/repos/$REPO/releases?per_page=1" "$tmp/release.json" 'Release metadata'
  tag=$(sed -n -E 's/.*"tag_name": *"([^"]+)".*/\1/p' "$tmp/release.json" | head -n 1)
  [ -n "$tag" ] || fail "no release found on github.com/$REPO"
fi
version=${tag#v}
base="https://github.com/$REPO/releases/download/$tag"
if [ "$os" = "mac" ]; then gpu_text="Metal, inside the core"; else gpu_text="$GPU"; fi
done_step "Release $tag ($platform, GPU: $gpu_text)"

step 'Reading the package manifest'
download_file "$base/SHA256SUMS.txt" "$tmp/SHA256SUMS.txt" 'SHA256SUMS.txt'

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
available=''; package_count=0
for file in $files; do
  if ! grep -q " \*\{0,1\}$file\$" "$tmp/SHA256SUMS.txt"; then
    case "$file" in
      "cluaiz-$version-$platform.tar.gz") fail "release $tag has no build for $platform" ;;
      *) printf "  ${GRAY}- %s is not in this release; models run on the CPU instead${NC}\n" "$file"; continue ;;
    esac
  fi
  available="$available $file"; package_count=$((package_count + 1))
done
step "Downloading packages (0/$package_count verified)"
verified=0
for file in $available; do
  printf '  Package %s/%s: %s\n' "$((verified + 1))" "$package_count" "$file"
  download_file "$base/$file" "$tmp/$file" "$file"
  printf '  Checking SHA-256: %s\n' "$file"
  expected=$(grep " \*\{0,1\}$file\$" "$tmp/SHA256SUMS.txt" | cut -d' ' -f1)
  actual=$(cd "$tmp" && $SHA "$file" | cut -d' ' -f1)
  [ "$expected" = "$actual" ] || fail "$file does not match SHA256SUMS.txt (expected $expected, got $actual)"
  verified=$((verified + 1))
  done_step "$file (checksum OK; $verified/$package_count verified)"
done

step "Installing into $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
unpacked=0
for file in $available; do
  printf '  Extracting %s/%s: %s\n' "$((unpacked + 1))" "$package_count" "$file"
  tar -xzf "$tmp/$file" -C "$INSTALL_DIR"
  unpacked=$((unpacked + 1))
done
chmod +x "$INSTALL_DIR/cluaiz"
done_step "Installed into $INSTALL_DIR"

# ── PATH ──────────────────────────────────────────────────────────────────────────────────────
step 'Preparing runtime and PATH'
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

done_step 'Runtime and PATH ready'

step 'Verifying the installed Engine'
"$INSTALL_DIR/cluaiz" --version
printf "\n  ${GREEN}[DONE] Cluaiz is installed (7/7 stages complete).${NC}\n"
printf "  ${GRAY}Next: cluaiz pull <model>   then   cluaiz run <model>${NC}\n\n"
