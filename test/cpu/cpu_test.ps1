# Cluaiz on a Windows PC without a graphics card: one command, about an hour, one archive on the Desktop.
# Installs the release with the website's command, then chat, speed and memory on small models, the coding
# agent, plugins and skills, speech, embeddings and a document. See test/cpu/README.md.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/cpu/cpu_test.ps1 | iex"
#
# Environment (optional):
#   CLUAIZ_VERSION   release to install (default: the newest, pre-releases included)
#   CPU_TEST_MODELS  models to test, separated by spaces (default below)
#   CPU_TEST_DIR     where everything is kept (default: ~\cluaiz-cputest)
#   CPU_TEST_EXE     use this cluaiz.exe and skip the install (for working on this script)

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$W = if ($env:CPU_TEST_DIR) { $env:CPU_TEST_DIR } else { Join-Path $HOME 'cluaiz-cputest' }
$OUT = Join-Path $W 'results'
$BinDir = Join-Path $W 'bin'
$TestHome = Join-Path $W 'home'
$Project = Join-Path $W 'project'
$Exe = if ($env:CPU_TEST_EXE) { $env:CPU_TEST_EXE } else { Join-Path $BinDir 'cluaiz.exe' }
$Port = 8011
$Url = "http://127.0.0.1:$Port"
$ModelList = if ($env:CPU_TEST_MODELS) { $env:CPU_TEST_MODELS -split '\s+' } else {
    @('prism-ml/Bonsai-4B-gguf:Q1_0', 'prism-ml/Bonsai-8B-gguf:Q1_0', 'unsloth/Qwen3-4B-GGUF:Q4_K_M')
}
$NeedGb = 12
$Crash = -1073740791  # 0xC0000409: the program ended itself
$Started = Get-Date
$Script:Rows = New-Object System.Collections.ArrayList
$Script:Speeds = New-Object System.Collections.ArrayList
$Script:Server = $null
$Script:Token = ''

