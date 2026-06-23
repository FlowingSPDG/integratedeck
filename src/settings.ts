import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { installNativeFeel } from "./native-feel";

installNativeFeel();

interface AppGlobalSettings {
  language: string;
  launchOnStartup: boolean;
}

interface SettingsDeviceEntry {
  deviceKey: string;
  serial: string;
  kind: string;
  product: string;
  label: string;
  status: "connected" | "available" | "offline";
  surfaceId?: string;
  firmwareVersion?: string;
  brightness: number;
  rows: number;
  columns: number;
}

type TabId = "general" | "devices" | "presets" | "companion";

const I18N = {
  ja: {
    title: "設定",
    general: "General",
    devices: "Devices",
    presets: "Presets",
    version: "バージョン",
    dataDir: "データフォルダ",
    language: "言語",
    startup: "Windows 起動時に integratedeck を起動する",
    save: "保存",
    device: "デバイス",
    selectDevice: "デバイスを選択…",
    noDevices: "接続履歴のあるデバイスがありません",
    label: "ラベル名",
    product: "製品名",
    serial: "シリアル",
    firmware: "ファームウェア",
    brightness: "明るさ",
    grid: "キーレイアウト",
    status: "状態",
    connect: "接続",
    connected: "接続中",
    available: "利用可能",
    offline: "オフライン",
    unknownFirmware: "—（未接続）",
    exportPreset: "書き出し",
    importPreset: "読み込み",
    copyPreset: "クリップボードにコピー",
    presetPreview: "プリセット JSON",
    saved: "保存しました",
    exported: "プリセットを書き出しました",
    imported: "プリセットを読み込みました",
    copied: "クリップボードにコピーしました",
    companion: "Companion",
    companionModules: "モジュール",
    companionConnections: "接続",
    moduleId: "モジュール ID",
    connectionLabel: "接続名",
    addConnection: "接続を追加",
    removeConnection: "削除",
    openModulesFolder: "モジュールフォルダを開く",
    noModules: "companion-module-* フォルダがありません",
    noConnections: "接続がありません",
    hostRunning: "ホスト稼働中",
    hostStopped: "ホスト停止",
  },
  en: {
    title: "Settings",
    general: "General",
    devices: "Devices",
    presets: "Presets",
    version: "Version",
    dataDir: "Data folder",
    language: "Language",
    startup: "Launch integratedeck at Windows startup",
    save: "Save",
    device: "Device",
    selectDevice: "Select a device…",
    noDevices: "No devices in history",
    label: "Label",
    product: "Product",
    serial: "Serial",
    firmware: "Firmware",
    brightness: "Brightness",
    grid: "Key layout",
    status: "Status",
    connect: "Connect",
    connected: "Connected",
    available: "Available",
    offline: "Offline",
    unknownFirmware: "— (not connected)",
    exportPreset: "Export",
    importPreset: "Import",
    copyPreset: "Copy to clipboard",
    presetPreview: "Preset JSON",
    saved: "Saved",
    exported: "Preset exported",
    imported: "Preset imported",
    copied: "Copied to clipboard",
    companion: "Companion",
    companionModules: "Modules",
    companionConnections: "Connections",
    moduleId: "Module ID",
    connectionLabel: "Connection label",
    addConnection: "Add connection",
    removeConnection: "Remove",
    openModulesFolder: "Open modules folder",
    noModules: "No companion-module-* folders found",
    noConnections: "No connections",
    hostRunning: "Host running",
    hostStopped: "Host stopped",
  },
} as const;

function t(lang: keyof typeof I18N) {
  return I18N[lang];
}

function formatUserError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const o = err as { message?: string; data?: string };
    if (typeof o.message === "string" && o.message.length > 0) return o.message;
    if (typeof o.data === "string" && o.data.length > 0) return o.data;
  }
  return String(err);
}

function statusLabel(status: SettingsDeviceEntry["status"], lang: keyof typeof I18N): string {
  const labels = t(lang);
  if (status === "connected") return labels.connected;
  if (status === "available") return labels.available;
  return labels.offline;
}

const app = document.getElementById("settings-app")!;

let activeTab: TabId = "general";
let language: keyof typeof I18N = "ja";
let globalSettings: AppGlobalSettings = { language: "ja", launchOnStartup: false };
let devices: SettingsDeviceEntry[] = [];
let selectedDeviceKey: string | null = null;
let selectedDevice: SettingsDeviceEntry | null = null;
let presetJson = "";

function strings() {
  return t(language);
}

function setStatus(message: string, kind: "normal" | "error" | "success" = "normal") {
  const el = document.getElementById("status-line");
  if (!el) return;
  el.textContent = message;
  el.className = `status-line${kind === "error" ? " error" : kind === "success" ? " success" : ""}`;
}

