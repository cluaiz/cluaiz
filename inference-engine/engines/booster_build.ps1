<#
.SYNOPSIS
Archer Sovereign Engine - Booster Build Script (Release Mode Enforcer)

.DESCRIPTION
This script strictly enforces cargo build in --release mode.
In the Sovereign Architecture, the Rust (CPU Manager) to C++ (GPU Worker)
FFI communication loop requires Link-Time Optimization (LTO) to eliminate
latency toll plazas. Compiling in Debug mode cripples the engine to ~23 TPS.
This script ensures the engine is always compiled with max optimizations,
driving inference to 60+ TPS.
#>

param(
    [switch]$RunInference,
    [switch]$RunBenchmark
)

$ErrorActionPreference = "Stop"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "   🚀 ARCHER SOVEREIGN ENGINE: BOOSTER COMPILER (V6)            " -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Cyan

Write-Host "[*] Checking Rust toolchain..."
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo is not installed or not in PATH."
}

Write-Host "[*] Engaging LLVM Link-Time Optimizations (LTO)..."
Write-Host "[*] Compiling sovereign architecture (Release Profile)..." -ForegroundColor Yellow

# Build Workspace entirely in release mode to fuse FFI calls natively
cargo build --workspace --release

if ($LASTEXITCODE -ne 0) {
    Write-Host "[X] Compilation Failed! Check Sovereign Core logic." -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "[+] Build Successful. Engine is now operating at MAX TQ/TPS." -ForegroundColor Green

if ($RunBenchmark) {
    Write-Host "[*] Launching Sovereign Benchmark (Agnostic)..." -ForegroundColor Magenta
    cargo test --release sovereign_benchmark -- --nocapture
}
elseif ($RunInference) {
    Write-Host "[*] Launching Engine..." -ForegroundColor Magenta
    cargo run --release
}
