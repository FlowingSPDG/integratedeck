# scripts

Build helper scripts. See `docs/verification.md` for runtime checks.

## fetch-node.ps1

Downloads a Node 20 LTS runtime and copies it into
`src-tauri/resources/node/` so the packaged app can run Companion modules and
Node SDK plugins without a system Node install.

```powershell
pwsh scripts/fetch-node.ps1 -Version 20.18.1 -Platform win-x64
pwsh scripts/fetch-node.ps1 -Version 20.18.1 -Platform linux-x64
```

The bundled binary is not committed; run this before `npm run tauri build` on a
release machine.
