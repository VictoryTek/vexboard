# Installs VexBoard git hooks by copying scripts/hooks/ into .git/hooks/.
# Run once after cloning: pwsh scripts/install-hooks.ps1
# (Git for Windows runs hooks via Git Bash; copying is used instead of symlinks.)

$ErrorActionPreference = 'Stop'

$Root      = Split-Path $PSScriptRoot -Parent
$HooksSrc  = Join-Path $Root 'scripts\hooks'
$HooksDst  = Join-Path $Root '.git\hooks'

if (-not (Test-Path $HooksDst)) {
    Write-Error "ERROR: $HooksDst does not exist — are you in a git repository?"
    exit 1
}

Get-ChildItem $HooksSrc | ForEach-Object {
    $src    = $_.FullName
    $name   = $_.Name
    $target = Join-Path $HooksDst $name

    if (Test-Path $target) {
        Write-Host "Overwriting: .git\hooks\$name"
    }

    Copy-Item -Force $src $target
    Write-Host "Installed:  .git\hooks\$name <- $src"
}

Write-Host 'Done. Git hooks installed.'
