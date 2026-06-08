# integratedeck

Cross-platform desktop app bridging **Elgato Stream Deck plugins** and **Bitfocus Companion modules** on diverse control surfaces — without marketplace accounts.

## Architecture

- **Rust / Tauri 2** — UI shell, Stream Deck WebSocket broker, profiles, orchestration
- **Node sidecar** — Companion module/surface host protocol (`sidecar/`)
- **Crates** — `ideck-core`, `ideck-surface`, `ideck-sd-host`, `ideck-comp-host`, `ideck-bridge`

## Development

```bash
npm install
npm run sidecar:build
npm run tauri dev
```

### Plugin directories

Data lives under the OS app data folder (`integratedeck/`):

- `plugins/streamdeck/` — `.sdPlugin` bundles
- `plugins/companion/` — `companion-module-*` folders
- `profiles/default.json` — active profile

## MVP flow

1. Start app → mock 3×5 surface registered
2. Place a `.sdPlugin` in `plugins/streamdeck/`, scan and load
3. Create slots on the grid, bind SD action UUIDs, trigger keyDown
4. `ideck-bridge` maps `setImage` / `setTitle` to surface cell updates

## License

MIT — respect Elgato Stream Deck SDK and Bitfocus Companion module licenses when distributing.
