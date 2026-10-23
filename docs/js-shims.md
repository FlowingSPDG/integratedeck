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

## 重要な訂正: NodeプラグインはBoaではなくNodeで動かす

`com.elgato.volume-controller` が Boa で起動できなかった本当の原因は、シムの不足ではなく
**起動先の誤判定**だった。Elgato のマニフェストは暗号化されており `Nodejs` ブロックが
読めないため、`bin/plugin.js` を持つ Node プラグインが「組み込みJSプラグイン」と
誤判定され、TCP もネイティブ拡張も無い Boa に送られていた。

`PluginLaunchStrategy::detect` は、マニフェストに `Nodejs` が無くても
`node_modules/` か `bin/plugin.js` を持つバンドルを **NodeScript** として扱うように変更。
実測: `node ...\bin/plugin.js -port 59508 -pluginUUID com.elgato.volume-controller
-registerEvent registerPlugin -info {...}` で起動し、アクション11件を登録。

したがってシムは「node_modules を持たない軽量JSプラグイン」用の保険であり、
TCP/ネイティブ拡張を使うプラグインは Node 側で動く。

## 既知の残件

- WebView（HTML）プラグインのホスティングは**対象外**とする方針（PI の WebView は必要）。
- 軽量JSプラグイン向けシムは未対応モジュール（crypto/net 等）で実行時エラーになる。
- `crypto` は本実装が必要（認証系モジュールはハッシュ/乱数を使う）。
- `net` は本実装が必要（TCP で機器と話すモジュールは WebSocket では代替できない）。
