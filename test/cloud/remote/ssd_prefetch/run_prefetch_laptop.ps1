# SSD expert prefetch A/B on the Windows laptop of run 20 (test/cloud/runs/2026-11-16-run20-laptop-moe-disk):
# Gemma 4 26B A4B IQ4_XS, official llama.cpp b12428 against Cluaiz, rotated order, 3 rounds.
#
# Configurations (one run each per round; the order rotates every round, because Windows cannot drop
# its file cache without an extra tool):
#   L        official llama-bench, run 20's flags: -ngl 99 -ncmoe 30 -fa 1 -t 8 -p 512 -n 128 -r 3
#   C        cluaiz bench, run 20's flags (--n-gpu-layers all --n-cpu-moe 30, -c 1024, flash attention on,
#            KV f16), CLUAIZ_MOE_CACHE not set, CLUAIZ_EXPERT_PREFETCH=off. Same command as run 20.
#   C-plan   cluaiz bench, the planner's own placement (no --n-gpu-layers / --n-cpu-moe), env not set
#   C-cache  as C-plan with CLUAIZ_MOE_CACHE=on, CLUAIZ_EXPERT_PREFETCH=off
#   C-pf     as C-plan with CLUAIZ_MOE_CACHE=on, CLUAIZ_EXPERT_PREFETCH=on
# Why C-cache and C-pf do not use --n-cpu-moe 30: a manual placement is the planner's "User setting"
# candidate, which never gets a MoE cache (engine/plan/src/planner.rs), and the prefetch runs only in
# the MoE cache upload path (engine/plan/src/expert_prefetch.rs). C-plan is their same-placement baseline.
# C-plan, C-cache and C-pf run with -vv: llama.cpp's info lines (MoE cache size, prefetch counters) reach
# the log only at debug level. C runs without -v, as in run 20.
#
# Before round 1 a warm-up load with run 20's placement (-p 0 -n 8 -r 1) lets the isolated home learn the
# VRAM this model takes, as the first forced load did in run 20; without it the planner keeps its fixed
# 1 GB VRAM gap and may choose CPU only.
#
# Per run: stdout (the tables) and stderr saved, the part of cluaiz_Core.log the run wrote, typeperf every
# second (free RAM, disk read bytes/s, disk read queue, GPU dedicated memory), free RAM before.
# Then results.md: one row per run, the median per configuration, and run 20's numbers beside them.
#
# Usage (PowerShell, Engine root):
#   powershell -ExecutionPolicy Bypass -File test\cloud\remote\ssd_prefetch\run_prefetch_laptop.ps1
# Small format check with another model:
#   ... -Model <file.gguf> -ExpectedSize <its size in bytes> -Rounds 1 -Reps 1 -Prompt 64 -Gen 16
param(
    [string]$Model = "$env:LOCALAPPDATA\cluaiz-test\models\gemma4-26b-a4b-iq4xs\google_gemma-4-26B-A4B-it-IQ4_XS.gguf",
    [long]$ExpectedSize = 14205004928,
    [int]$Rounds = 3,
    [int]$Reps = 3,
    [int]$Prompt = 512,
    [int]$Gen = 128,
    [int]$NCpuMoe = 30,
    [int]$Threads = 8,
    [string]$Llama = "$env:LOCALAPPDATA\cluaiz-test\llama-official-b12428\llama-bench.exe",
    [string]$Cluaiz = "$PSScriptRoot\..\..\..\..\target\debug\cluaiz.exe",
    [string]$Out = "$env:LOCALAPPDATA\cluaiz-test\prefetch-laptop-$(Get-Date -Format 'yyyyMMdd-HHmmss')",
    [string]$CluaizHome = ''
)
$ErrorActionPreference = 'Stop'

# The model must be complete: another process may still be downloading it
if (-not (Test-Path -LiteralPath $Model -PathType Leaf)) { Write-Error "model file not found: $Model"; exit 1 }
$size = (Get-Item -LiteralPath $Model).Length
if ($size -ne $ExpectedSize) {
    Write-Error "model size is $size bytes, expected $ExpectedSize (download not finished?). Nothing was run."
    exit 1
}
foreach ($exe in @($Llama, $Cluaiz)) {
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { Write-Error "not found: $exe"; exit 1 }
}
$Cluaiz = (Resolve-Path -LiteralPath $Cluaiz).Path

