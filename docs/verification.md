# integratedeck 検証マトリクス

最終更新: 2026-10-20

実機・モック・ユニットの別を明記する。`[x]` はこのブランチで確認済み、
`[ ]` は未確認または未実装。

## Stream Deck 公式プラグイン

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| ネイティブプラグイン起動 | 実機 | [x] | ATEM プラグインで registerPlugin / keyDown |
| HTML プラグイン WebView | 実機 | [x] | tutorial / Advanced HTTP を読み込み |
| HTML 複数同時ホスト | 実機 | [x] | tutorial slot=0 + tomato slot=1 を同時接続 |
| Property Inspector 表示 | 実機 | [x] | Advanced HTTP の PI フィールド描画 |
| PI setSettings 永続化 | 実機 | [x] | `url_press` がスロット設定へ保存 |
| Node SDK プラグイン起動 | ユニット | [x] | `PluginLaunchStrategy::NodeScript` |
| Node 実プラグイン動作 | 実機 | [ ] | Node 製プラグイン未検証 |

## Companion

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| モジュール load / init / destroy | 実機 | [x] | vMix モジュール読込 |
| definitions（actions/feedbacks/variables/presets） | 実機 | [x] | 214 actions / 39 feedbacks |
| config fields / secrets | 実機 | [x] | vMix の host/tcpPort/checkbox/static-text を取得 |
| action options | ユニット | [x] | `options` が callback へ到達 |
| feedback instance（control 単位） | ユニット | [x] | `connection.setFeedback` |
| feedback options UI | ユニット | [x] | ConfigPanel の SettingsEditor |
| module saveConfig 永続化 | ユニット | [x] | `connection.config` で config+secrets 保存 |
| 接続状態表示 | 実機 | [x] | vMix 接続直後に `status: ok` |
| 実 vMix 接続・実行 | 実機 | [ ] | vMix 本体が必要 |

## Studio

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| LAN 接続 / keepalive / 明るさ | 実機 | [x] | 192.168.3.48:5343 |
| LAN 自動再接続 | モック | [x] | `reconnects_after_server_close` |
| LAN キー画像（02 07） | 実機 | [x] | 実機がフレーム受理 |
| LAN キー / エンコーダ入力 | モック | [x] | `studio_lan.rs` integration test |
| LAN NFC（01 04） | ユニット | [x] | `studio_protocol` test |
| USB HID 列挙（0fd9:00aa） | ユニット | [x] | `studio_usb` ドライバ |
| USB 自動再接続 | 実装 | [x] | 5秒間隔で再列挙 |
| USB キー画像 / 入力 / LED | 実機 | [ ] | Studio USB 未接続 |
| USB/LAN 重複排除 | ユニット | [x] | `scan_all` |
| LCD パネル画像（02 0c） | 実機 | [x] | `set_studio_lcd` が LAN Studio へ送信成功 |
| NFC UID 取得 | 実機 | [x] | `get_last_nfc` が null（未タッチ）を返す |

## サーフェス / ダイヤル

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| XL HID 画像 / 入力 | 実機 | [x] | CL18I1A02341 |
| サーフェス検出UI | 実機 | [x] | `scan_surface_devices` が XL + Studio LAN を返す |
| ダイヤル SD / Companion / Built-in | 実機 | [x] | LAN StudioでBuilt-inをbind→simulate press |
| ダイヤル LED | 実機 | [ ] | Studio 実機の目視が必要 |
| Plus / Neo | 実機 | [ ] | 未接続 |

## VSD2

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| Hello / 暗号化 Authenticate | 実機 | [x] | 公式アプリ 7.6.0 |
| VirtualDeviceList | 実機 | [x] | Stream Deck Mobile |
| CreateVirtualDevice | 実機 | [ ] | `Error{code:5}` でペアリング待ち |
| クライアントcrate統合 | ビルド | [x] | `crates/ideck-vsd` をworkspaceへ追加 |
| 自前 VSD2 サーバ | 実機 | [x] | `start_vsd_server` + 実クライアントで KeypadConfig 3x5 / Ok |
| ソフトウェアサーフェス接続 | 実機 | [x] | Create で `integratedeck` サーフェスを登録 |
| KeyEvent配送 | 実機 | [x] | `vsd key event surface_id=... index=0` をログ確認 |
| KeyEvent→スロット実行（VSD経由） | 実機 | [x] | VSDクライアントのKeyEventでFolderページへ遷移 |
| サーフェス解除 | 実機 | [x] | `unregister_surface` で重複Studio/VSDを整理 |
| ButtonImage配信 | 実機 | [x] | `ensure_surface_slots` 後に index 0/7/8/11... のJPEGを受信 |
| Pi GPIO | 実装 | [x] | `ideck-vsd-gpio`（Linux + `--features gpio`、実機未検証） |

## 再現コマンド

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npx tsc --noEmit
npm run build
npm run tauri build -- --no-bundle
```

2026-10-20 時点のローカル実行:
- `cargo test --workspace` 全通過
- `cargo clippy --workspace --all-targets -- -D warnings` パス
- `npx tsc --noEmit` パス
- `npm run build` パス
- `npm run tauri build -- --no-bundle` 成功
- `target/release/integratedeck.exe` 起動確認（main / settings / tutorial WebView）

モック:
```bash
cargo test -p ideck-surface --test studio_lan
cargo test -p ideck-comp-host --test module_fixture
cargo test -p ideck-sd-host
```

実機:
```powershell
cargo run -p ideck-surface --example studio_lan_probe -- 192.168.3.48:5343
cargo run -p ideck-surface --example studio_probe
```