foreach ($dir in @($W, $TestHome, $Project)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
if (Test-Path $OUT) { Remove-Item -Recurse -Force $OUT }
New-Item -ItemType Directory -Force -Path $OUT | Out-Null
$Empty = Join-Path $OUT 'empty-input.txt'
Set-Content -Path $Empty -Value '' -Encoding ascii

$env:CLUAIZ_HOME = $TestHome
$env:CLUAIZ_PORT = "$Port"
$env:CLUAIZ_HOST = '127.0.0.1'
$env:CLUAIZ_NONINTERACTIVE = '1'

function Elapsed { $s = ((Get-Date) - $Started); '{0}:{1:00}' -f [int][math]::Floor($s.TotalMinutes), $s.Seconds }
$Script:StepNo = 0
function Step([string]$text) { $Script:StepNo++; Say ''; Say "== [step $($Script:StepNo) of 10, $(Elapsed) elapsed] $text" }
function Say([string]$text) {
    Write-Host $text
    Add-Content -Path (Join-Path $OUT 'cpu_test.log') -Value $text -Encoding utf8
}
function Result([string]$group, [string]$name, [string]$state, [string]$detail) {
    if ($detail.Length -gt 300) { $detail = $detail.Substring(0, 300) }
    [void]$Script:Rows.Add([pscustomobject]@{ group = $group; name = $name; state = $state; detail = $detail })
    Say ("   {0,-5} {1}: {2}" -f $state, $name, $detail)
}
function Short([string]$text, [int]$most = 160) {
    $line = ($text -replace '\s+', ' ').Trim()
    if ($line.Length -gt $most) { $line = $line.Substring($line.Length - $most) }
    $line
}
function Safe([string]$name) { $name -replace '[^A-Za-z0-9._-]', '_' }

# A program with its output in files and a time limit. Output in a pipe can wait for a child that outlives it.
function Run([string]$name, [string[]]$arguments, [int]$seconds = 900, [string]$cwd = $Project) {
    $o = Join-Path $OUT "$name.out.txt"
    $e = Join-Path $OUT "$name.err.txt"
    $quoted = $arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }
    $begin = Get-Date
    $p = Start-Process -FilePath $Exe -ArgumentList ($quoted -join ' ') -WorkingDirectory $cwd -NoNewWindow -PassThru `
        -RedirectStandardOutput $o -RedirectStandardError $e -RedirectStandardInput $Empty
    [void]$p.Handle
    $done = $p.WaitForExit($seconds * 1000)
    if (-not $done) { & taskkill.exe /PID $p.Id /T /F 2>$null | Out-Null }
    else { $p.WaitForExit() }
    $code = if ($done) { $p.ExitCode } else { 'TIMEOUT' }
    [pscustomobject]@{
        code = $code; seconds = [int]((Get-Date) - $begin).TotalSeconds
        out = (Get-Content -Raw -Path $o -ErrorAction SilentlyContinue) + ''
        err = (Get-Content -Raw -Path $e -ErrorAction SilentlyContinue) + ''
    }
}

function Start-Engine {
    Stop-Engine
    $log = Join-Path $OUT ('server-' + (Get-Date -Format 'HHmmss'))
    $Script:Server = Start-Process -FilePath $Exe -ArgumentList 'serve' -WorkingDirectory $Project -NoNewWindow -PassThru `
        -RedirectStandardOutput "$log.out.txt" -RedirectStandardError "$log.err.txt" -RedirectStandardInput $Empty
    [void]$Script:Server.Handle
    for ($i = 0; $i -lt 180; $i++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$Url/health" -TimeoutSec 3 | Out-Null
            $file = Join-Path $TestHome 'session.token'
            if (Test-Path $file) { $Script:Token = (Get-Content -Raw $file).Trim() }
            return $true
        } catch { Start-Sleep -Seconds 1 }
    }
    return $false
}
function Stop-Engine {
    if ($Script:Server -and -not $Script:Server.HasExited) { & taskkill.exe /PID $Script:Server.Id /T /F 2>$null | Out-Null }
    $Script:Server = $null
    Start-Sleep -Seconds 2
}
function Post([string]$path, $body, [int]$seconds = 900) {
    $json = [Text.Encoding]::UTF8.GetBytes(($body | ConvertTo-Json -Depth 12 -Compress))
    $headers = @{ Authorization = "Bearer $($Script:Token)" }
    try {
        $r = Invoke-WebRequest -UseBasicParsing -Method Post -Uri "$Url$path" -Headers $headers -ContentType 'application/json' -Body $json -TimeoutSec $seconds
        $text = [Text.Encoding]::UTF8.GetString($r.RawContentStream.ToArray())
        [pscustomobject]@{ ok = $true; status = [int]$r.StatusCode; text = $text }
    } catch {
        $status = 0; $text = "$($_.Exception.Message)"
        if ($_.Exception.Response) {
            $status = [int]$_.Exception.Response.StatusCode
            try { $text = (New-Object IO.StreamReader($_.Exception.Response.GetResponseStream())).ReadToEnd() } catch {}
        }
        [pscustomobject]@{ ok = $false; status = $status; text = $text }
    }
}
function Chat([string]$model, [string]$question, [string]$session = '', [string]$think = 'off', [int]$most = 300, $history = @()) {
    $messages = @($history) + @(@{ role = 'user'; content = $question })
    $body = @{ model = $model; messages = $messages; max_tokens = $most; think_mode = $think }
    if ($session) { $body.session_id = $session }
    $begin = Get-Date
    $r = Post '/v1/chat/completions' $body
    $seconds = ((Get-Date) - $begin).TotalSeconds
    $answer = ''; $engine = $null
    if ($r.ok) {
        try {
            $j = $r.text | ConvertFrom-Json
            $answer = "$($j.choices[0].message.content)"
            $engine = $j.engine
        } catch {}
    }
    [pscustomobject]@{ ok = $r.ok; status = $r.status; answer = $answer; engine = $engine; seconds = $seconds; raw = $r.text }
}
function Cep([string]$method, $params) {
    $r = Post '/v1/cep' @{ jsonrpc = '2.0'; id = 1; method = $method; params = $params }
    if (-not $r.ok) { return "HTTP $($r.status) $($r.text)" }
    $j = $r.text | ConvertFrom-Json
    if ($j.error) { return "error: $($j.error.message)" }
    return ($j.result | ConvertTo-Json -Depth 8 -Compress)
}
function Installed {
    (Run 'list' @('list') 120).out -split "`r?`n" | Where-Object { $_ -and $_ -notmatch '^ID\s' -and $_ -notmatch '^No models' } |
        ForEach-Object { ($_ -split '\s+')[0] } | Where-Object { $_ }
}
function Agent-Events([string]$text) {
    $calls = @(); $oks = @(); $answer = ''
    foreach ($line in ($text -split "`r?`n")) {
        if (-not $line.StartsWith('{')) { continue }
        try { $ev = $line | ConvertFrom-Json } catch { continue }
        if ($ev.type -eq 'tool_call') { $calls += "$($ev.name)" }
        if ($ev.type -eq 'tool_result') { $oks += "$($ev.ok)" }
        if ($ev.type -eq 'result') { $answer = "$($ev.result)$($ev.error)" }
    }
    [pscustomobject]@{ calls = ($calls -join ','); oks = ($oks -join ','); answer = $answer }
}

