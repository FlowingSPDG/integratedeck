import { convertFileSrc, invoke } from "@tauri-apps/api/core";

/** Show backend error strings as-is; unwrap nested invoke errors when needed. */
function formatUserError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object") {
    const o = err as { message?: string; data?: string };
    if (typeof o.message === "string" && o.message.length > 0) return o.message;
    if (typeof o.data === "string" && o.data.length > 0) return o.data;
  }
  return String(err);
}

interface Profile {
  id: string;
  name: string;
  pages: Page[];
  active_page_id?: string;
  surfaces: { surface_id: string; label: string }[];
}

interface Page {
  id: string;
  name: string;
  slots: Record<string, Slot>;
}

interface Slot {
  id: string;
  locator: {
    surface_id: string;
    page_id: string;
    row: number;
    column: number;
  };
  binding?: {
    kind: string;
    plugin_uuid?: string;
    action_uuid?: string;
  };
}

interface VisualState {
  image?: { format: string; data: string };
  title?: string;
  state_index?: number;
}

interface PluginScanEntry {
  path: string;
  name: string;
  bundleName: string;
  uuid?: string;
  version?: string;
}

interface SdPluginRuntime {
  status: string;
  path: string;
  name: string;
  pluginUuid: string;
  port: number;
  piUrl: string;
}

interface PluginRuntimeEntry {
  status: string;
  path: string;
  name: string;
  bundleName: string;
  uuid?: string;
  version?: string;
  port?: number;
  piUrl?: string;
}

interface SurfaceRuntimeEntry {
  surfaceId: string;
  label: string;
  backend: string;
  status: string;
  rows: number;
  columns: number;
  serial?: string;
  kind?: string;
  product?: string;
}

interface UsbDeviceRuntimeEntry {
  serial: string;
  kind: string;
  product: string;
  rows: number;
  columns: number;
  status: string;
  surfaceId?: string;
}

interface ConnectionRecord {
  id: string;
  module_id: string;
  label: string;
  enabled: boolean;
}

interface CompanionRuntimeStatus {
  sidecarRunning: boolean;
  connections: ConnectionRecord[];
  modules: PluginScanEntry[];
}

interface RuntimeStatus {
  sdPlugin?: SdPluginRuntime;
  plugins: PluginRuntimeEntry[];
  surfaces: SurfaceRuntimeEntry[];
  usbDevices: UsbDeviceRuntimeEntry[];
  streamdeckDirs: string[];
  companion: CompanionRuntimeStatus;
}

const app = document.getElementById("app")!;
let selectedSlotId: string | null = null;
let mockSurfaceId: string | null = null;
let activePageId: string | null = null;
let cellVisuals: Record<string, VisualState> = {};

