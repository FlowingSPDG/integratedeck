# integratedeck

Cross-platform desktop app bridging **Elgato Stream Deck plugins** and **Bitfocus Companion modules** on diverse control surfaces — without marketplace accounts.

## Architecture

- **Rust / Tauri 2** — Orchestrator hub, Stream Deck WebSocket broker, profiles, surface drivers
- **Embedded Boa** — `ideck-js-runtime` runs Companion modules and Stream Deck JS/HTML plugins in-process (no Node.js)
- **Crates** — `ideck-core`, `ideck-surface`, `ideck-js-runtime`, `ideck-sd-host`, `ideck-comp-host`, `ideck-bridge`

All routing flows: **physical device ↔ Orchestrator ↔ plugin system**. No independent sidecar process.

## Development

```bash
npm install
npm run tauri dev
```

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
