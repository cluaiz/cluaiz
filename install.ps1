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
$ProgressPreference = 'Continue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$Repo = 'cluaiz/cluaiz'
$InstallDir = if ($env:CLUAIZ_INSTALL_DIR) { $env:CLUAIZ_INSTALL_DIR } else { Join-Path $HOME '.cluaiz\bin' }
$Gpu = if ($env:CLUAIZ_GPU) { $env:CLUAIZ_GPU } else { 'auto' }

$E = [char]27
$CYAN = "$E[36m"; $GRAY = "$E[90m"; $GREEN = "$E[32m"; $NC = "$E[0m"
$script:InstallStep = 0
$script:InteractiveProgress = $false
try { $script:InteractiveProgress = $Host.UI.SupportsVirtualTerminal -and -not [Console]::IsOutputRedirected } catch { }
function Write-Step ([string]$msg) {
    $script:InstallStep++
    Write-Host ("`n  " + $CYAN + "[$script:InstallStep/7] " + $NC + $msg)
}
function Complete-Step ([string]$msg) { Write-Host ("  " + $GREEN + "[DONE] " + $NC + $msg) }

function Format-Bytes ([double]$bytes) {
    $units = @('B', 'KiB', 'MiB', 'GiB', 'TiB'); $unit = 0
    while ($bytes -ge 1024 -and $unit -lt $units.Count - 1) { $bytes /= 1024; $unit++ }
    return ('{0:0.0} {1}' -f $bytes, $units[$unit])
}

function Get-DownloadView ([double]$received, [double]$total, [double]$elapsed, [bool]$complete, [int]$tick) {
    # Same ticks as cmd/src/ui/spinner.rs; the CLI does not exist yet during bootstrap.
    $frames = @('⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏')
    $frame = if ($complete) { ' ' } else { $frames[$tick % $frames.Count] }
    $rate = if ($elapsed -gt 0) { $received / $elapsed } else { 0 }
    if ($complete -and $total -le 0) { $total = $received }
    if ($total -gt 0) {
        $percent = [Math]::Min(100, [Math]::Floor(100 * $received / $total))
        $filled = [int][Math]::Floor($percent / 5)
        $bar = ('#' * $filled) + ('-' * (20 - $filled))
        $left = [Math]::Max(0, $total - $received)
        $eta = if ($rate -gt 0) { '{0:0}s' -f [Math]::Ceiling($left / $rate) } else { '--' }
        $first = '  {0} [{1}] {2,3}% | {3} / {4}' -f $frame, $bar, $percent, (Format-Bytes $received), (Format-Bytes $total)
        $second = '  Left {0} | {1}/s | File ETA ~{2}' -f (Format-Bytes $left), (Format-Bytes $rate), $eta
    } else {
        $first = '  {0} [{1}] --% | {2} / unknown' -f $frames[$tick % $frames.Count], ('-' * 20), (Format-Bytes $received)
        $second = '  Left unknown | {0}/s | File ETA --' -f (Format-Bytes $rate)
    }
    return @($first, $second)
}

function Download-File ([string]$url, [string]$outPath, [string]$fileName) {
    $curl = Get-Command curl.exe -CommandType Application -ErrorAction Stop
    $headers = "$outPath.headers"; $errors = "$outPath.stderr"
    # These are direct executable arguments, never a cmd/PowerShell command string.
    foreach ($arg in @($url, $outPath, $headers)) {
        if ($arg.Contains('"')) { throw 'Download URL/path contains an unsupported quote' }
    }
    $arguments = '--fail --location --silent --show-error --connect-timeout 30 --speed-limit 1 --speed-time 120 --dump-header "{0}" --output "{1}" --url "{2}"' -f $headers, $outPath, $url
    Write-Host "  $fileName"
    $process = $null; $drawn = $false; $tick = 0; $nextLog = 0
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        $process = Start-Process -FilePath $curl.Source -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardError $errors
        $null = $process.Handle # PS5 must retain the handle to expose ExitCode after async exit.
        while ($true) {
            $process.Refresh()
            $finished = $process.HasExited
            if ($finished) {
                $process.WaitForExit()
                if ($process.ExitCode -ne 0) {
                    $detail = if (Test-Path -LiteralPath $errors) { Get-Content -LiteralPath $errors -Raw } else { '' }
                    throw "Download of $fileName failed (curl $($process.ExitCode)): $detail"
                }
            }
            $received = if (Test-Path -LiteralPath $outPath) { (Get-Item -LiteralPath $outPath).Length } else { 0 }
            $total = 0
            if (Test-Path -LiteralPath $headers) {
                try {
                    $lastResponse = ((Get-Content -LiteralPath $headers -Raw -ErrorAction Stop) -split '(?m)^HTTP/[^\r\n]+')[-1]
                    if ($lastResponse -match '(?im)^Content-Length:\s*(\d+)\s*$') { $total = [double]$Matches[1] }
                } catch { } # curl can be updating the headers; retry on the next tick.
            }
            if ($script:InteractiveProgress -or $clock.Elapsed.TotalSeconds -ge $nextLog -or $finished) {
                $view = Get-DownloadView $received $total $clock.Elapsed.TotalSeconds $finished $tick
                if ($script:InteractiveProgress -and $drawn) { Write-Host "$E[2A" -NoNewline }
                foreach ($line in $view) {
                    if ($script:InteractiveProgress) { Write-Host ("`r$E[2K" + $line) } else { Write-Host $line }
                }
                $drawn = $true; $nextLog = $clock.Elapsed.TotalSeconds + 5
            }
            if ($finished) { break }
            $tick++; Start-Sleep -Milliseconds 250
        }
    } finally {
        if ($process) {
            if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
            $process.Dispose()
        }
        Remove-Item -LiteralPath $headers, $errors -Force -ErrorAction SilentlyContinue
    }
}