app.innerHTML = `
  <header>
    <h1>integratedeck</h1>
    <span class="status" id="status">Starting…</span>
    <button id="btn-save">Save profile</button>
    <button id="btn-mock-surface">Add mock surface</button>
  </header>
  <aside>
    <div class="section">
      <h2>稼働状況</h2>
      <p id="runtime-updated">更新中…</p>
      <div id="running-plugin-card"></div>
    </div>
    <div class="section">
      <h2>Stream Deck プラグイン</h2>
      <p class="section-desc">スキャンした .sdPlugin の起動・停止を管理します（同時に1つのみ起動）。</p>
      <ul class="plugins" id="sd-plugins"></ul>
      <p class="status" id="scan-status">スキャンでプラグインを検索</p>
      <button type="button" id="btn-scan">プラグインをスキャン</button>
      <button type="button" id="btn-open-plugins-folder">フォルダを開く</button>
    </div>
    <div class="section">
      <h2>接続済みサーフェス</h2>
      <p class="section-desc">アプリが認識しているボタン面（モックまたは物理デバイス）。</p>
      <ul class="plugins" id="surfaces-list"></ul>
    </div>
    <div class="section">
      <h2>物理 Stream Deck (USB)</h2>
      <p class="section-desc">USB 接続の本体。Elgato アプリを終了してから接続してください。</p>
      <ul class="plugins" id="hid-devices"></ul>
      <p class="status" id="hid-status">USB デバイスをスキャン</p>
      <button type="button" id="btn-scan-hid">ハードウェアをスキャン</button>
    </div>
    <div class="section">
      <h2>Companion 接続</h2>
      <p class="section-desc">Bitfocus Companion のモジュール接続です。OBS・vMix など外部機器へのリンク設定で、Stream Deck プラグインとは別物です。</p>
      <p class="status" id="companion-status">Sidecar 状態を確認中…</p>
      <ul class="plugins" id="connections"></ul>
      <button id="btn-add-connection">テスト接続を追加</button>
    </div>
    <div class="section">
      <h2>Companion モジュール</h2>
      <ul class="plugins" id="companion-modules"></ul>
    </div>
    <p class="warn">物理 Stream Deck 利用時は Elgato Stream Deck アプリを終了してください。</p>
  </aside>
  <main>
    <div class="grid" id="grid"></div>
    <button id="btn-trigger">Trigger selected slot</button>
  </main>
  <div class="inspector">
    <div class="section">
      <h2>Property Inspector</h2>
      <p class="status" id="pi-status">Select a slot with an SD binding</p>
      <iframe id="pi-frame" title="Property Inspector" sandbox="allow-scripts allow-same-origin"></iframe>
    </div>
    <div class="section">
      <h2>Bind SD action</h2>
      <label>Plugin UUID <input id="plugin-uuid" type="text" style="width:100%" /></label>
      <label>Action UUID <input id="action-uuid" type="text" style="width:100%" /></label>
      <button id="btn-bind">Bind to selected slot</button>
    </div>
  </div>
`;

const statusEl = document.getElementById("status")!;
const scanStatusEl = document.getElementById("scan-status")!;
const gridEl = document.getElementById("grid")!;
const sdPluginsList = document.getElementById("sd-plugins")!;
const hidDevicesList = document.getElementById("hid-devices")!;
const hidStatusEl = document.getElementById("hid-status")!;
const companionModulesList = document.getElementById("companion-modules")!;
const surfacesList = document.getElementById("surfaces-list")!;
const runningPluginCard = document.getElementById("running-plugin-card")!;
const runtimeUpdatedEl = document.getElementById("runtime-updated")!;
const companionStatusEl = document.getElementById("companion-status")!;


async function refreshCellVisuals() {
  cellVisuals = await invoke<Record<string, VisualState>>("get_cell_visuals");
}

async function pollCellVisuals(times = 8) {
  for (let i = 0; i < times; i++) {
    await refreshCellVisuals();
    await new Promise((r) => setTimeout(r, 100));
  }
}

async function refresh() {
  const info = await invoke<{ version: string; data_dir: string }>("get_app_info");
  statusEl.textContent = `v${info.version} · ${info.data_dir}`;

  const profile = await invoke<Profile>("get_profile");
  activePageId = profile.active_page_id ?? profile.pages[0]?.id ?? null;

  if (!mockSurfaceId && profile.surfaces.length === 0) {
    mockSurfaceId = await invoke<string>("register_mock_surface", { name: "Mock 3×5" });
    await refresh();
    return;
  }

  if (profile.surfaces.length > 0) {
    mockSurfaceId = profile.surfaces[0].surface_id;
  }

  await refreshCellVisuals();
  renderGrid(profile);
  try {
    await refreshRuntimeStatus();
  } catch (e) {
    scanStatusEl.textContent = `状態取得に失敗: ${formatUserError(e)}`;
  }
}

function slotUuid(slot: Slot): string {
  return slot.id;
}

function visualForCell(row: number, col: number): VisualState | undefined {
  return cellVisuals[`${row},${col}`];
}

function renderSlotContent(el: HTMLButtonElement, row: number, col: number, slot?: Slot) {
  el.innerHTML = "";
  const visual = visualForCell(row, col);
  if (visual?.image?.data) {
    const img = document.createElement("img");
    const mime = visual.image.format === "jpeg" ? "jpeg" : "png";
    img.src = `data:image/${mime};base64,${visual.image.data}`;
    img.alt = visual.title ?? "";
    el.appendChild(img);
    if (visual.title) {
      const span = document.createElement("span");
      span.className = "slot-title";
      span.textContent = visual.title;
      el.appendChild(span);
    }
  } else if (visual?.title) {
    el.textContent = visual.title;
  } else {
    el.textContent = slot ? `${row},${col}` : "+";
  }
}

