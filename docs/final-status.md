# 現状サマリ（自動検証の最終状態）

integratedeck は、Stream Deck 公式プラグインと Bitfocus Companion モジュールを
同じサーフェスで動かすデスクトップアプリです。このファイルは「どこまで検証済みで、
何が人の手を待っているか」を1枚にまとめたものです。

詳細な検証マトリクスは [`verification.md`](verification.md)、再現手順は
[`smoke.md`](smoke.md) を参照してください。

## 検証状況

- 検証マトリクス: **185 項目が確認済み**
- 未確認: **1 項目**（公式アプリ側の操作が必要）
  - VSD2 `CreateVirtualDevice`（公式アプリ側のペアリングが必要。
    `scripts/capture-vsd.ps1` で通信を採取できれば実装可能）
- 実機確認待ち: **Plus のタッチストリップの実スワイプ**
  （アプリ側は空ページも含めて巡回するよう修正済み。届かない場合は
  設定 → Devices →「最後の入力」で切り分けできます）
- 直近のスモークテスト（実機）: Studio 32セル / Plus 8セル / 7ページ /
  Companion `ok` / プリセット実行 / ページ遷移 / ダイヤル回転 すべて成功

## 人の手が必要な確認（3点）

1. **Studio のダイヤル LED** — 左が赤、右が緑に見えるか（アプリから送信中）
2. **Plus のタッチストリップのスワイプ** — 左右にスワイプしてページが変わるか
   （アプリ側の遷移処理は注入テストで確認済み。物理ジェスチャのみ未確認）
3. **Studio の NFC** — タグをかざし、設定 → Devices →「更新」で UID が出るか
   （NFC入力の取り込みと表示は注入テストで確認済み）

## 実装済みの範囲（要約）

- **サーフェス**: Stream Deck +（キー/ダイヤル/タッチ）、Studio（USB/LAN、LCD、
  NFC読み取り、エンコーダLED）、XL/Mk.2、VSD2 仮想サーフェス、Pi GPIO（`gpio` feature）
- **Stream Deck プラグイン**: ネイティブ / Node / HTML(WebView)、PI、ダイヤル
  フィードバック、クラッシュ復旧と自動再バインド、`showAlert`/`showOk`、
  `applicationDidLaunch`/`Terminate`、`systemDidWakeUp`、`switchToProfile`
- **Companion**: Node サイドカーの監督・再起動、アクション/フィードバック/変数/
  プリセット、オプションフォームの自動生成、既定値の補完
- **編集**: 複数プロファイル（作成/切替/名前変更/削除、キー・プラグイン・アプリ連動で切替）、
  ページ/フォルダ（作成/名前変更/削除）、マルチアクション（ステップ毎の待機・並べ替え）、
  D&D、コピー&ペースト、デバイスプリセット、プロファイル入出力
- **運用**: 原子的な保存、壊れたプロファイルの退避、ログ（ローテーション・パニック記録）、
  起動時の自己修復（WebView2 ロック）、プラグイン子プロセスの道連れ終了（Job Object）、
  スタートアップ登録、インストーラ（MSI/NSIS、Node 同梱）

## 既知の制限

| 項目 | 理由 |
|---|---|
| Studio USB の明るさ変更 | USB の WriteFile が明るさフレームを拒否するため（LAN のみ対応） |
| タッチスワイプのプラグイン配送 | Elgato もプラグインへ公開していない |
| VSD2 `CreateVirtualDevice` | 公式アプリ側でのペアリング操作が必要 |
| Pi GPIO | Raspberry Pi 実機での確認が未実施 |

## 動かし方

```bash
npm install
npm run tauri dev     # 開発
npm run tauri build   # インストーラ（target/release/bundle）
```

データは `%APPDATA%\integratedeck`（プロファイル、接続、プリセット、ログ）。
