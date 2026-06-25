<#
.SYNOPSIS
Runs act to simulate GitHub Actions locally via Docker.
#>
Set-Location -Path "$PSScriptRoot\..\.."

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "🐳 Running Act (Docker CI Simulation)" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")

if (Get-Command act -ErrorAction SilentlyContinue) {
    Write-Host "Ensure Docker Desktop is running!" -ForegroundColor Yellow
    act push -P ubuntu-latest=catthehacker/ubuntu:act-latest -P ubuntu-22.04=catthehacker/ubuntu:act-22.04 -P ubuntu-20.04=catthehacker/ubuntu:act-20.04 -P windows-latest=-self-hosted -P macos-latest=-self-hosted
} else {
    Write-Host "`n❌ 'act' is not installed or not in PATH." -ForegroundColor Red
}