function renderGrid(profile: Profile) {
  gridEl.innerHTML = "";
  const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
  if (!page) return;

  for (let row = 0; row < 3; row++) {
    for (let col = 0; col < 5; col++) {
      const slot = Object.values(page.slots).find(
        (s) => s.locator.row === row && s.locator.column === col,
      );
      const el = document.createElement("button");
      el.className = "slot" + (slot && slotUuid(slot) === selectedSlotId ? " selected" : "");
      el.type = "button";
      el.dataset.row = String(row);
      el.dataset.col = String(col);
      renderSlotContent(el, row, col, slot);
      el.addEventListener("click", () => onSlotClick(page, row, col, slot));
      gridEl.appendChild(el);
    }
  }
}

async function sendSurfaceInput(row: number, col: number, type: "key_down" | "key_up") {
  if (!mockSurfaceId) return;
  await invoke("apply_surface_input", {
    surfaceId: mockSurfaceId,
    inputJson: JSON.stringify({
      type,
      address: { row, column: col },
    }),
  });
}

async function updatePropertyInspector(slot?: Slot) {
  const piStatus = document.getElementById("pi-status")!;
  const piFrame = document.getElementById("pi-frame") as HTMLIFrameElement;
  const actionUuid =
    slot?.binding?.kind === "stream_deck" ? slot.binding.action_uuid : undefined;

  if (!actionUuid) {
    piStatus.textContent = "Select a slot with an SD binding";
    piFrame.removeAttribute("src");
    return;
  }

  const piPath = await invoke<string | null>("get_property_inspector_url", { actionUuid });
  if (piPath) {
    piFrame.src = convertFileSrc(piPath);
    piStatus.textContent = `PI: ${actionUuid}`;
  } else {
    piFrame.removeAttribute("src");
    piStatus.textContent = "No property inspector for this action";
  }
}

async function onSlotClick(page: Page, row: number, col: number, existing?: Slot) {
  if (!mockSurfaceId || !activePageId) return;

  let slot = existing;
  if (!slot) {
    slot = await invoke<Slot>("create_slot", {
      surfaceId: mockSurfaceId,
      pageId: activePageId,
      row,
      column: col,
    });
    await refresh();
    slot =
      Object.values(page.slots).find((s) => s.locator.row === row && s.locator.column === col) ??
      slot;
  }
  selectedSlotId = slotUuid(slot);
  const profile = await invoke<Profile>("get_profile");
  renderGrid(profile);
  await updatePropertyInspector(slot);

  if (slot.binding?.kind === "stream_deck") {
    await sendSurfaceInput(row, col, "key_down");
    await sendSurfaceInput(row, col, "key_up");
    await refreshCellVisuals();
    renderGrid(profile);
  }
}

function pluginLabel(p: PluginScanEntry): string {
  if (p.name !== p.bundleName) {
    return `${escapeHtml(p.name)} <span class="muted">(${escapeHtml(p.bundleName)})</span>`;
  }
  return escapeHtml(p.name);
}

function statusBadge(status: string): string {
  const labels: Record<string, string> = {
    running: "起動中",
    stopped: "停止",
    connected: "接続済み",
    available: "未接続",
    disconnected: "切断",
  };
  const cls =
    status === "running" || status === "connected"
      ? status === "running"
        ? "badge-running"
        : "badge-connected"
      : status === "stopped" || status === "available"
        ? status === "stopped"
          ? "badge-stopped"
          : "badge-available"
        : "badge-disabled";
  return `<span class="badge ${cls}">${labels[status] ?? status}</span>`;
}

function backendLabel(backend: string): string {
  const labels: Record<string, string> = {
    mock: "モック",
    stream_deck_hid: "物理 SD",
    companion_sidecar: "Companion",
  };
  return labels[backend] ?? backend;
}

