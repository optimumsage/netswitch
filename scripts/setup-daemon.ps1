# Netswitch Daemon Setup Script (Windows)
# This script installs the daemon as a Windows service.

$AdminRole = [Security.Principal.WindowsBuiltInRole]::Administrator
$IsAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole($AdminRole)

if (-not $IsAdmin) {
    Write-Error "This script must be run as Administrator."
    exit 1
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# 1. Find the binary
if (Test-Path "$ScriptDir\netswitch-daemon.exe") {
    # Production bundle path
    $BinarySource = "$ScriptDir\netswitch-daemon.exe"
} else {
    # Development path
    $BinarySource = "$ScriptDir\..\target\release\netswitch-daemon.exe"
}

$InstallDir = "$env:ProgramFiles\Netswitch"
$BinaryDest = "$InstallDir\netswitch-daemon.exe"

if (-not (Test-Path $BinarySource)) {
    Write-Error "Daemon binary not found at $BinarySource."
    exit 1
}

Write-Host "🔧 Installing Netswitch Daemon from $BinarySource..." -ForegroundColor Cyan

# 1. Create directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir
}

# 2. Stop existing service if running
if (Get-Service -Name "NetswitchDaemon" -ErrorAction SilentlyContinue) {
    Stop-Service -Name "NetswitchDaemon" -Force
}

# 3. Copy binary
Copy-Item -Path $BinarySource -Destination $BinaryDest -Force

# 4. Create/Update Service
$Service = Get-Service -Name "NetswitchDaemon" -ErrorAction SilentlyContinue
if (-not $Service) {
    New-Service -Name "NetswitchDaemon" `
                -BinaryPathName "`"$BinaryDest`"" `
                -DisplayName "Netswitch Network Monitor" `
                -Description "Monitors network interfaces and handles failover." `
                -StartupType Automatic
}

Start-Service -Name "NetswitchDaemon"

Write-Host "🚀 Setup Complete! Netswitch Daemon is running as a service." -ForegroundColor Green
