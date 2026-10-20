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
| Node SDK プラグイン起動 | 実機 | [x] | Elgato hello-world をロード、nodeプロセスが公式引数で起動 |
| Node 実プラグイン動作 | 実機 | [x] | ブローカー接続 Established、action `newstring` を取得 |
| Node plugin のタイトル反映 | 実機 | [x] | setTitle `English\n\nClick Me` → keyDown後 `On` をログ確認 |
| Node plugin の画像反映 | 実機 | [x] | cat-keys keyDown後に 69,896 byte のPNGへ更新 |
| Node plugin のsetState | 実機 | [x] | フィクスチャで setState(1) → state_index=1 |

## Companion

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| モジュール load / init / destroy | 実機 | [x] | vMix モジュール読込 |
| definitions（actions/feedbacks/variables/presets） | 実機 | [x] | 214 actions / 39 feedbacks |
| vMix実接続 | 実機 | [x] | vMix 4K 29.0.0.48 / TCP 8099 / status ok |
| vMix action実行 | 実機 | [x] | `transitionMix` 実行がエラーなし |
| vMix feedback表示 | 実機 | [x] | `inputPreview`(input=3) が bgcolor green でXLセルへ反映 |
| config fields / secrets | 実機 | [x] | vMix の host/tcpPort/checkbox/static-text を取得 |
| action options | ユニット | [x] | `options` が callback へ到達 |
| feedback instance（control 単位） | ユニット | [x] | `connection.setFeedback` |
| feedback options UI | ユニット | [x] | ConfigPanel の SettingsEditor |
| module saveConfig 永続化 | ユニット | [x] | `connection.config` で config+secrets 保存 |
| 接続状態表示 | 実機 | [x] | vMix 接続直後に `status: ok` |

## Studio

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| LAN 接続 / keepalive / 明るさ | 実機 | [x] | 192.168.3.48:5343 |
| LAN 自動再接続 | モック | [x] | `reconnects_after_server_close` |
| LAN キー画像（02 07） | 実機 | [x] | 実機がフレーム受理 |
| LAN キー / エンコーダ入力 | モック | [x] | `studio_lan.rs` integration test |
| LAN NFC（01 04） | ユニット | [x] | `studio_protocol` test |
| USB HID 列挙（0fd9:00aa） | ユニット | [x] | `studio_usb` ドライバ |
| USB 自動接続 | 実機 | [x] | `0fd9:00aa A9IJA53330GUMR` を検出、32スロット自動作成 |
| USB キー画像 / LED / LCD | 実機 | [x] | key0赤・LED緑・LCD赤を送信 |
| USB キー / エンコーダ / NFC 入力 | 実機 | [x] | Studio key0のkey_down/up、両エンコーダのrotate/pressを履歴で確認（NFCは未タッチ） |
| USB/LAN 重複排除 | ユニット | [x] | `scan_all` |
| LCD パネル画像（02 0c） | 実機 | [x] | `set_studio_lcd` が LAN Studio へ送信成功 |
| NFC UID 取得 | 実機 | [x] | `get_last_nfc` が null（未タッチ）を返す |

## サーフェス / ダイヤル