function renderRunningPluginCard(sd?: SdPluginRuntime) {
  if (!sd) {
    runningPluginCard.innerHTML =
      '<p class="status">起動中の Stream Deck プラグインはありません</p>';
    return;
  }
  runningPluginCard.innerHTML = `
    <div class="instance-card">
      <div class="card-title">${escapeHtml(sd.name)} ${statusBadge(sd.status)}</div>
      <div class="card-meta">
        UUID: ${escapeHtml(sd.pluginUuid)}<br/>
        WebSocket: ${escapeHtml(sd.piUrl)} (port ${sd.port})<br/>
        ${escapeHtml(sd.path)}
      </div>
      <div class="card-actions">
        <button type="button" class="btn-sm" id="btn-stop-plugin">停止</button>
      </div>
    </div>`;
  document.getElementById("btn-stop-plugin")?.addEventListener("click", () => {
    void stopPlugin();
  });
}

function renderPluginList(plugins: PluginRuntimeEntry[], scanDirs: string[]) {
  sdPluginsList.innerHTML =
    plugins
      .map((p) => {
        const running = p.status === "running";
        const meta = [p.version, p.uuid].filter(Boolean).join(" · ");
        const title = meta ? `${p.path}\n${meta}` : p.path;
        const connInfo =
          running && p.port
            ? `<div class="card-meta">port ${p.port}${p.piUrl ? ` · ${escapeHtml(p.piUrl)}` : ""}</div>`
            : "";
        const action = running
          ? `<button type="button" class="btn-sm btn-stop-plugin" data-path="${encodeURIComponent(p.path)}">停止</button>`
          : `<button type="button" class="btn-sm btn-load-plugin" data-path="${encodeURIComponent(p.path)}">起動</button>`;
        return `<li class="runtime-item" title="${escapeHtml(title)}">
          <div class="item-row">
            <span class="item-name">${pluginLabel({ ...p, bundleName: p.bundleName })}</span>
            ${statusBadge(p.status)}
          </div>
          ${connInfo}
          <div class="card-actions">${action}</div>
        </li>`;
      })
      .join("") ||
    `<li class="empty">プラグインが見つかりません。以下に .sdPlugin を配置:\n${escapeHtml(scanDirs.join("\n"))}</li>`;

  scanStatusEl.textContent =
    plugins.length === 0
      ? `プラグインなし（配置先:\n${scanDirs.join("\n")}）`
      : `${plugins.length} 件 · 起動中 ${plugins.filter((p) => p.status === "running").length} 件`;
}

function renderSurfaces(surfaces: SurfaceRuntimeEntry[]) {
  surfacesList.innerHTML =
    surfaces
      .map((s) => {
        const details = [
          backendLabel(s.backend),
          `${s.rows}×${s.columns}`,
          s.serial ? `S/N ${s.serial}` : null,
          s.product ?? null,
        ]
          .filter(Boolean)
          .join(" · ");
        const disconnectBtn =
          s.backend === "stream_deck_hid" && s.status === "connected"
            ? `<button type="button" class="btn-sm btn-disconnect-surface" data-surface-id="${escapeHtml(s.surfaceId)}">切断</button>`
            : "";
        return `<li class="runtime-item">
          <div class="item-row">
            <span class="item-name">${escapeHtml(s.label)}</span>
            ${statusBadge(s.status)}
          </div>
          <div class="card-meta">${escapeHtml(details)}<br/>ID: ${escapeHtml(s.surfaceId)}</div>
          ${disconnectBtn ? `<div class="card-actions">${disconnectBtn}</div>` : ""}
        </li>`;
      })
      .join("") || '<li class="empty">接続済みサーフェスなし</li>';
}

function renderUsbDevices(devices: UsbDeviceRuntimeEntry[]) {
  hidDevicesList.innerHTML =
    devices
      .map((d) => {
        const connected = d.status === "connected";
        const action = connected
          ? `<span class="muted">surface ${escapeHtml(d.surfaceId ?? "")}</span>`
          : `<button type="button" class="btn-sm btn-connect-hid" data-serial="${encodeURIComponent(d.serial)}" data-kind="${escapeHtml(d.kind)}">接続</button>`;
        return `<li class="runtime-item" title="${escapeHtml(d.serial)}">
          <div class="item-row">
            <span class="item-name">${escapeHtml(d.product)} <span class="muted">(${d.rows}×${d.columns})</span></span>
            ${statusBadge(d.status)}
          </div>
          <div class="card-actions">${action}</div>
        </li>`;
      })
      .join("") || '<li class="empty">USB の Stream Deck が見つかりません</li>';
  hidStatusEl.textContent =
    devices.length === 0
      ? "USB デバイスなし。Elgato アプリを終了して再接続してください。"
      : `${devices.length} 台検出 · 接続済み ${devices.filter((d) => d.status === "connected").length} 台`;
}

