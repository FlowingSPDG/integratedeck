# integratedeck

Cross-platform desktop app bridging **Elgato Stream Deck plugins** and **Bitfocus Companion modules** on diverse control surfaces — without marketplace accounts.

## Architecture

- **Rust / Tauri 2** — Orchestrator hub, Stream Deck WebSocket broker, profiles, surface drivers
- **`ideck-js-runtime` (Boa)** — runs Stream Deck JS/HTML plugins in-process
- **Node sidecar** — Companion modules run under Node so they get the real
  `@companion-module/base` and sockets; the app supervises the process, restarts
  it after a crash and stops it on exit
- **Crates** — `ideck-core`, `ideck-surface`, `ideck-js-runtime`, `ideck-sd-host`, `ideck-comp-host`, `ideck-bridge`

All routing flows: **physical device ↔ Orchestrator ↔ plugin system**.

## Development

```bash
npm install
npm run tauri dev
```

### Build

```bash
npm run tauri build -- --no-bundle   # target/release/integratedeck(.exe)
```

Use the Tauri CLI: a plain `cargo build --release` skips the frontend build and
makes tauri-build emit a **dev** binary that loads `devUrl`
(`http://127.0.0.1:1420`) instead of the embedded `dist/` assets.

### Plugin directories

Data lives under the OS app data folder (`integratedeck/`):

- `plugins/streamdeck/` — `.sdPlugin` bundles
- `plugins/companion/` — `companion-module-*` folders
- `profiles/default.json` — active profile
- `connections.json` — Companion connection records

## MVP flow

1. Start app → mock or physical surface registered
2. Place a `.sdPlugin` in `plugins/streamdeck/`, scan and load
3. Create slots on the grid, bind SD or Companion actions
4. Property Inspector HTML connects via PI WebSocket (`registerPropertyInspector`)
5. `ideck-bridge` maps plugin output to surface cell updates

## License

MIT — respect Elgato Stream Deck SDK and Bitfocus Companion module licenses when distributing.