| 項目 | 種別 | 状態 | 根拠 |
|---|---|---|---|
| XL HID 画像 / 入力 | 実機 | [x] | CL18I1A02341 |
| サーフェス検出UI | 実機 | [x] | `scan_surface_devices` が XL + Studio LAN を返す |
| ダイヤル SD / Companion / Built-in | 実機 | [x] | LAN StudioでBuilt-inをbind→simulate press |
| ダイヤル → SDプラグイン（dialRotate） | 実機 | [x] | `dev.samwho.streamdeck.plus` をバインドし `simulate_encoder_rotate`→プラグインがsetTitle（1回のみ配送） |
| ダイヤル押下（dialDown/dialUp） | 実機 | [x] | `simulate_encoder` で dialDown/dialUp をログ確認 |
| ダイヤル設定（setSettings）の保存 | 実機 | [x] | 回転後に `settings.value` がエンコーダbindingへ保存 |
| ダイヤルbindingの再起動復元 | 実機 | [x] | 再起動＋再接続のみ（再バインド無し）で回転が配送、値が継続 |
| ダイヤルfeedbackがキーを汚染しない | ユニット | [x] | `dial_feedback_does_not_render_to_key_cells` |
| 同一ページ共有時のダイヤル状態 | ユニット | [x] | `shared_dial_context_updates_every_surface` |
| ダイヤル LED | 実機 | [ ] | Studio 実機の目視が必要 |
| Studio LCD にダイヤル値を表示 | 実機 | [x] | 回転でLCDが再描画（cache/lcd/*.jpg 更新、Page名+ダイヤル値+区切りを確認） |
| タッチストリップ文字のゾーン幅スケーリング | ユニット | [x] | 1424x280では拡大、800x100では従来サイズ |
| Plus タッチストリップ描画（800x100 / 4分割） | 実機 | [x] | `render_touch_strip` が `write_lcd_fill` へ5416バイト送信（ログ `touch strip rendered`） |
| ストリップ描画のタイトル/値/インジケータ | ユニット | [x] | `ideck-core::touch_strip` の描画テスト4件 |
| Plus タッチタップ→ダイヤル割当 | ユニット | [x] | `touch_strip_index`（800px を4分割）。実機タップは次段 |
| Plus タッチタップ→プラグイン（touchTap） | ユニット | [x] | `touch_tap_resolves_the_dial_under_the_finger`、`broker.touch_tap` が `tapPos`/`hold` を送信 |
| ダイヤルfeedbackの indicator / layout | ユニット | [x] | `dial_feedback_carries_indicator_and_layout` |
| Encoder JSON（layout / StackColor / TriggerDescription） | ユニット | [x] | `parses_encoder_layout_and_stack_color`、StackColorはダイヤルLEDへ反映 |
| Plus タッチスワイプ | 実装 | [ ] | `TouchScreenSwipe` は未処理（Elgato公式もプラグインへ非公開） |
| Plus / Neo | 実機 | [x] | Plus key0のkey_down/up、ダイヤル回転→プラグイン配送、ストリップ描画を確認 |
| Plus のキー画像が実機に出る | 実機 | [x] | 再起動後にスロットを持つサーフェスへ接続（8セル全て画像あり） |
| Studio LCD の復元 | 実機 | [x] | `studio LCD restored bytes=17367`、`cache/lcd/<surface>.jpg` を再送 |
| 重複サーフェスの自動削除 | 実機 | [x] | 接続時にスロット0の同名サーフェスを除去（Plus の重複が消え 1台に） |
| プラグイン用WebViewの遅延生成 | 実機 | [x] | 起動時の4枚一括生成をやめ、初回表示時に生成（起動ブロック104秒→実質0） |
| JSプラグインのNode互換シム | ユニット | [x] | events/ws をコンストラクタ化、https・http・net・process・os・stream・Buffer を追加（記録: docs/js-shims.md） |
| 起動順の最適化 | 実機 | [x] | 設定ウィンドウ生成前にオーケストレータ起動。起動〜デバイス接続が115秒→約36秒 |
| Studio の明るさ | 実装 | [x] | LAN のみ `brightness_packet` を送信（USB は WriteFile が拒否するため未対応） |
| プラグインの SVG setImage | ユニット | [x] | `plugin_payload_rasterizes_svg`（144px PNGへ変換） |

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

## 総合スモークテスト（2026-10-21 実機）

CDP（`--remote-debugging-port=9222`）経由でアプリのコマンドを叩いて確認した結果。

| 手順 | 結果 |
|---|---|
| 物理サーフェス接続 | streamdeck-hid + studio-usb の2台 |
| キー画像 | Studio 32/32、Plus は7/8（1つは未割当のため空白＝正常） |
| 設定ウィンドウを開く | ok |
| Companion 接続（vMix 29.0.0.48 / TCP 8099） | `status: ok` |
| Companion アクション実行（`transitionMix`） | エラーなし |
| Companion フィードバック（`inputPreview`, mix=1, input=2, bg=0x00ff00） | セルが **緑** に |
| 変数式 $(var) の展開（オプション値・フィードバック） | ユニット | [x] | module_fixture_roundtrip で $(input_name) → Camera 2 を確認 |
| フィードバックstyle内の変数展開 | ユニット | [x] | フィクスチャの text が fb Camera 2 に解決することを確認 |
| Studio LCD のダイヤル表示 | Page名 + ダイヤル値（12ノッチ→"12"） |
| Plus タッチストリップ描画 | 800x100 / 4ゾーンへ送信 |
| ダイヤル→SDプラグイン | dialRotate 1回配送、setTitle 反映 |

### 注意: Companion フィードバックのオプション形式

vMix のフィードバックは **モジュール固有のオプション形式**を要求する。
`inputPreview` は `{ mix: 1, input: "2", fg: 0xffffff, bg: 0x00ff00, tally: "" }` の形。
`fg`/`bg` は **数値**（`#00ff00` 文字列では空スタイルが返る）、`mix` も必須。
UI（SettingsEditor）から設定する場合はプリセット経由が確実。

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
