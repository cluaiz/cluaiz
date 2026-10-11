# Cluaiz 100% Direct System Plan Audit & Smoke Test
# Usage:
#   .\test\smoke.ps1                           (Audits all installed chat models against :8000)
#   .\test\smoke.ps1 -Port 8080                (Custom port)
#   .\test\smoke.ps1 -OutputFile ".\audit.txt" (Custom output text path)

param (
    [int]$Port = 8000,
    [string]$OutputFile = ".\test\system_plan_audit.txt"
)

$BaseUrl = "http://127.0.0.1:$Port"

Write-Host ""
Write-Host "==========================================================================================" -ForegroundColor Cyan
Write-Host "              CLUAIZ CHAT MODELS SYSTEM PLAN AUDIT (/v1/system/plan)                      " -ForegroundColor Cyan
Write-Host "==========================================================================================" -ForegroundColor Cyan
Write-Host "Target Server : $BaseUrl" -ForegroundColor DarkGray
Write-Host "Audit Output  : $OutputFile" -ForegroundColor DarkGray
Write-Host ""

# 1. Health Probe
try {
    $health = Invoke-RestMethod -Uri "$BaseUrl/health" -Method Get -TimeoutSec 3 -ErrorAction Stop
    Write-Host "Server Health : ONLINE (HTTP 200)" -ForegroundColor Green
} catch {
    Write-Host "Server Health : OFFLINE ($($_.Exception.Message))" -ForegroundColor Red
    Write-Host "Please start the engine daemon: cargo run --release -p cmd -- serve" -ForegroundColor Yellow
    exit 1
}

