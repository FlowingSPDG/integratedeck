import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { formatUserError } from "./utils/format-error";

type TabId = "general" | "devices" | "presets" | "companion";

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

interface ScanResult {
  companion: { path: string; name: string; bundleName: string }[];
}

interface ConnectionRecord {
  id: string;
  moduleId: string;
  label: string;
  enabled: boolean;
  config?: Record<string, unknown>;
  secrets?: Record<string, unknown>;
}

interface CompanionConfigField {
  id: string;
  type: string;
  label?: string;
  default?: unknown;
  choices?: Array<{ id: string | number; label: string }>;
  min?: number;
  max?: number;
}

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

function statusLabel(
  status: SettingsDeviceEntry["status"],
  lang: keyof typeof I18N,
): string {
  const labels = I18N[lang];
  if (status === "connected") return labels.connected;
  if (status === "available") return labels.available;
  return labels.offline;
}

export default function SettingsApp() {
  const [activeTab, setActiveTab] = useState<TabId>("general");
  const [language, setLanguage] = useState<keyof typeof I18N>("ja");
  const [globalSettings, setGlobalSettings] = useState<AppGlobalSettings>({
    language: "ja",
    launchOnStartup: false,
  });
  const [devices, setDevices] = useState<SettingsDeviceEntry[]>([]);
  const [selectedDeviceKey, setSelectedDeviceKey] = useState<string | null>(null);
  const [selectedDevice, setSelectedDevice] = useState<SettingsDeviceEntry | null>(null);
  const [presetJson, setPresetJson] = useState("");
  const [status, setStatus] = useState<{ message: string; kind: "normal" | "error" | "success" }>({
    message: "読み込み中…",
    kind: "normal",
  });
  const [appInfo, setAppInfo] = useState<{ version: string; data_dir: string } | null>(null);
  const [companionScan, setCompanionScan] = useState<ScanResult | null>(null);
  const [connections, setConnections] = useState<ConnectionRecord[]>([]);
  const [hostRunning, setHostRunning] = useState(false);
  const [newConnLabel, setNewConnLabel] = useState("");
  const [newConnModule, setNewConnModule] = useState("");
  const [configConnectionId, setConfigConnectionId] = useState<string | null>(null);
  const [companionStatuses, setCompanionStatuses] = useState<
    Record<string, { status?: string; message?: string }>
  >({});
  const [configFields, setConfigFields] = useState<CompanionConfigField[]>([]);
  const [configValues, setConfigValues] = useState<Record<string, unknown>>({});
  const [secretValues, setSecretValues] = useState<Record<string, unknown>>({});
  const [studioAddr, setStudioAddr] = useState("192.168.3.48:5343");
  const [surfaceDevices, setSurfaceDevices] = useState<
    Array<{
      driverId: string;
      deviceId: string;
      label: string;
      product: string;
      rows: number;
      columns: number;
    }>
  >([]);

  const s = I18N[language];

  const setStatusLine = useCallback(
    (message: string, kind: "normal" | "error" | "success" = "normal") => {
      setStatus({ message, kind });
    },
    [],
  );

  const loadGeneral = useCallback(async () => {
    setAppInfo(await invoke("get_app_info"));
  }, []);

  const loadDevices = useCallback(async () => {
    const list = await invoke<SettingsDeviceEntry[]>("list_settings_devices");
    setDevices(list);
    try {
      setSurfaceDevices(await invoke("scan_surface_devices"));
    } catch {
      setSurfaceDevices([]);
    }
    let key = selectedDeviceKey;
    if (!key && list.length > 0) key = list[0].deviceKey;
    setSelectedDeviceKey(key);
    if (key) {
      setSelectedDevice(
        await invoke<SettingsDeviceEntry | null>("get_settings_device", { deviceKey: key }),
      );
    } else {
      setSelectedDevice(null);
    }
  }, [selectedDeviceKey]);

  const loadPresets = useCallback(async () => {
    const list = await invoke<SettingsDeviceEntry[]>("list_settings_devices");
    setDevices(list);
    let key = selectedDeviceKey;
    if (!key && list.length > 0) key = list[0].deviceKey;
    setSelectedDeviceKey(key);
    if (key) {
      setPresetJson(
        (await invoke<string | null>("load_saved_device_preset", { deviceKey: key })) ?? "",
      );
    }
  }, [selectedDeviceKey]);

  const loadCompanion = useCallback(async () => {
    const [scan, conns, runtime, statuses] = await Promise.all([
      invoke<ScanResult>("scan_plugins"),
      invoke<ConnectionRecord[]>("list_connections"),
      invoke<{ companion: { companionHostRunning: boolean } }>("get_runtime_status"),
      invoke<Array<{ connectionId?: string; status?: string; message?: string }>>(
        "list_companion_status",
      ),
    ]);
    setCompanionScan(scan);
    setConnections(conns);
    setHostRunning(runtime.companion.companionHostRunning);
    const map: Record<string, { status?: string; message?: string }> = {};
    for (const entry of statuses) {
      if (entry.connectionId) {
        map[entry.connectionId] = { status: entry.status, message: entry.message };
      }
    }
    setCompanionStatuses(map);
    if (scan.companion[0]) setNewConnModule(scan.companion[0].name);
  }, []);

  const loadConnectionConfig = useCallback(
    async (record: ConnectionRecord) => {
      try {
        const result = await invoke<
          { fields?: CompanionConfigField[] } | CompanionConfigField[]
        >("get_connection_config_fields", { connectionId: record.id });
        const fields = Array.isArray(result) ? result : (result.fields ?? []);
        setConfigConnectionId(record.id);
        setConfigFields(fields);
        setConfigValues({ ...(record.config ?? {}) });
        setSecretValues({ ...(record.secrets ?? {}) });
      } catch (err) {
        setStatusLine(formatUserError(err), "error");
      }
    },
    [setStatusLine],
  );

  const saveConnectionConfig = async () => {
    if (!configConnectionId) return;
    try {
      await invoke("update_connection_config", {
        args: {
          connectionId: configConnectionId,
          config: configValues,
          secrets: secretValues,
        },
      });
      setStatusLine(s.saved, "success");
      setConfigConnectionId(null);
      await loadCompanion();
    } catch (err) {
      setStatusLine(formatUserError(err), "error");
    }
  };

  useEffect(() => {
    void invoke<AppGlobalSettings>("get_global_settings")
      .then((gs) => {
        setGlobalSettings(gs);
        setLanguage(gs.language === "en" ? "en" : "ja");
        setStatusLine("");
      })
      .catch((err) => setStatusLine(formatUserError(err), "error"));
  }, [setStatusLine]);

  useEffect(() => {
    if (activeTab === "general") void loadGeneral();
    else if (activeTab === "devices") void loadDevices();
    else if (activeTab === "presets") void loadPresets();
    else void loadCompanion();
  }, [activeTab, loadCompanion, loadDevices, loadGeneral, loadPresets]);

  const deviceOptions = (
    <select
      value={selectedDeviceKey ?? ""}
      onChange={(e) => {
        const key = e.target.value || null;
        setSelectedDeviceKey(key);
        if (activeTab === "devices") void loadDevices();
        if (activeTab === "presets") void loadPresets();
      }}
    >
      <option value="">{devices.length === 0 ? s.noDevices : s.selectDevice}</option>
      {devices.map((d) => (
        <option key={d.deviceKey} value={d.deviceKey}>
          {d.label} ({statusLabel(d.status, language)})
        </option>
      ))}
    </select>
  );

  return (
    <>
      <header className="settings-header">
        <h1>{s.title}</h1>
        <button
          type="button"
          className="btn-close"
          title="Close"
          onClick={() => {
            const win = getCurrentWindow();
            void win.setSkipTaskbar(true).then(() => win.hide());
          }}
        >
          ✕
        </button>
      </header>
      <nav className="settings-tabs">
        {(["general", "devices", "presets", "companion"] as TabId[]).map((tab) => (
          <button
            key={tab}
            type="button"
            className={`tab-btn${activeTab === tab ? " active" : ""}`}
            onClick={() => setActiveTab(tab)}
          >
            {s[tab]}
          </button>
        ))}
      </nav>
      <div className="settings-content">
        {activeTab === "general" && appInfo ? (
          <div className="tab-panel active" id="panel-general">
            <h2 className="section-title">{s.general}</h2>
            <dl className="meta-grid">
              <dt>{s.version}</dt>
              <dd>v{appInfo.version}</dd>
              <dt>{s.dataDir}</dt>
              <dd>{appInfo.data_dir}</dd>
            </dl>
            <div className="field">
              <label htmlFor="language-select">{s.language}</label>
              <select
                id="language-select"
                value={globalSettings.language}
                onChange={(e) =>
                  setGlobalSettings((g) => ({ ...g, language: e.target.value }))
                }
              >
                <option value="ja">日本語</option>
                <option value="en">English</option>
              </select>
            </div>
            <div className="field">
              <label>
                <input
                  type="checkbox"
                  checked={globalSettings.launchOnStartup}
                  onChange={(e) =>
                    setGlobalSettings((g) => ({ ...g, launchOnStartup: e.target.checked }))
                  }
                />
                {s.startup}
              </label>
            </div>
            <div className="actions">
              <button
                type="button"
                className="primary"
                onClick={async () => {
                  try {
                    await invoke("set_global_settings", { settings: globalSettings });
                    setLanguage(globalSettings.language === "en" ? "en" : "ja");
                    setStatusLine(s.saved, "success");
                  } catch (err) {
                    setStatusLine(formatUserError(err), "error");
                  }
                }}
              >
                {s.save}
              </button>
            </div>
          </div>
        ) : null}

        {activeTab === "devices" ? (
          <div className="tab-panel active" id="panel-devices">
            <h2 className="section-title">{s.devices}</h2>
            <div className="field">
              <label htmlFor="studio-lan-addr">Studio LAN</label>
              <div className="actions">
                <input
                  id="studio-lan-addr"
                  value={studioAddr}
                  onChange={(e) => setStudioAddr(e.target.value)}
                  placeholder="192.168.3.48:5343"
                />
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      await invoke("connect_surface_device", {
                        driverId: "studio-lan",
                        deviceId: studioAddr,
                      });
                      setStatusLine(
                        language === "ja"
                          ? "Studio LANに接続しました"
                          : "Connected Studio LAN",
                        "success",
                      );
                      await loadDevices();
                    } catch (err) {
                      setStatusLine(formatUserError(err), "error");
                    }
                  }}
                >
                  {s.connect}
                </button>
              </div>
            </div>
            <div className="field">
              <label>サーフェスデバイス</label>
              {surfaceDevices.length === 0 ? (
                <p className="muted">検出されたデバイスはありません</p>
              ) : (
                surfaceDevices.map((device) => (
                  <div className="actions" key={`${device.driverId}:${device.deviceId}`}>
                    <span>
                      {device.label} ({device.rows}×{device.columns})
                    </span>
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          const result = await invoke<{ surfaceId: string }>(
                            "connect_surface_device",
                            { driverId: device.driverId, deviceId: device.deviceId },
                          );
                          await invoke("ensure_surface_slots", {
                            surfaceId: result.surfaceId,
                          });
                          setStatusLine(
                            language === "ja"
                              ? `${device.label} に接続しました`
                              : `Connected ${device.label}`,
                            "success",
                          );
                          await loadDevices();
                        } catch (err) {
                          setStatusLine(formatUserError(err), "error");
                        }
                      }}
                    >
                      {s.connect}
                    </button>
                  </div>
                ))
              )}
            </div>
            <div className="field">
              <label htmlFor="device-select">{s.device}</label>
              <select
                id="device-select"
                value={selectedDeviceKey ?? ""}
                onChange={async (e) => {
                  const key = e.target.value || null;
                  setSelectedDeviceKey(key);
                  if (key) {
                    setSelectedDevice(
                      await invoke("get_settings_device", { deviceKey: key }),
                    );
                  } else {
                    setSelectedDevice(null);
                  }
                }}
              >
                <option value="">{devices.length === 0 ? s.noDevices : s.selectDevice}</option>
                {devices.map((d) => (
                  <option key={d.deviceKey} value={d.deviceKey}>
                    {d.label} ({statusLabel(d.status, language)})
                  </option>
                ))}
              </select>
            </div>
            {selectedDevice ? (
              <>
                <dl className="meta-grid">
                  <dt>{s.status}</dt>
                  <dd>
                    <span className={`status-badge ${selectedDevice.status}`}>
                      {statusLabel(selectedDevice.status, language)}
                    </span>
                  </dd>
                  <dt>{s.product}</dt>
                  <dd>{selectedDevice.product}</dd>
                  <dt>{s.serial}</dt>
                  <dd>{selectedDevice.serial}</dd>
                  <dt>{s.firmware}</dt>
                  <dd>{selectedDevice.firmwareVersion ?? s.unknownFirmware}</dd>
                  <dt>{s.grid}</dt>
                  <dd>
                    {selectedDevice.rows} × {selectedDevice.columns}
                  </dd>
                </dl>
                <DeviceForm
                  device={selectedDevice}
                  labels={s}
                  onSave={async (label, brightness) => {
                    if (!selectedDeviceKey) return;
                    await invoke("set_device_label", {
                      args: { deviceKey: selectedDeviceKey, label },
                    });
                    await invoke("set_device_brightness", {
                      args: { deviceKey: selectedDeviceKey, brightness },
                    });
                    setStatusLine(s.saved, "success");
                    await loadDevices();
                  }}
                />
                {selectedDevice.status !== "connected" ? (
                  <div className="actions">
                    <button
                      type="button"
                      id="btn-connect-device"
                      onClick={async () => {
                        if (!selectedDeviceKey) return;
                        try {
                          await invoke("connect_settings_device", {
                            deviceKey: selectedDeviceKey,
                          });
                          setStatusLine(s.connected, "success");
                          await loadDevices();
                        } catch (err) {
                          setStatusLine(formatUserError(err), "error");
                        }
                      }}
                    >
                      {s.connect}
                    </button>
                  </div>
                ) : null}
              </>
            ) : (
              <p className="empty-state">{s.noDevices}</p>
            )}
          </div>
        ) : null}

        {activeTab === "presets" ? (
          <div className="tab-panel active" id="panel-presets">
            <h2 className="section-title">{s.presets}</h2>
            <div className="field">
              <label htmlFor="preset-device-select">{s.device}</label>
              <div id="preset-device-select">{deviceOptions}</div>
            </div>
            {selectedDeviceKey ? (
              <>
                <div className="field">
                  <label htmlFor="preset-json">{s.presetPreview}</label>
                  <textarea
                    id="preset-json"
                    className="preset-preview"
                    spellCheck={false}
                    value={presetJson}
                    onChange={(e) => setPresetJson(e.target.value)}
                  />
                </div>
                <div className="actions">
                  <button
                    type="button"
                    className="primary"
                    onClick={async () => {
                      if (!selectedDeviceKey) return;
                      try {
                        const json = await invoke<string>("export_device_preset", {
                          deviceKey: selectedDeviceKey,
                        });
                        setPresetJson(json);
                        setStatusLine(s.exported, "success");
                      } catch (err) {
                        setStatusLine(formatUserError(err), "error");
                      }
                    }}
                  >
                    {s.exportPreset}
                  </button>
                  <button
                    type="button"
                    onClick={async () => {
                      if (!selectedDeviceKey) return;
                      try {
                        await invoke("import_device_preset", {
                          args: { deviceKey: selectedDeviceKey, json: presetJson },
                        });
                        setStatusLine(s.imported, "success");
                      } catch (err) {
                        setStatusLine(formatUserError(err), "error");
                      }
                    }}
                  >
                    {s.importPreset}
                  </button>
                  <button
                    type="button"
                    onClick={async () => {
                      if (!presetJson) return;
                      await navigator.clipboard.writeText(presetJson);
                      setStatusLine(s.copied, "success");
                    }}
                  >
                    {s.copyPreset}
                  </button>
                </div>
              </>
            ) : (
              <p className="empty-state">{s.noDevices}</p>
            )}
          </div>
        ) : null}

        {activeTab === "companion" && companionScan ? (
          <div className="tab-panel active" id="panel-companion">
            <h2 className="section-title">{s.companion}</h2>
            <p className="meta-line">{hostRunning ? s.hostRunning : s.hostStopped}</p>
            <div className="actions">
              <button
                type="button"
                onClick={async () => {
                  try {
                    await invoke("open_companion_modules_folder");
                  } catch (err) {
                    setStatusLine(formatUserError(err), "error");
                  }
                }}
              >
                {s.openModulesFolder}
              </button>
            </div>
            <h3 className="section-subtitle">{s.companionConnections}</h3>
            <table className="simple-table">
              <thead>
                <tr>
                  <th>{s.connectionLabel}</th>
                  <th>{s.moduleId}</th>
                  <th />
                  <th>状態</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {connections.length === 0 ? (
                  <tr>
                    <td colSpan={5}>{s.noConnections}</td>
                  </tr>
                ) : (
                  connections.map((c) => (
                    <tr key={c.id}>
                      <td>{c.label}</td>
                      <td>
                        <code>{c.moduleId}</code>
                      </td>
                      <td>{c.enabled ? "✓" : "—"}</td>
                      <td>
                        {companionStatuses[c.id]?.message ??
                          companionStatuses[c.id]?.status ??
                          "—"}
                      </td>
                      <td>
                        <button
                          type="button"
                          className="page-btn"
                          onClick={() => void loadConnectionConfig(c)}
                        >
                          設定
                        </button>{" "}
                        <button
                          type="button"
                          className="btn-remove-conn"
                          onClick={async () => {
                            try {
                              await invoke("remove_connection", { connectionId: c.id });
                              setStatusLine(s.saved, "success");
                              await loadCompanion();
                            } catch (err) {
                              setStatusLine(formatUserError(err), "error");
                            }
                          }}
                        >
                          {s.removeConnection}
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
            <div className="field">
              <label htmlFor="companion-module-select">{s.companionModules}</label>
              <select
                id="companion-module-select"
                value={newConnModule}
                onChange={(e) => setNewConnModule(e.target.value)}
              >
                {companionScan.companion.length === 0 ? (
                  <option value="">{s.noModules}</option>
                ) : (
                  companionScan.companion.map((m) => (
                    <option key={m.name} value={m.name}>
                      {m.name}
                    </option>
                  ))
                )}
              </select>
            </div>
            <div className="field">
              <label htmlFor="companion-label">{s.connectionLabel}</label>
              <input
                id="companion-label"
                type="text"
                placeholder="My connection"
                value={newConnLabel}
                onChange={(e) => setNewConnLabel(e.target.value)}
              />
            </div>
            <div className="actions">
              <button
                type="button"
                className="primary"
                disabled={companionScan.companion.length === 0}
                onClick={async () => {
                  if (!newConnModule) return;
                  try {
                    const record = await invoke<ConnectionRecord>("add_connection", {
                      args: {
                        moduleId: newConnModule,
                        label: newConnLabel.trim() || newConnModule,
                        config: {},
                        secrets: {},
                      },
                    });
                    setStatusLine(s.saved, "success");
                    setNewConnLabel("");
                    await loadCompanion();
                    await loadConnectionConfig(record);
                  } catch (err) {
                    setStatusLine(formatUserError(err), "error");
                  }
                }}
              >
                {s.addConnection}
              </button>
            </div>
            {configConnectionId ? (
              <div className="field">
                <label>接続設定</label>
                {configFields.map((field) => {
                  const label = field.label ?? field.id;
                  const value = configValues[field.id];
                  if (field.type === "static-text") {
                    return (
                      <p className="muted" key={field.id}>
                        {String(field.default ?? "")}
                      </p>
                    );
                  }
                  if (field.type === "checkbox") {
                    return (
                      <label key={field.id}>
                        <input
                          type="checkbox"
                          checked={Boolean(value ?? field.default ?? false)}
                          onChange={(e) =>
                            setConfigValues((current) => ({
                              ...current,
                              [field.id]: e.target.checked,
                            }))
                          }
                        />{" "}
                        {label}
                      </label>
                    );
                  }
                  if (field.type === "dropdown") {
                    return (
                      <label key={field.id}>
                        {label}
                        <select
                          value={String(value ?? field.default ?? "")}
                          onChange={(e) =>
                            setConfigValues((current) => ({
                              ...current,
                              [field.id]: e.target.value,
                            }))
                          }
                        >
                          {field.choices?.map((choice) => (
                            <option key={String(choice.id)} value={String(choice.id)}>
                              {choice.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    );
                  }
                  if (field.type === "secret") {
                    return (
                      <label key={field.id}>
                        {label}
                        <input
                          type="password"
                          value={String(secretValues[field.id] ?? "")}
                          onChange={(e) =>
                            setSecretValues((current) => ({
                              ...current,
                              [field.id]: e.target.value,
                            }))
                          }
                        />
                      </label>
                    );
                  }
                  return (
                    <label key={field.id}>
                      {label}
                      <input
                        type={field.type === "number" ? "number" : "text"}
                        value={String(value ?? field.default ?? "")}
                        onChange={(e) =>
                          setConfigValues((current) => ({
                            ...current,
                            [field.id]:
                              field.type === "number"
                                ? Number(e.target.value)
                                : e.target.value,
                          }))
                        }
                      />
                    </label>
                  );
                })}
                <div className="actions">
                  <button type="button" className="primary" onClick={() => void saveConnectionConfig()}>
                    {s.save}
                  </button>
                  <button type="button" onClick={() => setConfigConnectionId(null)}>
                    キャンセル
                  </button>
                </div>
              </div>
            ) : null}
          </div>
        ) : null}

        <div
          className={`status-line${status.kind === "error" ? " error" : status.kind === "success" ? " success" : ""}`}
        >
          {status.message}
        </div>
      </div>
    </>
  );
}

function DeviceForm({
  device,
  labels,
  onSave,
}: {
  device: SettingsDeviceEntry;
  labels: (typeof I18N)[keyof typeof I18N];
  onSave: (label: string, brightness: number) => Promise<void>;
}) {
  const [label, setLabel] = useState(device.label);
  const [brightness, setBrightness] = useState(device.brightness);

  useEffect(() => {
    setLabel(device.label);
    setBrightness(device.brightness);
  }, [device]);

  return (
    <>
      <div className="field">
        <label htmlFor="device-label">{labels.label}</label>
        <input id="device-label" type="text" value={label} onChange={(e) => setLabel(e.target.value)} />
      </div>
      <div className="field">
        <label htmlFor="device-brightness">{labels.brightness}</label>
        <div className="field-row">
          <input
            id="device-brightness"
            type="range"
            min={0}
            max={100}
            value={brightness}
            onChange={(e) => setBrightness(Number(e.target.value))}
          />
          <output>{brightness}</output>
        </div>
      </div>
      <div className="actions">
        <button type="button" className="primary" onClick={() => void onSave(label, brightness)}>
          {labels.save}
        </button>
      </div>
    </>
  );
}
