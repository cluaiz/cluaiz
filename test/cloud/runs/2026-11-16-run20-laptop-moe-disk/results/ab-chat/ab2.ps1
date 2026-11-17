# Cluaiz bench vs Cluaiz run (chat), same model, placement and context, alternated.
# Shows whether the chat path is slower than the engine's own generation loop.
param([int]$Rounds = 3, [string]$Out = "$PSScriptRoot\out2-$(Get-Date -Format 'yyyyMMdd-HHmmss')")
$ErrorActionPreference = 'Continue'   # PowerShell 5.1 turns native stderr lines into errors
$Model  = "$env:USERPROFILE\.cluaiz\models\chat\google_gemma-4-26b-a4b-it-gguf-IQ4_XS\google_gemma-4-26B-A4B-it-IQ4_XS.gguf"
$Cluaiz = "$env:USERPROFILE\.cluaiz\bin\cluaiz.exe"   # the installed release
New-Item -ItemType Directory -Force $Out | Out-Null
$env:CLUAIZ_HOME = "$env:LOCALAPPDATA\cluaiz-test\abhome"  # the owner's ~/.cluaiz stays untouched
$load = @('-c','16384','-t','8','--n-gpu-layers','all','--n-cpu-moe','30','--flash-attn','on','--cache-type-k','f16','--cache-type-v','f16')
for ($r = 1; $r -le $Rounds; $r++) {
    $t = Get-Date
    & $Cluaiz bench $Model -p 0 -n 400 -r 2 @load *> "$Out\r$r-bench.txt"
    "r$r-bench {0:n0}s" -f ((Get-Date) - $t).TotalSeconds
    $t = Get-Date
    & $Cluaiz run $Model --local -v --think off @load "Write a 300 word story about a robot." *> "$Out\r$r-run.txt"
    "r$r-run {0:n0}s" -f ((Get-Date) - $t).TotalSeconds
}
"OUT=$Out"
