# Stream Deck Virtual Stream Deck (VSD2) protocol notes

Findings from the installed Stream Deck app (7.6.0.23012):

- `ESDVSD2Server` listens on **TCP 28198** (all interfaces) and advertises via Bonjour.
- `VSD2/WiFi` / `VSD/WiFi` strings; protobuf C++ runtime is linked.
- The app currently logs `Virtual Stream Deck is not enabled`, so VSD2 only
  accepts clients after it is enabled in the desktop app settings.
- `workstationID` example: `P6vj3ekvOMbJUWPXRuVIIQ`.

## Protobuf message inventory (from binary strings)

```
vsd2.HelloFromServer   { workstationID, hostname, version, osName, osVersion }
vsd2.ClientInformation { name, userAgent, deviceModel, osName, osVersion }
vsd2.VirtualDevice     { id, name }
vsd2.KeypadConfig
vsd2.CreateVirtualDevice { name }
vsd2.OpenVirtualDevice   { deviceID }
vsd2.VirtualDeviceList
vsd2.VirtualDeviceOpened
vsd2.Context           { profileID }
vsd2.ContextUpdate     { profileID }
vsd2.Shortcut          { id, title, name, category }
vsd2.Shortcuts
vsd2.ShortcutsUpdate   { remove }
vsd2.Action            { title, name, category, shortcutID }
vsd2.TriggerShortcut   { deviceID, shortcutID }
vsd2.AssignShortcut    { profileID, shortcutID }
vsd2.Image             { ImageFormat, ImageType, Size, ... }
vsd2.Error             { reason }
vsd2.Ok
```

Transport framing is not yet confirmed (length-prefixed protobuf vs a stream
wrapper). Capture TCP 28198 with the mobile app to determine it.

## Button count

Physical devices have fixed key counts. VSD2 carries `KeypadConfig`,
`VirtualDevice`, and `CreateVirtualDevice`, so the **protocol is expected to
support a software-defined grid** rather than a hardware key count. The mobile
UI may still impose practical limits/pages; confirm with a capture and by
experimenting with different `KeypadConfig` values.

## Capture plan

1. Enable Virtual Stream Deck in the desktop app.
2. Put the phone and PC on the same LAN.
3. Capture:
   `tshark -i <iface> -f "tcp port 28198 or udp port 5353" -w vsd2.pcapng`
4. Connect the mobile app; identify Bonjour TXT, the handshake, and message order.
5. Decode protobuf with `protoc --decode_raw` or a Wireshark protobuf dissector.

## Implementation plan

- `prost` messages generated from the recovered `.proto` (field numbers from
  capture / descriptor extraction).
- VSD2 server in integratedeck: advertise Bonjour, accept `28198`, expose a
  configurable keypad grid, bridge shortcuts to the existing orchestrator.
- VSD2 client for software surfaces: Raspberry Pi GPIO → `TriggerShortcut`,
  and server → key images/feedback.
- Keep separate from Cora: VSD2 is desktop-app ↔ mobile/software surface over
  IP; Cora is host ↔ Stream Deck hardware.

## Captured handshake (mobile app 2.4.2, 2026-10-20)

Transport is **WebSocket** on TCP 28198 (not raw protobuf). The server
responds `101 Switching Protocols` with `Server: VSD2 Server`. Binary
WebSocket frames carry protobuf.

### Client -> server (first binary frame)

Outer message field **1** (length-delimited) contains:

```
field 1: bytes[32]                 # client id / nonce
field 2: ClientInformation {
  1: "iPhone"
  2: "Stream Deck/2.4.2"
  3: 2                             # device class?
  4: "iPhone15,4"
  5: "iOS"
  6: "26.6.1"
}
```

### Server -> client (first binary frame)

Outer message field **10** (HelloFromServer):

```
1: "P6vj3ekvOMbJUWPXRuVIIQ"        # workstationID
2: "DESKTOP-BUIITN0"               # hostname
3: "7.6.0.23012"                   # app version
4: 3                               # os type
5: bytes[32]                       # host id / nonce
7: "windows"
8: "11"
9: ServiceEndpoints {
  1: "192.168.3.44"
  1: "100.106.174.39"
  1: "172.24.16.1"
  1: "172.19.176.1"
  2: 28198
}
```

The oneof field numbers for the remaining messages (`VirtualDeviceList`,
`CreateVirtualDevice`, `KeypadConfig`, `Context`, `Image`, ...) still need a
capture while the mobile app opens a virtual device.

### Button count

`KeypadConfig` is a field of the virtual-device messages, so the grid is
software-defined. The mobile UI may impose practical limits; capture the
`CreateVirtualDevice` / `KeypadConfig` exchange to confirm the maximum rows and
columns.
