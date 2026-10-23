/** Connect official SD Property Inspector HTML via PI WebSocket (registerPropertyInspector). */

import { convertFileSrc } from "@tauri-apps/api/core";

export interface PiContext {
  port: number;
  context: string;
  actionUuid: string;
  pluginUuid: string;
  deviceId?: string;
  settings?: Record<string, unknown>;
}

function deviceIdForPlugin(pluginUuid: string): string {
  return `integratedeck-virtual-${pluginUuid.replace(/\./g, "-")}`;
}

function buildPiInfo(ctx: PiContext) {
  const deviceId = ctx.deviceId ?? deviceIdForPlugin(ctx.pluginUuid);
  return {
    application: {
      font: "Arial",
      language: "en",
      platform: "windows",
      platformVersion: "10.0",
      version: "7.0.0",
    },
    plugin: { uuid: ctx.pluginUuid, version: "1.0.0" },
    devices: [
      {
        id: deviceId,
        name: "integratedeck",
        size: { columns: 5, rows: 3 },
        type: 0,
      },
    ],
    colors: {
      buttonMouseOverBackgroundColor: "#464646FF",
      buttonPressedBackgroundColor: "#303030FF",
      buttonPressedBorderColor: "#000000FF",
      buttonPressedTextColor: "#FFFFFFFF",
      highlightColor: "#0078FFFF",
    },
    devicePixelRatio: 1,
  };
}

function buildActionInfo(ctx: PiContext) {
  const deviceId = ctx.deviceId ?? deviceIdForPlugin(ctx.pluginUuid);
  return {
    action: ctx.actionUuid,
    context: ctx.context,
    device: deviceId,
    payload: {
      settings: ctx.settings ?? {},
      isInMultiAction: false,
      controller: "Keypad",
      resources: {},
    },
  };
}

