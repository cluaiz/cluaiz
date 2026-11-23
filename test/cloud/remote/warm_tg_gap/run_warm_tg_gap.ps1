# Warm generation gap on the Windows laptop of runs 20 and 42: official llama-bench against one or more
# `cluaiz bench` builds, same placement and flags, alternated order, several rounds.
#
# Every config runs pp512 + tg128 (-Prompt / -Gen take comma lists) with 8 threads, flash attention on, KV f16, all blocks on the GPU and the
# experts of 30 blocks in RAM (run 42's L and C rows). The order rotates every round; Windows keeps the model
# in its file cache between runs when it fits in RAM, so round 1 is the warm-up and rounds 2+ are warm.
#
# Usage (PowerShell, Engine root; `name|exe` per Cluaiz config, `name|flags` per extra llama-bench config):
#   powershell -ExecutionPolicy Bypass -File test\cloud\remote\warm_tg_gap\run_warm_tg_gap.ps1 `
#     -Cluaiz 'C-debug|target\debug\cluaiz.exe','C-release|target\release\cluaiz.exe'
# A config named L is always added (official llama-bench). -LlamaExtra adds llama-bench configs with
# extra flags, e.g. -LlamaExtra 'L-poll0|--poll 0'. A config name may end in `@K=V;K2=V2` to run it with
# those environment variables. An exe named llama-bench.exe in -Cluaiz runs with L's flags. E.g. -Cluaiz 'C-active@OMP_WAIT_POLICY=ACTIVE|target\debug\cluaiz.exe'.
param(
    [string[]]$Cluaiz = @('C|target\debug\cluaiz.exe'),
    [string[]]$LlamaExtra = @(),
    [string]$Model = "$env:LOCALAPPDATA\cluaiz-test\models\gemma4-26b-a4b-iq4xs\google_gemma-4-26B-A4B-it-IQ4_XS.gguf",
    [long]$ExpectedSize = 14205004928,
    [int]$Rounds = 4,
    [int]$Reps = 3,
    [string]$Prompt = '512',
    [string]$Gen = '128',
    [int]$Ctx = 1024,
    # placement flags of every llama-bench / cluaiz config (CPU only: '-ngl 0 -dev none' / '--n-gpu-layers 0')
    [string]$LlamaPlacement = '',
    [string]$CluaizPlacement = '',
    [int]$NCpuMoe = 30,
    [int]$Threads = 8,
    [string]$Llama = "$env:LOCALAPPDATA\cluaiz-test\llama-official-b12428\llama-bench.exe",
    [string]$Out = "$env:LOCALAPPDATA\cluaiz-test\warm-tg-gap-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
)
$ErrorActionPreference = 'Stop'
# powershell -File passes a list as one comma-joined string
$Cluaiz = @($Cluaiz | ForEach-Object { $_ -split ',' })
$LlamaExtra = @($LlamaExtra | ForEach-Object { $_ -split ',' })

if ($ExpectedSize -gt 0 -and (Get-Item -LiteralPath $Model).Length -ne $ExpectedSize) { Write-Error "model size is not $ExpectedSize bytes"; exit 1 }
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path -LiteralPath $Out).Path
# Isolated home: the owner's ~/.cluaiz settings are not read or written
$env:CLUAIZ_HOME = "$env:LOCALAPPDATA\cluaiz-test\warm-tg-gap-home"
New-Item -ItemType Directory -Force $env:CLUAIZ_HOME | Out-Null
foreach ($k in @('CLUAIZ_MOE_CACHE', 'GGML_CUDA_NO_PINNED')) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
$env:CLUAIZ_EXPERT_PREFETCH = 'off'

function Step([string]$msg) {
    $line = "== {0:yyyy-MM-ddTHH:mm:ss} {1}" -f (Get-Date), $msg
    Write-Host $line
    Add-Content -LiteralPath "$Out\steps.log" -Value $line
}
function Avail-MiB { [math]::Round((Get-Counter '\Memory\Available MBytes').CounterSamples[0].CookedValue) }
function Quote([string]$a) { if ($a -match '\s') { '"' + $a + '"' } else { $a } }

