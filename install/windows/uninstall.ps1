$ErrorActionPreference = "Continue"

$ServiceName = "HydroServerSDL"
$InstallDir = "C:\Program Files\HydroServerSDL"

sc.exe stop $ServiceName | Out-Null
Start-Sleep -Seconds 3
sc.exe delete $ServiceName | Out-Null
Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue

Write-Host "Uninstalled."
