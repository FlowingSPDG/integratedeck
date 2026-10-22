# スモークテスト（CDP 経由）

実機をつないだ状態で、主要な経路をまとめて確認する手順です。アプリを
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` で起動し、
メイン画面（`http://tauri.localhost/` または開発時の `http://127.0.0.1:1420/`）の
DevTools コンソールで実行します。

```js
const invoke = window.__TAURI_INTERNALS__.invoke
const st = await invoke('get_runtime_status')
const studio = st.surfaces.find((s) => s.kind === 'studio-usb')
const plus = st.surfaces.find((s) => s.kind === 'plus')
const p = await invoke('get_profile')
const page = p.pages.find((x) => x.id === p.active_page_id)
const cells = async (id) => Object.keys(await invoke('get_cell_visuals', { surfaceId: id })).length
```

## 期待値

| # | 操作 | 期待 |
| --- | --- | --- |
| 1 | 起動直後 | Studio 32 セル / Plus 8 セル、ページ 7 枚 |
| 2 | Companion プリセットのキーを `trigger_slot` | vMix が反応（例: active が切り替わる） |
| 3 | `set_surface_page` で Folder ページへ | セル 0（空ページ）、UI のブレッドクラムも追従 |
| 4 | Page 1 に戻す | セル 32 |
| 5 | Plus のプラグインダイヤルを `simulate_encoder_rotate` | `encoders['2'].settings.value` が増減 |
| 6 | Stream Deck プラグインのキーを `key_down`/`key_up` | プラグインが反応（ログに `keyDown -> plugin`） |
| 7 | 終了時 | セル 32/8 のまま、ページ数が変わらない |

## 実機でしか確認できない項目

- Studio のダイヤル LED（赤/緑）が見えるか
- Plus のタッチストリップを左右スワイプしてページが変わるか
- Studio に NFC タグをかざし、設定 → Devices の「更新」で UID が出るか

## 参考

同じ手順は `docs/verification.md` の各機能の行と対応しています。API の詳細は
`src-tauri/src/commands.rs` を参照してください。
