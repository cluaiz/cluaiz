# Cluaiz Automated API Smoke Test Suite (PowerShell)
# Usage: ./test/smoke.ps1 [optional_port]

param (
    [int]$Port = 8000
)

$BaseUrl = "http://127.0.0.1:$Port"
$Passed = 0
$Failed = 0

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

Write-Host "🧪 Running Cluaiz API Smoke Tests against $BaseUrl..." -ForegroundColor Cyan

# 1. Test Server Models List (OpenAI Standard)
try {
    $res = Invoke-RestMethod -Uri "$BaseUrl/v1/models" -Method Get -ErrorAction Stop
    Assert-Test "GET /v1/models returns 200 list" ($null -ne $res.data)
} catch {
    Assert-Test "GET /v1/models returns 200 list" $false $_.Exception.Message
}

# 2. Test max_tokens: 0 rejection (OpenAI Spec compliance: must be >= 1)
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

# 3. Test Chat Completion Handling (Model Slot Validation)
try {
    $body = @{
        messages = @(@{ role = "user"; content = "Hello! Answer in one word: Test." })
        max_tokens = 10
    } | ConvertTo-Json -Compress
    $res = Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop
    $isValid = ($null -ne $res.choices) -or ($res.error.code -eq "model_not_found")
    Assert-Test "POST /v1/chat/completions handles request gracefully" $isValid "Response: $($res | ConvertTo-Json -Compress)"
} catch {
    Assert-Test "POST /v1/chat/completions handles request gracefully" $false $_.Exception.Message
}

# 4. Test think_mode parameter parsing
foreach ($mode in @("on", "On", "auto", "Auto")) {
    try {
        $body = @{
            messages = @(@{ role = "user"; content = "Say 'OK'" })
            think_mode = $mode
            max_tokens = 5
        } | ConvertTo-Json -Compress
        $res = Invoke-RestMethod -Uri "$BaseUrl/v1/chat/completions" -Method Post -Body $body -ContentType "application/json" -ErrorAction Stop
        $isValid = ($null -ne $res.choices) -or ($res.error.code -eq "model_not_found")
        Assert-Test "POST /v1/chat/completions (think_mode: '$mode')" $isValid
    } catch {
        Assert-Test "POST /v1/chat/completions (think_mode: '$mode')" $false $_.Exception.Message
    }
}

Write-Host "`n📊 Smoke Test Results: $Passed Passed, $Failed Failed" -ForegroundColor $(if ($Failed -eq 0) { "Green" } else { "Red" })
if ($Failed -gt 0) { exit 1 }