$bench = @('-p', "$Prompt", '-n', "$Gen", '-r', "$Reps", '-t', "$Threads")
if (-not $LlamaPlacement) { $LlamaPlacement = "-ngl 99 -ncmoe $NCpuMoe" }
if (-not $CluaizPlacement) { $CluaizPlacement = "--n-gpu-layers all --n-cpu-moe $NCpuMoe" }
$configs = [ordered]@{}
$configs['L'] = @{ exe = $Llama; argv = @('-m', $Model) + $bench + @($LlamaPlacement -split ' ' | Where-Object { $_ }) + @('-fa', '1') }
foreach ($spec in $LlamaExtra) {
    $name, $extra = $spec -split '\|', 2
    $configs[$name] = @{ exe = $Llama; argv = $configs['L'].argv + @($extra -split ' ') }
}
foreach ($spec in $Cluaiz) {
    # `name|exe` or `name|exe|load flags`: the flags replace the fixed placement (for example `-vv` for the
    # planner's own placement, or the placement plus `--batch-size 2048`)
    $name, $exe, $load = $spec -split '\|', 3
    $exe = (Resolve-Path -LiteralPath $exe).Path
    # another llama-bench build (for example official llama-bench beside other ggml DLLs) gets L's flags
    if ((Split-Path $exe -Leaf) -eq 'llama-bench.exe') { $configs[$name] = @{ exe = $exe; argv = $configs['L'].argv }; continue }
    $placement = if ($load) { @($load -split ' ' | Where-Object { $_ }) } else { @($CluaizPlacement -split ' ' | Where-Object { $_ }) }
    $configs[$name] = @{ exe = $exe; argv = @('bench', $Model) + $bench + @('-c', "$Ctx", '--flash-attn', 'on',
        '--cache-type-k', 'f16', '--cache-type-v', 'f16') + $placement }
}
# `name@K=V;K2=V2`: the environment of that config
foreach ($name in @($configs.Keys)) {
    $configs[$name].env = @{}
    if ($name -match '^[^@]+@(.+)$') {
        foreach ($kv in $matches[1] -split ';') { $k, $v = $kv -split '=', 2; $configs[$name].env[$k] = $v }
    }
}
$names = @($configs.Keys)
Step "configs: $(($names | ForEach-Object { "$_ = $($configs[$_].exe)" }) -join '; ')"
Step "available RAM MiB: $(Avail-MiB)"
try { nvidia-smi --query-gpu=name,memory.total,memory.used,utilization.gpu --format=csv | Set-Content -LiteralPath "$Out\gpu.txt" } catch {}

