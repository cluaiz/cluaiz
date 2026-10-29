# Installs Cluaiz from its GitHub release:
#
#   powershell -c "irm https://cluaiz.com/install.ps1 | iex"
#
# It picks the Windows core, adds the GPU pack for the GPU it finds (NVIDIA: CUDA, AMD / Intel:
# Vulkan), checks every file against the release's SHA256SUMS.txt and unpacks them into one folder.
# Nothing asks a question; the only other change is that folder on the user PATH.
#
# Environment (all optional):
#   CLUAIZ_VERSION      release to install, e.g. 0.1.0 (default: the newest release)
#   CLUAIZ_INSTALL_DIR  where the files go (default: ~\.cluaiz\bin)
#   CLUAIZ_GPU          auto | cuda | vulkan | cpu (default: auto)
#   CLUAIZ_CUDA         12 | 13 (default: the newest pack the NVIDIA driver and GPU can run, see below)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$Repo = 'cluaiz/cluaiz'
$InstallDir = if ($env:CLUAIZ_INSTALL_DIR) { $env:CLUAIZ_INSTALL_DIR } else { Join-Path $HOME '.cluaiz\bin' }
$Gpu = if ($env:CLUAIZ_GPU) { $env:CLUAIZ_GPU } else { 'auto' }

$E = [char]27
$CYAN = "$E[36m"; $GRAY = "$E[90m"; $GREEN = "$E[32m"; $NC = "$E[0m"
function Write-Step ([string]$msg) { Write-Host ("  " + $GRAY + "* " + $msg + $NC) }
function Complete-Step ([string]$msg) { Write-Host ("  " + $GREEN + "[DONE] " + $NC + $msg) }

# --- Logo (Variant 10 Gothic / Tiny auto-fit) ---
$TermWidth = 80
try {
    if ($Host.UI.RawUI.WindowSize.Width) { $TermWidth = $Host.UI.RawUI.WindowSize.Width }
} catch { }