New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path
# Isolated home: the owner's ~/.cluaiz settings and learned margins stay untouched. A new one per
# invocation, so a VRAM margin learned in an earlier invocation does not change this one's plans.
if (-not $CluaizHome) { $CluaizHome = "$env:LOCALAPPDATA\cluaiz-test\prefetch-home-$(Split-Path $Out -Leaf)" }
New-Item -ItemType Directory -Force $CluaizHome | Out-Null
$env:CLUAIZ_HOME = $CluaizHome
foreach ($k in @('CLUAIZ_MOE_CACHE', 'CLUAIZ_EXPERT_PREFETCH', 'GGML_CUDA_NO_PINNED')) {
    Remove-Item "env:$k" -ErrorAction SilentlyContinue
}
$EngineLog = Join-Path $CluaizHome 'cluaiz_Core.log'

function Step([string]$msg) {
    $line = "== {0:yyyy-MM-ddTHH:mm:ss} {1}" -f (Get-Date), $msg
    Write-Host $line
    Add-Content -LiteralPath "$Out\steps.log" -Value $line
}

function Free-RamMiB { [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1024) }

# Quote one argument for Start-Process (paths with spaces)
function Quote([string]$a) { if ($a -match '\s') { '"' + $a + '"' } else { $a } }

$counters = @(
    '\Memory\Available MBytes',
    '\PhysicalDisk(_Total)\Disk Read Bytes/sec',
    '\PhysicalDisk(_Total)\Avg. Disk Read Queue Length',
    '\GPU Adapter Memory(*)\Dedicated Usage'
)