# --- Mascot (High-Detail Braille) ---
# Keep artwork gaps as U+2800 (not ASCII spaces), in sync with install.sh.
Write-Host ""
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣠⣶⣾⣾⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣦⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣺⣿⣿⣿⡿⠛⠋⠈⠀⠀⠀⠀⠈⠉⠋⠀⠀⠀⠀⠀⠈⠋⠻⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⡟⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⣿⡏⠀⠀⠀⠀⠀⣠⣠⡀⠀⠀⠀⠀⠀⠀⠀⣀⣠⣀⠀⠀⠀⠀⠀⠸⣿⣿⡆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠠⣾⣿⣿⡂⠀⠀⠀⠀⣺⣿⣿⣷⡂⠀⠀⠀⠀⠀⠀⣾⣿⣿⡆⠀⠀⠀⠀⠀⣻⣿⣿⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣸⣿⣿⡏⠀⠀⠀⠀⠀⢺⣿⣿⣿⡂⠀⠀⠀⠀⠀⠈⣿⣿⣿⡏⠀⠀⠀⠀⠀⣺⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⣿⡯⠀⠀⠀⠀⠀⠈⠻⠻⠋⢠⣶⣶⣦⣶⣶⡆⠘⠻⠛⠂⠀⠀⠀⠀⠀⣺⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣺⣿⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⡿⣿⡿⠛⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣿⣿⣿⣿⣧⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣦⣢⣠⣀⣀⡀⡀⡀⡀⡀⣀⣀⣀⣠⣠⣦⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠸⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡂⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⡿⠿⡿⠛⠋⢻⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠛⠻⡿⠿⡿⠿⠋⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠋⠀⠀⠈⠙⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢿⣿⣿⣿⣿⣿⣿⡿⠂⠀⠀⠀⠀⠀⠈⠻⣿⣿⣿⣿⣿⣿⣿⡋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host "  ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⠻⠛⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠻⠻⠛⠋⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀" -ForegroundColor White
Write-Host ""
Write-Host "           ██████╗██╗     ██╗   ██╗ █████╗ ██╗███████╗" -ForegroundColor White
Write-Host "          ██╔════╝██║     ██║   ██║██╔══██╗██║╚══███╔╝" -ForegroundColor White
Write-Host "          ██║     ██║     ██║   ██║███████║██║  ███╔╝ " -ForegroundColor White
Write-Host "          ██║     ██║     ██║   ██║██╔══██║██║ ███╔╝  " -ForegroundColor White
Write-Host "          ╚██████╗███████╗╚██████╔╝██║  ██║██║███████╗" -ForegroundColor White
Write-Host "           ╚═════╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝" -ForegroundColor White
Write-Host ""
Write-Host "  Installing cluaiz..." -ForegroundColor Gray
Write-Host ""

