#!/bin/bash
# Run 28 on a fresh GCP e2-standard-2 (8 GB, Ubuntu 24.04): build and run the real OS sandbox test.
set -x
exec > ~/run28.log 2>&1
sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq bubblewrap build-essential pkg-config curl python3 >/dev/null
uname -a; lsb_release -ds; bwrap --version; sysctl kernel.apparmor_restrict_unprivileged_userns kernel.unprivileged_userns_clone 2>&1
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
. ~/.cargo/env
mkdir -p ~/src && tar -xzf ~/src.tgz -C ~/src
cd ~/src
mkdir -p ~/.cargo && [ -f ~/.cargo/credentials.toml ] || printf '[registry]\ntoken = "dummy-test-token"\n' > ~/.cargo/credentials.toml
mkdir -p ~/.ssh && [ -f ~/.ssh/run28_dummy ] || echo dummy > ~/.ssh/run28_dummy
time cargo test --release -p cluaiz-agents --test os_sandbox_real -- --ignored --nocapture
echo "EXIT $?"
time cargo test --release -p cluaiz-agents --lib os_sandbox
echo "DONE"