# One run: typeperf around the command, raw stdout/stderr kept, the engine log part copied
function Run-One([string]$tag, [string]$exe, [string[]]$argv, [hashtable]$envs) {
    Step "$tag : $(Split-Path $exe -Leaf) $($argv -join ' ') $(($envs.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' ')"
    "free RAM MiB before: $(Free-RamMiB)" | Set-Content -LiteralPath "$Out\$tag.info.txt"
    $perf = "$Out\$tag.perf.csv"
    $tp = Start-Process -FilePath typeperf -ArgumentList (@($counters | ForEach-Object { '"' + $_ + '"' }) + @('-si', '1', '-f', 'CSV', '-o', "`"$perf`"", '-y')) -WindowStyle Hidden -PassThru
    Start-Sleep 2
    $logStart = 0L
    if (Test-Path -LiteralPath $EngineLog) { $logStart = (Get-Item -LiteralPath $EngineLog).Length }
    $saved = @{}
    foreach ($k in $envs.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); Set-Item "env:$k" $envs[$k] }
    $t0 = Get-Date
    $p = Start-Process -FilePath $exe -ArgumentList ($argv | ForEach-Object { Quote $_ }) -NoNewWindow -PassThru `
        -RedirectStandardOutput "$Out\$tag.md" -RedirectStandardError "$Out\$tag.log"
    $null = $p.Handle   # PowerShell 5.1 reports no ExitCode unless the handle is taken before the exit
    $p.WaitForExit()
    $sec = ((Get-Date) - $t0).TotalSeconds
    foreach ($k in $envs.Keys) {
        if ($null -eq $saved[$k]) { Remove-Item "env:$k" -ErrorAction SilentlyContinue } else { Set-Item "env:$k" $saved[$k] }
    }
    Start-Sleep 1
    Stop-Process -Id $tp.Id -ErrorAction SilentlyContinue
    # the part of the engine log this run wrote (the log starts over past 20 MiB)
    if (Test-Path -LiteralPath $EngineLog) {
        $len = (Get-Item -LiteralPath $EngineLog).Length
        if ($len -lt $logStart) { $logStart = 0 }
        $fs = [IO.File]::Open($EngineLog, 'Open', 'Read', 'ReadWrite')
        try {
            $fs.Seek($logStart, 'Begin') | Out-Null
            $to = [IO.File]::Create("$Out\$tag.engine.log")
            try { $fs.CopyTo($to) } finally { $to.Close() }
        } finally { $fs.Close() }
    }
    Add-Content -LiteralPath "$Out\$tag.info.txt" -Value ("exit code: {0}`nseconds: {1:n1}`nfree RAM MiB after: {2}" -f $p.ExitCode, $sec, (Free-RamMiB))
    Step ("{0} done in {1:n0}s, exit {2}" -f $tag, $sec, $p.ExitCode)
}

Step "start: model $(Split-Path $Model -Leaf) $size bytes, rounds $Rounds, reps $Reps, pp$Prompt tg$Gen, n_cpu_moe $NCpuMoe, threads $Threads"
Step "CLUAIZ_HOME $CluaizHome"
try { & $Cluaiz --version | Select-Object -First 1 | ForEach-Object { Step "cluaiz: $_" } } catch { Step "cluaiz --version failed: $_" }
Step "free RAM MiB: $(Free-RamMiB)"
try { nvidia-smi --query-gpu=name,memory.total,memory.used,driver_version --format=csv | Set-Content -LiteralPath "$Out\gpu.txt" } catch { Step "nvidia-smi failed: $_" }

$bench = @('-p', "$Prompt", '-n', "$Gen", '-r', "$Reps", '-t', "$Threads")
$load = @('-c', '1024', '--flash-attn', 'on', '--cache-type-k', 'f16', '--cache-type-v', 'f16')
$split = @('--n-gpu-layers', 'all', '--n-cpu-moe', "$NCpuMoe")
$configs = [ordered]@{
    'L'       = @{ exe = $Llama;  argv = @('-m', $Model) + $bench + @('-ngl', '99', '-ncmoe', "$NCpuMoe", '-fa', '1'); env = @{} }
    'C'       = @{ exe = $Cluaiz; argv = @('bench', $Model) + $bench + $load + $split; env = @{ CLUAIZ_EXPERT_PREFETCH = 'off' } }
    'C-plan'  = @{ exe = $Cluaiz; argv = @('bench', $Model) + $bench + $load + @('-vv'); env = @{ CLUAIZ_EXPERT_PREFETCH = 'off' } }
    'C-cache' = @{ exe = $Cluaiz; argv = @('bench', $Model) + $bench + $load + @('-vv'); env = @{ CLUAIZ_MOE_CACHE = 'on'; CLUAIZ_EXPERT_PREFETCH = 'off' } }
    'C-pf'    = @{ exe = $Cluaiz; argv = @('bench', $Model) + $bench + $load + @('-vv'); env = @{ CLUAIZ_MOE_CACHE = 'on'; CLUAIZ_EXPERT_PREFETCH = 'on' } }
}
$names = @($configs.Keys)

Run-One 'warmup' $Cluaiz (@('bench', $Model, '-p', '0', '-n', '8', '-r', '1', '-t', "$Threads") + $load + $split) @{ CLUAIZ_EXPERT_PREFETCH = 'off' }

for ($r = 1; $r -le $Rounds; $r++) {
    for ($i = 0; $i -lt $names.Count; $i++) {
        $name = $names[($i + $r - 1) % $names.Count]
        $c = $configs[$name]
        Run-One "r${r}_$name" $c.exe $c.argv $c.env
    }
}

# ---------- summary ----------
function Median([double[]]$v) {
    if ($v.Count -eq 0) { return $null }
    $s = $v | Sort-Object
    if ($s.Count % 2) { $s[[int][math]::Floor($s.Count / 2)] } else { ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2 }
}

function Read-Run([string]$tag) {
    $row = [ordered]@{ pp = $null; ppsd = $null; tg = $null; tgsd = $null; exit = ''; minfree = ''; mbps = ''; readgb = ''; queue = ''; vram = '';
                       placement = ''; cache = ''; prefetch = '' }
    if (Test-Path -LiteralPath "$Out\$tag.md") {
        foreach ($line in Get-Content -LiteralPath "$Out\$tag.md") {
            $cells = @($line.Trim().Trim('|').Split('|') | ForEach-Object { $_.Trim() })
            if ($cells.Count -ge 2 -and $cells[-2] -match '^(pp|tg)\d+$' -and $cells[-1] -match '^([\d.]+)\s+\S+\s+([\d.]+)$') {
                if ($cells[-2].StartsWith('pp')) { $row.pp = [double]$matches[1]; $row.ppsd = [double]$matches[2] }
                else { $row.tg = [double]$matches[1]; $row.tgsd = [double]$matches[2] }
            }
        }
    }
    if (Test-Path -LiteralPath "$Out\$tag.info.txt") {
        $m = Select-String -LiteralPath "$Out\$tag.info.txt" -Pattern '^exit code: (.*)$'
        if ($m) { $row.exit = $m.Matches[0].Groups[1].Value }
    }
    $perf = "$Out\$tag.perf.csv"
    if ((Test-Path -LiteralPath $perf) -and (Get-Item -LiteralPath $perf).Length -gt 0) {
        $rows = @(Import-Csv -LiteralPath $perf)
        if ($rows.Count -gt 0) {
            $cols = @($rows[0].PSObject.Properties.Name)
            $cMem = $cols | Where-Object { $_ -like '*Available MBytes' } | Select-Object -First 1
            $cRd = $cols | Where-Object { $_ -like '*Disk Read Bytes/sec' } | Select-Object -First 1
            $cQ = $cols | Where-Object { $_ -like '*Avg. Disk Read Queue Length' } | Select-Object -First 1
            $cG = @($cols | Where-Object { $_ -like '*Dedicated Usage' })
            $num = { param($x) $d = 0.0; if ([double]::TryParse($x, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$d)) { $d } else { $null } }
            $mem = @($rows | ForEach-Object { & $num $_.$cMem } | Where-Object { $null -ne $_ })
            $rd = @($rows | ForEach-Object { & $num $_.$cRd } | Where-Object { $null -ne $_ })
            $q = @($rows | ForEach-Object { & $num $_.$cQ } | Where-Object { $null -ne $_ })
            $g = @($rows | ForEach-Object { $r0 = $_; ($cG | ForEach-Object { & $num $r0.$_ } | Measure-Object -Maximum).Maximum })
            if ($mem.Count) { $row.minfree = [math]::Round(($mem | Measure-Object -Minimum).Minimum) }
            if ($rd.Count) {
                $row.mbps = [math]::Round(($rd | Measure-Object -Average).Average / 1e6, 1)
                # one sample per second: the sum of bytes/s is the bytes read
                $row.readgb = [math]::Round(($rd | Measure-Object -Sum).Sum / 1e9, 2)
            }
            if ($q.Count) { $row.queue = [math]::Round(($q | Measure-Object -Maximum).Maximum, 2) }
            if ($g.Count) { $row.vram = [math]::Round(($g | Measure-Object -Maximum).Maximum / 1MB) }
        }
    }
    $logs = @("$Out\$tag.log", "$Out\$tag.engine.log") | Where-Object { Test-Path -LiteralPath $_ }
    if ($logs) {
        $pl = Select-String -LiteralPath $logs -Pattern 'n_gpu_layers = (-?\d+) \| n_cpu_moe = (\d+) \| n_ctx = (\d+) \| KV (\w+)' | Select-Object -First 1
        if ($pl) { $g2 = $pl.Matches[0].Groups; $row.placement = "ngl $($g2[1].Value) ncmoe $($g2[2].Value) ctx $($g2[3].Value) $($g2[4].Value)" }
        $mc = Select-String -LiteralPath $logs -Pattern 'MoE cache size = +([\d.]+) MiB' | Select-Object -First 1
        if ($mc) { $row.cache = $mc.Matches[0].Groups[1].Value }
        $pf = Select-String -LiteralPath $logs -Pattern 'llama_cluaiz_moe_prefetch: off: (\d+) calls, (\d+) ranges, (\d+) MiB prefetched' | Select-Object -Last 1
        $pf1 = Select-String -LiteralPath $logs -Pattern 'llama_cluaiz_moe_prefetch: first prefetch' | Select-Object -First 1
        if ($pf) { $row.prefetch = "$($pf.Matches[0].Groups[1].Value) calls, $($pf.Matches[0].Groups[3].Value) MiB" }
        elseif ($pf1) { $row.prefetch = 'ran (no end counters)' }
    }
    $row
}

$all = @{}
$md = New-Object System.Collections.Generic.List[string]
$md.Add("# SSD expert prefetch A/B, laptop (run 20 repeated)")
$md.Add("")
$md.Add("Model $(Split-Path $Model -Leaf), $size bytes; $Rounds rounds, rotated order; pp$Prompt tg$Gen, -r $Reps, $Threads threads, n_cpu_moe $NCpuMoe for L and C.")
$md.Add("Speeds are each tool's own table (mean and sd over the repetitions). Disk and RAM come from typeperf, one sample per second over the whole process (load included).")
$md.Add("")
$md.Add("| Run | Config | exit | pp t/s | tg t/s | min free RAM MiB | disk read MB/s (mean) | disk read GB | max read queue | peak GPU dedicated MiB | placement (Cluaiz -vv) | MoE cache MiB | prefetch |")
$md.Add("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
foreach ($tag in @('warmup') + @(for ($r = 1; $r -le $Rounds; $r++) { for ($i = 0; $i -lt $names.Count; $i++) { "r${r}_$($names[($i + $r - 1) % $names.Count])" } })) {
    $x = Read-Run $tag
    $cfg = if ($tag -eq 'warmup') { 'warm-up' } else { $tag.Split('_', 2)[1] }
    if ($tag -ne 'warmup') { if (-not $all.ContainsKey($cfg)) { $all[$cfg] = @() }; $all[$cfg] += , $x }
    $pp = if ($null -ne $x.pp) { "{0:F2} +- {1:F2}" -f $x.pp, $x.ppsd } else { '' }
    $tg = if ($null -ne $x.tg) { "{0:F2} +- {1:F2}" -f $x.tg, $x.tgsd } else { '' }
    $md.Add("| $tag | $cfg | $($x.exit) | $pp | $tg | $($x.minfree) | $($x.mbps) | $($x.readgb) | $($x.queue) | $($x.vram) | $($x.placement) | $($x.cache) | $($x.prefetch) |")
}
$md.Add("")
$md.Add("Median of the rounds (range in brackets):")
$md.Add("")
$md.Add("| Config | pp t/s | tg t/s | run 20 tg t/s (rounds 2-3, release build of 16cd9a5a) |")
$md.Add("|---|---|---|---|")
$ref = @{ 'L' = '15.9 (llama-bench b12428)'; 'C' = '14.9 (cluaiz bench, same flags)' }
foreach ($cfg in $names) {
    $runs = @($all[$cfg])
    $cell = {
        param($vals)
        $v = @($vals | Where-Object { $null -ne $_ })
        if ($v.Count) { "{0:F2} ({1:F2}-{2:F2})" -f (Median $v), ($v | Measure-Object -Minimum).Minimum, ($v | Measure-Object -Maximum).Maximum } else { '' }
    }
    $md.Add("| $cfg | $(& $cell ($runs | ForEach-Object { $_.pp })) | $(& $cell ($runs | ForEach-Object { $_.tg })) | $($ref[$cfg]) |")
}
$md.Add("")
$noPf = @($all['C-pf'] | Where-Object { -not $_.prefetch }).Count
$noCache = @(@($all['C-cache']) + @($all['C-pf']) | Where-Object { -not $_.cache }).Count
if ($noCache) { $md.Add("WARNING: $noCache C-cache/C-pf runs show no 'MoE cache size' line: the planner gave no MoE cache, so those rows measure no cache and no prefetch.") }
if ($noPf) { $md.Add("WARNING: $noPf C-pf runs show no prefetch line: the prefetch did not run there.") }
$md.Add("Run 20 reference: test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md (round 1 there: 13.66 and 14.35 t/s).")
[IO.File]::WriteAllLines("$Out\results.md", $md)
Step "done: $Out\results.md"

# The raw files keep paths and counter names; the user and machine names are replaced so the folder
# can go into test/cloud/runs/ as it is (the engine log writes paths with doubled backslashes)
Get-ChildItem -LiteralPath $Out -File | ForEach-Object {
    $t = [IO.File]::ReadAllText($_.FullName)
    $n = $t.Replace("\Users\$env:USERNAME\", '\Users\<user>\').Replace("\\Users\\$env:USERNAME\\", '\\Users\\<user>\\')
    $n = $n.Replace("\\$env:COMPUTERNAME\", '\\<machine>\')
    if ($n -ne $t) { [IO.File]::WriteAllText($_.FullName, $n) }
}
