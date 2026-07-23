# Beta : enregistre l'hôte de native messaging Kyber pour Chrome et Edge.
# À relancer si l'extension est rechargée avec un ID différent, ou si le
# binaire kyber-native-host.exe est déplacé/rebuild.

param(
  [string]$ExtensionId = "ngekfdpkgjmdiepbiedbfnmkaglfoeih"
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$releaseExe = Join-Path $repoRoot "src-tauri\target\release\kyber-native-host.exe"
$debugExe   = Join-Path $repoRoot "src-tauri\target\debug\kyber-native-host.exe"

if (Test-Path $releaseExe) { $exePath = $releaseExe }
elseif (Test-Path $debugExe) { $exePath = $debugExe }
else {
  Write-Error "kyber-native-host.exe introuvable. Build d'abord : cd src-tauri && cargo build --bin kyber-native-host"
}

$hostName = "com.kyber_security.native_host"
$manifestDir = Join-Path $env:LOCALAPPDATA "Kyber"
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
$manifestPath = Join-Path $manifestDir "native-host-manifest.json"

$manifest = @{
  name = $hostName
  description = "Hote de native messaging Kyber (beta)"
  path = $exePath
  type = "stdio"
  allowed_origins = @("chrome-extension://$ExtensionId/")
} | ConvertTo-Json

[System.IO.File]::WriteAllText($manifestPath, $manifest, (New-Object System.Text.UTF8Encoding($false)))

foreach ($browserKey in @(
  "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName",
  "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
)) {
  New-Item -Path $browserKey -Force | Out-Null
  Set-ItemProperty -Path $browserKey -Name "(default)" -Value $manifestPath
}

Write-Output "OK — hote natif enregistre pour Chrome et Edge."
Write-Output "Binaire   : $exePath"
Write-Output "Manifeste : $manifestPath"
Write-Output "Extension ID attendu : $ExtensionId"