function renderCompanionSection(companion: CompanionRuntimeStatus) {
  companionStatusEl.textContent = companion.sidecarRunning
    ? "Sidecar: 稼働中"
    : "Sidecar: 停止（Companion モジュール実行用）";

  companionModulesList.innerHTML =
    companion.modules
      .map(
        (p) =>
          `<li class="plugin-item" title="${escapeHtml(p.path)}">${escapeHtml(p.name)}</li>`,
      )
      .join("") || '<li class="empty">Companion モジュールなし</li>';

  const el = document.getElementById("connections")!;
  el.innerHTML =
    companion.connections
      .map(
        (c) =>
          `<li class="runtime-item">
            <div class="item-row">
              <span class="item-name">${escapeHtml(c.label)}</span>
              ${statusBadge(c.enabled ? "connected" : "disconnected")}
            </div>
            <div class="card-meta">module: ${escapeHtml(c.module_id)}<br/>id: ${escapeHtml(c.id)}</div>
          </li>`,
      )
      .join("") || '<li class="empty">Companion 接続なし（外部機器リンク未設定）</li>';
}

function renderRuntimeStatus(status: RuntimeStatus, scanDirs: string[]) {
  renderRunningPluginCard(status.sdPlugin);
  renderPluginList(status.plugins, scanDirs);
  renderSurfaces(status.surfaces);
  renderUsbDevices(status.usbDevices);
  renderCompanionSection(status.companion);

  const activeSurface = status.surfaces.find((s) => s.status === "connected");
  if (activeSurface && mockSurfaceId !== activeSurface.surfaceId) {
    mockSurfaceId = activeSurface.surfaceId;
  }
}

async function refreshRuntimeStatus(): Promise<RuntimeStatus> {
  const status = await invoke<RuntimeStatus>("get_runtime_status");
  renderRuntimeStatus(status, status.streamdeckDirs);
  const now = new Date();
  runtimeUpdatedEl.textContent = `最終更新 ${now.toLocaleTimeString()}`;
  return status;
}

async function loadPlugin(path: string) {
  statusEl.textContent = `起動中…`;
  const loaded = await invoke<{ pluginUuid: string; port: number; name: string }>("load_sd_plugin", {
    path,
  });
  statusEl.textContent = `${loaded.name} を起動 (port ${loaded.port})`;
  (document.getElementById("plugin-uuid") as HTMLInputElement).value = loaded.pluginUuid;
  await refreshRuntimeStatus();
}

async function stopPlugin() {
  await invoke("unload_sd_plugin");
  statusEl.textContent = "プラグインを停止しました";
  await refreshRuntimeStatus();
}

async function connectHid(serial: string, kind: string) {
  hidStatusEl.textContent = `接続中…`;
  const connected = await invoke<{
    surfaceId: string;
    label: string;
    rows: number;
    columns: number;
  }>("connect_hid_device", { serial, kind });
  mockSurfaceId = connected.surfaceId;
  statusEl.textContent = `物理サーフェス: ${connected.label}`;
  await refresh();
}

