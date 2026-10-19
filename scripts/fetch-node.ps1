# Fetch a Node runtime for the integratedeck bundle
#
# Usage:
#   pwsh scripts/fetch-node.ps1 -Version 20.18.1 -Platform win-x64
#   pwsh scripts/fetch-node.ps1 -Version 20.18.1 -Platform linux-x64
#
# The binary is written to src-tauri/resources/node/{node.exe|node}, which the
# Tauri bundle includes and the app exposes through IDECK_NODE.

param(
    [string]$Version = "20.18.1",
    [ValidateSet("win-x64", "linux-x64", "osx-x64", "osx-arm64")]
    [string]$Platform = "win-x64",
    [string]$Destination = "$PSScriptRoot/../src-tauri/resources/node"
)

$ErrorActionPreference = "Stop"

$archiveName = switch ($Platform) {
    "win-x64" { "node-v$Version-win-x64.zip" }
    "linux-x64" { "node-v$Version-linux-x64.tar.xz" }
    "osx-x64" { "node-v$Version-darwin-x64.tar.gz" }
    "osx-arm64" { "node-v$Version-darwin-arm64.tar.gz" }
}
$url = "https://nodejs.org/dist/v$Version/$archiveName"
$temp = Join-Path ([System.IO.Path]::GetTempPath()) "ideck-node-$Version-$Platform"

Write-Host "Downloading $url"
New-Item -ItemType Directory -Force -Path $temp | Out-Null
$archive = Join-Path $temp $archiveName
Invoke-WebRequest -Uri $url -OutFile $archive

if ($archive -like "*.zip") {
    Expand-Archive -Path $archive -DestinationPath $temp -Force
} else {
    tar -xf $archive -C $temp
}

$root = Get-ChildItem $temp -Directory | Select-Object -First 1
$binary = switch ($Platform) {
    "win-x64" { Join-Path $root.FullName "node.exe" }
    default { Join-Path $root.FullName "bin/node" }
}

if (-not (Test-Path $binary)) {
    throw "node binary not found in $($root.FullName)"
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$target = Join-Path $Destination (Split-Path $binary -Leaf)
Copy-Item $binary $target -Force
Write-Host "Installed $target"
