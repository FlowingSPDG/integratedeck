# Bundled Node runtime

Place a Node 20 LTS runtime here before packaging:

```text
resources/node/node.exe        (Windows)
resources/node/node            (Linux/macOS)
```

`src-tauri/src/lib.rs` sets `IDECK_NODE` to this path when the file exists.
Resolution order in `ideck-sd-host` / `ideck-comp-host` is:

1. `IDECK_NODE` environment variable
2. this bundled runtime
3. `node` from `PATH`

The binary itself is intentionally not committed; add it in the release build
step or install Node on the target machine as a fallback.