Write-Host ""
if ($TermWidth -ge 60) {
    Write-Host "     ▄████▄   ██▓     █    ██  ▄▄▄       ██▓▒███████▒" -ForegroundColor Cyan
    Write-Host "    ▒██▀ ▀█  ▓██▒     ██  ▓██▒▒████▄    ▓██▒▒ ▒ ▒ ▄▀░" -ForegroundColor Cyan
    Write-Host "    ▒▓█    ▄ ▒██░    ▓██  ▒██░▒██  ▀█▄  ▒██▒░ ▒ ▄▀▒░ " -ForegroundColor Cyan
    Write-Host "    ▒▓▓▄ ▄██▒▒██░    ▓▓█  ░██░░██▄▄▄▄██ ░██░  ▄▀▒   ░" -ForegroundColor Cyan
    Write-Host "    ▒ ▓███▀ ░░██████▒▒▒█████▓  ▓█   ▓██▒░██░▒███████▒" -ForegroundColor Cyan
    Write-Host "    ░ ░▒ ▒  ░░ ▒░▓  ░░▒▓▒ ▒ ▒  ▒▒   ▓▒█░░▓  ░▒▒ ▓░▒░▒" -ForegroundColor Cyan
    Write-Host "      ░  ▒   ░ ░ ▒  ░░░▒░ ░ ░   ▒   ▒▒ ░ ▒ ░░░▒ ▒ ░ ▒" -ForegroundColor Cyan
    Write-Host "    ░          ░ ░    ░░░ ░ ░   ░   ▒    ▒ ░░ ░ ░ ░ ░" -ForegroundColor Cyan
    Write-Host "    ░ ░          ░  ░   ░           ░  ░ ░    ░ ░    " -ForegroundColor Cyan
    Write-Host "    ░                                       ░        " -ForegroundColor Cyan
} else {
    Write-Host "    [ C L U A I Z ]" -ForegroundColor Cyan
}
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
                # A pack runs on a GPU it has ready-made code for, given a driver of the same CUDA major version.
                # For other GPUs (e.g. Turing, RTX 20) the driver compiles the pack's code on first use, and only
                # a driver at least as new as the pack's toolkit can: nvidia-smi prints the newest CUDA version
                # the driver supports. Toolkits and ready-made GPUs of cluaize-llama-driver.yml (windows,
                # llama.cpp's default architectures): CUDA 13.4 for 8.6 8.9 12.0 12.1, needs 7.5+; CUDA 12.4 for 8.6 8.9
                $drv = [version]'0.0'; $caps = @()
                try {
                    if ((& nvidia-smi | Out-String) -match 'CUDA (UMD )?Version:\s*(\d+\.\d+)') { $drv = [version]$Matches[2] }
                    $caps = @(& nvidia-smi --query-gpu=compute_cap --format=csv,noheader | ForEach-Object { [int]($_.Trim().Replace('.', '')) })
                } catch { }
                # True when every GPU can run the pack
                function Test-Pack ([int]$major, [version]$toolkit, [int]$lowest, [int[]]$ready) {
                    if ($drv.Major -lt $major -or $caps.Count -eq 0) { return $false }
                    foreach ($c in $caps) {
                        if ($c -lt $lowest) { return $false }
                        if ($drv -lt $toolkit -and $ready -notcontains $c) { return $false }
                    }
                    return $true
                }
                if (Test-Pack 13 '13.4' 75 @(86, 89, 120, 121)) { $Cuda = '13' }
                elseif (Test-Pack 12 '12.4' 50 @(86, 89)) { $Cuda = '12' }
                else {
                    Write-Host "  $GRAY- The NVIDIA driver (CUDA $drv) is too old for the CUDA packs; update it for CUDA. Using Vulkan meanwhile$NC"
                    $Gpu = 'vulkan'
                }
            }
            if ($Gpu -eq 'cuda') {
                Write-Host "  $GRAY- NVIDIA: CUDA $Cuda drivers$NC"
                $Files += "cluaiz-$Version-$Platform-llama-cuda$Cuda.zip", "cluaiz-$Version-$Platform-cudart$Cuda.zip", "cluaiz-$Version-$Platform-onnx-cuda$Cuda.zip"
            }
        }
    }
    if ($Gpu -eq 'vulkan') { $Files += "cluaiz-$Version-$Platform-llama-vulkan.zip" }

    # --- Download, check, unpack ---
    function Download-FileWithProgress ([string]$url, [string]$outPath, [string]$fileName) {
        Add-Type -AssemblyName System.Net.Http -ErrorAction SilentlyContinue
        Write-Step "Downloading $fileName"
        $client = [System.Net.Http.HttpClient]::new()
        try {
            $resp = $client.GetAsync($url, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
            $null = $resp.EnsureSuccessStatusCode()
            $total = $resp.Content.Headers.ContentLength
            $stream = $resp.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
            $fs = [System.IO.FileStream]::new($outPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
            try {
                $buf = [byte[]]::new(65536)
                $read = 0
                $lastTime = [DateTime]::MinValue
                while (($cnt = $stream.Read($buf, 0, $buf.Length)) -gt 0) {
                    $fs.Write($buf, 0, $cnt)
                    $read += $cnt
                    $now = [DateTime]::UtcNow
                    if (($now - $lastTime).TotalMilliseconds -ge 150) {
                        $lastTime = $now
                        if ($total -and $total -gt 0) {
                            $pct = [int](($read / $total) * 100)
                            $mbRead = [math]::Round($read / 1MB, 1)
                            $mbTot = [math]::Round($total / 1MB, 1)
                            Write-Host ("`r  " + $GRAY + "* Downloading $fileName " + $CYAN + "[$pct%]" + $GRAY + " ($mbRead MB / $mbTot MB)" + $NC) -NoNewline
                        } else {
                            $mbRead = [math]::Round($read / 1MB, 1)
                            Write-Host ("`r  " + $GRAY + "* Downloading $fileName " + $GRAY + "($mbRead MB)" + $NC) -NoNewline
                        }
                    }
                }
                Write-Host ""
            } finally {
                $fs.Dispose()
                $stream.Dispose()
            }
        } catch {
            Invoke-WebRequest $url -OutFile $outPath
        } finally {
            $client.Dispose()
        }
    }

    $Got = @()
    foreach ($file in $Files) {
        if (-not $Sums.ContainsKey($file)) {
            if ($file -eq $Core) { throw "Release $Tag has no build for $Platform" }
            Write-Host "  $GRAY- $file is not in this release; models run on the CPU instead$NC"
            continue
        }
        Download-FileWithProgress "$Base/$file" "$Tmp\$file" "$file"
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
