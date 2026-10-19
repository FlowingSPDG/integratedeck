
## Pairing status (2026-10-20)

A custom VSD2 client (`crates/ideck-vsd`) completes Hello and Authenticate against
the stock 7.6.0 app, receives `Ok` and `VirtualDeviceList`, but
`CreateVirtualDevice` returns `Error{code:5}` while `clientNeedsAuthentication`
is true. Holding the WebSocket open for 30s does not surface a desktop pairing
dialog; enumerating the Stream Deck windows shows only the main window and
Preferences.

So the stock app does not currently expose a pairing UI for this client. A
standalone VSD2 server (own device/grid) or an official pairing path is required
before virtual devices can be opened.
