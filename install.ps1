# Installs Cluaiz from its GitHub release:
#
#   irm https://raw.githubusercontent.com/cluaiz/cluaiz/main/install.ps1 | iex
#
# It picks the Windows core, adds the GPU pack for the GPU it finds (NVIDIA: CUDA, AMD / Intel:
# Vulkan), checks every file against the release's SHA256SUMS.txt and unpacks them into one folder.
# Nothing asks a question; the only other change is that folder on the user PATH.
#
# Environment (all optional):
#   CLUAIZ_VERSION      release to install, e.g. 0.1.0 (default: the newest release)
#   CLUAIZ_INSTALL_DIR  where the files go (default: ~\.cluaiz\bin)
#   CLUAIZ_GPU          auto | cuda | vulkan | cpu (default: auto)
#   CLUAIZ_CUDA         12 | 13 (default: 13 with NVIDIA driver 580+ on an RTX 20 series or newer, else 12)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$Repo = 'cluaiz/cluaiz'
$InstallDir = if ($env:CLUAIZ_INSTALL_DIR) { $env:CLUAIZ_INSTALL_DIR } else { Join-Path $HOME '.cluaiz\bin' }
$Gpu = if ($env:CLUAIZ_GPU) { $env:CLUAIZ_GPU } else { 'auto' }

$E = [char]27
$GRAY = "$E[90m"; $GREEN = "$E[32m"; $NC = "$E[0m"
function Write-Step ([string]$msg) { Write-Host ("  " + $GRAY + "* " + $msg + $NC) }
function Complete-Step ([string]$msg) { Write-Host ("  " + $GREEN + "[DONE] " + $NC + $msg) }

# --- Logo ---
$C1 = [char]0x2591; $C2 = [char]0x2580; $C3 = [char]0x2584; $C4 = [char]0x2588
Write-Host ""
Write-Host "  $C1$C2$C3$C1$C1$C1$C1$C1$C1$C1$C1$C4$C2$C2$C1$C4$C1$C1$C1$C4$C1$C4$C1$C4$C2$C4$C1$C2$C4$C2$C1$C2$C2$C4" -ForegroundColor Cyan
Write-Host "  $C1$C1$C3$C2$C1$C1$C1$C1$C1$C1$C1$C4$C1$C1$C1$C4$C1$C1$C1$C4$C1$C4$C1$C4$C2$C4$C1$C1$C4$C1$C1$C3$C2$C1" -ForegroundColor Cyan
Write-Host "  $C1$C2$C1$C1$C1$C2$C2$C2$C1$C1$C1$C2$C2$C2$C1$C2$C2$C2$C1$C2$C2$C2$C1$C2$C1$C2$C1$C2$C2$C2$C1$C2$C2$C2" -ForegroundColor Cyan
Write-Host ""
Write-Host "  >_ Installing cluaiz..." -ForegroundColor Gray
Write-Host ""

