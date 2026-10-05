# Pone la palabra clave del plugin de PixPin Max en los ajustes de Flow Launcher.
#
# Flow guarda las palabras clave de cada plugin instalado en
# %APPDATA%\FlowLauncher\Settings\Settings.json
#   (PluginSettings.Plugins.<ID>.ActionKeywords)
# y ese valor manda sobre el de plugin.json: cambiar plugin.json no basta en
# un plugin que ya estaba instalado.
#
# - Flow tiene que estar cerrado (al salir reescribe Settings.json y se
#   comeria el cambio): si esta abierto, no se toca nada y se sale con 2.
# - Se lee y se comprueba el JSON; despues se cambia SOLO el texto de ese
#   ActionKeywords (el resto del fichero queda byte a byte), se vuelve a
#   comprobar y se escribe en UTF-8 sin BOM, con copia en Settings.json.pixpin.bak.
# - Si el plugin aun no esta en los ajustes, no hay nada que hacer (Flow
#   tomara las de plugin.json al instalarlo): sale con 0.
#
# Uso: powershell -ExecutionPolicy Bypass -File instalar-palabra-clave.ps1 [-Palabras p,P]
# Salida: 0 hecho (o nada que hacer), 2 Flow abierto, 3 ajustes rotos o no se pudo.

param(
    [string[]]$Palabras = @('p', 'P'),
    [string]$Ajustes = (Join-Path $env:APPDATA 'FlowLauncher\Settings\Settings.json'),
    # Solo para probarlo sobre una copia de los ajustes con Flow abierto.
    [switch]$SinMirarSiFlowEstaAbierto
)

$ErrorActionPreference = 'Stop'
$Id = '8ABF9A87927D461EA54F3F6082337EF2'

if (-not $SinMirarSiFlowEstaAbierto -and (Get-Process -Name 'Flow.Launcher' -ErrorAction SilentlyContinue)) {
    Write-Host 'Flow Launcher esta abierto: cierralo antes (reescribe sus ajustes al salir).'
    exit 2
}
if (-not (Test-Path -LiteralPath $Ajustes)) {
    Write-Host "No hay ajustes de Flow ($Ajustes): nada que hacer."
    exit 0
}

$utf8 = New-Object System.Text.UTF8Encoding($false)
$texto = [System.IO.File]::ReadAllText($Ajustes, $utf8)
if ($texto.Length -gt 0 -and $texto[0] -eq [char]0xFEFF) { $texto = $texto.Substring(1) }

try {
    $antes = $texto | ConvertFrom-Json
} catch {
    Write-Host "Settings.json no es JSON valido; no se toca: $($_.Exception.Message)"
    exit 3
}
$plugin = $antes.PluginSettings.Plugins.$Id
if ($null -eq $plugin) {
    Write-Host 'El plugin de PixPin aun no esta en los ajustes de Flow: tomara las de plugin.json.'
    exit 0
}
$ya = @($plugin.ActionKeywords)
if (($ya -join "`n") -ceq ($Palabras -join "`n")) {
    Write-Host "Ya estaba: $($Palabras -join ', ')"
    exit 0
}

# El objeto del plugin: desde "<ID>": { hasta su ActionKeywords (el objeto es
# plano salvo esa lista, asi que no hay llaves por medio).
$patron = '("' + $Id + '"\s*:\s*\{[^{}]*?"ActionKeywords"\s*:\s*)\[[^\]]*\]'
$coincidencias = [regex]::Matches($texto, $patron)
if ($coincidencias.Count -ne 1) {
    Write-Host "No encuentro una sola entrada ActionKeywords del plugin ($($coincidencias.Count)); no se toca."
    exit 3
}
$lista = '[' + (($Palabras | ForEach-Object { '"' + ($_ -replace '\\', '\\' -replace '"', '\"') + '"' }) -join ', ') + ']'
$m = $coincidencias[0]
$nuevo = $texto.Substring(0, $m.Index) + $m.Groups[1].Value + $lista + $texto.Substring($m.Index + $m.Length)

try {
    $despues = $nuevo | ConvertFrom-Json
} catch {
    Write-Host "El cambio dejaria el JSON roto; no se toca: $($_.Exception.Message)"
    exit 3
}
$quedan = @($despues.PluginSettings.Plugins.$Id.ActionKeywords)
if (($quedan -join "`n") -cne ($Palabras -join "`n")) {
    Write-Host 'El cambio no dio lo esperado; no se toca.'
    exit 3
}

Copy-Item -LiteralPath $Ajustes -Destination "$Ajustes.pixpin.bak" -Force
[System.IO.File]::WriteAllText($Ajustes, $nuevo, $utf8)
Write-Host "Palabra clave de PixPin Max en Flow: $($ya -join ', ') -> $($Palabras -join ', ')"
exit 0
