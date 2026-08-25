#!/usr/bin/env bash
# Cluaiz Automated One-Liner Installer (Linux & macOS)
# Usage: curl -fsSL https://cluaiz.com/install.sh | bash
#    or: curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.sh | bash

set -euo pipefail

HUB_PATH="${HOME}/.cluaiz"
BIN_PATH="${HUB_PATH}/bin"
MODELS_PATH="${HUB_PATH}/models"
CONFIG_PATH="${HUB_PATH}/engine/config"
REPO="cluaiz/cluaiz"

# --- UI Formatting ---
BOLD='\033[1m'
CYAN='\033[0;36m'
GRAY='\033[0;90m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

write_step() { echo -ne "  ${GRAY}* $1...${NC}"; }
complete_step() { echo -e "\r  ${GREEN}[DONE]${NC} $1   "; }
write_error() { echo -e "\n  ${RED}[ERROR] $1${NC}"; }

clear 2>/dev/null || true
echo -e "\n  ${CYAN}██████╗██╗     ██╗   ██╗ █████╗ ██╗███████╗"
echo -e " ██╔════╝██║     ██║   ██║██╔══██╗██║╚══███╔╝"
echo -e " ██║     ██║     ██║   ██║███████║██║  ███╔╝ "
echo -e " ██║     ██║     ██║   ██║██╔══██║██║ ███╔╝  "
echo -e " ╚██████╗███████╗╚██████╔╝██║  ██║██║███████╗"
echo -e "  ╚═════╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝${NC}"
echo -e "  ${GRAY}>_ Installing Cluaiz Native AI Runtime...${NC}\n"

# 1. Directory Structure Provisioning
write_step "Provisioning environment directories"
mkdir -p "$BIN_PATH" "$MODELS_PATH" "$CONFIG_PATH"
complete_step "Provisioning environment directories"

# 2. Platform & Architecture Resolution
write_step "Detecting hardware architecture"
OS_TYPE=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH_TYPE=$(uname -m)

case "$OS_TYPE" in
    linux) OS="linux" ;;
    darwin) OS="darwin" ;;
    *) write_error "Unsupported operating system: $OS_TYPE"; exit 1 ;;
esac

case "$ARCH_TYPE" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) write_error "Unsupported CPU architecture: $ARCH_TYPE"; exit 1 ;;
esac

TARGET="${OS}-${ARCH}"
complete_step "Hardware detected: $TARGET"

# 3. Binary Download
write_step "Downloading Cluaiz binary ($TARGET)"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/cluaiz-${TARGET}.tar.gz"
FALLBACK_URL="https://github.com/${REPO}/releases/latest/download/cluaiz"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

if curl -sLf "$DOWNLOAD_URL" -o "$TMP_DIR/cluaiz.tar.gz" 2>/dev/null; then
    tar -xzf "$TMP_DIR/cluaiz.tar.gz" -C "$BIN_PATH"
    chmod +x "$BIN_PATH/cluaiz"
elif curl -sLf "$FALLBACK_URL" -o "$BIN_PATH/cluaiz" 2>/dev/null; then
    chmod +x "$BIN_PATH/cluaiz"
else
    write_error "Failed to download binary from GitHub Releases for $TARGET"
    echo -e "  ${GRAY}Build from source using: cargo build --release${NC}"
    exit 1
fi
complete_step "Cluaiz binary mounted at $BIN_PATH/cluaiz"

# 4. Environment Path Configuration
write_step "Configuring shell environment PATH"
SHELL_NAME=$(basename "${SHELL:-bash}")
SHELL_RC="$HOME/.bashrc"
if [ "$SHELL_NAME" = "zsh" ]; then
    SHELL_RC="$HOME/.zshrc"
fi

if [[ ":$PATH:" != *":$BIN_PATH:"* ]]; then
    if ! grep -q "cluaiz_ROOT" "$SHELL_RC" 2>/dev/null; then
        echo -e "\n# Cluaiz Environment\nexport cluaiz_ROOT=\"$HUB_PATH\"\nexport PATH=\"\$PATH:$BIN_PATH\"" >> "$SHELL_RC"
    fi
    export cluaiz_ROOT="$HUB_PATH"
    export PATH="$PATH:$BIN_PATH"
fi
complete_step "Environment configured in $SHELL_RC"

echo -e "\n  ${GREEN}[DONE] Installation successful!${NC}\n"
echo -e "  ${CYAN}🚀 Getting Started:${NC}"
echo -e "     ${GRAY}cluaiz serve          # Start OpenAI-compatible API daemon (:8000)${NC}"
echo -e "     ${GRAY}cluaiz                # Launch Interactive Terminal Dashboard${NC}"
echo -e "     ${GRAY}cluaiz pull <model>   # Pull GGUF/ONNX model from Hugging Face${NC}\n"
