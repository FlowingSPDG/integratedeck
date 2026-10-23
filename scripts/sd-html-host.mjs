#!/usr/bin/env node
/**
 * Loads an SD HTML plugin (CodePath: app.html) and registers via WebSocket.
 * Usage: node sd-html-host.mjs <htmlPath> <port> <pluginUUID> <infoJson>
 */
import { readFileSync, existsSync } from 'node:fs'
import { pathToFileURL } from 'node:url'
import { dirname } from 'node:path'

const [htmlPath, port, pluginUUID, infoJson] = process.argv.slice(2)
if (!htmlPath || !port || !pluginUUID) {
  console.error('usage: sd-html-host.mjs <html> <port> <uuid> <info>')
  process.exit(1)
}

const info = infoJson ?? '{}'

globalThis.connectElgatoStreamDeckSocket = (
  inPort,
  inUUID,
  inRegisterEvent,
  inInfo,
) => {
  const ws = new WebSocket(`ws://127.0.0.1:${inPort}`)
  ws.addEventListener('open', () => {
    ws.send(JSON.stringify({ event: inRegisterEvent, uuid: inUUID }))
  })
  ws.addEventListener('message', (ev) => {
    if (globalThis.__sdOnMessage) globalThis.__sdOnMessage(String(ev.data))
  })
  globalThis.__sdWs = ws
}

async function main() {
  const jsPath = htmlPath.replace(/\.html?$/i, '.js')
  if (existsSync(jsPath)) {
    await import(pathToFileURL(jsPath).href)
  } else {
    readFileSync(htmlPath, 'utf8')
  }
  connectElgatoStreamDeckSocket(port, pluginUUID, 'registerPlugin', info)
  setInterval(() => {}, 60_000)
}

main().catch((e) => {
  console.error(e)
  process.exit(1)
})
