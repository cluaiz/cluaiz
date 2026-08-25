# Cluaiz Automated One-Liner Installer (Windows PowerShell)
# Usage: irm https://cluaiz.com/install.ps1 | iex
#    or: powershell -c "irm https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.ps1 | iex"

param (
    [string]$Version = 'latest'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# --- UI Formatting ---
$E = [char]27
$BOLD = "$E[1m"; $CYAN = "$E[36m"; $GRAY = "$E[90m"; $GREEN = "$E[32m"; $RED = "$E[31m"; $NC = "$E[0m"

function Write-Step ([string]$msg) {
    Write-Host ("  " + $GRAY + "* " + $msg + $NC) -NoNewline
}

function Complete-Step ([string]$msg) {
    $clear = "`r" + (" " * 80) + "`r"
    Write-Host -NoNewline $clear
    Write-Host ("  " + $GREEN + "[DONE] " + $NC + $msg)
}

function Write-Fail ([string]$msg) { 
    Write-Host ("`n  " + $RED + "[ERROR] " + $msg + $NC) -ForegroundColor Red
}

# --- UTF-8 & Security Protocol ---
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls13

Clear-Host
Write-Host ""
Write-Host "   ██████╗██╗     ██╗   ██╗ █████╗ ██╗███████╗" -ForegroundColor Cyan
Write-Host "  ██╔════╝██║     ██║   ██║██╔══██╗██║╚══███╔╝" -ForegroundColor Cyan
Write-Host "  ██║     ██║     ██║   ██║███████║██║  ███╔╝ " -ForegroundColor Cyan
Write-Host "  ██║     ██║     ██║   ██║██╔══██║██║ ███╔╝  " -ForegroundColor Cyan
Write-Host "  ╚██████╗███████╗╚██████╔╝██║  ██║██║███████╗" -ForegroundColor Cyan
Write-Host "   ╚═════╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝" -ForegroundColor Cyan
Write-Host ""
Write-Host "  >_ Installing Cluaiz Native AI Runtime..." -ForegroundColor Gray
Write-Host ""

try {
    $HubPath = if ($env:cluaiz_ROOT) { $env:cluaiz_ROOT } else { Join-Path $HOME '.cluaiz' }
    $BinPath = Join-Path $HubPath 'bin'
    $ModelsPath = Join-Path $HubPath 'models'
    $ConfigPath = Join-Path $HubPath 'engine\config'

    # 1. Directory Structure Provisioning
    $step1 = 'Provisioning environment directories'
    Write-Step $step1
    foreach ($p in @($BinPath, $ModelsPath, $ConfigPath)) {
        if (-not (Test-Path $p)) { New-Item -ItemType Directory -Path $p -Force | Out-Null }
    }
    Complete-Step $step1

    # 2. Architecture Resolution
    $step2 = 'Detecting hardware architecture'
    Write-Step $step2
    $Arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'windows-arm64' } else { 'windows-x86_64' }
    Complete-Step "$step2 ($Arch)"

    # 3. Binary Download
    $step3 = "Downloading Cluaiz binary ($Arch)"
    Write-Step $step3
    
    $DownloadUrl = if ($Version -eq 'latest') {
        "https://github.com/cluaiz/cluaiz/releases/latest/download/cluaiz-${Arch}.zip"
    } else {
        "https://github.com/cluaiz/cluaiz/releases/download/${Version}/cluaiz-${Arch}.zip"
    }

    $ZipTarget = Join-Path $HubPath 'cluaiz-download.zip'
    $ExeTarget = Join-Path $BinPath 'cluaiz.exe'

    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipTarget -ErrorAction Stop
        Expand-Archive -Path $ZipTarget -DestinationPath $BinPath -Force
        Remove-Item $ZipTarget -Force -ErrorAction SilentlyContinue
    } catch {
        # Fallback to direct raw binary if zip is unavailable
        $DirectUrl = "https://github.com/cluaiz/cluaiz/releases/latest/download/cluaiz.exe"
        Invoke-WebRequest -Uri $DirectUrl -OutFile $ExeTarget -ErrorAction Stop
    }
    Complete-Step "Cluaiz binary mounted at $ExeTarget"

    # 4. Environment Path Configuration
    $step4 = 'Configuring User PATH environment'
    Write-Step $step4
    [System.Environment]::SetEnvironmentVariable('cluaiz_ROOT', $HubPath, 'User')
    $CurrentPath = [System.Environment]::GetEnvironmentVariable('Path', 'User')
    if ($CurrentPath -notlike "*$BinPath*") {
        $NewPath = if ($CurrentPath.EndsWith(';')) { "$CurrentPath$BinPath" } else { "$CurrentPath;$BinPath" }
        [System.Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
        $env:Path = "$env:Path;$BinPath"
    }
    Complete-Step $step4

    Write-Host ("`n  " + $GREEN + "[DONE] Installation successful!" + $NC)
    Write-Host ""
    Write-Host "  🚀 Getting Started:" -ForegroundColor Cyan
    Write-Host "     cluaiz serve          # Start OpenAI-compatible API daemon (:8000)" -ForegroundColor Gray
    Write-Host "     cluaiz                # Launch Interactive Terminal Dashboard" -ForegroundColor Gray
    Write-Host "     cluaiz pull <model>   # Pull GGUF/ONNX model from Hugging Face" -ForegroundColor Gray
    Write-Host ""

} catch {
    Write-Fail ("Installation failed: " + $_.Exception.Message)
    Write-Host "`n  [Troubleshoot] Check your internet connection or build from source with 'cargo build --release'." -ForegroundColor Gray
}