$PathBefore = [Environment]::GetEnvironmentVariable('Path', 'User')
try {
    # ------ 1. The machine ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'checking this PC'
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    $cs = Get-CimInstance Win32_ComputerSystem
    $os = Get-CimInstance Win32_OperatingSystem
    $disk = Get-PSDrive -Name ($W.Substring(0, 1))
    $freeGb = [math]::Round($disk.Free / 1GB, 1)
    $ramGb = [math]::Round($cs.TotalPhysicalMemory / 1GB, 1)
    $gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
    $battery = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1
    $power = if (-not $battery) { 'no battery' } elseif ($battery.BatteryStatus -eq 2) { 'on the charger' } else { 'ON BATTERY' }
    @("OS: $($os.Caption) $($os.Version)", "CPU: $($cpu.Name), $($cpu.NumberOfCores) cores, $($cpu.NumberOfLogicalProcessors) threads",
      "RAM: $ramGb GB, free $([math]::Round($os.FreePhysicalMemory / 1MB, 1)) GB", "Disk free: $freeGb GB", "Graphics: $($gpus -join '; ')", "Power: $power") |
        Tee-Object -FilePath (Join-Path $OUT 'machine.txt') | ForEach-Object { Say "   $_" }
    Result 'machine' 'disk' $(if ($freeGb -ge $NeedGb) { 'PASS' } else { 'FAIL' }) "$freeGb GB free, about $NeedGb GB needed"
    Result 'machine' 'power' $(if ($power -eq 'ON BATTERY') { 'WARN' } else { 'PASS' }) "$power (on battery the speed numbers are lower)"
    if ($freeGb -lt $NeedGb) { throw "Not enough free disk: $freeGb GB, about $NeedGb GB needed" }

    # ------ 2. Install ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'installing with the website command'
    if ($env:CPU_TEST_EXE) {
        Result 'install' 'install' 'SKIP' "using $Exe"
    } else {
        $inst = Join-Path $OUT 'install.log'
        $env:CLUAIZ_INSTALL_DIR = $BinDir
        $p = Start-Process -FilePath 'powershell.exe' -NoNewWindow -PassThru -RedirectStandardOutput $inst -RedirectStandardError "$inst.err" -RedirectStandardInput $Empty `
            -ArgumentList '-NoProfile -ExecutionPolicy Bypass -Command "irm https://cluaiz.com/install.ps1 | iex"'
        [void]$p.Handle
        $begin = Get-Date
        if (-not $p.WaitForExit(40 * 60 * 1000)) { & taskkill.exe /PID $p.Id /T /F 2>$null | Out-Null }
        $seconds = [int]((Get-Date) - $begin).TotalSeconds
        $ok = Test-Path $Exe
        $packs = (Select-String -Path $inst -Pattern 'Extracting' -ErrorAction SilentlyContinue | ForEach-Object { ($_.Line -split ':')[-1].Trim() }) -join ', '
        Result 'install' 'install' $(if ($ok) { 'PASS' } else { 'FAIL' }) "$seconds s; packs: $packs"
        if (-not $ok) { throw 'The install did not produce cluaiz.exe; see install.log' }
    }
    $version = (Run 'version' @('--version') 60).out.Trim()
    Result 'install' 'version' $(if ($version) { 'PASS' } else { 'FAIL' }) $version
    (Get-ChildItem -Path (Split-Path $Exe) -File | Select-Object Name, Length | Format-Table -AutoSize | Out-String -Width 120) | Set-Content (Join-Path $OUT 'installed_files.txt')
    $status = Run 'status' @('status') 120
    Result 'install' 'hardware seen' $(if ($status.code -eq 0) { 'PASS' } else { 'FAIL' }) (Short $status.out 240)

    # ------ 3. Models ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'downloading the models'
    $before = @(Installed)
    $tested = @()
    foreach ($model in $ModelList) {
        Say "   downloading $model"
        $r = Run ('pull_' + (Safe $model)) @('pull', $model) 2400
        $after = @(Installed)
        $new = @($after | Where-Object { $before -notcontains $_ })
        $id = if ($new.Count) { $new[0] } else { $null }
        if (-not $id) {
            $key = ((($model -split '/')[-1] -split ':')[0] -replace '(?i)-gguf$', '').ToLower()
            $id = $after | Where-Object { $_.ToLower().Contains($key) } | Select-Object -First 1
        }
        $before = $after
        Result 'models' "pull $model" $(if ($r.code -eq 0 -and $id) { 'PASS' } else { 'FAIL' }) "exit $($r.code), $($r.seconds) s, id $id $(if ($r.code -ne 0) { Short ($r.out + $r.err) })"
        if ($r.code -eq 0 -and $id) { $tested += $id }
    }
    if (-not $tested) { throw 'No model could be downloaded; see pull_*.txt' }

    # ------ 4. Server ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'starting the server'
    $up = Start-Engine
    Result 'server' 'server starts' $(if ($up) { 'PASS' } else { 'FAIL' }) "port $Port"
    if (-not $up) { throw 'The server did not start; see server-*.txt' }

    # ------ 5. Chat on every model ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'chat, speed and memory on each model (slow on a CPU)'
    foreach ($id in $tested) {
        Say "   -- $id"
        (Run ('show_' + (Safe $id)) @('show', $id) 120) | Out-Null
        $cold = Chat $id 'What is the capital of France? Answer with one word.'
        Set-Content -Path (Join-Path $OUT ('chat_' + (Safe $id) + '.json')) -Value $cold.raw -Encoding utf8
        Result 'chat' "$id first answer" $(if ($cold.answer -match '(?i)paris') { 'PASS' } elseif ($cold.ok) { 'WARN' } else { 'FAIL' }) ("{0:N0} s with loading: {1}" -f $cold.seconds, (Short "$($cold.answer)$(if (-not $cold.ok) { $cold.raw })"))
        if (-not $cold.ok) { continue }
        $load = $cold.engine.load
        $gpuLayers = "$($load.n_gpu_layers)"
        Result 'chat' "$id placement" $(if ($load.placement -match '(?i)cpu' -or $gpuLayers -eq '0') { 'PASS' } else { 'WARN' }) "placement $($load.placement), GPU layers $gpuLayers, n_ctx $($load.n_ctx), KV $($load.cache_type)"

        $gen = @(); $prompt = @()
        foreach ($i in 1..3) {
            $s = Chat $id "Write about 100 words on why rain falls. Attempt $i." '' 'off' 160
            if ($s.engine.timings.predicted_per_second) { $gen += [double]$s.engine.timings.predicted_per_second }
            if ($s.engine.timings.prompt_per_second) { $prompt += [double]$s.engine.timings.prompt_per_second }
        }
        $genAvg = if ($gen) { [math]::Round(($gen | Measure-Object -Average).Average, 1) } else { 0 }
        $promptAvg = if ($prompt) { [math]::Round(($prompt | Measure-Object -Average).Average, 1) } else { 0 }
        $proc = Get-Process -Id $Script:Server.Id -ErrorAction SilentlyContinue
        $ram = if ($proc) { [math]::Round($proc.WorkingSet64 / 1GB, 2) } else { 0 }
        [void]$Script:Speeds.Add([pscustomobject]@{ model = $id; generation_tps = $genAvg; prompt_tps = $promptAvg; ram_gb = $ram; placement = "$($load.placement)" })
        Result 'speed' "$id speed" $(if ($genAvg -gt 0) { 'PASS' } else { 'FAIL' }) "generation $genAvg t/s, prompt $promptAvg t/s, server RAM $ram GB"

        $session = 'cpu-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
        $first = Chat $id 'Please keep this in mind: my code word is MANGO-42. Reply with OK.' $session
        $hist = @(@{ role = 'user'; content = 'Please keep this in mind: my code word is MANGO-42. Reply with OK.' }, @{ role = 'assistant'; content = $first.answer })
        $next = Chat $id 'What is my code word? Answer with the code word only.' $session 'off' 300 $hist
        Result 'chat' "$id next turn" $(if ($next.answer -match 'MANGO') { 'PASS' } elseif ($next.ok) { 'WARN' } else { 'FAIL' }) "reused $($next.engine.context.cache_n) of $($next.engine.context.prompt_tokens) tokens: $(Short $next.answer 80)"

        $thinking = Chat $id 'What is 12 + 30? Give only the number.' '' 'auto' 800
        Result 'chat' "$id with thinking on default" $(if ($thinking.answer -match '42') { 'PASS' } elseif ($thinking.ok) { 'WARN' } else { 'FAIL' }) ("{0:N0} s: {1}" -f $thinking.seconds, (Short $thinking.answer 80))

        $stream = Post '/v1/chat/completions' @{ model = $id; messages = @(@{ role = 'user'; content = 'Say hello.' }); max_tokens = 40; think_mode = 'off'; stream = $true }
        $chunks = ([regex]::Matches($stream.text, '(?m)^data:')).Count
        Result 'chat' "$id streaming" $(if ($stream.ok -and $chunks -gt 1 -and $stream.text -match '\[DONE\]') { 'PASS' } else { 'FAIL' }) "$chunks parts"
    }
    $bad = Post '/v1/chat/completions' @{ model = 'no-such-model'; messages = @(@{ role = 'user'; content = 'hi' }) } 60
    Result 'chat' 'a wrong model name' $(if ($bad.status -in 400, 404) { 'PASS' } else { 'FAIL' }) "HTTP $($bad.status)"

    # ------ 6. Plugins, skills ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'plugins and skills'
    foreach ($item in @(@('plugin', 'math'), @('plugin', 'time'), @('plugin', 'sysinfo'), @('plugin', 'text'), @('skill', 'tdd-workflow'))) {
        $r = Run "install_$($item[1])" @($item[0], 'install', $item[1]) 300
        Result 'tools' "install $($item[1])" $(if ($r.code -eq 0) { 'PASS' } else { 'FAIL' }) "exit $($r.code)"
    }
    $bad = Run 'install_bad' @('mcp', 'install', 'no-such-tool-xyz') 120
    Result 'tools' 'a wrong tool name exits 1' $(if ($bad.code -eq 1) { 'PASS' } else { 'FAIL' }) "exit $($bad.code)"
    $up = Start-Engine
    $math = Cep 'tools.call' @{ kind = 'plugin'; name = 'math'; fn = 'math_evaluate'; input = @{ expression = '17 * 23' } }
    Result 'tools' 'math plugin' $(if ($math -match '391') { 'PASS' } else { 'FAIL' }) (Short $math)
    $time = Cep 'tools.call' @{ kind = 'plugin'; name = 'time'; fn = 'time_now'; input = @{} }
    Result 'tools' 'time plugin' $(if ($time -match 'timestamp_ms') { 'PASS' } else { 'FAIL' }) (Short $time)
    $sys = Cep 'tools.call' @{ kind = 'plugin'; name = 'sysinfo'; fn = 'sysinfo_telemetry'; input = @{} }
    Result 'tools' 'sysinfo plugin' $(if ($sys -match 'Windows') { 'PASS' } else { 'FAIL' }) (Short $sys)
    $text = Cep 'tools.call' @{ kind = 'plugin'; name = 'text'; fn = 'text_transform'; input = @{ action = 'base64_encode'; input = 'hello' } }
    Result 'tools' 'text plugin' $(if ($text -match 'aGVsbG8=') { 'PASS' } else { 'FAIL' }) (Short $text)

    # ------ 7. Speech, embeddings, a document ------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'speech, embeddings and a document'
    $sentence = 'The quick brown fox jumps over the lazy dog near the river bank.'
    foreach ($m in @('onnx-community/Kokoro-82M-v1.0-ONNX', 'onnx-community/moonshine-tiny-ONNX', 'sentence-transformers/all-MiniLM-L6-v2')) {
        $r = Run ('pull_' + (Safe $m)) @('pull', $m) 1500
        Result 'media' "pull $m" $(if ($r.code -eq 0) { 'PASS' } else { 'FAIL' }) "exit $($r.code), $($r.seconds) s"
    }
    $up = Start-Engine
    $all = @(Installed)
    $tts = $all | Where-Object { $_ -match '(?i)kokoro' } | Select-Object -First 1
    $stt = $all | Where-Object { $_ -match '(?i)moonshine' } | Select-Object -First 1
    $emb = $all | Where-Object { $_ -match '(?i)minilm' } | Select-Object -First 1
    $wav = Join-Path $OUT 'speech.wav'
    if ($tts) {
        $begin = Get-Date
        $r = Post '/v1/audio/speech' @{ model = $tts; input = $sentence; voice = 'default' }
        $seconds = [int]((Get-Date) - $begin).TotalSeconds
        if ($r.ok) {
            $uri = ($r.text | ConvertFrom-Json).output.audio_data
            [IO.File]::WriteAllBytes($wav, [Convert]::FromBase64String(($uri -split ',', 2)[1]))
        }
        Result 'media' 'text to speech' $(if (Test-Path $wav) { 'PASS' } else { 'FAIL' }) "$seconds s, $(if (Test-Path $wav) { (Get-Item $wav).Length } else { $r.text }) bytes"
    }
    if ($stt -and (Test-Path $wav)) {
        $begin = Get-Date
        $r = Post '/v1/audio/transcriptions' @{ model = $stt; file = $wav; language = 'en' }
        $heard = [regex]::Match($r.text, '"text"\s*:\s*"([^"]*)"').Groups[1].Value
        Result 'media' 'speech to text' $(if ($heard -match '(?i)lazy dog') { 'PASS' } elseif ($r.ok) { 'WARN' } else { 'FAIL' }) ("{0} s: {1}" -f [int]((Get-Date) - $begin).TotalSeconds, (Short "$heard$(if (-not $r.ok) { $r.text })"))
    }
    if ($emb) {
        $r = Post '/v1/embeddings' @{ model = $emb; input = @('hello world', 'a second text') }
        $count = if ($r.ok) { @(($r.text | ConvertFrom-Json).data).Count } else { 0 }
        Result 'media' 'embeddings' $(if ($count -eq 2) { 'PASS' } else { 'FAIL' }) "$count vectors"
    }
    $doc = Join-Path $Project 'report.txt'
    Set-Content -Path $doc -Value 'Invoice 7741. The total amount due is 2650 rupees.' -Encoding utf8
    $r = Post '/v1/ingest' @{ source = $doc }
    Result 'media' 'document' $(if ($r.text -match '7741') { 'PASS' } else { 'FAIL' }) (Short $r.text)

    # ------ 8. The coding agent ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'the coding agent (slow on a CPU)'
    Stop-Engine
    $agentModel = $tested | Where-Object { $_ -match '(?i)qwen' } | Select-Object -First 1
    if (-not $agentModel) { $agentModel = $tested[-1] }
    Say "   model: $agentModel"
    New-Item -ItemType Directory -Force -Path (Join-Path $Project '.git') | Out-Null
    Set-Content -Path (Join-Path $Project 'calc.py') -Value 'print(6 * 7)' -Encoding ascii
    Set-Content -Path (Join-Path $Project 'notes.txt') -Value 'The launch code is 4471.' -Encoding ascii
    $base = @('agent', '--model', $agentModel, '--output-format', 'stream-json')
    $r = Run 'agent_read' ($base + @('-p', 'Read calc.py and say in one line what it prints.')) 900
    $ev = Agent-Events $r.out
    Result 'agent' 'reads a file' $(if ($ev.answer -match '42') { 'PASS' } elseif ($r.code -eq 0) { 'WARN' } else { 'FAIL' }) "exit $($r.code), $($r.seconds) s, tools $($ev.calls): $(Short $ev.answer 90)"
    $r = Run 'agent_continue' ($base + @('--continue', '-p', 'Which file did you read in the last request? Answer with its name only.')) 900
    $ev = Agent-Events $r.out
    Result 'agent' '--continue remembers' $(if ($ev.answer -match 'calc') { 'PASS' } elseif ($r.code -eq 0) { 'WARN' } else { 'FAIL' }) "exit $($r.code), $($r.seconds) s: $(Short $ev.answer 90)"
    $r = Run 'agent_command' ($base + @('--yes', '-p', 'Use run_command to run: hostname. Then say DONE.')) 900
    $ev = Agent-Events $r.out
    Result 'agent' 'runs a command' $(if ($r.code -eq $Crash) { 'FAIL' } elseif ($ev.calls -match 'run_command' -and $ev.oks -match 'True') { 'PASS' } else { 'WARN' }) "exit $($r.code), $($r.seconds) s, tools $($ev.calls)"
    $r = Run 'agent_network' ($base + @('--yes', '-p', 'Use run_command with curl.exe -s https://example.com and say the page title.')) 900
    $ev = Agent-Events $r.out
    Result 'agent' 'a slow command does not crash' $(if ($r.code -eq $Crash) { 'FAIL' } else { 'PASS' }) "exit $($r.code), $($r.seconds) s, tools $($ev.calls)"
    $r = Run 'agent_refused' ($base + @('-p', 'Create a file named made.txt containing hello.')) 900
    Result 'agent' 'no approver: nothing is written' $(if (-not (Test-Path (Join-Path $Project 'made.txt'))) { 'PASS' } else { 'FAIL' }) "exit $($r.code), $($r.seconds) s"
    $r = Run 'agent_plugin' ($base + @('--yes', '-p', 'What is 17 * 23? Use the math tool.')) 900
    $ev = Agent-Events $r.out
    Result 'agent' 'uses a plugin' $(if ($ev.calls -match 'math') { 'PASS' } elseif ($ev.answer -match '391') { 'WARN' } else { 'FAIL' }) "exit $($r.code), tools $($ev.calls): $(Short $ev.answer 70)"

    # Each Bonsai model too: a call written as text and never run is a failure of the Engine, not of the model
    foreach ($bonsai in @($tested | Where-Object { $_ -match '(?i)bonsai' })) {
        $r = Run ('agent_read_' + (Safe $bonsai)) @('agent', '--model', $bonsai, '--output-format', 'stream-json', '-p', 'Read calc.py and say in one line what it prints.') 900
        $ev = Agent-Events $r.out
        Result 'agent' "$bonsai reads a file" $(if ($ev.answer -match 'tool_call') { 'FAIL' } elseif ($ev.answer -match '42') { 'PASS' } else { 'WARN' }) "exit $($r.code), $($r.seconds) s, tools $($ev.calls): $(Short $ev.answer 90)"
    }

    # ------ 9. A long chat ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Step 'a long chat: the context fills up'
    $up = Start-Engine
    $small = $tested[0]
    $session = 'cpu-long-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
    $hist = @(); $errors = 0; $alive = $true
    foreach ($i in 1..12) {
        $q = "Turn $i. Write two sentences about the number $i."
        $c = Chat $small $q $session 'off' 120 $hist
        if (-not $c.ok) { $errors++ }
        $hist = @($hist) + @(@{ role = 'user'; content = $q }, @{ role = 'assistant'; content = $c.answer })
        if ($Script:Server.HasExited) { $alive = $false; break }
    }
    Result 'long' '12 turns' $(if ($alive -and $errors -eq 0) { 'PASS' } else { 'FAIL' }) "errors $errors, server alive $alive"
    Stop-Engine
} catch {
    Result 'stopped' 'the test stopped early' 'FAIL' "$($_.Exception.Message)"
} finally {
    # ------ 10. Results ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
    Stop-Engine
    if ($PathBefore -cne [Environment]::GetEnvironmentVariable('Path', 'User')) {
        [Environment]::SetEnvironmentVariable('Path', $PathBefore, 'User')
    }
    Step 'writing the results'
    $Script:Rows | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OUT 'results.json') -Encoding utf8
    $Script:Speeds | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $OUT 'speeds.json') -Encoding utf8
    $count = { param($s) @($Script:Rows | Where-Object { $_.state -eq $s }).Count }
    $md = New-Object System.Collections.ArrayList
    [void]$md.Add("# Cluaiz CPU test, $(Get-Date -Format 'yyyy-MM-dd HH:mm'), $(Elapsed)")
    [void]$md.Add('')
    [void]$md.Add('```')
    [void]$md.Add((Get-Content (Join-Path $OUT 'machine.txt') -ErrorAction SilentlyContinue | Out-String).Trim())
    [void]$md.Add('```')
    [void]$md.Add('')
    [void]$md.Add("PASS $(& $count 'PASS'), WARN $(& $count 'WARN'), FAIL $(& $count 'FAIL'), SKIP $(& $count 'SKIP')")
    [void]$md.Add('')
    [void]$md.Add('| Model | Generation t/s | Prompt t/s | Server RAM GB | Placement |')
    [void]$md.Add('|---|---|---|---|---|')
    foreach ($s in $Script:Speeds) { [void]$md.Add("| $($s.model) | $($s.generation_tps) | $($s.prompt_tps) | $($s.ram_gb) | $($s.placement) |") }
    [void]$md.Add('')
    [void]$md.Add('| State | Group | Test | Detail |')
    [void]$md.Add('|---|---|---|---|')
    foreach ($row in $Script:Rows) { [void]$md.Add("| $($row.state) | $($row.group) | $($row.name) | $(($row.detail -replace '\|', '/')) |") }
    $md | Set-Content (Join-Path $OUT 'SUMMARY.md') -Encoding utf8
    Say ''
    Say '== summary'
    foreach ($s in $Script:Speeds) { Say ("   {0}: {1} t/s, RAM {2} GB" -f $s.model, $s.generation_tps, $s.ram_gb) }
    Say "   PASS $(& $count 'PASS'), WARN $(& $count 'WARN'), FAIL $(& $count 'FAIL')"
    foreach ($row in ($Script:Rows | Where-Object { $_.state -eq 'FAIL' })) { Say "   FAIL $($row.name): $(Short $row.detail 110)" }
    $desktop = [Environment]::GetFolderPath('Desktop')
    $archive = Join-Path $desktop ("cluaiz-cpu-results-{0}.zip" -f (Get-Date -Format 'yyyyMMdd-HHmm'))
    Remove-Item -Path $Empty -ErrorAction SilentlyContinue
    Compress-Archive -Path (Join-Path $OUT '*') -DestinationPath $archive -Force
    Write-Host "== done in $(Elapsed)"
    Write-Host "== results: $archive (send this file)"
    Write-Host "To remove everything afterwards: Remove-Item -Recurse -Force '$W'"
}
