$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..")
$SourceBinary = Join-Path $RepoRoot "target\release\sdl-service.exe"
$InstallDir = "C:\Program Files\HydroServerSDL"
$DataDir = "C:\ProgramData\HydroServerSDL"
$ServiceName = "HydroServerSDL"

if (-not (Test-Path $SourceBinary)) {
    throw "Missing release service binary at $SourceBinary. Build it first with: cargo build -p sdl-service --release"
}

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    sc.exe stop $ServiceName | Out-Null
    Start-Sleep -Seconds 2
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 2
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType Directory -Force -Path "$DataDir\logs" | Out-Null

Copy-Item -Path $SourceBinary -Destination "$InstallDir\sdl-service.exe" -Force

icacls $InstallDir /grant "NT AUTHORITY\LocalService:(OI)(CI)(RX)" /T /C | Out-Null
icacls $DataDir /grant "NT AUTHORITY\LocalService:(OI)(CI)(M)" /T /C | Out-Null

sc.exe create $ServiceName binPath= "`"$InstallDir\sdl-service.exe`"" start= auto DisplayName= "HydroServer Streaming Data Loader" | Out-Null
sc.exe failure $ServiceName reset= 86400 actions= restart/0/restart/0/restart/60000 | Out-Null
sc.exe config $ServiceName obj= "NT AUTHORITY\LocalService" password= "" | Out-Null
sc.exe start $ServiceName | Out-Null

Write-Host "Installed and started."
