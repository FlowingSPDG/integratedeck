
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

## Official app probe (2026-10-23)

With the stock Stream Deck app running (its VSD server listens on 127.0.0.1:28198),
a fresh client gets:

- `HelloFromServer { client_needs_authentication: true, service_endpoints: [...] }`
- `Ok` after `Authenticate`
- `VirtualDeviceList` that already contains a paired device
  (`b75a7b09…`, "Stream Deck Mobile")
- `CreateVirtualDevice` still fails with `Error{code:5}` and **no pairing dialog
  appears** on the desktop, even with the app freshly started.

So the desktop app only trusts clients whose key was paired elsewhere (the mobile
app / account flow). Reproducing `CreateVirtualDevice` needs a capture of that
pairing handshake; the self-hosted VSD2 server in `ideck-vsd` does not have this
limitation and is verified end to end (see verification.md).