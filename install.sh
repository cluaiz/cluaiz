#!/bin/bash
# cluaiz Core Infrastructure Installer - VERSION 0.1.0
# Industrial Standard Deployment Script

set -euo pipefail

HUB_PATH="${HOME}/.cluaiz"
REPO="cluaiz/cluaiz"

# --- UI Matrix (Industrial) ---
BOLD='\033[1m'; CYAN='\033[0;36m'; GRAY='\033[0;90m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; RED='\033[0;31m'; NC='\033[0m'

write_step() { echo -ne "  ${GRAY}[ ] $1...${NC}"; }
complete_step() { echo -e "\r  ${GREEN}[✓]${NC} $1   "; }
write_success() { echo -e "\n  ${GREEN}[OK] $1${NC}"; }
write_error() { echo -e "\n  ${RED}[ERR] $1${NC}"; }

# --- Header ---
clear
echo -e "\n  ${BOLD}cluaiz CORE INFRASTRUCTURE (V0.1.0)${NC}"
echo -e "  ${GRAY}Industrial Deployment Sequence${NC}\n"

# 1. Environment Provisioning
write_step "Provisioning environment"
mkdir -p "$HUB_PATH/bin" "$HUB_PATH/apps/cli" "$HUB_PATH/engine" "$HUB_PATH/interface-engines/kernels" "$HUB_PATH/interface-engines/drivers"
complete_step "Provisioning environment"

# 2. System Integration
if [[ ":$PATH:" != *":$HUB_PATH/bin:"* ]]; then
    SHELL_RC="$HOME/.bashrc"
    [[ "$SHELL" == *"zsh"* ]] && SHELL_RC="$HOME/.zshrc"
    if ! grep -q "cluaiz_ROOT" "$SHELL_RC" 2>/dev/null; then
        echo -e "\n# cluaiz Environment\nexport cluaiz_ROOT=\"$HUB_PATH\"\nexport PATH=\"\$PATH:$HUB_PATH/bin\"" >> "$SHELL_RC"
    fi
    export cluaiz_ROOT="$HUB_PATH"
    export PATH="$PATH:$HUB_PATH/bin"
fi

# 3. Sovereign Registry Sync
write_step "Synchronizing Neural Registry"
MASTER_REGISTRY_URL="https://raw.githubusercontent.com/cluaiz/cluaiz/main/package.json"
MASTER_JSON=$(curl -sL "$MASTER_REGISTRY_URL")

OS_TYPE=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH_TYPE=$(uname -m)
case "$OS_TYPE" in
    linux) OS="linux"; EXT="so" ;;
    darwin) OS="mac"; EXT="dylib" ;;
    *) write_error "Unsupported OS"; exit 1 ;;
esac
case "$ARCH_TYPE" in
    x86_64) ARCH="x64" ;;
    aarch64|arm64) ARCH="arm64" ;;
    *) write_error "Unsupported Arch"; exit 1 ;;
esac
PLATFORM="$OS-$ARCH"
complete_step "Synchronizing Neural Registry"

# --- CLI Deployment (Driven by package.json) ---
CLI_MANIFEST_URL=$(echo "$MASTER_JSON" | grep -oE '"manifest_url": "[^"]+"' | sed -n '1p' | cut -d'"' -f4)
CLI_MANIFEST=$(curl -sL "$CLI_MANIFEST_URL")
CLI_URL=$(echo "$CLI_MANIFEST" | grep -oE '"'"$PLATFORM"'" : "[^"]+"' | cut -d'"' -f4)

if [ -n "$CLI_URL" ]; then
    write_step "Retrieving CLI ($PLATFORM)"
    curl -sL "$CLI_URL" -o "$HUB_PATH/apps/cli/cluaiz"
    chmod +x "$HUB_PATH/apps/cli/cluaiz"
    ln -sf "$HUB_PATH/apps/cli/cluaiz" "$HUB_PATH/bin/cluaiz"
    complete_step "Retrieving CLI ($PLATFORM)"
fi

# --- Engine Deployment (Driven by package.json) ---
ENGINE_MANIFEST_URL=$(echo "$MASTER_JSON" | grep -oE '"manifest_url": "[^"]+"' | sed -n '2p' | cut -d'"' -f4)
ENGINE_MANIFEST=$(curl -sL "$ENGINE_MANIFEST_URL")
ENGINE_URL=$(echo "$ENGINE_MANIFEST" | grep -oE '"'"$PLATFORM"'" : "[^"]+"' | cut -d'"' -f4)

write_step "Retrieving Core Engine"
curl -sL "$ENGINE_URL" -o "$HUB_PATH/engine/cluaiz-engine.$EXT"
complete_step "Retrieving Core Engine"

# --- Kernel Deployment (Driven by package.json) ---
KERNEL_MANIFEST_URL=$(echo "$MASTER_JSON" | grep -oE '"manifest_url": "[^"]+"' | sed -n '3p' | cut -d'"' -f4)
KERNEL_MANIFEST=$(curl -sL "$KERNEL_MANIFEST_URL")

if [[ "$OS" == "mac" ]]; then
    TARGET_PLATFORM="$PLATFORM"
else
    if [[ "$ARCH" == "arm64" ]]; then
        TARGET_PLATFORM="linux-arm64"
    else
        HAS_AVX512=$(grep -o "avx512f" /proc/cpuinfo | head -1 || echo "")
        TARGET_PLATFORM=$([ -n "$HAS_AVX512" ] && echo "linux-x64-avx512" || echo "linux-x64-avx2")
    fi
fi

KERNEL_URL=$(echo "$KERNEL_MANIFEST" | grep -oE '"'"$TARGET_PLATFORM"'" : "[^"]+"' | cut -d'"' -f4)

if [ -n "$KERNEL_URL" ]; then
    write_step "Retrieving Core Kernel ($TARGET_PLATFORM)"
    curl -sL "$KERNEL_URL" -o "$HUB_PATH/interface-engines/kernels/cluaiz-llama.$EXT"
    complete_step "Retrieving Core Kernel ($TARGET_PLATFORM)"
fi

write_success "Deployment successful."

# 🧠 cluaizdb FFI Brain Setup
echo ""
echo -e "  ${YELLOW}>_ Optional: Enable the cluaizdb Memory Brain? (y/n)${NC}"
read -p "    Choice: " brainChoice
if [[ "$brainChoice" =~ ^[Yy]$ ]]; then
    export cluaizdb_FFI=1
    echo -e "    ${GREEN}[ENABLED]${NC} cluaizdb FFI Memory Brain activated."
else
    export cluaizdb_FFI=0
    echo -e "    ${GRAY}[DISABLED]${NC} Using legacy file-based memory."
fi

# 🧬 Pre-Flight Calibration
echo -e "\n  ${CYAN}>_ Synchronizing Hardware DNA...${NC}"
"$HUB_PATH/bin/cluaiz" --calibrate

echo -e "\n  ${GRAY}>_ Launching CLI...${NC}"

# Launch CLI
"$HUB_PATH/bin/cluaiz"
