# integratedeck 検証マトリクス

最終更新: 2026-10-20

実機・モック・ユニットの別を明記する。`[x]` はこのブランチで確認済み、
`[ ]` は未確認または未実装。

## Stream Deck 公式プラグイン

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| ネイティブプラグイン起動 | 実機 | [x] | ATEM プラグインで registerPlugin / keyDown |
| HTML プラグイン WebView | 実機 | [x] | tutorial / Advanced HTTP を読み込み |
| HTML 複数同時ホスト | ユニット | [x] | `plugin-host*` プール 4 面 |
| Property Inspector 表示 | 実機 | [x] | Advanced HTTP の PI フィールド描画 |
| PI setSettings 永続化 | 実機 | [x] | `url_press` がスロット設定へ保存 |
| Node SDK プラグイン起動 | ユニット | [x] | `PluginLaunchStrategy::NodeScript` |
| Node 実プラグイン動作 | 実機 | [ ] | Node 製プラグイン未検証 |

## Companion

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| モジュール load / init / destroy | 実機 | [x] | vMix モジュール読込 |
| definitions（actions/feedbacks/variables/presets） | 実機 | [x] | 214 actions / 39 feedbacks |
| config fields / secrets | ユニット | [x] | fixture + Settings UI |
| action options | ユニット | [x] | `options` が callback へ到達 |
| feedback instance（control 単位） | ユニット | [x] | `connection.setFeedback` |
| feedback options UI | ユニット | [x] | ConfigPanel の SettingsEditor |
| 実 vMix 接続・実行 | 実機 | [ ] | vMix 本体が必要 |

## Studio

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| LAN 接続 / keepalive / 明るさ | 実機 | [x] | 192.168.3.48:5343 |
| LAN キー画像（02 07） | 実機 | [x] | 実機がフレーム受理 |
| LAN キー / エンコーダ入力 | モック | [x] | `studio_lan.rs` integration test |
| LAN NFC（01 04） | ユニット | [x] | `studio_protocol` test |
| USB HID 列挙（0fd9:00aa） | ユニット | [x] | `studio_usb` ドライバ |
| USB キー画像 / 入力 / LED | 実機 | [ ] | Studio USB 未接続 |
| USB/LAN 重複排除 | ユニット | [x] | `scan_all` |
| LCD パネル画像（02 0c） | ユニット | [x] | `lcd_image_packets` |

## サーフェス / ダイヤル

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| XL HID 画像 / 入力 | 実機 | [x] | CL18I1A02341 |
| ダイヤル SD / Companion / Built-in | モック | [x] | bind + rotate/press |
| ダイヤル LED | 実機 | [ ] | Studio 実機の目視が必要 |
| Plus / Neo | 実機 | [ ] | 未接続 |

## VSD2

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| Hello / 暗号化 Authenticate | 実機 | [x] | 公式アプリ 7.6.0 |
| VirtualDeviceList | 実機 | [x] | Stream Deck Mobile |
| CreateVirtualDevice | 実機 | [ ] | `Error{code:5}` でペアリング待ち |
| 自前 VSD2 サーバ | 実装 | [ ] | 計画 Phase 5 |
| Pi GPIO | 実装 | [ ] | 計画 Phase 5 |

## 再現コマンド

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npx tsc --noEmit
npm run build
npm run tauri build -- --no-bundle
```

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