function renderShell() {
  const s = strings();
  app.innerHTML = `
    <header class="settings-header">
      <h1>${s.title}</h1>
      <button type="button" class="btn-close" id="btn-close" title="Close">✕</button>
    </header>
    <nav class="settings-tabs">
      <button type="button" class="tab-btn ${activeTab === "general" ? "active" : ""}" data-tab="general">${s.general}</button>
      <button type="button" class="tab-btn ${activeTab === "devices" ? "active" : ""}" data-tab="devices">${s.devices}</button>
      <button type="button" class="tab-btn ${activeTab === "presets" ? "active" : ""}" data-tab="presets">${s.presets}</button>
      <button type="button" class="tab-btn ${activeTab === "companion" ? "active" : ""}" data-tab="companion">${s.companion}</button>
    </nav>
    <div class="settings-content">
      <div class="tab-panel ${activeTab === "general" ? "active" : ""}" id="panel-general"></div>
      <div class="tab-panel ${activeTab === "devices" ? "active" : ""}" id="panel-devices"></div>
      <div class="tab-panel ${activeTab === "presets" ? "active" : ""}" id="panel-presets"></div>
      <div class="tab-panel ${activeTab === "companion" ? "active" : ""}" id="panel-companion"></div>
      <div class="status-line" id="status-line"></div>
    </div>
  `;

  app.querySelectorAll(".tab-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      activeTab = (btn as HTMLElement).dataset.tab as TabId;
      renderShell();
      renderActivePanel();
    });
  });

  document.getElementById("btn-close")?.addEventListener("click", () => {
    void getCurrentWindow().hide();
  });

  renderActivePanel();
}

function renderActivePanel() {
  if (activeTab === "general") void renderGeneralPanel();
  else if (activeTab === "devices") void renderDevicesPanel();
  else if (activeTab === "presets") void renderPresetsPanel();
  else void renderCompanionPanel();
}

