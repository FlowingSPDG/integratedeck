/**
 * Stream Deck plugin bootstrap (embedded Boa, no Node.js).
 */

if (typeof globalThis.EventTarget === 'undefined') {
  globalThis.Event = class Event {
    constructor(type, init = {}) {
      this.type = String(type)
      this.bubbles = !!init.bubbles
      this.cancelable = !!init.cancelable
      this.data = init.data
    }
  }

  globalThis.EventTarget = class EventTarget {
    constructor() {
      this._listeners = new Map()
    }

    addEventListener(type, listener) {
      const key = String(type)
      if (!this._listeners.has(key)) this._listeners.set(key, [])
      this._listeners.get(key).push(listener)
    }

    removeEventListener(type, listener) {
      const key = String(type)
      const list = this._listeners.get(key)
      if (!list) return
      const idx = list.indexOf(listener)
      if (idx >= 0) list.splice(idx, 1)
    }

    dispatchEvent(event) {
      const key = String(event?.type ?? event)
      const list = [...(this._listeners.get(key) ?? [])]
      for (const listener of list) {
        listener.call(this, event)
      }
      return true
    }
  }
}

globalThis.module = { exports: {} }
globalThis.exports = globalThis.module.exports

globalThis.connectElgatoStreamDeckSocket = (
  inPort,
  inUUID,
  inRegisterEvent,
  inInfo,
) => {
  const ws = new WebSocket(`ws://127.0.0.1:${inPort}`)
  ws.addEventListener('open', () => {
    ws.send(
      JSON.stringify({
        event: inRegisterEvent,
        uuid: inUUID,
      }),
    )
  })
  ws.addEventListener('message', (ev) => {
    if (globalThis.__sdOnMessage) {
      globalThis.__sdOnMessage(String(ev.data))
    }
  })
  ws.addEventListener('error', (ev) => {
    console.error('Stream Deck plugin WebSocket error', ev)
  })
  globalThis.__sdWs = ws
}

function joinPath(base, rel) {
  const sep = base.includes('\\') ? '\\' : '/'
  const normalized = base.endsWith(sep) ? base : base + sep
  return (
    normalized +
    rel
      .replace(/^\.?\//, '')
      .replace(/\//g, sep)
      .replace(/\\/g, sep)
  )
}

function loadClassicScript(filePath) {
  const source = globalThis.__ideckFs.readFileSync(filePath)
  const fn = new Function(source)
  fn()
}

function loadHtmlScripts(htmlPath) {
  const html = globalThis.__ideckFs.readFileSync(htmlPath)
  const baseDir = htmlPath.replace(/[/\\][^/\\]*$/, '')
  const re = /<script\b[^>]*\bsrc=['"]([^'"]+)['"][^>]*>/gi
  let match
  while ((match = re.exec(html)) !== null) {
    const src = match[1]
    if (/^https?:\/\//i.test(src)) continue
    loadClassicScript(joinPath(baseDir, src))
  }
}

function buildArgv(entryPath, port, pluginUUID, registerEvent, info) {
  const infoStr = typeof info === 'string' ? info : JSON.stringify(info ?? {})
  return [
    'integratedeck',
    entryPath,
    '-port',
    String(port),
    '-pluginUUID',
    pluginUUID,
    '-registerEvent',
    registerEvent,
    '-info',
    infoStr,
  ]
}

globalThis.__ideckSdStart = async function __ideckSdStart(params) {
  const {
    mode,
    entryPath,
    pluginDir,
    port,
    pluginUUID,
    registerEvent,
    info,
  } = params

  const platform = pluginDir.includes('\\') ? 'win32' : 'darwin'
  globalThis.process = {
    argv: buildArgv(entryPath, port, pluginUUID, registerEvent, info),
    env: { NODE_ENV: 'production' },
    cwd: () => pluginDir,
    platform,
  }

  if (mode === 'html') {
    const jsPath = entryPath.replace(/\.html?$/i, '.js')
    if (globalThis.__ideckFs.existsSync(entryPath)) {
      loadHtmlScripts(entryPath)
    } else if (globalThis.__ideckFs.existsSync(jsPath)) {
      await import(globalThis.__ideckFs.toFileUrl(jsPath))
    } else {
      globalThis.__ideckFs.readFileSync(entryPath)
    }
    const infoStr = typeof info === 'string' ? info : JSON.stringify(info ?? {})
    globalThis.connectElgatoStreamDeckSocket(
      port,
      pluginUUID,
      registerEvent || 'registerPlugin',
      infoStr,
      '{}',
    )
  } else {
    await import(globalThis.__ideckFs.toFileUrl(entryPath))
  }

  setInterval(() => {}, 60_000)
  return { started: true, mode, entryPath }
}
