/** Connect official SD Property Inspector HTML via PI WebSocket (registerPropertyInspector). */

import { convertFileSrc } from "@tauri-apps/api/core";

export interface PiContext {
  port: number;
  context: string;
  actionUuid: string;
  pluginUuid: string;
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

function buildPiInfo(ctx: PiContext) {
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
        id: "integratedeck-virtual-1",
        name: "integratedeck",
        size: { columns: 5, rows: 3 },
        type: 0,
      },
    ],
    colors: {
      buttonMouseOverBackgroundColor: "#464646FF",
      buttonPressedBackgroundColor: "#303030FF",
      highlightColor: "#0078FFFF",
    },
  };
}

function buildActionInfo(ctx: PiContext) {
  return {
    action: ctx.actionUuid,
    context: ctx.context,
    device: "integratedeck-virtual-1",
    payload: {
      settings: {},
      isInMultiAction: false,
    },
  };
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

/** Inject &lt;base&gt; so sdpi.css / sdtools.common.js resolve inside the .sdPlugin folder. */
function injectPiDocumentPreamble(html: string, baseHref: string, queryString: string): string {
  const escapedQs = queryString.replace(/\\/g, "\\\\").replace(/'/g, "\\'");
  const preamble =
    `<base href="${baseHref}">` +
    `<script>(function(){var s='?${escapedQs}';` +
    `try{Object.defineProperty(window.location,'search',{configurable:true,get:function(){return s;}});}` +
    `catch(e){}})();</script>`;

  if (/<head[^>]*>/i.test(html)) {
    return html.replace(/<head[^>]*>/i, (m) => m + preamble);
  }
  if (/<html[^>]*>/i.test(html)) {
    return html.replace(/<html[^>]*>/i, (m) => m + `<head>${preamble}</head>`);
  }
  return `<head>${preamble}</head>${html}`;
}

export function buildPiBridgeScript(ctx: PiContext): string {
  const info = JSON.stringify(buildPiInfo(ctx));
  const actionInfo = JSON.stringify(buildActionInfo(ctx));
  return `
(function() {
  if (window.__integratedeckPiConnected) return;
  window.__integratedeckPiConnected = true;
  if (typeof connectElgatoStreamDeckSocket === 'function') {
    connectElgatoStreamDeckSocket(
      ${ctx.port},
      ${JSON.stringify(ctx.context)},
      'registerPropertyInspector',
      ${info},
      ${actionInfo}
    );
    return;
  }
  var ws = new WebSocket('ws://127.0.0.1:${ctx.port}');
  ws.onopen = function() {
    ws.send(JSON.stringify({ event: 'registerPropertyInspector', uuid: ${JSON.stringify(ctx.context)} }));
  };
  ws.onmessage = function(ev) {
    try {
      var msg = JSON.parse(ev.data);
      if (msg.event === 'sendToPropertyInspector') {
        document.dispatchEvent(new CustomEvent('sendToPropertyInspector', { detail: msg.payload }));
      }
      if (msg.event === 'didReceiveSettings') {
        document.dispatchEvent(new CustomEvent('didReceiveSettings', { detail: msg.payload }));
      }
    } catch (e) {}
  };
  window.__sdpiWs = ws;
})();
`;
}

export async function loadPiInFrame(
  iframe: HTMLIFrameElement,
  piFilePath: string,
  ctx: PiContext,
): Promise<void> {
  const piAssetUrl = convertFileSrc(piFilePath);
  const baseHref = piDirectoryAssetBase(piFilePath);
  const queryString = buildPiQueryString(ctx);

  const resp = await fetch(piAssetUrl);
  if (!resp.ok) {
    throw new Error(`Property Inspector HTML の読み込みに失敗しました (${resp.status})`);
  }

  let html = injectPiDocumentPreamble(await resp.text(), baseHref, queryString);

  iframe.removeAttribute("src");
  iframe.srcdoc = html;

  await new Promise<void>((resolve) => {
    iframe.onload = () => resolve();
  });

  try {
    const doc = iframe.contentDocument;
    if (!doc) return;
    if (doc.querySelector("script[data-integratedeck-pi-bridge]")) return;
    const script = doc.createElement("script");
    script.setAttribute("data-integratedeck-pi-bridge", "1");
    script.textContent = buildPiBridgeScript(ctx);
    (doc.head || doc.documentElement).appendChild(script);
  } catch {
    /* srcdoc iframe is same-origin with parent */
  }
}
