/**
 * integratedeck WebView host shim for Stream Deck HTML/browser plugins.
 *
 * Tauri injects this script into the `plugin-host` WebviewWindow before any page
 * script runs. The plugin host passes its configuration through the `__ideckSd`
 * query parameter (JSON, percent-encoded), so the shim also works after reloads
 * and navigations.
 *
 * The Stream Deck application calls `connectElgatoStreamDeckSocket(port, uuid,
 * registerEvent, info, actionInfo)` after the plugin page has loaded. This shim
 * does the same:
 *
 *  1. If the page ships its own `connectElgatoStreamDeckSocket` (the common
 *     `libs/js/stream-deck.js` setup), call it and let the page's own WebSocket
 *     talk to the broker. Incoming events then flow through the page's handlers.
 *  2. If the page has no such function, install a fallback bridge that connects
 *     to the broker, sends `registerPlugin` and forwards incoming messages to
 *     `window.__sdOnMessage`, `window` `message` listeners and
 *     `integratedeck:sd-message` listeners.
 */
;(function () {
  'use strict'

  var CONFIG_KEY = '__ideckSd'

  function safeParse(text, fallback) {
    try {
      return JSON.parse(text)
    } catch (_err) {
      return fallback
    }
  }

  function readConfig() {
    var params
    try {
      params = new URLSearchParams(window.location.search)
    } catch (_err) {
      return null
    }

    var raw = params.get(CONFIG_KEY)
    if (!raw) return null

    var config = safeParse(raw, null)
    if (!config || !config.port || !config.pluginUUID) return null

    config.port = Number(config.port) || 0
    if (!config.port) return null
    config.registerEvent = config.registerEvent || 'registerPlugin'
    config.info = config.info || '{}'
    config.actionInfo = config.actionInfo || '{}'
    return config
  }

  var config = readConfig()
  if (!config) return

  window.__ideckSd = config

  var installed = false

  /**
   * Forwards a broker message to the page without a page-provided WebSocket.
   */
  function forwardMessage(data) {
    var text = typeof data === 'string' ? data : String(data)

    try {
      if (typeof window.__sdOnMessage === 'function') {
        window.__sdOnMessage(text)
      }
    } catch (err) {
      console.error('[integratedeck] __sdOnMessage threw', err)
    }

    try {
      window.dispatchEvent(new MessageEvent('message', { data: text }))
    } catch (err) {
      console.error('[integratedeck] message dispatch failed', err)
    }

    try {
      window.dispatchEvent(
        new CustomEvent('integratedeck:sd-message', { detail: text }),
      )
    } catch (err) {
      console.error('[integratedeck] sd-message dispatch failed', err)
    }
  }

  /**
   * Fallback bridge used when the page does not define
   * `connectElgatoStreamDeckSocket` itself.
   */
  function fallbackConnect(port, uuid, registerEvent, info, actionInfo) {
    var socket = new WebSocket('ws://127.0.0.1:' + port)
    var connection = {
      websocket: socket,
      port: port,
      uuid: uuid,
      messageType: registerEvent,
      appInfo: safeParse(info, {}),
      actionInfo: safeParse(actionInfo, {}),
      send: function (message) {
        if (socket.readyState !== WebSocket.OPEN) return false
        socket.send(typeof message === 'string' ? message : JSON.stringify(message))
        return true
      },
    }

    socket.addEventListener('open', function () {
      socket.send(
        JSON.stringify({
          event: registerEvent || 'registerPlugin',
          uuid: uuid,
        }),
      )
    })

    socket.addEventListener('message', function (event) {
      forwardMessage(event.data)
    })

    socket.addEventListener('error', function (event) {
      console.error('[integratedeck] plugin WebSocket error', event)
    })

    window.__sdWs = socket
    window.__sdBridge = connection
    return connection
  }

  function install() {
    if (installed) return
    installed = true

    var port = String(config.port)
    var uuid = config.pluginUUID
    var registerEvent = config.registerEvent
    var info = config.info
    var actionInfo = config.actionInfo

    var pageConnect = window.connectElgatoStreamDeckSocket
    if (typeof pageConnect === 'function' && pageConnect !== fallbackConnect) {
      try {
        pageConnect.call(window, port, uuid, registerEvent, info, actionInfo)
        return
      } catch (err) {
        console.error(
          '[integratedeck] connectElgatoStreamDeckSocket failed; using fallback bridge',
          err,
        )
      }
    }

    if (typeof window.connectElgatoStreamDeckSocket !== 'function') {
      window.connectElgatoStreamDeckSocket = fallbackConnect
    }

    try {
      fallbackConnect(port, uuid, registerEvent, info, actionInfo)
    } catch (err) {
      console.error('[integratedeck] fallback WebSocket bridge failed', err)
    }
  }

  if (document.readyState === 'complete') {
    window.setTimeout(install, 0)
  } else {
    window.addEventListener(
      'load',
      function () {
        window.setTimeout(install, 0)
      },
      { once: true },
    )
  }
})()
