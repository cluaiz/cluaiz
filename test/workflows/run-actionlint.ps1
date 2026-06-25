<#
.SYNOPSIS
Runs actionlint to statically check GitHub Actions workflows.
#>
Set-Location -Path "$PSScriptRoot\..\.."

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "[*] Running Actionlint (Syntax Check)" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")

if (Get-Command actionlint -ErrorAction SilentlyContinue) {
    actionlint
    if ($LASTEXITCODE -eq 0) {
        Write-Host "[OK] Actionlint passed! No syntax errors found." -ForegroundColor Green
    } else {
        Write-Host "[Error] Actionlint found errors. Please fix them above." -ForegroundColor Red
        exit $LASTEXITCODE
    }
} else {
    Write-Host "`n[Warning] actionlint is not installed or not in PATH." -ForegroundColor Yellow
}
