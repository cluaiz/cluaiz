# Install and CPU runs on clean machines: Linux x64 / ARM64, Windows Server, macOS (2026-11-09 / 10)

> **Summary.** Release v0.1.0 (first build, commit `e3beb91c`) installed by its one command on clean cloud
> machines: AWS Graviton2 (Linux ARM64, the first run of that core on hardware), an AWS Sapphire Rapids Xeon
> (Ubuntu 24.04 and Windows Server 2025), Docker images of Ubuntu 22.04 / 24.04, Debian 12 / 13, Rocky 9 and
> Fedora 42, and a GitHub macOS 15 runner (Apple M1, virtual). Every supported system installed, answered with
> Qwen3 0.6B and benchmarked; older Linux stopped with the glibc message. The runs found the bugs fixed in
> `f2769a6c`, `18edfb0c`, `2c9c76b9`, `c30bb908`, `772d790d` and `b3293e20`; v0.1.0 was then rebuilt from
> `bbaae441` with those fixes.

No GPU in this run: these machines have none (or, on macOS, a virtual one), so it covers install, the CPU
build, the planner and the server. Model: `unsloth/Qwen3-0.6B-GGUF` Q4_K_M, `cluaiz bench -r 2` (pp512, tg128).

## Machines and results

| Machine | OS | Install | Model answers | pp512 t/s | tg128 t/s | CPU build ggml loaded |
|---|---|---|---|---:|---:|---|
| AWS t4g.small: Graviton2 (Neoverse-N1), 2 vCPU, 1.8 GB | Ubuntu 24.04 ARM64 | ✅ `install.sh` | ✅ | 56.6 | 23.9 | `armv8.2_2` |
| AWS c7i-flex.large: Xeon Platinum 8488C (AVX-512, AMX), 1 core / 2 threads, 3.7 GB | Ubuntu 24.04 x64 | ✅ `install.sh` | ✅ | 192.3 | 19.3 | `sapphirerapids` |
| AWS m7i-flex.large: same Xeon, 7.7 GB | Windows Server 2025 | ✅ `install.ps1` | ✅ | 130.9 | 18.5 | `cascadelake` (no sapphirerapids build on Windows) |
| GitHub `macos-15`: Apple M1 (virtual), 7 GB | macOS 15.8 | ✅ `install.sh` | ✅ | 176.7 | 2.1 | Metal (`MTL0`, virtual GPU) |
| Docker on the x64 machine | Debian 13, Fedora 42, Ubuntu 24.04 | ✅ | ✅ | | | |
| Docker on the x64 machine | Ubuntu 22.04 (glibc 2.35), Debian 12 (2.36), Rocky 9 (2.34) | ⛔ stops: "Cluaiz needs glibc 2.38 or newer" | | | | |

Also on the 3.7 GB x64 machine: Qwen3 4B Q4_K_M (2.3 GB file) planned CPU only, 4096 tokens, 2.91 GB, answered;
pp512 23.1 t/s, tg128 3.3 t/s.

The macOS runner's GPU is virtual: the run shows that auto placement uses Metal (`Full GPU`, all layers), not
how fast a real Mac is. Speeds on a real Mac: [M5 run](../../../mac/runs/2026-10-28-m5-air-16gb/SUMMARY.md).

## Checks

| Check | Result |
|---|---|
| PATH after install | bash: `~/.bashrc`; zsh with no `~/.zshrc` (a new macOS account): the file is created; fish: `conf.d/cluaiz.fish`. A new shell finds `cluaiz` in each (Ubuntu and macOS) |
| Windows PATH | the user PATH gets `~\.cluaiz\bin` |
| `cluaiz status` | names the GPU backend that loaded (`none, models run on the CPU`, or `MTL0: ...`) |
| Port already in use | `port 8011 is already in use by another program ... cluaiz permission port <number>`, exit 1 |
| `cluaiz serve` + `/health` | 200; stops on Ctrl+C |

## Bugs found, and their fixes

| Bug | Fix |
|---|---|
| The README Quick Start `cluaiz run <repository> "prompt"` printed "already downloaded" and did not answer; a first `run` downloaded and stopped | `f2769a6c` |
| GitHub answered HTTP 500 for a release file for a few minutes after publishing, and the install failed | `18edfb0c` (curl retries 3 times) |
| `api.github.com` answered 403 (60 calls an hour per IP) on GitHub's macOS runners; the same happens on shared office, CI and mobile IPs | `2c9c76b9` (fall back to the releases feed) |
| The pull summary showed Qwen3-0.6B as "6B" | `c30bb908` |
| `status` named Windows Server 2025 "Windows 11" | `772d790d` |
| `cluaiz mcp start` and `cluaiz plugin link` only printed a message | `b3293e20` (removed) |
| The installer printed color codes into logs | `18edfb0c` |
| `cluaiz.com/install.sh` answered 403 (a Cloudflare challenge) to cloud IPs | the site's bot setting was changed; 200 from AWS since |

## Not covered

GPU packs (CUDA 12 / 13 on Windows and Linux other than the RTX 3050 and the T4 above, Vulkan), integrated
GPUs, two GPUs, and speed on a real Mac.
