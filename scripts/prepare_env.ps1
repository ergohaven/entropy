<#
Installs what a Windows host needs to build Entropy.

Native Windows builds produce the .exe only. Every package format
(MSI, deb, rpm, archlinux, AppImage) is built in the Linux toolchain
container, so Docker is part of the prerequisites here.

  -DryRun   print the commands instead of running them
#>
[CmdletBinding()]
param(
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

function Write-Step { param($Message) Write-Host "==> $Message" -ForegroundColor Cyan }
function Write-Warn { param($Message) Write-Host "!! $Message" -ForegroundColor Yellow }

function Invoke-Step {
    param([string]$Exe, [string[]]$Arguments)
    if ($DryRun) {
        Write-Host "  $Exe $($Arguments -join ' ')"
        return
    }
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe exited with $LASTEXITCODE" }
}

function Test-Command { param($Name) $null -ne (Get-Command $Name -ErrorAction SilentlyContinue) }

if (-not (Test-Command winget)) {
    Write-Warn 'winget not found. Install App Installer from the Microsoft Store, or install the tools below by hand:'
    Write-Warn '  Rust (https://rustup.rs), go-task (https://taskfile.dev), Docker Desktop (https://docker.com)'
    exit 1
}

# Rust needs the MSVC linker from the Visual Studio build tools; rustup prompts
# for it on first run, but installing it up front keeps `task build` unattended.
$packages = @(
    @{ Id = 'Rustlang.Rustup'; Command = 'rustup'; Why = 'Rust toolchain' },
    @{ Id = 'Microsoft.VisualStudio.2022.BuildTools'; Command = 'cl'; Why = 'MSVC linker for the native build' },
    @{ Id = 'Task.Task'; Command = 'task'; Why = 'task runner' },
    @{ Id = 'Docker.DockerDesktop'; Command = 'docker'; Why = 'Linux/Windows packaging container' }
)

foreach ($pkg in $packages) {
    if (Test-Command $pkg.Command) {
        Write-Step "$($pkg.Why): already installed"
        continue
    }
    Write-Step "Installing $($pkg.Id) — $($pkg.Why)"
    Invoke-Step 'winget' @('install', '--exact', '--id', $pkg.Id, '--accept-package-agreements', '--accept-source-agreements')
}

# Версия Rust берётся из .tool-versions, как на Linux и macOS: там её выбирает
# asdf, а здесь без явного выбора `cargo` собирал бы чем угодно — и «собралось
# локально» ничего не говорило бы о сборке релиза.
$toolVersions = Join-Path $PSScriptRoot '..\.tool-versions'
$rustLine = Select-String -Path $toolVersions -Pattern '^rust\s+(\S+)' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($rustLine) {
    $rustVersion = $rustLine.Matches[0].Groups[1].Value
    Write-Step "Pinned Rust toolchain: $rustVersion"
    if (Test-Command rustup) {
        Invoke-Step 'rustup' @('toolchain', 'install', $rustVersion)
        Invoke-Step 'rustup' @('override', 'set', $rustVersion, '--path', (Resolve-Path (Join-Path $PSScriptRoot '..')).Path)
    }
    else {
        Write-Warn "rustup is not on PATH yet — reopen the shell and rerun this script to select Rust $rustVersion"
    }
}
else {
    Write-Warn "no rust entry in .tool-versions; leaving the default toolchain in place"
}

if ($DryRun) { Write-Step 'dry run — nothing was installed' }

Write-Step 'Done. Native build: task build. Packages: task docker:dist (needs Docker running).'
