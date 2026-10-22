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
| `deviceDidDisconnect` | 実装 | [x] | サーフェス切断時に全プラグインへ通知（`notify plugin: deviceDidDisconnect` をログで確認） |
| `unregisterEvent` | 実装 | [x] | プラグインの解除要求で `registered_events` から削除 |
| プラグイン異常終了の検出 | 実装 | [x] | ソケット切断を2秒待って判定。`SD plugin disconnected: dev.miseinul.atem` をログ確認 |
| プラグイン自動再起動 | 実装 | [x] | 終了検出後に1回だけ再起動（ATEM のPIDが 32952→32844 に変わり `status: running` へ復帰） |
| プラグインのプロセス後始末 | 実装 | [x] | 終了時に子プロセスをkill。ウィンドウを閉じた後、プラグインプロセスが 0 件であることを確認（以前は再起動ごとに残留し 78 個まで蓄積） |
| インストール済みプラグイン全数起動 | 実機 | [x] | 22本すべて `running`（Discord / Tutorial / Advanced HTTP / vMix / ATEM / Plus Dial / Cat Keys / Control Center / Hello World / Lights-Out / OBS Studio / Streamlabs / Volume Controller / V-160HD / V-60HD / Tomato / Spotify / Kairos / OBS WebSocket / Lhm / Keycreator / Navigation）。action 数は manifest と一致（Lhm と Keycreator は 0 個） |

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
| Companion サイドカーの終了処理 | 実装 | [x] | `shutdown_kills_the_sidecar_and_removes_its_script`（プロセス停止と一時スクリプト削除） |
| サイドカー一時スクリプトの掃除 | 実装 | [x] | 起動時に `%TEMP%/ideck-companion-sidecar-*.mjs` を一掃（317個→1個）。終了時も自前のファイルを削除 |
| サイドカー異常終了の自動再起動 | 実装 | [x] | チャンネル切断を検知して 2→30秒のバックオフで再生成し、接続とfeedbackを復元（`companion module engine stopped; restarting`） |
| サイドカー停止の検知方式 | 実装 | [x] | stdout のEOFで `alive=false` にし、ドレインが2秒ごとに監視。kill 後 `companion module engine stopped; restarting in 2s` → 再生成 → vMix が `status: ok`（214 actions）に復帰 |
| サイドカーの親プロセス監視 | 実装 | [x] | stdin EOF と ppid 監視で、強制終了時も数秒で終了（PID 20960 が消えることを確認） |
| プリセット定義の取得 | 実機 | [x] | vMix 751 件（うち 750 件が step 持ち） |
| プリセットのキー割当 | 実機 | [x] | サイドバーから D&D → `companion_preset` バインド、押下で `transitionMix` 実行（active 2→1） |
| Companionアクションの既定オプション補完 | 実機 | [x] | 空オプションで `transitionMix` を割当→`{mix:1,functionID:"Cut",duration:"1000",input:""}` が補完され、押下でカット実行（active 1→2） |
| Companionアクション名の表示 | 実機 | [x] | キー見出し・ダイヤル表示が `action_id` ではなく定義名「Transition - Transition Mix」になる（プリセットは従来どおり style.text） |
| Companionオプションのフォーム生成 | 実機 | [x] | モジュール定義から Mix / Select transition / Duration / Input を生成し、変更が即保存（`functionID: Fade`）。フィードバックは Input / Mix / 色2つ / Tally を生成 |
| JSONエディタの併設 | 実装 | [x] | 生成フォームの下に「JSON で編集」を折り畳みで残す |
| SDアクションのD&D（UI） | 実機 | [x] | サイドバーの Tutorial「Plugins」を Studio (1,10) へドラッグ→ `stream_deck` binding 生成、押下でセル画像が更新 |
| プリセットのダイヤル割当 | 実機 | [x] | ダイヤル欄のプリセット選択→「プリセット割当」→ 押下で down/up 実行（active 1→2） |
| プリセットのマルチアクション step | 実機 | [x] | `companion_preset` step を含む Multi Action の押下で `transitionMix` 実行（active 2→1） |
| プリセットの回転 step | ユニット | [x] | `module_fixture_roundtrip` が `rotate_left` phase の実行を検証（vMix は down/up のみのため実機では未確認） |
| プリセットの style 適用 | 実機 | [x] | `audio_audioToggle` でタイトルが style.text「Toggle Audio」に、背景色が style.bgcolor に（未指定時は従来色） |
| プリセットの feedback 自動登録 | 実機 | [x] | `feedbacks[0]` を `companion_feedbacks` に登録し `connection.setFeedback` を送信。`inputPreview` を当てるとセルが緑 (0,255,0) に |
| プリセット割当先のダイヤル表示 | 実装 | [x] | style.bgcolor を LED へ、style.text をストリップ/LCD へ反映。style は binding に保存し再起動後も復元（再起動後に `get_dial_visuals` が `title: "Cut"`） |
| スロット解除時の feedback 破棄 | ユニット | [x] | `unbind_slot` が `removeFeedback` を送りブリッジ登録も解除。`unregistering_a_cell_drops_every_index` |
| モジュール一覧の検出 | 実機 | [x] | npm名のモジュール（`studiocoast-vmix`）を検出し、接続追加のドロップダウンに表示 |
| 接続の追加/削除 | 実機 | [x] | 追加→`status: ok`→config fields 取得→削除 をIPC経由で確認（設定画面のCompanionタブと同一コマンド） |
| 接続設定の更新 | 実機 | [x] | `update_connection_config` 後も `status: ok`・actions 214 / presets 751 を維持 |
| プロファイル書き出し | 実機 | [x] | `export_profile` が 7ページ / 99スロットの JSON を返す |
| プロファイルのインポート | 実機 | [x] | ページ名を変えた JSON を import → 反映・32/8セル再描画・ダイヤルも 8→9 で動作（ルーティングとコンテキストを再構築） |
| インポート後の再同期 | 実装 | [x] | ルーティング表・ブリッジ・surface→page・可視状態をリセットして再ハイドレート |
| デバイスプリセット書き出し | 実機 | [x] | studio-usb のプリセットが32スロットの JSON として出力 |
| デバイスプリセットのインポート | 実機 | [x] | Plus のプリセットを export→import、8セル再描画とダイヤル 9→10 を確認（インポート後にダイヤル文脈も再登録） |
| 設定画面のタブ描画 | 実機 | [x] | General / Devices / Presets / Companion の4タブが実データで描画 |
| スタートアップ登録 | 実機 | [x] | 設定トグルで `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` に exe を登録／削除（`reg query` で確認）。起動時に保存値と同期し、外部で削除しても再登録される |
| スタートアップ状態の表示 | 実装 | [x] | `get_global_settings` は保存値ではなく OS の実状態（`is_enabled`）を返す |
| コマンドACLの整合性 | 実機 | [x] | `generate_handler!` の96コマンドと許可リストを突合。`check_startup_conflicts` / `open_companion_modules_folder` が未許可で機能停止していたのを修正 |
| 起動時競合チェック | 実機 | [x] | `check_startup_conflicts` が `{conflicts: []}` を返す（以前はACLエラーで常に失敗） |
| ログファイル出力 | 実装 | [x] | `%APPDATA%\integratedeck\logs\integratedeck.log` に出力。5MB超で `.log.1` へローテーション。設定に「ログフォルダを開く」ボタン |
| パニックのログ記録 | 実装 | [x] | パニックフックで `panic: ...` をログファイルへ記録。起動時に `version` と `exe` パスも出力 |
| Companionセルのサーフェス分離 | ユニット | [x] | `two_surfaces_can_share_a_cell_without_clobbering_each_other`（Studio(0,0) と Plus(0,0) が共存） |

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
| ダイヤル解除でコンテキスト解放 | 実機 | [x] | 解除後の回転が配送されない（dispatch が増えない）ことをログで確認 |
| ダイヤルfeedbackがキーを汚染しない | ユニット | [x] | `dial_feedback_does_not_render_to_key_cells` |
| 同一ページ共有時のダイヤル状態 | ユニット | [x] | `shared_dial_context_updates_every_surface` |
| ダイヤルbindingの共有範囲 | 仕様 | [x] | ダイヤルはページ単位（サーフェス非依存）。同じページを使う複数サーフェスでは同じ binding が動く（Plus実測: StudioのプリセットがPlus側ダイヤルにも出る） |
| ページ切替時のダイヤル解除 | 実機 | [x] | 空の Folder ページへ切替でダイヤル表示が消え（`get_dial_visuals` が空）、戻すと復帰（タイトル "13"）。古いコンテキストは破棄 |
| ページ切替時のダイヤル再描画 | 実装 | [x] | 再登録時に `willAppear` を送り、プラグインが値を再送するようにした |
| ページ切替時のキー解除 | 実機 | [x] | Page1 の (1,8) に Tutorial を割当→`keyDown -> plugin`、空ページへ切替後の同一セルは `keyDown not bound to a plugin`、戻すと再びプラグインへ |
| ページ切替時のCompanionフィードバック | 実機 | [x] | transitionMix スロットに inputPreview（preview=2）を付け、空ページへ往復しても bgcolor (0,255,0) を維持 |
| スナップショット貼付（SD binding） | 実機 | [x] | `apply_slot_snapshot` で Tutorial binding を貼付→押下が `keyDown -> plugin` に解決（コンテキストが再登録される） |
| 接続直後のCompanionステータス | 実機 | [x] | `connection.add` の直後に出る status イベントが購読前に失われ、再起動直後は状態が空だった。add成功時に `comp_status` を補完し、Settings の Companion タブに `✓ ok` が即表示される |
| プロファイル全体の書き出し/読み込み | 実機 | [x] | Settings > Presets から約157KBのJSONを書き出し、「上書きする」をチェックして読み込み→「プロファイルを復元しました」。ページ7・32/8セルを維持 |
| デバイスの切断／再接続 | 実機 | [x] | Settings > Devices の「切断」で Plus を解放（`available`）。自動接続が取り戻さないよう `disabled` を記録し、「接続」で復帰（8セル） |
| Companion変数の一覧 | 実機 | [x] | Settings > Companion に「変数」表を追加。「更新」で vMix の24変数（`$(connected_state) true` 等）を表示 |
| マルチアクションのステップ編集 | 実機 | [x] | ステップごとの待機ms入力と↑↓並べ替えを追加。2ステップで 750ms 設定＋並べ替えが `steps:[750,0]` として保存され、実行も確認 |
| NFCタグの表示 | 実機 | [x] | NFC入力（`04:12:AB:CD…`）を注入すると `get_last_nfc` がUIDを返し、Settings > Devices の「更新」で `04:12:AB:CD:EF:01:02` が表示される。物理タップは実機確認待ち |
| プロファイル切替 | 実機 | [x] | ツールバーの選択で Default（7ページ・32/8セル）⇄ 新規（1ページ・0セル）を切替。戻すと元のセル数に復帰し、サーフェスの重複もなし |
| プロファイル切替キー | 実機 | [x] | 組み込み「Switch Profile」を追加。割当先を選び、押下で Default→Target へ切替（切替後は戻して削除） |
| ダイヤルの組み込みアクション（回転） | 実機 | [x] | 回転では実行されなかった組み込みアクションを回転でも実行。Switch Page を割当てたダイヤルを回すと Page 1→Folder へ遷移 |
| ダイヤルのマルチアクション | 実機 | [x] | ダイヤル押下でマルチアクションを実行（プロファイル経由で設定）。Companion step を含む場合もカットが動作（active 1→2） |
| マルチアクションのダイヤル割当 | 実機 | [x] | キーで作ったマルチアクションを「選択中を割当」でダイヤルへコピー（`bind_encoder_multi_action`）。押下で実行（active 2→1） |
| ダイヤルの表示名 | 実機 | [x] | 組み込み（「Switch Page」）とマルチアクション（「Multi Action」）をダイヤルに割当てると、LCD/ストリップのラベルが設定される（解除で消える） |
| ページ削除時のプリセット同期 | 実機 | [x] | ページ削除時にデバイスプリセットからも該当ページを除去。再起動しても削除したページが復活しないことを確認 |
| メインウィンドウの起動失敗検出 | 実装 | [x] | WebView2のロックで「ウィンドウはできるがページが無い」状態を検出し、プロファイル退避→再試行（従来はヘッドレスで起動していた） |
| 起動ログのノイズ除去 | 実装 | [x] | デバイス二重接続の競合を警告ではなくdebugに、fontdbの不正フォント警告を抑制、favicon要求のエラーをインラインfaviconで解消 |
| プラグイン子プロセスの道連れ終了 | 実機 | [x] | Windows Job Object（KILL_ON_JOB_CLOSE）に各プラグインプロセスを割当。`Stop-Process -Force`（タスクマネージャ相当）でアプリを落としても atem/plugin/plus/http_client/ESDDiscord が0件 |
| タッチスワイプ入力 | 実機 | [x] | 内容のある2ページ目を作って注入すると、左スワイプで移動（1セル描画）→右スワイプで復帰。入力は `get_last_surface_input` にも記録される。物理ジェスチャ自体は実機確認待ち |
| ダイヤルの組み込みアクション（回転） | 実機 | [x] | 回転では実行されなかった組み込みアクションを回転でも実行するよう修正。Switch Page を割当てたダイヤルを回すと Page 1→Folder へ遷移 |
| プロファイル作成/名前変更/削除 | 実機 | [x] | Settings > General。新規作成→空の1ページ、インラインで名前変更、削除でDefaultへフォールバック。最後の1つは削除不可 |
| デバイスプリセットのプロファイル分離 | 実装 | [x] | `profiles/presets/<profileId>/` に保存。新規プロファイルが旧ページを継承しない |
| サーフェスIDの引き継ぎ | 実機 | [x] | 再起動でPlusのIDが変わっても、モック側のスロットを物理サーフェスへ移行して8セル復帰 |
| showAlert / showOk のフラッシュ | 実機 | [x] | 検証用の最小HTMLプラグイン（keyDown→showAlert→showOk）を作成して確認。`visual-updated` に赤のフラッシュ画像（984B）→復元→緑→復元の順で流れる |
| プラグインフォルダを開く | 実装 | [x] | Settings > General にボタン追加（`open_plugins_folder`）。SDプラグインの配置先を開く |
| Studio の切断制限 | 制限 | [x] | Studio はドライバ内部のタスクがハンドルを保持し、同一セッションでの再接続に失敗するため、切断操作を明示的に拒否（ケーブルを抜くかアプリ終了） |
| キー解決の診断ログ | 実装 | [x] | `keyDown -> plugin` / `keyDown not bound to a plugin` を debug で出力 |
| サーフェスごとのダイヤル数尊重 | 実装 | [x] | `surface_dial_count` で上限を設け、2ダイヤルのStudioに3つ目のゾーンを作らない（Studio: 0,1 / Plus: 0,1,2）。回転も 10→11 で動作 |
| 連続入力のスループット | 実機 | [x] | ダイヤル押下20連打=25ms、回転40ノッチ=42ms。実行中もプラグイン6本・2サーフェス接続を維持（Plus Dial の値は上限30でクランプ） |
| 接続時のダイヤルコンテキスト登録 | 実装 | [x] | サーフェス接続時に現在ページ＋アクティブページのダイヤルを登録（`registering page dial contexts`）。起動直後の回転が 6→7・タイトル "7" で動作 |
| プラグイン再起動後のダイヤル復帰 | 実機 | [x] | Plus Dial をkill → 自動再起動・再登録後、手動再バインド無しで回転が 7→8・タイトル "8" |
| ダイヤル LED | 実機 | [ ] | Studio 実機の目視が必要（赤/緑を送信済み） |
| HID 読み取り失敗からの復帰 | 実装 | [x] | 連続10回まで再試行。プラグイン起動時の `ERROR_OPERATION_ABORTED` で入力ループが止まる問題の対策 |
| Studio LCD にダイヤル値を表示 | 実機 | [x] | 回転でLCDが再描画（cache/lcd/*.jpg 更新、Page名+ダイヤル値+区切りを確認） |
| タッチストリップ文字のゾーン幅スケーリング | ユニット | [x] | 1424x280では拡大、800x100では従来サイズ |
| Plus タッチストリップ描画（800x100 / 4分割） | 実機 | [x] | `render_touch_strip` が `write_lcd_fill` へ5416バイト送信（ログ `touch strip rendered`） |
| ストリップ描画のタイトル/値/インジケータ | ユニット | [x] | `ideck-core::touch_strip` の描画テスト4件 |
| Plus タッチタップ→ダイヤル割当 | ユニット | [x] | `touch_strip_index`（800px を4分割）。実機タップは次段 |
| Plus タッチタップ→プラグイン（touchTap） | ユニット | [x] | `touch_tap_resolves_the_dial_under_the_finger`、`broker.touch_tap` が `tapPos`/`hold` を送信 |
| ダイヤルfeedbackの indicator / layout | ユニット | [x] | `dial_feedback_carries_indicator_and_layout` |
| Encoder JSON（layout / StackColor / TriggerDescription） | ユニット | [x] | `parses_encoder_layout_and_stack_color`、StackColorはダイヤルLEDへ反映 |
| Plus タッチスワイプ | 実機 | [x] | `TouchScreenSwipe` を `TouchSwipe` として受け、左右スワイプでページ移動。IPC注入で 32→0→32 セルと往復を確認（実機スワイプは未実施） |
| スワイプのページ選択 | ユニット | [x] | `swipe_walks_top_level_pages_in_both_directions`、`swipe_stays_inside_the_folder_of_the_current_page`、`swipe_skips_pages_without_content` |
| 空ページのスキップ | ユニット | [x] | スロットの無いページは飛ばす（全ページが空なら従来どおり循環）。空ページから開始した場合は最寄りの非空ページへ |
| Plus / Neo | 実機 | [x] | Plus key0のkey_down/up、ダイヤル回転→プラグイン配送、ストリップ描画を確認 |
| フォルダの遷移先変更 | 実機 | [x] | 既存フォルダに `set_switch_page_target` で別ページを指定→`childPageId` が更新され、押下でそのページへ遷移（32→1）、戻るキーで復帰（→32） |
| ページ削除時の参照解除 | 実機 | [x] | フォルダの子ページを削除すると、そのフォルダボタンの binding も解除される（`unbinding a button whose page was deleted`） |
| ページ名の変更 | 実機 | [x] | ブレッドクラムのダブルクリック／✎ でインライン編集。`rename_page` で保存（例: "Main Deck"）、空文字は「ページ名を入力してください」で拒否 |
| ページ名変更時のLCD再描画 | 実機 | [x] | 名前変更で Studio LCD のキャッシュが再生成（mtime とハッシュが変化）。ページ名が即座に反映される |
| ハードウェアのページ移動にUIが追従 | 実機 | [x] | `page-changed` イベントを追加。選択中サーフェスのページを切り替えるとブレッドクラムが「Page 1」→「Folder」→「Page 1」と追従 |
| 言語設定の適用範囲 | 仕様 | [x] | 言語設定は設定画面のみ（メイン画面は日本語固定）。ラベルを「設定画面の言語」に変更して誤解を解消 |
| 強制終了時のプラグイン残留 | 実機 | [x] | Job Object（KILL_ON_JOB_CLOSE）に子プロセスを割当。タスクマネージャ相当の強制終了後も ATEM/プラグインのプロセスが0件になることを確認 |
| マルチアクションのD&D作成 | 実機 | [x] | サイドバーのアクションをマルチアクション編集領域へドラッグ→step追加。Companion step を含む Multi Action を実行しカット（active 1→2） |
| リリースビルドの動作確認 | ビルド | [x] | `npm run tauri build -- --no-bundle` の exe を Vite 停止状態で起動 → `http://tauri.localhost/` から埋め込みアセットを読み込み、2サーフェス・32/8セル・Companion `ok`・6プラグイン・`rename_page` まで動作 |
| release ビルド方法の注意 | — | [x] | `cargo build --release` 単体では tauri-build が dev cfg を付け devUrl を読む exe になる。`npm run tauri build`（または CI の tauri-action）を使う |
| フォルダ割当時の子ページ自動作成 | 仕様 | [x] | `bind_slot_builtin(openchild)` は "Folder" 子ページを自動作成（Elgato同等）。遷移先は後から変更できる |
| Plus のキー画像が実機に出る | 実機 | [x] | 再起動後にスロットを持つサーフェスへ接続（8セル全て画像あり） |
| Studio LCD の復元 | 実機 | [x] | `studio LCD restored bytes=17367`、`cache/lcd/<surface>.jpg` を再送 |
| 重複サーフェスの自動削除 | 実機 | [x] | 接続時にスロット0の同名サーフェスを除去（Plus の重複が消え 1台に） |
| プロファイル保存の原子性 | ユニット | [x] | 一時ファイル＋rename。`write_atomic_replaces_the_file_and_leaves_no_temp` / `write_atomic_cleans_up_when_the_rename_fails` |
| 壊れたプロファイルの退避 | ユニット | [x] | 解析失敗時は `default.json.corrupt-<epoch>` として保持し、空プロファイルで起動（`keeps_the_unreadable_file_next_to_the_original`） |
| 壊れたプロファイルからの復旧 | 実機 | [x] | `default.json` を壊して起動 → 警告ログと `default.json.corrupt-<epoch>` を残して新規プロファイルで起動。復元後は32セル・ダイヤル表示も復帰 |
| WebView2プロファイルのロック自動復旧 | 実装 | [x] | 0x800700AA 検出時に `EBWebView` を退避して再試行。`recognises_the_locked_profile_error` / `keeps_only_the_three_newest_parked_profiles` |
| プラグイン用WebViewの遅延生成 | 実機 | [x] | 起動時の4枚一括生成をやめ、初回表示時に生成（起動ブロック104秒→実質0） |
| JSプラグインのNode互換シム | ユニット | [x] | events/ws をコンストラクタ化、https・http・net・process・os・stream・Buffer を追加（記録: docs/js-shims.md） |
| 起動順の最適化 | 実機 | [x] | 設定ウィンドウの生成を setup 完了後（main thread）へ遅延。起動〜デバイス接続が 115秒 → 約3秒（Plus 1.4s / Studio 2.6s、プロファイルを消した状態でも同じ） |
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
| 変数式 $(var) の展開（オプション値・フィードバック） | module_fixture_roundtrip で $(input_name) → Camera 2 を確認 |
| Companion プリセット定義の取得（2引数API） | vMix で 751件・うち750件が steps 付き |
| プリセットをダイヤルに割当・押下で実行 | `mix_1cut` → `transitionMix` 実行、vMix の active/preview が反転 |
| プリセットをキーへD&Dして実行 | サイドバーのプリセットをスロットへドラッグ→押下で active 2→1 |
| Multi Action の step にプリセット | preset step 経由で `transitionMix` 実行 |
| プリセットのローテート位相 | フィクスチャで `rotate_left` → アクション実行を確認 |
| フィードバックstyle内の変数展開 | フィクスチャの text が fb Camera 2 に解決することを確認 |
| マルチアクション内のCompanionアクション | 実機 | [x] | 空オプションで step 追加→既定値が補完され、Multi Action 経由でカット実行（active 2→1） |
| フィードバックの既定オプション補完 | 実機 | [x] | `inputPreview` を空オプションで付与→既定値（input=1, mix=1, fg, bg, tally）が入りセルが緑 (0,255,0) |
| タッチスワイプ（IPC注入） | Studio セルが 32→0→32 と往復、`touch swipe` ログ2件 |
| Studio LCD のダイヤル表示 | Page名 + ダイヤル値（12ノッチ→"12"） |
| Plus タッチストリップ描画 | 800x100 / 4ゾーンへ送信 |
| ダイヤル→SDプラグイン | dialRotate 1回配送、setTitle 反映 |

### 注意: Companion フィードバックのオプション形式

vMix のフィードバックは **モジュール固有のオプション形式**を要求する。
`inputPreview` は `{ mix: 1, input: "2", fg: 0xffffff, bg: 0x00ff00, tally: "" }` の形。
`fg`/`bg` は **数値**（`#00ff00` 文字列では空スタイルが返る）、`mix` も必須。
UI（SettingsEditor）から設定する場合はプリセット経由が確実。

## 既知の制限（意図的な非対応）

| 項目 | 状態 | 理由 |
|---|---|---|
| HTML プラグインのホスティング | 非対応 | ユーザー判断で対象外（Property Inspector の WebView は対応済み） |
| `switchToProfile` | 実機 | プラグインの要求で同名プロファイルへ切替（検証用プラグインで Default→Second を確認）。該当名が無ければ警告 |
| Stream Deck のプロファイル自動切替（アプリ連動） | 非対応 | 上記と同じ理由 |
| `applicationDidLaunch` / `applicationDidTerminate` | 非対応 | プロファイル自動切替のためのプロセス監視。単一プロファイルモデルでは用途がない |
| `systemDidWakeUp` | 非対応 | スリープ復帰イベントの購読が未実装（復帰後の HID 再試行は実装済み） |
| Studio USB の明るさ変更 | 非対応 | USB の WriteFile が明るさフレームを拒否するため。LAN 接続時のみ対応 |
| タッチスワイプのプラグイン配送 | 非対応 | Elgato もプラグインへ公開していない。アプリ内のページ移動に使用 |
| Companion ダイヤルへの「アクション」直接割当 | 拡張 | Companion 本家はプリセットのみ。integratedeck では回転ごとに実行する拡張も選べる |
| VSD2 `CreateVirtualDevice` | 未検証 | 公式アプリ側でのペアリング操作が必要 |
| Pi GPIO 出力 | 未検証 | Linux + `gpio` feature。実機（Raspberry Pi）での確認が未実施 |

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