function base64Json(obj: unknown): string {
  const str = JSON.stringify(obj);
  const bytes = new TextEncoder().encode(str);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

function piDirectoryAssetBase(piFilePath: string): string {
  const normalized = piFilePath.replace(/\\/g, "/");
  const slash = normalized.lastIndexOf("/");
  const dir = slash >= 0 ? normalized.slice(0, slash) : normalized;
  const base = convertFileSrc(dir);
  return base.endsWith("/") ? base : `${base}/`;
}

function buildPiQueryString(ctx: PiContext): string {
  return new URLSearchParams({
    port: String(ctx.port),
    propertyInspectorUUID: ctx.context,
    registerEvent: "registerPropertyInspector",
    info: base64Json(buildPiInfo(ctx)),
    actionInfo: base64Json(buildActionInfo(ctx)),
  }).toString();
}

/** Inject &lt;base&gt; and PI WebSocket bridge before plugin scripts in &lt;head&gt;. */
function injectPiDocumentPreamble(
  html: string,
  baseHref: string,
  queryString: string,
): string {
  const escapedQs = queryString.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
  const bridge = buildPiBridgeScript();
  const preamble =
    `<base href="${baseHref}">` +
    `<script>(function(){var s='?${escapedQs}';` +
    `try{Object.defineProperty(window.location,'search',{configurable:true,get:function(){return s;}});}` +
    `catch(e){}})();</script>` +
    `<script data-integratedeck-pi-bridge="1">${bridge}</script>`;

  if (/<head[^>]*>/i.test(html)) {
    return html.replace(/<head[^>]*>/i, (m) => m + preamble);
  }
  if (/<html[^>]*>/i.test(html)) {
    return html.replace(/<html[^>]*>/i, (m) => m + `<head>${preamble}</head>`);
  }
  return `<head>${preamble}</head>${html}`;
}

export function buildPiBridgeScript(): string {
  return `
(function() {
  if (window.connectElgatoStreamDeckSocket) return;
  window.connectElgatoStreamDeckSocket = function(inPort, inPropertyInspectorUUID, inRegisterEvent) {
    if (window.__sdpiWs) return;
    var ws = new WebSocket('ws://127.0.0.1:' + inPort);
    ws.onopen = function() {
      ws.send(JSON.stringify({ event: inRegisterEvent, uuid: inPropertyInspectorUUID }));
    };
    ws.onmessage = function(ev) {
      try {
        var raw = ev.data;
        var msg = typeof raw === 'string' ? JSON.parse(raw) : raw;
        if (msg.event === 'sendToPropertyInspector') {
          document.dispatchEvent(new CustomEvent('sendToPropertyInspector', { detail: msg.payload }));
        }
        if (msg.event === 'didReceiveSettings') {
          document.dispatchEvent(new CustomEvent('didReceiveSettings', { detail: msg }));
        }
      } catch (e) {}
    };
    window.__sdpiWs = ws;
  };
})();
`;
}

function ensurePiWebSocketConnected(win: Window, ctx: PiContext): void {
  type PiWindow = Window & {
    __sdpiWs?: WebSocket;
    connectElgatoStreamDeckSocket?: (
      port: number,
      uuid: string,
      registerEvent: string,
      info: string,
      actionInfo: string,
    ) => void;
  };
  const piWin = win as PiWindow;
  if (piWin.__sdpiWs) return;
  const connect = piWin.connectElgatoStreamDeckSocket;
  if (typeof connect !== "function") return;
  const infoJson = JSON.stringify(buildPiInfo(ctx));
  const actionInfoJson = JSON.stringify(buildActionInfo(ctx));
  connect.call(
    piWin,
    ctx.port,
    ctx.context,
    "registerPropertyInspector",
    infoJson,
    actionInfoJson,
  );
}

function attachPiContextMenuGuard(doc: Document): void {
  doc.addEventListener(
    "contextmenu",
    (event) => {
      const target = event.target;
      if (
        !(target instanceof HTMLInputElement) &&
        !(target instanceof HTMLTextAreaElement) &&
        !(target instanceof HTMLSelectElement) &&
        !(target instanceof HTMLElement && target.isContentEditable)
      ) {
        event.preventDefault();
      }
    },
    true,
  );
}

/** Forward plugin → PI messages emitted by the Rust host. */
export function dispatchPiMessage(
  iframe: HTMLIFrameElement,
  payload: unknown,
): void {
  try {
    const doc = iframe.contentDocument;
    if (!doc) return;
    doc.dispatchEvent(
      new CustomEvent("sendToPropertyInspector", { detail: payload }),
    );
  } catch {
    /* iframe may be empty or cross-origin during load */
  }
}

let piLoadGeneration = 0;

/** Invalidate in-flight PI loads; optionally clear the iframe immediately. */
export function beginPiLoad(iframe?: HTMLIFrameElement): number {
  piLoadGeneration += 1;
  if (iframe) {
    iframe.removeAttribute("src");
    iframe.srcdoc = "";
  }
  return piLoadGeneration;
}

function isPiLoadCurrent(generation: number): boolean {
  return generation === piLoadGeneration;
}

export async function loadPiInFrame(
  iframe: HTMLIFrameElement,
  piFilePath: string,
  ctx: PiContext,
  generation: number,
): Promise<void> {
  const piAssetUrl = convertFileSrc(piFilePath);
  const baseHref = piDirectoryAssetBase(piFilePath);
  const queryString = buildPiQueryString(ctx);

  iframe.removeAttribute("src");
  iframe.srcdoc = "";

  const resp = await fetch(piAssetUrl);
  if (!isPiLoadCurrent(generation)) return;
  if (!resp.ok) {
    throw new Error(`Property Inspector HTML の読み込みに失敗しました (${resp.status})`);
  }

  const html = injectPiDocumentPreamble(await resp.text(), baseHref, queryString);
  if (!isPiLoadCurrent(generation)) return;

  iframe.srcdoc = html;

  await new Promise<void>((resolve) => {
    iframe.onload = () => resolve();
  });
  if (!isPiLoadCurrent(generation)) return;

  try {
    const doc = iframe.contentDocument;
    const win = iframe.contentWindow;
    if (!doc || !win) return;
    attachPiContextMenuGuard(doc);
    ensurePiWebSocketConnected(win, ctx);
  } catch {
    /* srcdoc iframe is same-origin with parent */
  }
}
