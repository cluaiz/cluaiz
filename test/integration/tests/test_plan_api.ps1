$regPath = "C:\Users\Aryan\.cluaiz\engine\config\model_registry.json"
$reg = Get-Content $regPath -Raw | ConvertFrom-Json
$models = @($reg.installed_models.PSObject.Properties | Where-Object { $_.Value.category -eq "chat" } | ForEach-Object { $_.Name })
$models += "qwen2.5-7b-instruct-q4_k_m"

$headers = @{
    "Content-Type"  = "application/json"
    "Authorization" = "Bearer sk-cluaiz-126dab8f03bd463e8825f3d08809e3cf"
}

$outFile = "C:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\integration\tests\live_model_plans_test_output.txt"
$results = @()
$modelRows = @()
$hwSnapshot = $null

foreach ($m in $models) {
    $body = @{ model = $m; batch_tokens = 1 } | ConvertTo-Json
    try {
        $res = Invoke-RestMethod -Uri "http://127.0.0.1:8080/v1/system/plan" -Method Post -Headers $headers -Body $body
        $results += "=== MODEL: $m ==="
        $results += ($res | ConvertTo-Json -Depth 10)

        if ($res.status -eq "success") {
            $mInfo = $res.model_info
            $plan = $res.execution_plan
            $hw = $res.hardware_info
            if (-not $hwSnapshot -and $hw) {
                $hwSnapshot = $hw
            }

            $modelRows += [PSCustomObject]@{
                Model          = $mInfo.model_id
                Arch           = $mInfo.architecture
                Size_GB        = $mInfo.model_size_gb
                Layers_Total   = $mInfo.total_layers
                Mode           = $plan.placement_mode
                GPU_Layers     = $plan.gpu_layers
                RAM_Layers     = $plan.ram_layers
                SSD_Layers     = $plan.ssd_layers
                KV_Location    = $plan.kv_cache_location
                RAM_Compute    = $plan.ram_layers_compute_target
                GPU_ms_L       = $plan.compute_capability_rates.gpu_speed_ms_per_layer
                CPU_ms_L       = $plan.compute_capability_rates.cpu_speed_ms_per_layer
                Power_Ratio    = "$($plan.compute_capability_rates.gpu_to_cpu_power_ratio)x"
                VRAM_Used_GB   = $plan.model_memory_breakdown.model_in_vram_gb
                RAM_Used_GB    = $plan.model_memory_breakdown.model_in_ram_gb
                KV_GB          = $plan.model_memory_breakdown.kv_cache_memory_gb
            }
        } else {
            $modelRows += [PSCustomObject]@{
                Model          = $m
                Arch           = "N/A"
                Size_GB        = 0.0
                Layers_Total   = 0
                Mode           = "ERROR"
                GPU_Layers     = 0
                RAM_Layers     = 0
                SSD_Layers     = 0
                KV_Location    = "None"
                RAM_Compute    = "None"
                GPU_ms_L       = 0.0
                CPU_ms_L       = 0.0
                Power_Ratio    = "0x"
                VRAM_Used_GB   = 0.0
                RAM_Used_GB    = 0.0
                KV_GB          = 0.0
            }
        }
    } catch {
        $results += "=== MODEL: $m (ERROR) ==="
        $results += $_.Exception.Message
        $modelRows += [PSCustomObject]@{
            Model          = $m
            Arch           = "HTTP ERROR"
            Size_GB        = 0.0
            Layers_Total   = 0
            Mode           = "FAILED"
            GPU_Layers     = 0
            RAM_Layers     = 0
            SSD_Layers     = 0
            KV_Location    = "None"
            RAM_Compute    = "None"
            GPU_ms_L       = 0.0
            CPU_ms_L       = 0.0
            Power_Ratio    = "0x"
            VRAM_Used_GB   = 0.0
            RAM_Used_GB    = 0.0
            KV_GB          = 0.0
        }
    }
}

$hwTableStr = ""
if ($hwSnapshot) {
    $hwObj = [PSCustomObject]@{
        CPU_Model            = $hwSnapshot.cpu.model
        CPU_Cores_Threads    = "$($hwSnapshot.cpu.cores)C / $($hwSnapshot.cpu.threads)T"
        CPU_Clock            = "$($hwSnapshot.cpu.clock_ghz) GHz"
        CPU_Peak_GFLOPs      = $hwSnapshot.cpu.peak_gflops
        CPU_Load_Pct         = "$($hwSnapshot.cpu.current_load_pct)%"
        RAM_Total_GB         = $hwSnapshot.ram.total_ram_gb
        RAM_Free_GB          = $hwSnapshot.ram.free_ram_gb
        RAM_Safety_Buffer_GB = $hwSnapshot.ram.safety_buffer_gb
        RAM_Usable_GB        = $hwSnapshot.ram.usable_ram_gb
        RAM_Read_Speed       = "$($hwSnapshot.ram.read_speed_gbps) GB/s"
        RAM_Access_Latency   = "$($hwSnapshot.ram.access_latency_ns) ns"
        GPU_Model            = $hwSnapshot.gpu.model
        GPU_Total_VRAM_GB    = $hwSnapshot.gpu.total_vram_gb
        GPU_Free_VRAM_GB     = $hwSnapshot.gpu.free_vram_gb
        GPU_Safety_Buffer_GB = $hwSnapshot.gpu.safety_buffer_gb
        GPU_Usable_VRAM_GB   = $hwSnapshot.gpu.usable_vram_gb
        GPU_VRAM_Speed       = "$($hwSnapshot.gpu.vram_speed_gbps) GB/s"
        GPU_Compute_Cap      = $hwSnapshot.gpu.compute_capability
        Workspace_Buffer_GB  = "$($hwSnapshot.workspace_buffer.compute_workspace_gb) GB ($($hwSnapshot.workspace_buffer.purpose))"
        Storage_SSD_Drive    = "$($hwSnapshot.storage_ssd.drive) ($($hwSnapshot.storage_ssd.drive_type))"
        Storage_SSD_Space    = "$($hwSnapshot.storage_ssd.free_storage_gb) GB Free / $($hwSnapshot.storage_ssd.total_storage_gb) GB Total"
        Storage_SSD_Health   = $hwSnapshot.storage_ssd.health_status
        PCIe_Bus_Type        = $hwSnapshot.gpu_pcie_bus.bus_type
        PCIe_Transfer_Speed  = "$($hwSnapshot.gpu_pcie_bus.transfer_speed_gbps) GB/s"
        PCIe_Bus_Latency     = "$($hwSnapshot.gpu_pcie_bus.bus_latency_us) us"
    }
    $hwTableStr = $hwObj | Format-List | Out-String
}

$modelTableStr = $modelRows | Format-Table -Property Model, Arch, Size_GB, Layers_Total, Mode, GPU_Layers, RAM_Layers, SSD_Layers, KV_Location, RAM_Compute, Power_Ratio, VRAM_Used_GB, RAM_Used_GB, KV_GB -AutoSize | Out-String

$fullSummary = @"
================================================================================
PART 1: REAL HARDWARE TELEMETRY & BUFFERS SNAPSHOT
================================================================================
$hwTableStr

================================================================================
PART 2: CLEAN MODEL PLACEMENT, COMPUTE RATES & MEMORY BREAKDOWN
================================================================================
$modelTableStr
"@

$results += "`n" + $fullSummary
$results | Out-File -FilePath $outFile -Encoding utf8

Write-Output $fullSummary
