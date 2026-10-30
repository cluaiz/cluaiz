param([string]$Source, [string]$Url, [string]$Destination, [switch]$Live)
$ErrorActionPreference = 'Stop'
$text = [IO.File]::ReadAllText($Source, [Text.Encoding]::UTF8)
# Load the actual download/progress functions, never the installation entry point.
. ([ScriptBlock]::Create($text.Substring(0, $text.IndexOf('# --- Mascot'))))
$script:InteractiveProgress = [bool]$Live
Download-File $Url $Destination 'Fixture archive'
