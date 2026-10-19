# WebView host for HTML / browser Stream Deck plugins

Some Stream Deck plugins are plain web apps: their manifest `CodePath` points at an
HTML file (for example `com.elgato.tutorial.sdPlugin` → `app.html`) and the page
expects `window`, the DOM and a real `WebSocket`. The embedded Boa engine cannot run
them (it fails with parse/runtime errors such as `must be declarative environment`),
so those plugins are hosted in a Tauri `WebviewWindow` instead.

Node/JS plugins (`CodePath` ending in `.js`/`.mjs`/`.cjs` or a manifest with a
`Nodejs` section) are untouched and still run in Boa via `launch/embedded.rs`.

## Flow

1. `load_sd_plugin` → `StreamDeckBroker::start` binds a loopback broker on an
   ephemeral port, loads `manifest.json` and calls `PluginProcess::spawn`.
2. `PluginLaunchStrategy::detect` returns `HtmlWebView { html }` for `.html`/`.htm`
   entry points. (Anything else keeps using `JsPlugin` / `NativeBinary`.)
3. `PluginProcess::spawn` asks the installed
   `ideck_sd_host::WebviewPluginHost` to open the page. No Boa engine is created.
   The request carries the plugin UUID/name, bundle dir, HTML path, broker port,
   `registerEvent`, the JSON `info` payload and `DefaultWindowSize`.
4. The Tauri implementation (`src-tauri/src/plugin_host.rs`):
   - allows the plugin bundle directory in the asset-protocol scope at runtime,
   - navigates the hidden `plugin-host` window to
     `http://asset.localhost/<encodeURIComponent(html path)>?__ideckSd=<config>`,
   - applies the manifest window size/title, then shows and focuses the window.
5. `src-tauri/assets/plugin_host_shim.js` is injected into every document of that
   window before page scripts run. It decodes `__ideckSd` and, once the page has
   loaded, calls `connectElgatoStreamDeckSocket(port, uuid, registerEvent, info,
   actionInfo)` — exactly what the official Stream Deck app does. The usual
   `libs/js/stream-deck.js` page then opens its own WebSocket to the broker and
   sends `registerPlugin`; incoming events reach the page's own handlers.
6. If the page never defines `connectElgatoStreamDeckSocket` (or it throws), the
   shim installs a fallback bridge: it opens the broker socket, sends
   `registerPlugin`, and forwards incoming messages to `window.__sdOnMessage`,
   `window` `message` listeners and `integratedeck:sd-message` listeners. The
   socket/bridge are exposed as `window.__sdWs` / `window.__sdBridge`.
7. Unloading the plugin drops the broker and `PluginProcess`, which calls
   `close()` on the host: the window is hidden and navigated to `about:blank` so
   the page, its socket and its timers go away.

The plugin page is an ordinary WebSocket client of the existing broker, so
`keyDown`/`keyUp`, `setTitle`, `setImage`, `sendToPropertyInspector`, etc. keep
working through the normal `BrokerEvent` pipeline. No IPC is required from the
plugin page.

## Files

| File | Role |
| --- | --- |
| `crates/ideck-sd-host/src/launch/webview.rs` | `WebviewPluginHost` trait, request type, process-wide host registry, `DefaultWindowSize` reader |
| `crates/ideck-sd-host/src/launch.rs` | launch strategy detection + re-exports |
| `crates/ideck-sd-host/src/plugin.rs` | routes `HtmlWebView` to the installed WebView host (Boa fallback when none is installed, e.g. headless tests) |
| `src-tauri/src/plugin_host.rs` | hidden window creation during `setup`, asset-scope + URL building, show/hide/focus, `focus_plugin_host_window` command |
| `src-tauri/assets/plugin_host_shim.js` | injected shim: Elgato-compatible connect call + fallback bridge |
| `src-tauri/src/lib.rs` | creates the window and installs the host before the orchestrator starts |
| `src-tauri/capabilities/plugin-host.json` | isolated capability for the `plugin-host` window (no app commands) |
| `src-tauri/permissions/app-commands.toml` | allows the main UI to call `focus_plugin_host_window` |

## Manual test (tutorial plugin)

The Elgato tutorial plugin is a browser plugin (`app.html`, 1320×816) and is a good
smoke test. It is picked up by the normal plugin scan because
`paths::sd_plugin_scan_roots` includes `%APPDATA%\Elgato\StreamDeck\Plugins`.

```powershell
cd C:\Users\Flowing\integratedeck
npm run tauri dev
```

Then, in the main window (debug builds have DevTools enabled — `Ctrl+Shift+I`):

1. Load the plugin through the UI: expand the `com.elgato.tutorial` group in the
   Actions sidebar and press **起動**, or run the same command from the DevTools
   console:

   ```js
   await window.__TAURI_INTERNALS__.invoke("load_sd_plugin", {
     path: "C:\\Users\\Flowing\\AppData\\Roaming\\Elgato\\StreamDeck\\Plugins\\com.elgato.tutorial.sdPlugin",
   });
   ```

2. Expected result: a window titled `com.elgato.tutorial — integratedeck` opens at
   1320×816 with the tutorial page. The log shows `plugin host window opened` and
   the broker receives `registerPlugin` (visible as a `PluginRegistered` event /
   the plugin list switching to running).

3. Optional end-to-end check without hardware: bind one of the tutorial actions to
   a slot and simulate a key, which makes the plugin call `window.open(...)`:

   ```js
   // slotId: any slot id from get_profile
   await window.__TAURI_INTERNALS__.invoke("bind_slot_sd", {
     args: { slotId: "<slot-id>", pluginUuid: "com.elgato.tutorial", actionUuid: "com.elgato.tutorial.plugins" },
   });
   await window.__TAURI_INTERNALS__.invoke("simulate_slot_key", {
     args: { slotId: "<slot-id>", phase: "key_down" },
   });
   await window.__TAURI_INTERNALS__.invoke("simulate_slot_key", {
     args: { slotId: "<slot-id>", phase: "key_up" },
   });
   ```

4. Stop it with **停止** (or `unload_sd_plugin`) — the window hides and the broker
   port is released. Use `focus_plugin_host_window` to reopen the window for a
   still-loaded plugin.

## Notes and limitations

- **One HTML plugin window.** The `plugin-host` window is created once (Windows
  WebView2 deadlocks when a webview is created from an IPC command, see
  `windows.rs`) and reused. Loading a second HTML plugin fails with
  `plugin host window is already hosting <uuid>` until the first one is unloaded.
- Closing the plugin window only hides it; the plugin keeps running. The app exits
  with the main window.
- The asset protocol scope is extended at runtime for the plugin bundle, so plugins
  installed outside `%APPDATA%` (e.g. a dev checkout under `plugins/streamdeck`) work
  too.
- The `plugin-host` window has its own capability with no `allow-app-commands`, so
  plugin JS cannot call integratedeck IPC commands.
- CSP in `tauri.conf.json` already allows `ws://127.0.0.1:*`, which is what the
  plugin page uses to reach the broker.
- If the log says `no WebView plugin host installed; running HTML plugin ... in Boa`,
  the host was not registered (or `ideck-sd-host` is used standalone/headless) and
  browser plugins will fail the old way.
- Blank/403 page: check the `asset scope ...` log line and that the HTML path from
  the manifest exists.