$Tmp = $null
try {
    Write-Step 'Checking this system'
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw "Windows on $env:PROCESSOR_ARCHITECTURE is not supported yet (x64 only)" }
    $Platform = 'win-x64'
    if (-not (Get-Command curl.exe -CommandType Application -ErrorAction SilentlyContinue)) { throw 'curl.exe is required (included with current Windows versions)' }

    # --- GPU ---
    if ($Gpu -eq 'auto') {
        $Gpu = 'cpu'
        $names = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
        if ($names -match 'NVIDIA') { $Gpu = 'cuda' }
        # Discrete AMD / Intel Arc; an integrated GPU is often slower than the CPU (CLUAIZ_GPU=vulkan to try it)
        elseif ($names -match 'AMD|Radeon|Intel\(R\) Arc') { $Gpu = 'vulkan' }
    }

    Complete-Step "Platform $Platform, requested GPU: $Gpu"
    $Tmp = Join-Path ([IO.Path]::GetTempPath()) ("cluaiz-install-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Force $Tmp | Out-Null

    # --- Release ---
    Write-Step 'Finding the release'
    if ($env:CLUAIZ_VERSION) {
        $Tag = 'v' + $env:CLUAIZ_VERSION.TrimStart('v')
    } else {
        # The newest release, pre-releases included
        Download-File "https://api.github.com/repos/$Repo/releases?per_page=1" "$Tmp\release.json" 'Release metadata'
        $Tag = @(Get-Content -LiteralPath "$Tmp\release.json" -Raw | ConvertFrom-Json)[0].tag_name
        if (-not $Tag) { throw "No release found on github.com/$Repo" }
    }
    $Version = $Tag.TrimStart('v')
    $Base = "https://github.com/$Repo/releases/download/$Tag"
    Complete-Step "Release $Tag ($Platform, GPU: $Gpu)"

    Write-Step 'Reading the package manifest'
    Download-File "$Base/SHA256SUMS.txt" "$Tmp\SHA256SUMS.txt" 'SHA256SUMS.txt'
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
    $Available = @()
    foreach ($file in $Files) {
        if (-not $Sums.ContainsKey($file)) {
            if ($file -eq $Core) { throw "Release $Tag has no build for $Platform" }
            Write-Host "  $GRAY- $file is not in this release; models run on the CPU instead$NC"
            continue
        }
        $Available += $file
    }
    Write-Step "Downloading packages (0/$($Available.Count) verified)"
    $Got = @()
    foreach ($file in $Available) {
        Write-Host "  Package $($Got.Count + 1)/$($Available.Count): $file"
        Download-File "$Base/$file" "$Tmp\$file" "$file"
        Write-Host "  Checking SHA-256: $file"
        $actual = (Get-FileHash "$Tmp\$file" -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $Sums[$file]) { throw "$file does not match SHA256SUMS.txt (expected $($Sums[$file]), got $actual)" }
        $Got += $file
        Complete-Step "$file (checksum OK; $($Got.Count)/$($Available.Count) verified)"
    }

    # A running server keeps its files open
    if (Get-Process cluaiz -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$InstallDir\*" }) {
        throw "Cluaiz is running from $InstallDir; stop it (or close the app) and run this again"
    }

    Write-Step "Installing into $InstallDir"
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    $unpacked = 0
    foreach ($file in $Got) {
        Write-Host "  Extracting $($unpacked + 1)/$($Got.Count): $file"
        Expand-Archive "$Tmp\$file" -DestinationPath $InstallDir -Force
        $unpacked++
    }
    Complete-Step "Installed into $InstallDir"

    # --- Microsoft Visual C++ runtime (the engine is built with MSVC) ---
    Write-Step 'Preparing runtime and PATH'
    $sys = [Environment]::SystemDirectory
    if (-not ((Test-Path "$sys\vcruntime140_1.dll") -and (Test-Path "$sys\msvcp140.dll") -and (Test-Path "$sys\vcomp140.dll"))) {
        Download-File 'https://aka.ms/vs/17/release/vc_redist.x64.exe' "$Tmp\vc_redist.x64.exe" 'Microsoft Visual C++ runtime'
        Write-Host '  Installing Microsoft runtime (Windows may ask for permission)...'
        $runtime = Start-Process "$Tmp\vc_redist.x64.exe" -ArgumentList '/install', '/quiet', '/norestart' -Verb RunAs -Wait -PassThru
        if ($runtime.ExitCode -notin @(0, 1638, 3010)) { throw "Microsoft runtime installer failed (exit $($runtime.ExitCode))" }
        Complete-Step 'Microsoft Visual C++ runtime'
    }

    # --- PATH ---
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($UserPath -split ';') -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('Path', (([string]$UserPath).TrimEnd(';') + ';' + $InstallDir), 'User')
        Write-Host "  ${GRAY}Open a new terminal to use 'cluaiz'$NC"
    }
    $env:Path = "$InstallDir;$env:Path"
    Complete-Step 'Runtime and PATH ready'

    Write-Step 'Verifying the installed Engine'
    & (Join-Path $InstallDir 'cluaiz.exe') --version
    if ($LASTEXITCODE -ne 0) { throw "Installed Engine failed its version check (exit $LASTEXITCODE)" }
    Write-Host ("`n  " + $GREEN + "[DONE] Cluaiz is installed (7/7 stages complete)." + $NC)
    Write-Host "  ${GRAY}Next: cluaiz pull <model>   then   cluaiz run <model>$NC`n"
}
catch {
    Write-Host ("`n  [ERROR] Install failed: " + $_.Exception.Message) -ForegroundColor Red
    throw
}
finally {
    if ($Tmp -and (Test-Path -LiteralPath $Tmp)) {
        $resolved = [IO.Path]::GetFullPath($Tmp)
        if ([IO.Path]::GetDirectoryName($resolved) -eq [IO.Path]::GetTempPath().TrimEnd('\') -and [IO.Path]::GetFileName($resolved) -like 'cluaiz-install-*') {
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    }
}
