# Compila el plugin y lo instala en Flow Launcher.
# Uso:  powershell -ExecutionPolicy Bypass -File instalar.ps1
#
# - Compila pixpin-lanzador.exe en release.
# - Lo copia con plugin.json e Images\ a %APPDATA%\FlowLauncher\Plugins\PixPin Max\,
#   quitando antes cualquier otra carpeta con el mismo ID (con dos, Flow no
#   carga ninguna).
# - Cierra Flow mientras copia (tiene el exe abierto), pone la palabra clave
#   «p» en sus ajustes y lo vuelve a abrir.

$ErrorActionPreference = 'Stop'
$repo = $PSScriptRoot
$pluginsFlow = Join-Path $env:APPDATA 'FlowLauncher\Plugins'
if (-not (Test-Path $pluginsFlow)) { throw "No encuentro Flow Launcher ($pluginsFlow)" }

$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Push-Location $repo
try { & cargo build --release -p pixpin-lanzador; if ($LASTEXITCODE -ne 0) { throw "cargo build fallo" } }
finally { Pop-Location }

$exe = Join-Path $repo 'target\release\pixpin-lanzador.exe'
$fuente = Join-Path $repo 'pixpin-lanzador\plugin'
$id = (Get-Content (Join-Path $fuente 'plugin.json') -Raw | ConvertFrom-Json).ID
$destino = Join-Path $pluginsFlow 'PixPin Max'

$flow = @(Get-Process Flow.Launcher -ErrorAction SilentlyContinue)
if ($flow) {
    $flow | ForEach-Object { $_.CloseMainWindow() | Out-Null }
    for ($i = 0; $i -lt 25 -and (Get-Process Flow.Launcher -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 200 }
    Get-Process Flow.Launcher -ErrorAction SilentlyContinue | Stop-Process -Force
}
Get-Process pixpin-lanzador -ErrorAction SilentlyContinue | Stop-Process -Force

# Otras carpetas con el mismo ID (una instalacion anterior con otro nombre).
Get-ChildItem $pluginsFlow -Directory | Where-Object {
    $pj = Join-Path $_.FullName 'plugin.json'
    $_.FullName -ne $destino -and (Test-Path $pj) -and
        ((Get-Content $pj -Raw | ConvertFrom-Json).ID -eq $id)
} | Remove-Item -Recurse -Force

New-Item -ItemType Directory -Force $destino | Out-Null
Copy-Item $exe $destino -Force
Copy-Item (Join-Path $fuente 'plugin.json') $destino -Force
Copy-Item (Join-Path $fuente 'Images') $destino -Recurse -Force
Write-Host "Plugin instalado: $destino (escribe p)"

& powershell -ExecutionPolicy Bypass -File (Join-Path $repo 'pixpin-lanzador\instalar-palabra-clave.ps1')
if ($LASTEXITCODE -ne 0) { Write-Warning "No se pudo poner la palabra clave p en Flow (salida $LASTEXITCODE)" }

$exeFlow = Join-Path $env:LOCALAPPDATA 'FlowLauncher\Flow.Launcher.exe'
if (Test-Path $exeFlow) { Start-Process $exeFlow; Write-Host "Flow Launcher abierto" }