# 2. Fetch Installed Models & Filter Strictly for Chat Models (Zero Khichdi)
Write-Host "Discovering installed chat models from $BaseUrl/v1/models/installed..." -ForegroundColor Cyan
$installedRes = $null
try {
    $installedRes = Invoke-RestMethod -Uri "$BaseUrl/v1/models/installed" -Method Get -TimeoutSec 5 -ErrorAction Stop
} catch {
    Write-Host "❌ Failed to fetch installed models: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}

$allModels = @($installedRes.installed)
$chatModels = @($allModels | Where-Object { 
    $_.category -eq "chat" -or ($_.supported_tasks -and ($_.supported_tasks -contains "chat-completion" -or $_.supported_tasks -contains "text-generation"))
})

if ($chatModels.Count -eq 0) {
    Write-Host "⚠️ No chat models found in registry. Install or import a chat model first." -ForegroundColor Yellow
    exit 0
}

Write-Host "Discovered $($chatModels.Count) installed Chat LLM(s) (Non-Chat filtered out)." -ForegroundColor Green
Write-Host ""

$AuditResults = @()
$HardwareSnapshot = $null
$ReportLines = [System.Collections.Generic.List[string]]::new()

$ReportLines.Add("=================================================================================================================================================================================")
$ReportLines.Add("                                                   CLUAIZ AUTONOMOUS SYSTEM PLAN EXECUTION DEEP AUDIT REPORT                                                     ")
$ReportLines.Add("=================================================================================================================================================================================")
$ReportLines.Add("Generated At : $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')")
$ReportLines.Add("Engine Host  : $BaseUrl")

# 3. Iterate & Test Each Chat Model
$index = 0
foreach ($m in $chatModels) {
    $index++
    $modelId = $m.id

    $payload = @{ model = $modelId } | ConvertTo-Json -Compress
    try {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $res = Invoke-RestMethod -Uri "$BaseUrl/v1/system/plan" -Method Post -Body $payload -ContentType "application/json" -TimeoutSec 15 -ErrorAction Stop
        $sw.Stop()

        if ($res.status -eq "success") {
            if ($null -eq $HardwareSnapshot) {
                $HardwareSnapshot = $res.hardware_info
                $ReportLines.Add("Probed GPU   : $($HardwareSnapshot.gpu.model) (Total VRAM: $($HardwareSnapshot.gpu.total_vram_gb) GB | Initial Free: $($HardwareSnapshot.gpu.free_vram_gb) GB)")
                $ReportLines.Add("Probed CPU   : $($HardwareSnapshot.cpu.model) ($($HardwareSnapshot.cpu.cores) Cores / $($HardwareSnapshot.cpu.threads) Threads)")
                $ReportLines.Add("Probed RAM   : $($HardwareSnapshot.ram.free_ram_gb) GB Free / $($HardwareSnapshot.ram.total_ram_gb) GB Total")
                $ReportLines.Add("PCIe Highway : $($HardwareSnapshot.gpu_pcie_bus.bus_type) @ $($HardwareSnapshot.gpu_pcie_bus.transfer_speed_gbps) GB/s (Latency: $($HardwareSnapshot.gpu_pcie_bus.bus_latency_us)us)")
                $ReportLines.Add("---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------")
            }

            $plan = $res.execution_plan
            $set = $res.active_user_settings
            $hw = $res.hardware_info
            $mem = $plan.model_memory_breakdown
            $rates = $plan.compute_capability_rates
            
            $gpuLayers = [int]$plan.gpu_layers
            $ramLayers = [int]$plan.ram_layers
            $ssdLayers = [int]$plan.ssd_layers
            $totalLayers = $gpuLayers + $ramLayers + $ssdLayers

            $modelInVramGb = [Math]::Round([double]$mem.model_in_vram_gb, 2)
            $modelInRamGb  = [Math]::Round([double]$mem.model_in_ram_gb, 2)

            $fileSizeBytes = 0
            if ($m.files -and $m.files.Count -gt 0) {
                $primary = $m.files | Where-Object { $_.is_primary } | Select-Object -First 1
                if ($primary) { $fileSizeBytes = $primary.size_bytes } else { $fileSizeBytes = $m.files[0].size_bytes }
            }
            $fileSizeGb = if ($fileSizeBytes -gt 0) { [Math]::Round(($fileSizeBytes / 1073741824.0), 2) } else { 0.0 }
            $totalModelGb  = if ($fileSizeGb -gt 0) { $fileSizeGb } else { [Math]::Round(($modelInVramGb + $modelInRamGb), 2) }

            $kvCacheGb     = [Math]::Round([double]$mem.kv_cache_memory_gb, 2)
            $vramBufGb     = [Math]::Round([double]$set.vram_buffer_gb, 2)
            $ramBufGb      = [Math]::Round([double]$set.ram_buffer_gb, 2)
            $freeVramLeft  = [Math]::Round([double]$mem.remaining_free_vram_gb, 2)
            $freeRamLeft   = [Math]::Round([double]$mem.remaining_free_ram_gb, 2)
            $kvLocation    = "$($plan.kv_cache_location)".ToUpper()

            # Exact field as it comes directly from the JSON, ensuring gpu_only is gpu and infeasible is none
            $ramComputeTarget = if ($plan.ram_layers_compute_target -and $plan.ram_layers_compute_target -ne "none") {
                "$($plan.ram_layers_compute_target)"
            } elseif ($plan.placement_mode -eq "gpu_only") {
                "gpu"
            } else {
                "none"
            }

            $item = [PSCustomObject]@{
                ModelId           = $modelId
                Status            = "SUCCESS"
                LatencyMs         = $sw.ElapsedMilliseconds
                PlacementMode     = "$($plan.placement_mode)".ToUpper()
                TotalModelGb      = $totalModelGb
                GpuLayers         = $gpuLayers
                RamLayers         = $ramLayers
                TotalLayers       = $totalLayers
                ActiveContext     = [int]$set.n_ctx
                KvLocation        = $kvLocation
                KvCacheGb         = $kvCacheGb
                ModelVramGb       = $modelInVramGb
                ModelRamGb       = $modelInRamGb
                VramBufferGb      = $vramBufGb
                RamBufferGb       = $ramBufGb
                FreeVramLeftGb    = $freeVramLeft
                FreeRamLeftGb     = $freeRamLeft
                ComputeWinner     = "$($rates.compute_winner)"
                RamComputeTarget  = $ramComputeTarget
            }
            $AuditResults += $item

            # Complete Full Card in Box Format (Matching Cluaiz Resource Execution Plan)
            $modelInfo = $res.model_info
            $candidates = $res.candidates
            $archDesc = "$($modelInfo.architecture) (Layers: $($modelInfo.total_layers), Heads: $($modelInfo.attention_heads), KV Heads: $($modelInfo.kv_heads), Head Dim: $($modelInfo.head_dim))"
            $inputsDesc = "$($modelInfo.inputs -join ', ')"
            $attachDesc = "$($modelInfo.supported_attachments -join ', ')"

            $cardLines = @(
                "╔═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗",
                "║                                       CLUAIZ RESOURCE EXECUTION PLAN : $modelId",
                "╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣",
                "║ [MODEL IDENTITY & SPECIFICATIONS]",
                "║   • Model ID             : $modelId",
                "║   • Architecture         : $archDesc",
                "║   • Model File Size      : $($item.TotalModelGb) GB (On-Disk GGUF Binary)",
                "║   • Inputs / Attachments : Inputs: [$inputsDesc] | Attachments: [$attachDesc]",
                "╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣",
                "║ [TOKEN CONTEXT CAPACITY]",
                "║   • Native Model Limit   : $($modelInfo.max_input_context) tokens (Architecture Ceiling)",
                "║   • Active Context Window: $($item.ActiveContext) tokens (Allocated Working Context)",
                "║   • Max Output Context   : $($modelInfo.max_output_context) tokens",
                "╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣",
                "║ [HARDWARE CAPACITY & BUFFERS]",
                "║   • GPU Target           : $($hw.gpu.model) (Total: $($hw.gpu.total_vram_gb) GB, Usable: $($hw.gpu.usable_vram_gb) GB, Safety Buffer: $($item.VramBufferGb) GB)",
                "║   • Host System RAM      : Total: $($hw.ram.total_ram_gb) GB, Free: $($hw.ram.free_ram_gb) GB, Usable: $($hw.ram.usable_ram_gb) GB, Safety Buffer: $($item.RamBufferGb) GB",
                "║   • PCIe Highway         : $($hw.gpu_pcie_bus.bus_type) @ $($hw.gpu_pcie_bus.transfer_speed_gbps) GB/s (Latency: $($hw.gpu_pcie_bus.bus_latency_us)us)",
                "╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣",
                "║ [LAYER BREAKDOWN & PLACEMENT DECISION]",
                "║   • Selected Mode        : $($item.PlacementMode)",
                "║   • GPU Offload Layers   : $($item.GpuLayers) / $($item.TotalLayers) Layers",
                "║   • Host RAM / CPU Layers: $($item.RamLayers) / $($item.TotalLayers) Layers",
                "║   • SSD Disk Layers      : $ssdLayers Layers (Zero Disk Thrashing)",
                "║   • KV Cache Location    : $($item.KvLocation) ($($item.KvCacheGb) GB)",
                "║   • Compute Target       : $($item.RamComputeTarget)",
                "║   • Memory Allocation    : VRAM Model: $($item.ModelVramGb) GB | RAM Model: $($item.ModelRamGb) GB | KV: $($item.KvCacheGb) GB",
                "║   • Remaining Memory     : Free VRAM Left: $($item.FreeVramLeftGb) GB | Free RAM Left: $($item.FreeRamLeftGb) GB",
                "║   • Compute Winner       : $($item.ComputeWinner) ($($rates.summary))",
                "╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣",
                "║ [CANDIDATE TIERS EVALUATION]"
            )

            if ($candidates -and $candidates.Count -gt 0) {
                foreach ($c in $candidates) {
                    $statusStr = if ($c.feasible) { "FEASIBLE" } else { "REJECTED" }
                    $cardLines += "║   • $($c.tier,-14) : [$statusStr] TPS: $($c.estimated_tps) | VRAM Req: $($c.vram_required_gb) GB | Reason: $($c.reason)"
                }
            } else {
                $cardLines += "║   • No tier candidates reported in execution plan."
            }

            $cardLines += "╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝"
            $cardLines += ""

            # Print Card to Console
            foreach ($cline in $cardLines) {
                if ($cline -match "Selected Mode\s+:\s+GPU_ONLY" -or $cline -match "\[FEASIBLE\]") {
                    Write-Host $cline -ForegroundColor Green
                } elseif ($cline -match "Selected Mode\s+:\s+HYBRID") {
                    Write-Host $cline -ForegroundColor Yellow
                } elseif ($cline -match "\[REJECTED\]" -or $cline -match "Selected Mode\s+:\s+INFEASIBLE") {
                    Write-Host $cline -ForegroundColor DarkGray
                } elseif ($cline -match "║ \[") {
                    Write-Host $cline -ForegroundColor Cyan
                } else {
                    Write-Host $cline -ForegroundColor White
                }
                $ReportLines.Add($cline)
            }

        } else {
            Write-Host "[$index/$($chatModels.Count)] ❌ FAILED: $modelId ($($res.message))" -ForegroundColor Red
        }
    } catch {
        Write-Host "[$index/$($chatModels.Count)] ❌ ERROR: $modelId ($($_.Exception.Message))" -ForegroundColor Red
    }
}

# 4. Master Wide Comparison Table (100% Direct JSON Data)
$tableHeader = "{0,-32} | {1,-7} | {2,-10} | {3,-5} | {4,-5} | {5,-5} | {6,-7} | {7,-7} | {8,-7} | {9,-8} | {10,-8} | {11,-8} | {12,-9} | {13,-25}" -f `
    "CHAT MODEL ID", "SIZE", "MODE", "GPU_L", "RAM_L", "TOT_L", "CTX", "CTX_LOC", "KV_MEM", "MOD_VRAM", "MOD_RAM", "VRAM_BUF", "VRAM_LEFT", "RAM_LAYERS_COMPUTE_TARGET"

$ReportLines.Add("==================================================================================================================================================================================================")
$ReportLines.Add("                                                                MASTER CHAT MODELS COMPARISON TABLE                                                               ")
$ReportLines.Add("==================================================================================================================================================================================================")
$ReportLines.Add($tableHeader)
$ReportLines.Add("--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------")

foreach ($r in $AuditResults) {
    $line = "{0,-32} | {1,-7} | {2,-10} | {3,-5} | {4,-5} | {5,-5} | {6,-7} | {7,-7} | {8,-7} | {9,-8} | {10,-8} | {11,-8} | {12,-9} | {13,-25}" -f `
        $r.ModelId.PadRight(32).Substring(0, [Math]::Min(32, $r.ModelId.Length)), `
        ("$($r.TotalModelGb)GB"), `
        $r.PlacementMode, `
        $r.GpuLayers, `
        $r.RamLayers, `
        $r.TotalLayers, `
        $r.ActiveContext, `
        $r.KvLocation, `
        ("$($r.KvCacheGb)GB"), `
        ("$($r.ModelVramGb)GB"), `
        ("$($r.ModelRamGb)GB"), `
        ("$($r.VramBufferGb)GB"), `
        ("$($r.FreeVramLeftGb)GB"), `
        $r.RamComputeTarget
    $ReportLines.Add($line)
}

$ReportLines.Add("==================================================================================================================================================================================================")
$ReportLines.Add("Total Chat Models Evaluated: $($AuditResults.Count) | Direct 1:1 Mapping From /v1/system/plan Live Probing")
$ReportLines.Add("==================================================================================================================================================================================================")

# Write Final Table to Console
Write-Host ""
Write-Host "==================================================================================================================================================================================================" -ForegroundColor DarkCyan
Write-Host "                                                                MASTER CHAT MODELS COMPARISON TABLE                                                               " -ForegroundColor Cyan
Write-Host "==================================================================================================================================================================================================" -ForegroundColor DarkCyan
Write-Host $tableHeader -ForegroundColor White
Write-Host "--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------" -ForegroundColor DarkCyan

foreach ($r in $AuditResults) {
    $line = "{0,-32} | {1,-7} | {2,-10} | {3,-5} | {4,-5} | {5,-5} | {6,-7} | {7,-7} | {8,-7} | {9,-8} | {10,-8} | {11,-8} | {12,-9} | {13,-25}" -f `
        $r.ModelId.PadRight(32).Substring(0, [Math]::Min(32, $r.ModelId.Length)), `
        ("$($r.TotalModelGb)GB"), `
        $r.PlacementMode, `
        $r.GpuLayers, `
        $r.RamLayers, `
        $r.TotalLayers, `
        $r.ActiveContext, `
        $r.KvLocation, `
        ("$($r.KvCacheGb)GB"), `
        ("$($r.ModelVramGb)GB"), `
        ("$($r.ModelRamGb)GB"), `
        ("$($r.VramBufferGb)GB"), `
        ("$($r.FreeVramLeftGb)GB"), `
        $r.RamComputeTarget

    if ($r.PlacementMode -eq "GPU_ONLY") {
        Write-Host $line -ForegroundColor Green
    } elseif ($r.PlacementMode -eq "HYBRID") {
        Write-Host $line -ForegroundColor Yellow
    } else {
        Write-Host $line -ForegroundColor Red
    }
}
Write-Host "==================================================================================================================================================================================================" -ForegroundColor DarkCyan

# 5. Save to Persistent .txt File
try {
    $parentDir = Split-Path -Path $OutputFile -Parent
    if (-not [string]::IsNullOrWhiteSpace($parentDir) -and -not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
    }
    $ReportLines | Out-File -FilePath $OutputFile -Encoding utf8
    Write-Host ""
    Write-Host "✅ Full audit report and master table saved to: ${OutputFile}" -ForegroundColor Green
} catch {
    Write-Host "⚠️ Failed to write audit file to ${OutputFile}: $($_.Exception.Message)" -ForegroundColor Yellow
}
