# A/B speed test: Cluaiz bench vs official llama-bench, same model and placement, alternated.
# A per-second sampler records free RAM, disk reads and GPU dedicated/shared memory for every run.
param(
    [int]$Rounds = 3,
    [string]$Out = "$PSScriptRoot\out-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
)
# Continue: PowerShell 5.1 turns a native program's stderr lines into errors
$ErrorActionPreference = 'Continue'
$Model  = "$env:USERPROFILE\.cluaiz\models\chat\google_gemma-4-26b-a4b-it-gguf-IQ4_XS\google_gemma-4-26B-A4B-it-IQ4_XS.gguf"
$Llama  = "$env:LOCALAPPDATA\cluaiz-test\llama-official-b12428\llama-bench.exe"
$Cluaiz = "$PSScriptRoot\..\..\target\release\cluaiz.exe"
New-Item -ItemType Directory -Force $Out | Out-Null
# isolated home: the owner's ~/.cluaiz settings and learned margins stay untouched
$env:CLUAIZ_HOME = "$env:LOCALAPPDATA\cluaiz-test\abhome"
New-Item -ItemType Directory -Force $env:CLUAIZ_HOME | Out-Null

$counters = @(
    '\Memory\Available MBytes',
    '\PhysicalDisk(_Total)\Disk Read Bytes/sec',
    '\GPU Adapter Memory(*)\Dedicated Usage',
    '\GPU Adapter Memory(*)\Shared Usage'
)

function Run-One([string]$name, [string]$exe, [string[]]$argv, [hashtable]$envs) {
    $log = "$Out\$name.txt"; $res = "$Out\$name.res.csv"
    $job = Start-Job -ScriptBlock {
        param($c, $f)
        while ($true) {
            $s = Get-Counter -Counter $c -ErrorAction SilentlyContinue
            foreach ($x in $s.CounterSamples) { "{0:o},{1},{2}" -f $s.Timestamp, $x.Path, [math]::Round($x.CookedValue) | Add-Content $f }
        }
    } -ArgumentList (,$counters), $res
    Start-Sleep 2
    foreach ($k in $envs.Keys) { Set-Item "env:$k" $envs[$k] }
    $t0 = Get-Date
    & $exe @argv *> $log
    $sec = ((Get-Date) - $t0).TotalSeconds
    foreach ($k in $envs.Keys) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
    Stop-Job $job; Remove-Job $job
    "{0} done in {1:n0}s" -f $name, $sec
}

$common = @('-p','512','-n','128','-r','3','-t','8')
for ($r = 1; $r -le $Rounds; $r++) {
    Run-One "r$r-llama"  $Llama  ($common + @('-m',$Model,'-ngl','99','-ncmoe','30','-fa','1')) @{}
    Run-One "r$r-cluaiz" $Cluaiz (@('bench',$Model) + $common + @('-c','1024','--n-gpu-layers','all','--n-cpu-moe','30','--flash-attn','on','--cache-type-k','f16','--cache-type-v','f16')) @{}
    Run-One "r$r-cluaiz-nopin" $Cluaiz (@('bench',$Model) + $common + @('-c','1024','--n-gpu-layers','all','--n-cpu-moe','30','--flash-attn','on','--cache-type-k','f16','--cache-type-v','f16')) @{ GGML_CUDA_NO_PINNED = '1' }
}
"OUT=$Out"