async function disconnectSurface(surfaceId: string) {
  await invoke("disconnect_hid_device", { surfaceId });
  if (mockSurfaceId === surfaceId) {
    mockSurfaceId = null;
  }
  statusEl.textContent = "デバイスを切断しました";
  await refresh();
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

async function runScan() {
  const btn = document.getElementById("btn-scan") as HTMLButtonElement;
  btn.disabled = true;
  scanStatusEl.textContent = "スキャン中…";
  try {
    await refreshRuntimeStatus();
  } catch (e) {
    const msg = formatUserError(e);
    scanStatusEl.textContent = `スキャン失敗: ${msg}`;
    statusEl.textContent = `スキャン失敗: ${msg}`;
    throw e;
  } finally {
    btn.disabled = false;
  }
}

sdPluginsList.addEventListener("click", async (ev) => {
  const loadBtn = (ev.target as HTMLElement).closest<HTMLElement>(".btn-load-plugin");
  const stopBtn = (ev.target as HTMLElement).closest<HTMLElement>(".btn-stop-plugin");
  if (loadBtn?.dataset.path) {
    try {
      await loadPlugin(decodeURIComponent(loadBtn.dataset.path));
    } catch (e) {
      statusEl.textContent = `起動失敗: ${formatUserError(e)}`;
    }
    return;
  }
  if (stopBtn?.dataset.path) {
    try {
      await stopPlugin();
    } catch (e) {
      statusEl.textContent = `停止失敗: ${formatUserError(e)}`;
    }
  }
});

hidDevicesList.addEventListener("click", async (ev) => {
  const btn = (ev.target as HTMLElement).closest<HTMLElement>(".btn-connect-hid");
  if (!btn?.dataset.serial || !btn.dataset.kind) return;
  try {
    await connectHid(decodeURIComponent(btn.dataset.serial), btn.dataset.kind);
  } catch (e) {
    const msg = formatUserError(e);
    hidStatusEl.textContent = msg;
    statusEl.textContent = msg;
  }
});

surfacesList.addEventListener("click", async (ev) => {
  const btn = (ev.target as HTMLElement).closest<HTMLElement>(".btn-disconnect-surface");
  if (!btn?.dataset.surfaceId) return;
  try {
    await disconnectSurface(btn.dataset.surfaceId);
  } catch (e) {
    statusEl.textContent = `切断失敗: ${formatUserError(e)}`;
  }
});

document.getElementById("btn-scan")!.addEventListener("click", () => {
  void runScan();
});

document.getElementById("btn-scan-hid")!.addEventListener("click", () => {
  void runScan();
});

document.getElementById("btn-open-plugins-folder")!.addEventListener("click", async () => {
  try {
    const dir = await invoke<string>("open_plugins_folder");
    scanStatusEl.textContent = `Opened plugins folder:\n${dir}`;
  } catch (e) {
    scanStatusEl.textContent = `Could not open folder: ${e}`;
  }
});
document.getElementById("btn-save")!.addEventListener("click", async () => {
  await invoke("save_profile");
  statusEl.textContent = "Profile saved";
});

document.getElementById("btn-mock-surface")!.addEventListener("click", async () => {
  mockSurfaceId = await invoke("register_mock_surface", { name: "Mock surface" });
  await refresh();
});

document.getElementById("btn-bind")!.addEventListener("click", async () => {
  if (!selectedSlotId) return;
  const pluginUuid = (document.getElementById("plugin-uuid") as HTMLInputElement).value;
  const actionUuid = (document.getElementById("action-uuid") as HTMLInputElement).value;
  await invoke("bind_slot_sd", {
    slotId: selectedSlotId,
    pluginUuid,
    actionUuid,
  });
  statusEl.textContent = "Bound SD action";
  await pollCellVisuals();
  const profile = await invoke<Profile>("get_profile");
  renderGrid(profile);
  const slot = Object.values(
    profile.pages.find((p) => p.id === activePageId)?.slots ?? {},
  ).find((s) => s.id === selectedSlotId);
  await updatePropertyInspector(slot);
});

document.getElementById("btn-trigger")!.addEventListener("click", async () => {
  if (!selectedSlotId) return;
  await invoke("trigger_slot", { slotId: selectedSlotId });
  await pollCellVisuals();
  renderGrid(await invoke<Profile>("get_profile"));
});

document.getElementById("btn-add-connection")!.addEventListener("click", async () => {
  await invoke("add_connection", {
    moduleId: "companion-module-generic-test",
    label: "テスト接続",
    config: {},
  });
  await refreshRuntimeStatus();
  const ping = await invoke<string | null>("sidecar_ping");
  if (ping) statusEl.textContent = `Sidecar ping: ${ping}`;
});

refresh().catch((e) => {
  statusEl.textContent = formatUserError(e);
});

setInterval(() => {
  void refreshRuntimeStatus().catch(() => {
    /* keep polling */
  });
}, 3000);
