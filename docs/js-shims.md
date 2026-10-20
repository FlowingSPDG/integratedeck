# 埋め込みJSランタイムのNode互換シム

integratedeck は HTML プラグインを WebView で、Node SDK プラグインを Node で、
そして「JSのみで完結する軽量プラグイン」を組み込みJSエンジン（Boa）で動かす。
Boa には Node の標準モジュールが無いため、プラグインが読み込み時に `require` する
ものを中心にシムを用意している。

## 方針

- **読み込み時に落ちないこと**を最優先にする。プラグインは使わない機能でも
  トップレベルで `require` することが多く、`Cannot find module` で全体が起動しない。
- 実際に使われたときに初めて明確なエラーを出す（無言で失敗させない）。
- コンストラクタとして `extends` されるもの（`EventEmitter` / `WebSocket` /
  `stream.*`）は必ず「関数・クラス」として提供する。

## 実装済み（機能する）

| モジュール | 内容 |
|---|---|
| `node:path` | join/resolve/dirname/basename/extname/sep など |
| `node:url` | URL 変換の最小実装 |
| `node:fs` | existsSync / readFileSync（同期のみ） |
| `node:events` | `EventEmitter` クラス（`require('events')` はクラス自身を返し、`.EventEmitter` も持つ） |
| `node:process` | platform/arch/env/argv/cwd/nextTick ほか |
| `node:os` | platform/arch/homedir/tmpdir/EOL ほか |
| `node:stream` | Stream / Readable / Writable / Duplex / Transform（メモリ内のみ） |
| `ws` | グローバル WebSocket を `extends` 可能なクラスでラップ、`Server` も提供 |
| `Buffer` | `from/alloc/concat/isBuffer`、`toString('utf8'\|'hex'\|'base64')`、`slice/copy/equals` |
| `@companion-module/base` | モジュールAPIの最小シム |

## スタブ（読み込みは成功、使用時にエラー）

`net` / `tls` / `dns` / `crypto` / `child_process` / `zlib` / `buffer` /
`querystring` / `assert` / `readline` / `tty` / `string_decoder` / `timers` /
`http2` / `perf_hooks` / `worker_threads` / `async_hooks`

`crates/ideck-js-runtime/assets/shims/node_unsupported.js` が汎用スタブ。
呼ぶと `integratedeck: '<module>.<method>' is not supported` を投げる。

## 既知の残件

- `com.elgato.volume-controller` だけが `TypeError: superclass must be a constructor`
  で起動に失敗する（他3本は起動OK）。原因究明のため:
  - 失敗ログに `root=<module_root>` を追加済み（プラグイン特定は完了）
  - `require` を debug ログで出力（`RUST_LOG=ideck_js_runtime=debug`）
  - 直前に読まれるのは `bufferutil` / `utf-8-validate`（`ws` のオプション依存）
  - 次段: UIを介さず検証できるよう、ランタイムを直接叩く統合テスト
    （`crates/ideck-js-runtime/tests/`）を用意し、スタックトレースを取る
- `crypto` は本実装が必要（認証系モジュールはハッシュ/乱数を使う）。
- `net` は本実装が必要（TCP で機器と話すモジュールは WebSocket では代替できない）。