async function renderGeneralPanel() {
  const s = strings();
  const info = await invoke<{ version: string; data_dir: string }>("get_app_info");
  const panel = document.getElementById("panel-general")!;
  panel.innerHTML = `
    <h2 class="section-title">${s.general}</h2>
    <dl class="meta-grid">
      <dt>${s.version}</dt><dd>v${info.version}</dd>
      <dt>${s.dataDir}</dt><dd>${info.data_dir}</dd>
    </dl>
    <div class="field">
      <label for="language-select">${s.language}</label>
      <select id="language-select">
        <option value="ja" ${globalSettings.language === "ja" ? "selected" : ""}>日本語</option>
        <option value="en" ${globalSettings.language === "en" ? "selected" : ""}>English</option>
      </select>
    </div>
    <div class="field">
      <label>
        <input type="checkbox" id="startup-checkbox" ${globalSettings.launchOnStartup ? "checked" : ""} />
        ${s.startup}
      </label>
    </div>
    <div class="actions">
      <button type="button" class="primary" id="btn-save-general">${s.save}</button>
    </div>
  `;

  document.getElementById("btn-save-general")!.addEventListener("click", async () => {
    const lang = (document.getElementById("language-select") as HTMLSelectElement).value;
    const launchOnStartup = (document.getElementById("startup-checkbox") as HTMLInputElement).checked;
    try {
      await invoke("set_global_settings", {
        settings: { language: lang, launchOnStartup },
      });
      globalSettings = { language: lang, launchOnStartup };
      language = lang === "en" ? "en" : "ja";
      setStatus(strings().saved, "success");
      renderShell();
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });
}

function deviceOptions(selected: string | null): string {
  const s = strings();
  if (devices.length === 0) {
    return `<option value="">${s.noDevices}</option>`;
  }
  return `<option value="">${s.selectDevice}</option>${devices
    .map(
      (d) =>
        `<option value="${d.deviceKey}" ${d.deviceKey === selected ? "selected" : ""}>${d.label} (${statusLabel(d.status, language)})</option>`,
    )
    .join("")}`;
}

async function loadSelectedDevice() {
  if (!selectedDeviceKey) {
    selectedDevice = null;
    return;
  }
  selectedDevice = await invoke<SettingsDeviceEntry | null>("get_settings_device", {
    deviceKey: selectedDeviceKey,
  });
}

async function renderDevicesPanel() {
  const s = strings();
  devices = await invoke<SettingsDeviceEntry[]>("list_settings_devices");
  if (!selectedDeviceKey && devices.length > 0) {
    selectedDeviceKey = devices[0].deviceKey;
  }
  await loadSelectedDevice();
  const d = selectedDevice;

  const panel = document.getElementById("panel-devices")!;
  panel.innerHTML = `
    <h2 class="section-title">${s.devices}</h2>
    <div class="field">
      <label for="device-select">${s.device}</label>
      <select id="device-select">${deviceOptions(selectedDeviceKey)}</select>
    </div>
    ${
      d
        ? `
      <dl class="meta-grid">
        <dt>${s.status}</dt><dd><span class="status-badge ${d.status}">${statusLabel(d.status, language)}</span></dd>
        <dt>${s.product}</dt><dd>${d.product}</dd>
        <dt>${s.serial}</dt><dd>${d.serial}</dd>
        <dt>${s.firmware}</dt><dd>${d.firmwareVersion ?? s.unknownFirmware}</dd>
        <dt>${s.grid}</dt><dd>${d.rows} × ${d.columns}</dd>
      </dl>
      <div class="field">
        <label for="device-label">${s.label}</label>
        <input type="text" id="device-label" value="${d.label.replace(/"/g, "&quot;")}" />
      </div>
      <div class="field">
        <label for="device-brightness">${s.brightness}</label>
        <div class="field-row">
          <input type="range" id="device-brightness" min="0" max="100" value="${d.brightness}" />
          <output id="brightness-value">${d.brightness}</output>
        </div>
      </div>
      <div class="actions">
        <button type="button" class="primary" id="btn-save-device">${s.save}</button>
        ${
          d.status !== "connected"
            ? `<button type="button" id="btn-connect-device">${s.connect}</button>`
            : ""
        }
      </div>
    `
        : `<p class="empty-state">${s.noDevices}</p>`
    }
  `;

  const select = document.getElementById("device-select") as HTMLSelectElement | null;
  select?.addEventListener("change", async () => {
    selectedDeviceKey = select.value || null;
    await renderDevicesPanel();
  });

  const brightness = document.getElementById("device-brightness") as HTMLInputElement | null;
  const brightnessValue = document.getElementById("brightness-value");
  brightness?.addEventListener("input", () => {
    if (brightnessValue) brightnessValue.textContent = brightness.value;
  });

  document.getElementById("btn-save-device")?.addEventListener("click", async () => {
    if (!selectedDeviceKey || !d) return;
    const label = (document.getElementById("device-label") as HTMLInputElement).value;
    const value = Number((document.getElementById("device-brightness") as HTMLInputElement).value);
    try {
      await invoke("set_device_label", {
        args: { deviceKey: selectedDeviceKey, label },
      });
      await invoke("set_device_brightness", {
        args: { deviceKey: selectedDeviceKey, brightness: value },
      });
      setStatus(strings().saved, "success");
      await renderDevicesPanel();
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });

  document.getElementById("btn-connect-device")?.addEventListener("click", async () => {
    if (!selectedDeviceKey) return;
    try {
      await invoke("connect_settings_device", { deviceKey: selectedDeviceKey });
      setStatus(strings().connected, "success");
      await renderDevicesPanel();
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });
}

async function renderPresetsPanel() {
  const s = strings();
  devices = await invoke<SettingsDeviceEntry[]>("list_settings_devices");
  if (!selectedDeviceKey && devices.length > 0) {
    selectedDeviceKey = devices[0].deviceKey;
  }

  if (selectedDeviceKey) {
    presetJson =
      (await invoke<string | null>("load_saved_device_preset", {
        deviceKey: selectedDeviceKey,
      })) ?? "";
  }

  const panel = document.getElementById("panel-presets")!;
  panel.innerHTML = `
    <h2 class="section-title">${s.presets}</h2>
    <div class="field">
      <label for="preset-device-select">${s.device}</label>
      <select id="preset-device-select">${deviceOptions(selectedDeviceKey)}</select>
    </div>
    ${
      selectedDeviceKey
        ? `
      <div class="field">
        <label for="preset-json">${s.presetPreview}</label>
        <textarea id="preset-json" class="preset-preview" spellcheck="false">${presetJson.replace(/</g, "&lt;")}</textarea>
      </div>
      <div class="actions">
        <button type="button" class="primary" id="btn-export-preset">${s.exportPreset}</button>
        <button type="button" id="btn-import-preset">${s.importPreset}</button>
        <button type="button" id="btn-copy-preset">${s.copyPreset}</button>
      </div>
    `
        : `<p class="empty-state">${s.noDevices}</p>`
    }
  `;

  const select = document.getElementById("preset-device-select") as HTMLSelectElement | null;
  select?.addEventListener("change", async () => {
    selectedDeviceKey = select.value || null;
    await renderPresetsPanel();
  });

  document.getElementById("btn-export-preset")?.addEventListener("click", async () => {
    if (!selectedDeviceKey) return;
    try {
      const json = await invoke<string>("export_device_preset", {
        deviceKey: selectedDeviceKey,
      });
      presetJson = json;
      const textarea = document.getElementById("preset-json") as HTMLTextAreaElement;
      if (textarea) textarea.value = json;
      setStatus(strings().exported, "success");
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });

  document.getElementById("btn-import-preset")?.addEventListener("click", async () => {
    if (!selectedDeviceKey) return;
    const textarea = document.getElementById("preset-json") as HTMLTextAreaElement;
    try {
      await invoke("import_device_preset", {
        args: { deviceKey: selectedDeviceKey, json: textarea.value },
      });
      setStatus(strings().imported, "success");
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });

  document.getElementById("btn-copy-preset")?.addEventListener("click", async () => {
    const textarea = document.getElementById("preset-json") as HTMLTextAreaElement;
    if (!textarea?.value) return;
    await navigator.clipboard.writeText(textarea.value);
    setStatus(strings().copied, "success");
  });
}

interface ScanResult {
  companion: { path: string; name: string; bundleName: string }[];
}

interface ConnectionRecord {
  id: string;
  moduleId: string;
  label: string;
  enabled: boolean;
}

async function renderCompanionPanel() {
  const s = strings();
  const scan = await invoke<ScanResult>("scan_plugins");
  const connections = await invoke<ConnectionRecord[]>("list_connections");
  const status = await invoke<{ companion: { companionHostRunning: boolean } }>("get_runtime_status");

  const moduleOptions = scan.companion.length
    ? scan.companion
        .map(
          (m) =>
            `<option value="${m.name.replace(/"/g, "&quot;")}">${m.name}</option>`,
        )
        .join("")
    : "";

  const connectionRows = connections.length
    ? connections
        .map(
          (c) => `
        <tr>
          <td>${c.label}</td>
          <td><code>${c.moduleId}</code></td>
          <td>${c.enabled ? "✓" : "—"}</td>
          <td><button type="button" class="btn-remove-conn" data-id="${c.id}">${s.removeConnection}</button></td>
        </tr>`,
        )
        .join("")
    : `<tr><td colspan="4">${s.noConnections}</td></tr>`;

  const panel = document.getElementById("panel-companion")!;
  panel.innerHTML = `
    <h2 class="section-title">${s.companion}</h2>
    <p class="meta-line">${status.companion.companionHostRunning ? s.hostRunning : s.hostStopped}</p>
    <div class="actions">
      <button type="button" id="btn-open-companion-folder">${s.openModulesFolder}</button>
    </div>
    <h3 class="section-subtitle">${s.companionConnections}</h3>
    <table class="simple-table">
      <thead><tr><th>${s.connectionLabel}</th><th>${s.moduleId}</th><th></th><th></th></tr></thead>
      <tbody>${connectionRows}</tbody>
    </table>
    <div class="field">
      <label for="companion-module-select">${s.companionModules}</label>
      <select id="companion-module-select">
        ${scan.companion.length ? moduleOptions : `<option value="">${s.noModules}</option>`}
      </select>
    </div>
    <div class="field">
      <label for="companion-label">${s.connectionLabel}</label>
      <input type="text" id="companion-label" placeholder="My connection" />
    </div>
    <div class="actions">
      <button type="button" class="primary" id="btn-add-connection" ${scan.companion.length ? "" : "disabled"}>${s.addConnection}</button>
    </div>
  `;

  document.getElementById("btn-open-companion-folder")?.addEventListener("click", async () => {
    try {
      await invoke("open_companion_modules_folder");
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });

  document.getElementById("btn-add-connection")?.addEventListener("click", async () => {
    const moduleId = (document.getElementById("companion-module-select") as HTMLSelectElement).value;
    const label = (document.getElementById("companion-label") as HTMLInputElement).value.trim();
    if (!moduleId) return;
    try {
      await invoke("add_connection", {
        args: {
          moduleId,
          label: label || moduleId,
          config: {},
        },
      });
      setStatus(strings().saved, "success");
      await renderCompanionPanel();
    } catch (err) {
      setStatus(formatUserError(err), "error");
    }
  });

  panel.querySelectorAll(".btn-remove-conn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const id = (btn as HTMLElement).dataset.id;
      if (!id) return;
      try {
        await invoke("remove_connection", { connectionId: id });
        setStatus(strings().saved, "success");
        await renderCompanionPanel();
      } catch (err) {
        setStatus(formatUserError(err), "error");
      }
    });
  });
}

async function init() {
  renderShell();
  setStatus("読み込み中…");

  try {
    globalSettings = await invoke<AppGlobalSettings>("get_global_settings");
    language = globalSettings.language === "en" ? "en" : "ja";
    renderShell();
  } catch (err) {
    setStatus(formatUserError(err), "error");
  }
}

init().catch((err) => {
  renderShell();
  setStatus(formatUserError(err), "error");
});
