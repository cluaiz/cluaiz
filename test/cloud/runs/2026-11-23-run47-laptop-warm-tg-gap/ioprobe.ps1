param([string]$Exe, [string[]]$Argv, [string]$Out)
$env:CLUAIZ_HOME = "$env:LOCALAPPDATA\cluaiz-test\warm-tg-gap-home"
$env:CLUAIZ_EXPERT_PREFETCH = 'off'
$p = Start-Process -FilePath $Exe -ArgumentList $Argv -NoNewWindow -PassThru -RedirectStandardOutput "$Out.md" -RedirectStandardError "$Out.log"
$null = $p.Handle
$lines = @()
while (-not $p.HasExited) {
  $w = Get-CimInstance Win32_Process -Filter "ProcessId=$($p.Id)" -ErrorAction SilentlyContinue
  if ($w) { $lines += "{0:HH:mm:ss} read_GB {1:n2} ops {2} ws_MB {3:n0} private_MB {4:n0}" -f (Get-Date), ($w.ReadTransferCount/1e9), $w.ReadOperationCount, ($w.WorkingSetSize/1MB), ($w.PrivatePageCount/1MB) }
  Start-Sleep 1
}
$lines | Set-Content "$Out.io.txt"
$lines | Select-Object -Last 3
