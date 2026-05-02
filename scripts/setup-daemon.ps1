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

Write-Host "Installing Netswitch Daemon from $BinarySource..." -ForegroundColor Cyan

# 1. Create directory
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir
}

# 2. Stop existing service if running
$ExistingService = Get-Service -Name "NetswitchDaemon" -ErrorAction SilentlyContinue
if ($ExistingService) {
    if ($ExistingService.Status -ne 'Stopped') {
        Write-Host "Stopping existing service..."
        Stop-Service -Name "NetswitchDaemon" -Force
    }
}

# 3. Copy binary
# Wait a moment for file handles to be released
Start-Sleep -Seconds 1
Copy-Item -Path $BinarySource -Destination $BinaryDest -Force

# 4. Create/Update Service
if (-not $ExistingService) {
    Write-Host "Creating new service..."
    New-Service -Name "NetswitchDaemon" `
                -BinaryPathName "`"$BinaryDest`"" `
                -DisplayName "Netswitch Network Monitor" `
                -Description "Monitors network interfaces and handles failover." `
                -StartupType Automatic
} else {
    Write-Host "Updating existing service configuration..."
    # Update binary path just in case it changed
    sc.exe config "NetswitchDaemon" binPath= "`"$BinaryDest`""
}

Write-Host "Starting service..."
Start-Service -Name "NetswitchDaemon"

Write-Host "Setup Complete! Netswitch Daemon is running as a service." -ForegroundColor Green
