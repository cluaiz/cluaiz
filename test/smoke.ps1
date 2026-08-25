# Cluaiz Automated API Smoke Test Suite (PowerShell)
# Usage: 
#   ./test/smoke.ps1                  (Tests against currently running server on :8000)
#   ./test/smoke.ps1 -StartServer     (Automatically boots cluaiz serve, tests, and shuts down)
#   ./test/smoke.ps1 -Port 8080       (Custom port)

param (
    [int]$Port = 8000,
    [switch]$StartServer
)

$BaseUrl = "http://127.0.0.1:$Port"
$Passed = 0
$Failed = 0
$Skipped = 0
$ServerProcess = $null

function Assert-Test {
    param (
        [string]$Name,
        [bool]$Condition,
        [string]$Details = ""
    )
    if ($Condition) {
        Write-Host "  ✅ PASS: $Name" -ForegroundColor Green
        $script:Passed++
    } else {
        Write-Host "  ❌ FAIL: $Name ($Details)" -ForegroundColor Red
        $script:Failed++
    }
}

function Skip-Test {
    param (
        [string]$Name,
        [string]$Reason
    )
    Write-Host "  ⚠️ SKIP: $Name ($Reason)" -ForegroundColor Yellow
    $script:Skipped++
}

# 🚀 Server Lifecycle Management
if ($StartServer) {
    Write-Host "🚀 Starting Cluaiz Engine Daemon on port $Port..." -ForegroundColor Cyan
    $ServerProcess = Start-Process -FilePath ".\target\debug\cluaiz.exe" -ArgumentList "serve" -PassThru -NoNewWindow
    
    # Wait for server readiness
    $Ready = $false
    for ($i = 0; $i -lt 30; $i++) {
        Start-Sleep -Milliseconds 500
        try {
            $h = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -TimeoutSec 1 -ErrorAction SilentlyContinue
            if ($null -ne $h) { $Ready = $true; break }
        } catch {}
    }
    if (-not $Ready) {
        Write-Host "❌ Failed to start server within 15 seconds." -ForegroundColor Red
        if ($ServerProcess) { Stop-Process -Id $ServerProcess.Id -Force }
        exit 1
    }
}

try {
    Write-Host "🧪 Running Cluaiz API Smoke Tests against $BaseUrl..." -ForegroundColor Cyan

    # 1. Test Server Health Check
    try {
        $res = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -ErrorAction Stop
        Assert-Test "GET /health returns 200" $true
    } catch {
        Assert-Test "GET /health returns 200" $false $_.Exception.Message
    }

    # 2. Test OpenAI Standard Models List
    $InstalledModels = @()
    try {
        $res = Invoke-RestMethod -Uri "$BaseUrl/v1/models" -Method Get -ErrorAction Stop
        $isList = ($res.object -eq "list") -and ($null -ne $res.data)
        Assert-Test "GET /v1/models returns OpenAI list format" $isList
        $InstalledModels = $res.data
    } catch {
        Assert-Test "GET /v1/models returns OpenAI list format" $false $_.Exception.Message
    }

    # 3. Test max_tokens: 0 rejection (OpenAI Spec compliance: must be >= 1)
    try {
        $body = @{
            messages = @(@{ role = "user"; content = "Hello" })
            max_tokens = 0
        } | ConvertTo-Json -Compress
        $res = Invoke-WebRequest -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop
        Assert-Test "POST /v1/chat/completions (max_tokens: 0) rejected" $false "Expected 400 but got $($res.StatusCode)"
    } catch {
        $statusCode = 0
        if ($_.Exception.Response) {
            $statusCode = [int]$_.Exception.Response.StatusCode
        }
        Assert-Test "POST /v1/chat/completions (max_tokens: 0) returns 400" ($statusCode -eq 400) "Got Status $statusCode"
    }

    # 4. Determine if an active model exists for inference tests
    $HasActiveModel = $false
    try {
        $testProbe = @{
            messages = @(@{ role = "user"; content = "Test" })
            max_tokens = 1
        } | ConvertTo-Json -Compress
        $probeRes = Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $testProbe -ContentType "application/json" -ErrorAction Stop
        if ($null -ne $probeRes.choices -and $probeRes.choices.Count -gt 0) {
            $HasActiveModel = $true
        }
    } catch {}

    if (-not $HasActiveModel) {
        Skip-Test "POST /v1/chat/completions (inference & samplers)" "No active model loaded in chat_slot (Install/load a model to run live generation tests)"
    } else {
        # 5. Live Inference: Temperature 0.0 Determinism Test
        try {
            $body0 = @{
                messages = @(@{ role = "user"; content = "Say the word 'Cluaiz' and nothing else." })
                temperature = 0.0
                max_tokens = 10
            } | ConvertTo-Json -Compress
            $run1 = (Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $body0 -ContentType "application/json").choices[0].message.content
            $run2 = (Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $body0 -ContentType "application/json").choices[0].message.content
            Assert-Test "Temperature 0.0 produces deterministic output" ($run1 -eq $run2) "Run1: '$run1' vs Run2: '$run2'"
        } catch {
            Assert-Test "Temperature 0.0 produces deterministic output" $false $_.Exception.Message
        }

        # 6. Live Inference: think_mode case variations
        foreach ($mode in @("on", "On", "auto", "Auto")) {
            try {
                $bodyMode = @{
                    messages = @(@{ role = "user"; content = "Say 'OK'" })
                    think_mode = $mode
                    max_tokens = 10
                } | ConvertTo-Json -Compress
                $res = Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $bodyMode -ContentType "application/json" -ErrorAction Stop
                $hasContent = $null -ne $res.choices[0].message.content
                Assert-Test "POST /v1/chat/completions (think_mode: '$mode')" $hasContent
            } catch {
                Assert-Test "POST /v1/chat/completions (think_mode: '$mode')" $false $_.Exception.Message
            }
        }
    }

    Write-Host "`n📊 Smoke Test Results: $Passed Passed, $Failed Failed, $Skipped Skipped" -ForegroundColor $(if ($Failed -eq 0) { "Green" } else { "Red" })
    if ($Failed -gt 0) { exit 1 }

} finally {
    if ($ServerProcess) {
        Write-Host "🛑 Stopping Cluaiz Engine Daemon..." -ForegroundColor Cyan
        Stop-Process -Id $ServerProcess.Id -Force
    }
}