try {
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw "Windows on $env:PROCESSOR_ARCHITECTURE is not supported yet (x64 only)" }
    $Platform = 'win-x64'

    # --- GPU ---
    if ($Gpu -eq 'auto') {
        $Gpu = 'cpu'
        $names = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
        if ($names -match 'NVIDIA') { $Gpu = 'cuda' }
        # Discrete AMD / Intel Arc; an integrated GPU is often slower than the CPU (CLUAIZ_GPU=vulkan to try it)
        elseif ($names -match 'AMD|Radeon|Intel\(R\) Arc') { $Gpu = 'vulkan' }
    }

    # --- Release ---
    Write-Step 'Finding the release'
    if ($env:CLUAIZ_VERSION) {
        $Tag = 'v' + $env:CLUAIZ_VERSION.TrimStart('v')
    } else {
        # The newest release, pre-releases included
        $Tag = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases?per_page=1")[0].tag_name
        if (-not $Tag) { throw "No release found on github.com/$Repo" }
    }
    $Version = $Tag.TrimStart('v')
    $Base = "https://github.com/$Repo/releases/download/$Tag"
    Complete-Step "Release $Tag ($Platform, GPU: $Gpu)"

    $Tmp = Join-Path ([IO.Path]::GetTempPath()) ("cluaiz-install-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Force $Tmp | Out-Null
    Invoke-WebRequest "$Base/SHA256SUMS.txt" -OutFile "$Tmp\SHA256SUMS.txt"
    $Sums = @{}
    foreach ($line in Get-Content "$Tmp\SHA256SUMS.txt") {
        $parts = $line -split '\s+\*?', 2
        if ($parts.Count -eq 2) { $Sums[$parts[1].Trim()] = $parts[0].ToLower() }
    }

    $Core = "cluaiz-$Version-$Platform.zip"
    $Files = @($Core)
    switch ($Gpu) {
        'cuda'   {
            $Cuda = $env:CLUAIZ_CUDA
            if (-not $Cuda) {
                # CUDA 13 needs driver 580+ and compute capability 7.5+ (Turing); older drivers and GPUs use CUDA 12
                $Cuda = '12'
                try {
                    $driver = [int]((& nvidia-smi --query-gpu=driver_version --format=csv,noheader | Select-Object -First 1).Split('.')[0])
                    $cap = (& nvidia-smi --query-gpu=compute_cap --format=csv,noheader | ForEach-Object { [int]($_.Trim().Replace('.', '')) } | Measure-Object -Minimum).Minimum
                    if ($driver -ge 580 -and $cap -ge 75) { $Cuda = '13' }
                } catch { }
            }
            Write-Host "  $GRAY- NVIDIA: CUDA $Cuda drivers$NC"
            $Files += "cluaiz-$Version-$Platform-llama-cuda$Cuda.zip", "cluaiz-$Version-$Platform-cudart$Cuda.zip", "cluaiz-$Version-$Platform-onnx-cuda$Cuda.zip"
        }
        'vulkan' { $Files += "cluaiz-$Version-$Platform-llama-vulkan.zip" }
    }

    # --- Download, check, unpack ---
    $Got = @()
    foreach ($file in $Files) {
        if (-not $Sums.ContainsKey($file)) {
            if ($file -eq $Core) { throw "Release $Tag has no build for $Platform" }
            Write-Host "  $GRAY- $file is not in this release; models run on the CPU instead$NC"
            continue
        }
        Write-Step "Downloading $file"
        Invoke-WebRequest "$Base/$file" -OutFile "$Tmp\$file"
        $actual = (Get-FileHash "$Tmp\$file" -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $Sums[$file]) { throw "$file does not match SHA256SUMS.txt (expected $($Sums[$file]), got $actual)" }
        Complete-Step "$file (checksum OK)"
        $Got += $file
    }

    # A running server keeps its files open
    if (Get-Process cluaiz -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir\*" }) {
        throw "Cluaiz is running from $InstallDir; stop it (or close the app) and run this again"
    }

    Write-Step "Installing into $InstallDir"
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    foreach ($file in $Got) { Expand-Archive "$Tmp\$file" -DestinationPath $InstallDir -Force }
    Complete-Step "Installed into $InstallDir"

    # --- Microsoft Visual C++ runtime (the engine is built with MSVC) ---
    $sys = [Environment]::SystemDirectory
    if (-not ((Test-Path "$sys\vcruntime140_1.dll") -and (Test-Path "$sys\msvcp140.dll") -and (Test-Path "$sys\vcomp140.dll"))) {
        Write-Step 'Installing the Microsoft Visual C++ runtime (Windows may ask for permission)'
        Invoke-WebRequest 'https://aka.ms/vs/17/release/vc_redist.x64.exe' -OutFile "$Tmp\vc_redist.x64.exe"
        Start-Process "$Tmp\vc_redist.x64.exe" -ArgumentList '/install', '/quiet', '/norestart' -Verb RunAs -Wait
        Complete-Step 'Microsoft Visual C++ runtime'
    }

    # --- PATH ---
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($UserPath -split ';') -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('Path', ($UserPath.TrimEnd(';') + ';' + $InstallDir), 'User')
        Write-Host "  ${GRAY}Open a new terminal to use 'cluaiz'$NC"
    }
    $env:Path = "$InstallDir;$env:Path"

    & (Join-Path $InstallDir 'cluaiz.exe') --version
    Write-Host ("`n  " + $GREEN + "[DONE] Cluaiz is installed." + $NC)
    Write-Host "  ${GRAY}Next: cluaiz pull <model>   then   cluaiz run <model>$NC`n"
}
catch {
    Write-Host ("`n  [ERROR] Install failed: " + $_.Exception.Message) -ForegroundColor Red
}
finally {
    if ($Tmp -and (Test-Path $Tmp)) { Remove-Item -Recurse -Force $Tmp }
}
