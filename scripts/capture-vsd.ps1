# Capture the VSD2 websocket between the stock Stream Deck app and a client.
#
# The stock VSD server listens on 127.0.0.1:28198; pairing traffic is what we
# need to see to reproduce `CreateVirtualDevice` for a new client.
#
# Usage (PowerShell, from the repo root):
#   pwsh scripts/capture-vsd.ps1 -Seconds 90 -Output vsd-pairing.pcap
#
# Then, while it runs:
#   1. open the Stream Deck app
#   2. remove an existing virtual device (or use a phone that is not paired yet)
#   3. pair / add the device
#
# Analyse afterwards with:
#   tshark -r vsd-pairing.pcap -Y "tcp.port==28198" -T fields -e tcp.payload

param(
    [int]$Seconds = 60,
    [string]$Output = "vsd-pairing.pcap",
    [string]$Interface = "\Device\NPF_Loopback",
    [int]$Port = 28198
)

$ErrorActionPreference = "Stop"

$tsharkPath = (Get-Command tshark -ErrorAction SilentlyContinue).Source
if (-not $tsharkPath) {
    $fallback = "C:\Program Files\Wireshark\tshark.exe"
    if (-not (Test-Path $fallback)) {
        throw "tshark not found. Install Wireshark/Npcap and retry."
    }
    $tsharkPath = $fallback
}

Write-Host "Capturing $Port on $Interface for $Seconds s -> $Output"
& $tsharkPath -i $Interface -f "tcp port $Port" -w $Output -a "duration:$Seconds"
if (-not (Test-Path $Output)) {
    throw "capture failed"
}

$size = (Get-Item $Output).Length
Write-Host ("Capture written: {0} ({1} bytes)" -f $Output, $size)
if ($size -le 24) {
    Write-Host "No packets captured - is the Stream Deck app running and a client connecting?"
}