$rows = New-Object System.Collections.Generic.List[object]
for ($r = 1; $r -le $Rounds; $r++) {
    for ($i = 0; $i -lt $names.Count; $i++) {
        $name = $names[($i + $r - 1) % $names.Count]
        $c = $configs[$name]
        $tag = "r${r}_$name"
        $before = Avail-MiB
        # disk reads during the run (one typeperf sample per second): 0 GB means the model was in the file cache
        $perf = "$Out\$tag.perf.csv"
        $tp = Start-Process -FilePath typeperf -ArgumentList @('"\PhysicalDisk(_Total)\Disk Read Bytes/sec"', '"\Memory\Available MBytes"', '-si', '1', '-f', 'CSV', '-o', "`"$perf`"", '-y') -WindowStyle Hidden -PassThru
        Start-Sleep 2
        $saved = @{}
        foreach ($k in $c.env.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); Set-Item "env:$k" $c.env[$k] }
        $t0 = Get-Date
        $p = Start-Process -FilePath $c.exe -ArgumentList ($c.argv | ForEach-Object { Quote $_ }) -NoNewWindow -PassThru `
            -RedirectStandardOutput "$Out\$tag.md" -RedirectStandardError "$Out\$tag.log"
        $null = $p.Handle
        $p.WaitForExit()
        $sec = ((Get-Date) - $t0).TotalSeconds
        foreach ($k in $c.env.Keys) { if ($null -eq $saved[$k]) { Remove-Item "env:$k" -ErrorAction SilentlyContinue } else { Set-Item "env:$k" $saved[$k] } }
        Stop-Process -Id $tp.Id -ErrorAction SilentlyContinue
        Start-Sleep 1
        $readGb = $null; $minAvail = $null
        if ((Test-Path -LiteralPath $perf) -and (Get-Item -LiteralPath $perf).Length -gt 0) {
            $samples = @(Import-Csv -LiteralPath $perf)
            $cols = @($samples[0].PSObject.Properties.Name)
            $num = { param($x) $d = 0.0; if ([double]::TryParse($x, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$d)) { $d } else { $null } }
            $rd = @($samples | ForEach-Object { & $num $_.($cols[1]) } | Where-Object { $null -ne $_ })
            $av = @($samples | ForEach-Object { & $num $_.($cols[2]) } | Where-Object { $null -ne $_ })
            if ($rd.Count) { $readGb = [math]::Round(($rd | Measure-Object -Sum).Sum / 1e9, 2) }
            if ($av.Count) { $minAvail = [math]::Round(($av | Measure-Object -Minimum).Minimum) }
        }
        $tests = [ordered]@{}
        foreach ($line in Get-Content -LiteralPath "$Out\$tag.md") {
            $cells = @($line.Trim().Trim('|').Split('|') | ForEach-Object { $_.Trim() })
            if ($cells.Count -ge 2 -and $cells[-1] -match '^([\d.]+)\s+\S+\s+([\d.]+)$') {
                $mean = [double]$matches[1]
                if ($cells[-2] -match '^(pp|tg)\d+$') { $tests[$cells[-2]] = $mean }
            }
        }
        $speeds = ($tests.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '
        $row = [pscustomobject]@{ round = $r; config = $name; speeds = $speeds; exit = $p.ExitCode; seconds = [math]::Round($sec, 1); avail_before_mib = $before; avail_after_mib = (Avail-MiB); min_avail_mib = $minAvail; disk_read_gb = $readGb }
        $rows.Add($row)
        Step ("{0}: {1} t/s, exit {2} {3:n0}s avail {4}->{5} MiB (min {6}), disk read {7} GB" -f $tag, $speeds, $p.ExitCode, $sec, $row.avail_before_mib, $row.avail_after_mib, $minAvail, $readGb)
    }
}
$rows | Export-Csv -NoTypeInformation -LiteralPath "$Out\results.csv"

# Per config and test: the median over rounds 2+ (round 1 is the warm-up of the file cache)
function Median([double[]]$v) { $s = @($v | Sort-Object); if ($s.Count -eq 0) { return $null }; if ($s.Count % 2) { $s[[int][math]::Floor($s.Count / 2)] } else { ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2 } }
$testNames = @($rows | ForEach-Object { $_.speeds -split ' ' } | Where-Object { $_ } | ForEach-Object { ($_ -split '=')[0] } | Select-Object -Unique)
$md = @("| config | " + (($testNames | ForEach-Object { "$_ median, rounds 2+ (all rounds)" }) -join ' | ') + " |", (("|---" * ($testNames.Count + 1)) + "|"))
foreach ($n in $names) {
    $cellsOut = foreach ($t in $testNames) {
        $all = @($rows | Where-Object { $_.config -eq $n } | ForEach-Object {
            $m = [regex]::Match($_.speeds, "(?:^| )$t=([\d.]+)"); if ($m.Success) { [pscustomobject]@{ round = $_.round; v = [double]$m.Groups[1].Value } } })
        "$(Median @($all | Where-Object { $_.round -ge 2 } | ForEach-Object { $_.v })) ($(($all | ForEach-Object { $_.v }) -join ', '))"
    }
    $md += "| $n | " + ($cellsOut -join ' | ') + " |"
}
$md | Set-Content -LiteralPath "$Out\results.md"
$md | ForEach-Object { Step $_ }
