# Run a packaged executable without the build runner's developer DLL directories.
# Usage: powershell.exe -NoProfile -File run_clean_windows.ps1 <executable> [arguments...]
$ErrorActionPreference = 'Stop'
if ($args.Count -lt 1) { throw 'A packaged executable is required' }

$executable = (Resolve-Path -LiteralPath $args[0]).ProviderPath
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Packaged executable is not a file: $executable"
}
$engineArgs = @($args | Select-Object -Skip 1)

# The executable's folder is searched by the Windows loader. Do not let the repository's
# working directory, Git, CUDA, OpenSSL or compiler installations satisfy a missing DLL.
$env:Path = @([Environment]::SystemDirectory, $env:WINDIR) -join ';'
Set-Location -LiteralPath (Split-Path -Parent $executable)
& $executable @engineArgs
exit $LASTEXITCODE
