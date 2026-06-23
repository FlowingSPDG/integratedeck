import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beginPiLoad, loadPiInFrame } from "./pi-webview";
import { installNativeFeel } from "./native-feel";

installNativeFeel();

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
  parent_page_id?: string;
}

interface SlotAppearance {
  title?: string;
  default_image?: { format: string; data: string };
}

interface Slot {
  id: string;
  locator: {
    surface_id: string;
    page_id: string;
    row: number;
    column: number;
  };
  label?: string;
  appearance?: SlotAppearance;
  binding?: Record<string, unknown>;
}

interface NormalizedBinding {
  type: "stream_deck" | "companion" | "builtin" | "multi_action";
  pluginUuid?: string;
  actionUuid?: string;
  connectionId?: string;
  actionId?: string;
  settings?: unknown;
  options?: unknown;
  steps?: MultiActionStep[];
  delayMs?: number;
}

interface MultiActionStep {
  binding: Record<string, unknown>;
  delay_before_ms?: number;
}

const BUILTIN_OPEN_FOLDER = "com.elgato.streamdeck.profile.openchild";
const BUILTIN_BACK = "com.elgato.streamdeck.profile.backtoparent";
const BUILTIN_SWITCH_PAGE = "com.elgato.streamdeck.profile.rotate";
const BUILTIN_MULTI = "com.elgato.streamdeck.multiactions.routine";

function normalizeBinding(binding?: Record<string, unknown>): NormalizedBinding | null {
  if (!binding) return null;

  const nested = binding.kind;
  if (typeof nested === "string") {
    if (nested === "stream_deck" && binding.plugin_uuid && binding.action_uuid) {
      return {
        type: "stream_deck",
        pluginUuid: String(binding.plugin_uuid),
        actionUuid: String(binding.action_uuid),
        settings: binding.settings,
      };
    }
    if (nested === "companion" && binding.connection_id && binding.action_id) {
      return {
        type: "companion",
        connectionId: String(binding.connection_id),
        actionId: String(binding.action_id),
        options: binding.options,
      };
    }
    if (nested === "built_in" && binding.action_id) {
      return {
        type: "builtin",
        actionId: String(binding.action_id),
        settings: binding.settings,
      };
    }
    if (nested === "multi_action") {
      return {
        type: "multi_action",
        steps: (binding.steps as MultiActionStep[]) ?? [],
        delayMs: typeof binding.delay_ms === "number" ? binding.delay_ms : 200,
      };
    }
  }

  if (nested && typeof nested === "object") {
    const tag = (nested as { kind?: string }).kind;
    if (tag === "built_in") {
      const n = nested as { action_id?: string; settings?: unknown };
      if (n.action_id) {
        return { type: "builtin", actionId: n.action_id, settings: n.settings };
      }
    }
    if (tag === "multi_action") {
      const n = nested as { steps?: MultiActionStep[]; delay_ms?: number };
      return {
        type: "multi_action",
        steps: n.steps ?? [],
        delayMs: n.delay_ms ?? 200,
      };
    }
    if (tag === "stream_deck") {
      const n = nested as { plugin_uuid?: string; action_uuid?: string; settings?: unknown };
      if (n.plugin_uuid && n.action_uuid) {
        return {
          type: "stream_deck",
          pluginUuid: n.plugin_uuid,
          actionUuid: n.action_uuid,
          settings: n.settings,
        };
      }
    }
    if (tag === "companion") {
      const n = nested as { connection_id?: string; action_id?: string; options?: unknown };
      if (n.connection_id && n.action_id) {
        return {
          type: "companion",
          connectionId: n.connection_id,
          actionId: n.action_id,
          options: n.options,
        };
      }
    }
  }

  return null;
}

interface VisualState {
  image?: { format: string; data: string };
  title?: string;
  state_index?: number;
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

interface PluginLibraryEntry {
  id: string;
  name: string;
  path: string;
  source: string;
  status: string;
  port?: number;
  actions: { id: string; name: string; source: string }[];
}

interface ActionLibrary {
  streamdeck: PluginLibraryEntry[];
  companion: PluginLibraryEntry[];
}

interface RuntimeStatus {
  sdPlugins: { pluginUuid: string; name: string; status: string; port: number }[];
  plugins: {
    path: string;
    name: string;
    status: string;
    uuid?: string;
  }[];
  surfaces: SurfaceRuntimeEntry[];
  usbDevices: UsbDeviceRuntimeEntry[];
  streamdeckDirs: string[];
  companion: {
    companionHostRunning: boolean;
    connections: { id: string; module_id: string; label: string; enabled: boolean }[];
    modules: { path: string; name: string }[];
  };
}

interface ConflictingApp {
  id: "stream_deck" | "companion" | string;
  displayName: string;
  processName: string;
}

interface StartupConflictReport {
  conflicts: ConflictingApp[];
}

interface DragActionPayload {
  source: "streamdeck" | "companion" | "builtin";
  pluginId: string;
  actionId: string;
  actionName: string;
  pluginPath?: string;
}

const app = document.getElementById("app")!;

let selectedSlotId: string | null = null;
let selectedSurfaceId: string | null = null;
let activePageId: string | null = null;
let gridRows = 3;
let gridCols = 5;
let cellVisuals: Record<string, VisualState> = {};
let actionLibrary: ActionLibrary = { streamdeck: [], companion: [] };
let expandedGroups = new Set<string>();
let isDraggingAction = false;
let suppressSettingsAutosave = false;
let settingsAutosaveTimer: ReturnType<typeof setTimeout> | null = null;

const DRAG_THRESHOLD_PX = 6;
let dragGhost: HTMLElement | null = null;
let activeDragPayload: DragActionPayload | null = null;

app.innerHTML = `
  <header class="toolbar">
    <span class="toolbar-brand">integratedeck</span>
    <div class="toolbar-select">
      <label>デバイス</label>
      <select id="device-picker"></select>
    </div>
    <div class="toolbar-select">
      <label>プロファイル</label>
      <select id="page-picker"></select>
    </div>
    <span class="toolbar-spacer"></span>
    <span class="toolbar-status" id="status">Starting…</span>
    <button type="button" id="btn-settings">⚙</button>
  </header>
  <div id="startup-conflict-banner" class="startup-conflict-banner hidden" role="alert" aria-live="polite"></div>
  <div class="workspace">
    <div class="center-panel">
      <div class="device-frame">
        <div class="device-shell">
          <div class="device-label" id="device-label">Stream Deck</div>
          <div class="grid" id="grid"></div>
        </div>
        <div class="page-nav" id="page-nav"></div>
      </div>
      <div class="config-panel" id="config-panel">
        <p class="config-placeholder" id="config-placeholder">キーを選択してアクションを設定してください</p>
        <div id="slot-appearance" class="slot-appearance hidden">
          <h2>キーの表示</h2>
          <div class="appearance-fields">
            <label class="field-inline">タイトル
              <input type="text" id="slot-title-input" placeholder="ボタンタイトル" />
            </label>
            <label class="field-inline">デフォルト画像
              <input type="file" id="slot-image-input" accept="image/png,image/jpeg,image/webp" />
            </label>
            <button type="button" id="btn-clear-slot-image" class="btn-xs">画像をクリア</button>
            <div id="slot-image-preview" class="slot-image-preview"></div>
          </div>
        </div>
        <div id="multi-action-editor" class="multi-action-editor hidden">
          <h2>マルチアクション</h2>
          <p class="muted">アクションリストからステップをドラッグして追加</p>
          <ul id="multi-action-steps" class="multi-action-steps"></ul>
          <label class="field-inline">ステップ間隔 (ms)
            <input type="number" id="multi-action-delay" min="0" max="10000" value="200" />
          </label>
        </div>
        <div id="builtin-settings" class="builtin-settings hidden">
          <h2>ナビゲーション設定</h2>
          <div id="builtin-settings-body"></div>
        </div>
        <div id="pi-section" class="hidden">
          <h2 id="pi-title">Property Inspector</h2>
          <p class="muted" id="pi-status"></p>
          <iframe id="pi-frame" title="Property Inspector" sandbox="allow-scripts allow-same-origin"></iframe>
          <div id="native-settings" class="hidden">
            <label>Settings JSON
              <textarea id="settings-json" rows="4" style="width:100%"></textarea>
            </label>
          </div>
        </div>
      </div>
    </div>
    <aside class="action-sidebar">
      <div class="action-sidebar-header">
        <h2>Actions</h2>
        <input type="search" id="action-search" placeholder="アクションを検索…" />
      </div>
      <div class="action-list" id="action-list"></div>
    </aside>
  </div>
`;

const statusEl = document.getElementById("status")!;
const gridEl = document.getElementById("grid")!;
const devicePicker = document.getElementById("device-picker") as HTMLSelectElement;
const pagePicker = document.getElementById("page-picker") as HTMLSelectElement;
const actionListEl = document.getElementById("action-list")!;
const configPlaceholder = document.getElementById("config-placeholder")!;
const piSection = document.getElementById("pi-section")!;
const startupConflictBanner = document.getElementById("startup-conflict-banner")!;

function payloadFromActionEl(el: HTMLElement): DragActionPayload {
  return {
    source: el.dataset.source as "streamdeck" | "companion" | "builtin",
    pluginId: el.dataset.pluginId!,
    actionId: el.dataset.actionId!,
    actionName: el.dataset.actionName!,
    pluginPath: el.dataset.pluginPath
      ? decodeURIComponent(el.dataset.pluginPath)
      : undefined,
  };
}

function bindingStepLabel(step: MultiActionStep): string {
  const b = step.binding as Record<string, unknown>;
  const kind = b.kind as string | undefined;
  if (kind === "stream_deck") {
    return `SD: ${String(b.action_uuid ?? "?")}`;
  }
  if (kind === "companion") {
    return `Companion: ${String(b.action_id ?? "?")}`;
  }
  if (kind === "built_in") {
    return `Built-in: ${String(b.action_id ?? "?")}`;
  }
  return "Step";
}

function folderChildPageId(settings: unknown): string | null {
  if (!settings || typeof settings !== "object") return null;
  const s = settings as Record<string, unknown>;
  const id = s.childPageId ?? s.child_page_id ?? s.ProfileUUID ?? s.profile_uuid;
  return typeof id === "string" ? id : null;
}

function clearDragHighlights() {
  gridEl.querySelectorAll(".slot.drag-over").forEach((el) => el.classList.remove("drag-over"));
}

function findSlotAt(clientX: number, clientY: number): HTMLElement | null {
  const hit = document.elementFromPoint(clientX, clientY);
  const direct = hit?.closest(".slot") as HTMLElement | null;
  if (direct) return direct;

  const rect = gridEl.getBoundingClientRect();
  if (
    clientX < rect.left ||
    clientX > rect.right ||
    clientY < rect.top ||
    clientY > rect.bottom
  ) {
    return null;
  }

  const gap = 8;
  const cellSize = parseInt(getComputedStyle(document.documentElement).getPropertyValue("--slot-size")) || 72;
  const relX = clientX - rect.left;
  const relY = clientY - rect.top;
  const col = Math.floor(relX / (cellSize + gap));
  const row = Math.floor(relY / (cellSize + gap));
  if (col < 0 || col >= gridCols || row < 0 || row >= gridRows) return null;

  return gridEl.querySelector(
    `.slot[data-row="${row}"][data-col="${col}"]`,
  ) as HTMLElement | null;
}

function setDropHighlight(slotEl: HTMLElement | null) {
  clearDragHighlights();
  slotEl?.classList.add("drag-over");
}

function createDragGhost(name: string): HTMLElement {
  const ghost = document.createElement("div");
  ghost.className = "drag-ghost";
  ghost.textContent = name;
  document.body.appendChild(ghost);
  return ghost;
}

function positionGhost(ghost: HTMLElement, x: number, y: number) {
  ghost.style.left = `${x + 12}px`;
  ghost.style.top = `${y + 12}px`;
}

function cleanupPointerDrag() {
  dragGhost?.remove();
  dragGhost = null;
  activeDragPayload = null;
  isDraggingAction = false;
  clearDragHighlights();
  document.body.classList.remove("action-dragging");
}

function beginActionPointerDrag(e: PointerEvent, itemEl: HTMLElement) {
  if (e.button !== 0) return;

  const payload = payloadFromActionEl(itemEl);
  const startX = e.clientX;
  const startY = e.clientY;
  let dragging = false;

  const onMove = (ev: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD_PX) return;
      dragging = true;
      isDraggingAction = true;
      activeDragPayload = payload;
      dragGhost = createDragGhost(payload.actionName);
      document.body.classList.add("action-dragging");
    }
    if (dragGhost) {
      positionGhost(dragGhost, ev.clientX, ev.clientY);
      setDropHighlight(findSlotAt(ev.clientX, ev.clientY));
    }
  };

  const onUp = (ev: PointerEvent) => {
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerup", onUp);
    document.removeEventListener("pointercancel", onUp);

    if (dragging && activeDragPayload) {
      const hit = document.elementFromPoint(ev.clientX, ev.clientY);
      const multiEditor = document.getElementById("multi-action-editor");
      if (
        selectedSlotId &&
        multiEditor &&
        !multiEditor.classList.contains("hidden") &&
        hit?.closest(".multi-action-editor")
      ) {
        void invoke("add_multi_action_step", {
          args: {
            slotId: selectedSlotId,
            source: activeDragPayload.source,
            pluginId: activeDragPayload.pluginId,
            actionId: activeDragPayload.actionId,
          },
        }).then(() => refresh());
      } else {
        const slotEl = findSlotAt(ev.clientX, ev.clientY);
        if (slotEl) {
          const row = Number(slotEl.dataset.row);
          const col = Number(slotEl.dataset.col);
          if (!Number.isNaN(row) && !Number.isNaN(col)) {
            void applyActionToCell(row, col, activeDragPayload);
          } else {
            statusEl.textContent = "ドロップ先のキーを特定できませんでした";
          }
        } else {
          statusEl.textContent = "キー上で離してください";
        }
      }
    }
    cleanupPointerDrag();
  };

  document.addEventListener("pointermove", onMove);
  document.addEventListener("pointerup", onUp);
  document.addEventListener("pointercancel", onUp);
}

function setupActionPointerDrag() {
  actionListEl.addEventListener("pointerdown", (ev) => {
    const item = (ev.target as HTMLElement).closest(".action-item");
    if (!item) return;
    ev.preventDefault();
    beginActionPointerDrag(ev as PointerEvent, item as HTMLElement);
  });
}

setupActionPointerDrag();

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function statusBadge(status: string): string {
  const labels: Record<string, string> = {
    running: "起動中",
    stopped: "停止",
    connected: "接続済",
    available: "未接続",
  };
  const cls =
    status === "running" || status === "connected" ? "badge-running" : "badge-stopped";
  return `<span class="badge ${cls}">${labels[status] ?? status}</span>`;
}

function visualForCell(row: number, col: number): VisualState | undefined {
  return cellVisuals[`${row},${col}`];
}

function updateGridCss() {
  gridEl.style.gridTemplateColumns = `repeat(${gridCols}, var(--slot-size))`;
  gridEl.style.gridTemplateRows = `repeat(${gridRows}, var(--slot-size))`;
}

async function refreshCellVisuals() {
  cellVisuals = await invoke<Record<string, VisualState>>("get_cell_visuals");
}

async function pollCellVisuals(times = 8) {
  for (let i = 0; i < times; i++) {
    await refreshCellVisuals();
    await new Promise((r) => setTimeout(r, 100));
  }
}

async function refreshActionLibrary() {
  if (isDraggingAction) return;
  actionLibrary = await invoke<ActionLibrary>("list_action_library");
  renderActionSidebar();
}

function renderSlotContent(el: HTMLElement, row: number, col: number, slot?: Slot) {
  el.innerHTML = "";
  const visual = visualForCell(row, col);
  const defaultImg = slot?.appearance?.default_image;
  const imgData = visual?.image?.data ?? defaultImg?.data;
  const imgFormat = visual?.image?.format ?? defaultImg?.format ?? "png";
  const title = visual?.title ?? slot?.appearance?.title ?? slot?.label;

  if (imgData) {
    const img = document.createElement("img");
    const mime = imgFormat === "jpeg" ? "jpeg" : "png";
    img.src = `data:image/${mime};base64,${imgData}`;
    img.alt = title ?? "";
    el.appendChild(img);
    if (title) {
      const span = document.createElement("span");
      span.className = "slot-title";
      span.textContent = title;
      el.appendChild(span);
    }
  } else if (title) {
    el.textContent = title;
  } else if (slot?.binding) {
    const b = normalizeBinding(slot.binding);
    if (b?.type === "builtin") {
      el.textContent =
        b.actionId === BUILTIN_OPEN_FOLDER
          ? "📁"
          : b.actionId === BUILTIN_BACK
            ? "◀"
            : b.actionId === BUILTIN_SWITCH_PAGE
              ? "⇄"
              : b.actionId === BUILTIN_MULTI
                ? "⚡"
                : "Nav";
    } else {
      el.textContent = b?.type === "stream_deck" ? "SD" : b?.type === "companion" ? "Comp" : "";
    }
  } else {
    el.textContent = "";
  }
}

function findSlot(page: Page, row: number, col: number, surfaceId: string): Slot | undefined {
  return Object.values(page.slots).find(
    (s) =>
      s.locator.row === row &&
      s.locator.column === col &&
      s.locator.surface_id === surfaceId,
  );
}

function renderGrid(profile: Profile) {
  gridEl.innerHTML = "";
  updateGridCss();
  const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
  if (!page || !selectedSurfaceId) return;

  for (let row = 0; row < gridRows; row++) {
    for (let col = 0; col < gridCols; col++) {
      const slot = findSlot(page, row, col, selectedSurfaceId);
      const el = document.createElement("div");
      el.className =
        "slot" + (slot && slot.id === selectedSlotId ? " selected" : "");
      el.setAttribute("role", "button");
      el.tabIndex = 0;
      el.dataset.row = String(row);
      el.dataset.col = String(col);
      renderSlotContent(el, row, col, slot);

      el.addEventListener("click", () => void onSlotClick(page, row, col, slot));
      el.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" || ev.key === " ") {
          ev.preventDefault();
          void onSlotClick(page, row, col, slot);
        }
      });

      gridEl.appendChild(el);
    }
  }
}

function renderPagePicker(profile: Profile) {
  const activePage = profile.pages.find((p) => p.id === activePageId);
  const breadcrumb: string[] = [];
  let cursor = activePage;
  while (cursor) {
    breadcrumb.unshift(cursor.name);
    cursor = cursor.parent_page_id
      ? profile.pages.find((p) => p.id === cursor!.parent_page_id)
      : undefined;
  }

  pagePicker.innerHTML = profile.pages
    .map(
      (p) =>
        `<option value="${p.id}"${p.id === activePageId ? " selected" : ""}>${escapeHtml(p.name)}</option>`,
    )
    .join("");

  const pageNav = document.getElementById("page-nav")!;
  const backBtn =
    activePage?.parent_page_id
      ? `<button type="button" class="page-btn page-back" data-page-id="${activePage.parent_page_id}">◀</button>`
      : "";
  pageNav.innerHTML = `
    ${backBtn}
    <span class="page-breadcrumb">${escapeHtml(breadcrumb.join(" / "))}</span>
    ${profile.pages
      .filter((p) => !p.parent_page_id)
      .map(
        (p, i) =>
          `<button type="button" class="page-btn${p.id === activePageId ? " page-active" : ""}" data-page-id="${p.id}">${i + 1}</button>`,
      )
      .join("")}
  `;
  pageNav.querySelectorAll(".page-btn[data-page-id]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const pageId = (btn as HTMLElement).dataset.pageId!;
      void setActivePage(pageId);
    });
  });
}

function renderDevicePicker(surfaces: SurfaceRuntimeEntry[]) {
  const connected = surfaces.filter((s) => s.status === "connected");
  if (
    connected.length > 0 &&
    (!selectedSurfaceId || !connected.some((s) => s.surfaceId === selectedSurfaceId))
  ) {
    const physical = connected.find((s) => s.backend !== "mock");
    selectedSurfaceId = (physical ?? connected[0]).surfaceId;
  }

  devicePicker.innerHTML = connected
    .map(
      (s) =>
        `<option value="${s.surfaceId}"${s.surfaceId === selectedSurfaceId ? " selected" : ""}>${escapeHtml(s.label)} (${s.rows}×${s.columns})</option>`,
    )
    .join("");

  if (selectedSurfaceId) {
    devicePicker.value = selectedSurfaceId;
  }
  const selected = connected.find((s) => s.surfaceId === selectedSurfaceId);
  const labelEl = document.getElementById("device-label")!;
  if (selected) {
    labelEl.textContent = selected.label;
    gridRows = selected.rows;
    gridCols = selected.columns;
  }
}

function filterActions(query: string, entries: PluginLibraryEntry[]): PluginLibraryEntry[] {
  const q = query.toLowerCase().trim();
  if (!q) return entries;
  return entries
    .map((entry) => ({
      ...entry,
      actions: entry.actions.filter(
        (a) =>
          a.name.toLowerCase().includes(q) ||
          a.id.toLowerCase().includes(q) ||
          entry.name.toLowerCase().includes(q),
      ),
    }))
    .filter((e) => e.actions.length > 0 || e.name.toLowerCase().includes(q));
}

function renderPluginGroup(entry: PluginLibraryEntry, sectionKey: string): string {
  const groupKey = `${sectionKey}:${entry.id || entry.path}`;
  const isExpanded = expandedGroups.has(groupKey);

  const startStop =
    entry.source === "streamdeck"
      ? entry.status === "running"
        ? `<button type="button" class="btn-xs btn-stop-plugin" data-uuid="${escapeHtml(entry.id)}">停止</button>`
        : `<button type="button" class="btn-xs btn-load-plugin" data-path="${encodeURIComponent(entry.path)}">起動</button>`
      : "";

  const actionsHtml = isExpanded
    ? entry.actions
        .map(
          (a) =>
            `<div class="action-item"
              data-source="${entry.source}"
              data-plugin-id="${escapeHtml(entry.id)}"
              data-action-id="${escapeHtml(a.id)}"
              data-action-name="${escapeHtml(a.name)}"
              data-plugin-path="${encodeURIComponent(entry.path)}">
              <span class="action-icon"></span>
              <span>${escapeHtml(a.name)}</span>
            </div>`,
        )
        .join("")
    : "";

  return `
    <div class="plugin-group" data-group="${escapeHtml(groupKey)}">
      <div class="plugin-group-header" data-toggle="${escapeHtml(groupKey)}">
        <span class="chevron">${isExpanded ? "▼" : "▶"}</span>
        <span class="plugin-name">${escapeHtml(entry.name)}</span>
        ${statusBadge(entry.status)}
        <span class="plugin-controls">${startStop}</span>
      </div>
      ${isExpanded ? `<div class="plugin-group-actions">${actionsHtml || '<p class="muted">アクションなし</p>'}</div>` : ""}
    </div>`;
}

function renderActionSidebar() {
  const query = (document.getElementById("action-search") as HTMLInputElement).value;
  const sd = filterActions(query, actionLibrary.streamdeck);
  const comp = filterActions(query, actionLibrary.companion);

  actionListEl.innerHTML = `
    <div class="action-section-title">Stream Deck プラグイン (${sd.length})</div>
    ${sd.map((e) => renderPluginGroup(e, "sd")).join("") || '<p class="muted">プラグインなし — 設定からスキャン</p>'}
    <div class="action-section-title">Companion (${comp.length})</div>
    ${comp.map((e) => renderPluginGroup(e, "comp")).join("") || '<p class="muted">Companion 接続なし</p>'}
  `;

  actionListEl.querySelectorAll(".plugin-group-header[data-toggle]").forEach((header) => {
    header.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest(".plugin-controls")) return;
      const key = (header as HTMLElement).dataset.toggle!;
      if (expandedGroups.has(key)) expandedGroups.delete(key);
      else expandedGroups.add(key);
      renderActionSidebar();
    });
  });

  actionListEl.querySelectorAll(".btn-load-plugin").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const path = decodeURIComponent((btn as HTMLElement).dataset.path!);
      void loadPlugin(path);
    });
  });

  actionListEl.querySelectorAll(".btn-stop-plugin").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const uuid = (btn as HTMLElement).dataset.uuid!;
      void stopPlugin(uuid);
    });
  });
}

async function saveSlotAppearance(title?: string, imageBase64?: string, clearImage = false) {
  if (!selectedSlotId) return;
  try {
    await invoke("update_slot_appearance", {
      args: {
        slotId: selectedSlotId,
        title: title ?? null,
        defaultImageBase64: imageBase64 ?? null,
        clearImage,
      },
    });
    await refreshCellVisuals();
    const profile = await invoke<Profile>("get_profile");
    renderGrid(profile);
  } catch (err) {
    statusEl.textContent = `表示設定の保存に失敗: ${formatUserError(err)}`;
  }
}

function renderMultiActionEditor(binding: NormalizedBinding) {
  const editor = document.getElementById("multi-action-editor")!;
  const list = document.getElementById("multi-action-steps")!;
  const delayInput = document.getElementById("multi-action-delay") as HTMLInputElement;
  editor.classList.remove("hidden");
  delayInput.value = String(binding.delayMs ?? 200);
  list.innerHTML =
    (binding.steps ?? [])
      .map(
        (step, i) =>
          `<li class="multi-action-step" data-index="${i}">
            <span>${escapeHtml(bindingStepLabel(step))}</span>
            <button type="button" class="btn-xs btn-remove-step" data-index="${i}">削除</button>
          </li>`,
      )
      .join("") || '<li class="muted">ステップなし — 右の Actions からドラッグ</li>';

  list.querySelectorAll(".btn-remove-step").forEach((btn) => {
    btn.addEventListener("click", () => {
      const idx = Number((btn as HTMLElement).dataset.index);
      const steps = [...(binding.steps ?? [])];
      steps.splice(idx, 1);
      void saveMultiAction(steps, Number(delayInput.value));
    });
  });
}

async function saveMultiAction(steps: MultiActionStep[], delayMs: number) {
  if (!selectedSlotId) return;
  await invoke("update_multi_action", {
    args: { slotId: selectedSlotId, steps, delayMs },
  });
  await refresh();
  const profile = await invoke<Profile>("get_profile");
  const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
  const slot = page ? Object.values(page.slots).find((s) => s.id === selectedSlotId) : undefined;
  if (slot) await updatePropertyInspector(slot);
}

function renderBuiltinSettings(binding: NormalizedBinding, profile: Profile) {
  const panel = document.getElementById("builtin-settings")!;
  const body = document.getElementById("builtin-settings-body")!;
  panel.classList.remove("hidden");

  if (binding.actionId === BUILTIN_SWITCH_PAGE) {
    const current = (binding.settings as Record<string, unknown> | undefined)?.targetPageId ??
      (binding.settings as Record<string, unknown> | undefined)?.target_page_id;
    body.innerHTML = `
      <label>切り替え先ページ
        <select id="switch-page-select">
          ${profile.pages
            .map(
              (p) =>
                `<option value="${p.id}"${p.id === current ? " selected" : ""}>${escapeHtml(p.name)}</option>`,
            )
            .join("")}
        </select>
      </label>`;
    body.querySelector("#switch-page-select")?.addEventListener("change", (ev) => {
      const target = (ev.target as HTMLSelectElement).value;
      if (selectedSlotId) {
        void invoke("set_switch_page_target", {
          args: { slotId: selectedSlotId, targetPageId: target },
        });
      }
    });
    return;
  }

  if (binding.actionId === BUILTIN_OPEN_FOLDER) {
    const childId = folderChildPageId(binding.settings);
    body.innerHTML = `
      <p class="muted">フォルダ内のキーを編集するには、このボタンを選択した状態でグリッドがフォルダページに切り替わります。</p>
      ${childId ? `<button type="button" class="btn-xs" id="btn-open-folder-page">フォルダを開く</button>` : ""}`;
    body.querySelector("#btn-open-folder-page")?.addEventListener("click", () => {
      if (childId) void setActivePage(childId);
    });
    return;
  }

  body.innerHTML = `<p class="muted">${escapeHtml(binding.actionId ?? "")} — 追加設定は不要です</p>`;
}

async function updatePropertyInspector(slot?: Slot) {
  const piStatus = document.getElementById("pi-status")!;
  const piFrame = document.getElementById("pi-frame") as HTMLIFrameElement;
  const nativeSettings = document.getElementById("native-settings")!;
  const settingsJson = document.getElementById("settings-json") as HTMLTextAreaElement;
  const piTitle = document.getElementById("pi-title")!;
  const appearanceSection = document.getElementById("slot-appearance")!;
  const multiEditor = document.getElementById("multi-action-editor")!;
  const builtinPanel = document.getElementById("builtin-settings")!;
  const titleInput = document.getElementById("slot-title-input") as HTMLInputElement;
  const imagePreview = document.getElementById("slot-image-preview")!;
  const binding = normalizeBinding(slot?.binding);

  suppressSettingsAutosave = true;
  if (settingsAutosaveTimer) {
    clearTimeout(settingsAutosaveTimer);
    settingsAutosaveTimer = null;
  }

  multiEditor.classList.add("hidden");
  builtinPanel.classList.add("hidden");

  try {
    if (!selectedSlotId || !slot) {
      configPlaceholder.classList.remove("hidden");
      appearanceSection.classList.add("hidden");
      piSection.classList.add("hidden");
      beginPiLoad(piFrame);
      await invoke("focus_pi_slot", { slotId: null });
      return;
    }

    configPlaceholder.classList.add("hidden");
    appearanceSection.classList.remove("hidden");

    titleInput.value = slot.appearance?.title ?? slot.label ?? "";
    imagePreview.innerHTML = "";
    if (slot.appearance?.default_image?.data) {
      const fmt = slot.appearance.default_image.format === "jpeg" ? "jpeg" : "png";
      imagePreview.innerHTML = `<img src="data:image/${fmt};base64,${slot.appearance.default_image.data}" alt="preview" />`;
    }

    if (!binding) {
      piSection.classList.add("hidden");
      beginPiLoad(piFrame);
      await invoke("focus_pi_slot", { slotId: null });
      return;
    }

    if (binding.type === "multi_action") {
      piSection.classList.add("hidden");
      beginPiLoad(piFrame);
      await invoke("focus_pi_slot", { slotId: null });
      renderMultiActionEditor(binding);
      return;
    }

    if (binding.type === "builtin") {
      piSection.classList.add("hidden");
      beginPiLoad(piFrame);
      await invoke("focus_pi_slot", { slotId: null });
      const profile = await invoke<Profile>("get_profile");
      renderBuiltinSettings(binding, profile);
      return;
    }

    piSection.classList.remove("hidden");
    const piGeneration = beginPiLoad(piFrame);
    await invoke("focus_pi_slot", { slotId: selectedSlotId });

    if (binding.type === "companion") {
      nativeSettings.classList.remove("hidden");
      settingsJson.value = JSON.stringify(binding.options ?? {}, null, 2);
      piTitle.textContent = "Companion アクション設定";
      piStatus.textContent = binding.actionId ?? "";
      return;
    }

    if (!binding.pluginUuid || !binding.actionUuid) {
      piTitle.textContent = "Property Inspector";
      piStatus.textContent = "バインディング情報が不完全です";
      return;
    }

    piTitle.textContent = `Stream Deck: ${binding.actionUuid}`;
    const piPath = await invoke<string | null>("get_property_inspector_url", {
      args: {
        pluginUuid: binding.pluginUuid,
        actionUuid: binding.actionUuid,
      },
    });
    const piCtx = await invoke<{
      port: number;
      context: string;
      actionUuid: string;
      pluginUuid: string;
      deviceId: string;
      settings: Record<string, unknown>;
    } | null>("get_pi_context", { slotId: selectedSlotId });

    if (piPath && piCtx) {
      nativeSettings.classList.add("hidden");
      await loadPiInFrame(
        piFrame,
        piPath,
        {
          port: piCtx.port,
          context: piCtx.context,
          actionUuid: piCtx.actionUuid,
          pluginUuid: piCtx.pluginUuid,
          deviceId: piCtx.deviceId,
          settings: piCtx.settings,
        },
        piGeneration,
      );
      piStatus.textContent = piCtx.context;
    } else if (piCtx) {
      nativeSettings.classList.remove("hidden");
      settingsJson.value = JSON.stringify(binding.settings ?? {}, null, 2);
      piStatus.textContent = "ネイティブ設定（PI HTML なし）";
    } else {
      nativeSettings.classList.add("hidden");
      piStatus.textContent = "PI コンテキストを取得できません（プラグイン未起動？）";
    }
  } finally {
    suppressSettingsAutosave = false;
  }
}

async function saveNativeSettings() {
  if (!selectedSlotId) return;
  const settingsJson = document.getElementById("settings-json") as HTMLTextAreaElement;
  try {
    const settings = JSON.parse(settingsJson.value) as unknown;
    await invoke("update_slot_settings", {
      args: {
        slotId: selectedSlotId,
        settings,
      },
    });
  } catch (err) {
    statusEl.textContent = `設定の保存に失敗: ${formatUserError(err)}`;
  }
}

function scheduleNativeSettingsSave() {
  if (suppressSettingsAutosave || !selectedSlotId) return;
  if (settingsAutosaveTimer) clearTimeout(settingsAutosaveTimer);
  settingsAutosaveTimer = setTimeout(() => {
    settingsAutosaveTimer = null;
    void saveNativeSettings();
  }, 400);
}

async function onSlotClick(page: Page, row: number, col: number, existing?: Slot) {
  if (!selectedSurfaceId || !activePageId) return;

  let slot = existing;
  if (!slot) {
    slot = await invoke<Slot>("create_slot", {
      args: {
        surfaceId: selectedSurfaceId,
        pageId: activePageId,
        row,
        column: col,
      },
    });
    const profile = await invoke<Profile>("get_profile");
    slot = findSlot(
      profile.pages.find((p) => p.id === activePageId) ?? page,
      row,
      col,
      selectedSurfaceId,
    ) ?? slot;
  }

  selectedSlotId = slot.id;

  const binding = normalizeBinding(slot.binding);
  if (binding?.type === "builtin" && binding.actionId === BUILTIN_OPEN_FOLDER) {
    const childId = folderChildPageId(binding.settings);
    if (childId && childId !== activePageId) {
      await setActivePage(childId);
      const profileAfter = await invoke<Profile>("get_profile");
      slot =
        findSlot(
          profileAfter.pages.find((p) => p.id === childId) ?? profileAfter.pages[0],
          row,
          col,
          selectedSurfaceId,
        ) ?? slot;
    }
  }

  const profile = await invoke<Profile>("get_profile");
  renderGrid(profile);
  const selectedSlot =
    profile.pages
      .flatMap((p) => Object.values(p.slots))
      .find((s) => s.id === selectedSlotId) ?? slot;
  await updatePropertyInspector(selectedSlot);
}

async function applyActionToCell(row: number, col: number, payload: DragActionPayload) {
  if (!selectedSurfaceId || !activePageId) {
    statusEl.textContent = "デバイスまたはページが未選択です";
    return;
  }

  const profile = await invoke<Profile>("get_profile");
  const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
  if (!page) return;

  let slot = findSlot(page, row, col, selectedSurfaceId);
  if (!slot) {
    slot = await invoke<Slot>("create_slot", {
      args: {
        surfaceId: selectedSurfaceId,
        pageId: activePageId,
        row,
        column: col,
      },
    });
  }

  selectedSlotId = slot.id;
  statusEl.textContent = `配置中: ${payload.actionName}…`;

  try {
    if (payload.source === "builtin") {
      await invoke("bind_slot_builtin", {
        args: {
          slotId: slot.id,
          actionId: payload.actionId,
        },
      });
    } else if (payload.source === "streamdeck") {
      const plugin = actionLibrary.streamdeck.find((p) => p.id === payload.pluginId);
      if (plugin?.status !== "running" && plugin?.path) {
        await invoke("load_sd_plugin", { path: plugin.path });
        await refreshActionLibrary();
      }
      await invoke("bind_slot_sd", {
        args: {
          slotId: slot.id,
          pluginUuid: payload.pluginId,
          actionUuid: payload.actionId,
        },
      });
    } else {
      await invoke("bind_slot_companion", {
        args: {
          slotId: slot.id,
          connectionId: payload.pluginId,
          actionId: payload.actionId,
          options: {},
        },
      });
    }

    await pollCellVisuals();
    const updatedProfile = await invoke<Profile>("get_profile");
    const updatedSlot = findSlot(
      updatedProfile.pages.find((p) => p.id === activePageId) ?? page,
      row,
      col,
      selectedSurfaceId,
    );
    renderGrid(updatedProfile);
    await updatePropertyInspector(updatedSlot);
    statusEl.textContent = `${payload.actionName} を配置しました`;
  } catch (err) {
    statusEl.textContent = `配置失敗: ${formatUserError(err)}`;
  }
}

async function setActivePage(pageId: string) {
  await invoke("set_active_page", { pageId });
  activePageId = pageId;
  selectedSlotId = null;
  configPlaceholder.classList.remove("hidden");
  document.getElementById("slot-appearance")!.classList.add("hidden");
  document.getElementById("multi-action-editor")!.classList.add("hidden");
  document.getElementById("builtin-settings")!.classList.add("hidden");
  piSection.classList.add("hidden");
  await refresh();
}

async function showStartupConflictsIfNeeded() {
  if (startupConflictBanner.dataset.dismissed === "true") {
    return;
  }

  try {
    const report = await invoke<StartupConflictReport>("check_startup_conflicts");
    if (report.conflicts.length === 0) {
      startupConflictBanner.classList.add("hidden");
      return;
    }

    const appNames = report.conflicts.map((conflict) => {
      if (conflict.id === "stream_deck") {
        return "Elgato Stream Deck（公式アプリ）";
      }
      if (conflict.id === "companion") {
        return "Bitfocus Companion";
      }
      return conflict.displayName;
    });

    startupConflictBanner.innerHTML = `
      <div class="startup-conflict-banner__content">
        <strong>競合の可能性があります</strong>
        <p>${appNames.join(" と ")} が実行中です。Stream Deck デバイスやモジュールの競合が起きる場合があります。使用しないアプリは終了してください。</p>
      </div>
      <button type="button" class="startup-conflict-banner__close" aria-label="警告を閉じる">×</button>
    `;

    startupConflictBanner.classList.remove("hidden");
    startupConflictBanner
      .querySelector(".startup-conflict-banner__close")
      ?.addEventListener("click", () => {
        startupConflictBanner.dataset.dismissed = "true";
        startupConflictBanner.classList.add("hidden");
      });
  } catch (err) {
    console.warn("startup conflict check failed:", err);
  }
}

async function refresh() {
  const info = await invoke<{ version: string; data_dir: string }>("get_app_info");
  statusEl.textContent = `v${info.version}`;

  const profile = await invoke<Profile>("get_profile");
  activePageId = profile.active_page_id ?? profile.pages[0]?.id ?? null;

  if (profile.surfaces.length === 0) {
    selectedSurfaceId = await invoke<string>("register_mock_surface", { name: "Mock 3×5" });
    await refresh();
    return;
  }

  if (!selectedSurfaceId) {
    selectedSurfaceId = profile.surfaces[0].surface_id;
  }

  await refreshCellVisuals();
  renderPagePicker(profile);

  try {
    const status = await invoke<RuntimeStatus>("get_runtime_status");
    renderDevicePicker(status.surfaces);
    renderGrid(profile);
    await refreshActionLibrary();
  } catch (e) {
    statusEl.textContent = formatUserError(e);
  }
}

async function loadPlugin(path: string) {
  statusEl.textContent = "プラグイン起動中…";
  const loaded = await invoke<{ pluginUuid: string; name: string }>("load_sd_plugin", { path });
  statusEl.textContent = `${loaded.name} を起動`;
  expandedGroups.add(`sd:${loaded.pluginUuid}`);
  await refresh();
}

async function stopPlugin(pluginUuid: string) {
  await invoke("unload_sd_plugin", { pluginUuid });
  statusEl.textContent = "プラグインを停止しました";
  await refresh();
}

document.getElementById("btn-settings")!.addEventListener("click", () => {
  void invoke("open_settings_window");
});

document.getElementById("settings-json")!.addEventListener("input", () => {
  scheduleNativeSettingsSave();
});

document.getElementById("slot-title-input")!.addEventListener("change", (ev) => {
  void saveSlotAppearance((ev.target as HTMLInputElement).value);
});

document.getElementById("slot-image-input")!.addEventListener("change", (ev) => {
  const file = (ev.target as HTMLInputElement).files?.[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = () => {
    void saveSlotAppearance(undefined, String(reader.result ?? ""));
  };
  reader.readAsDataURL(file);
});

document.getElementById("btn-clear-slot-image")!.addEventListener("click", () => {
  void saveSlotAppearance(undefined, undefined, true);
  (document.getElementById("slot-image-input") as HTMLInputElement).value = "";
});

document.getElementById("multi-action-delay")!.addEventListener("change", async (ev) => {
  if (!selectedSlotId) return;
  const profile = await invoke<Profile>("get_profile");
  const page = profile.pages.find((p) => p.id === activePageId);
  const slot = page && Object.values(page.slots).find((s) => s.id === selectedSlotId);
  const binding = normalizeBinding(slot?.binding);
  if (binding?.type === "multi_action") {
    void saveMultiAction(binding.steps ?? [], Number((ev.target as HTMLInputElement).value));
  }
});

document.getElementById("action-search")!.addEventListener("input", () => renderActionSidebar());

devicePicker.addEventListener("change", async () => {
  selectedSurfaceId = devicePicker.value;
  selectedSlotId = null;
  await refresh();
});

pagePicker.addEventListener("change", () => {
  void setActivePage(pagePicker.value);
});

void listen("visual-updated", () => {
  void refreshCellVisuals().then(() =>
    invoke<Profile>("get_profile").then(renderGrid),
  );
});

void listen<{ surfaceId?: string }>("surfaces-changed", (ev) => {
  if (ev.payload.surfaceId) {
    selectedSurfaceId = ev.payload.surfaceId;
  }
  void refresh();
});

void listen<{ status: string; message?: string; row?: number; column?: number }>(
  "plugin-status",
  (ev) => {
    if (ev.payload.message) {
      statusEl.textContent = `[${ev.payload.status}] ${ev.payload.message}`;
    }
    if (ev.payload.row !== undefined && ev.payload.column !== undefined) {
      flashSlot(ev.payload.row, ev.payload.column, ev.payload.status);
    }
  },
);

function flashSlot(row: number, col: number, kind: string) {
  const el = gridEl.querySelector(
    `.slot[data-row="${row}"][data-col="${col}"]`,
  ) as HTMLElement | null;
  if (!el) return;
  el.classList.remove("slot-flash-alert", "slot-flash-ok");
  el.classList.add(kind === "alert" ? "slot-flash-alert" : "slot-flash-ok");
  window.setTimeout(() => {
    el.classList.remove("slot-flash-alert", "slot-flash-ok");
  }, 500);
}

void showStartupConflictsIfNeeded();
refresh().catch((e) => {
  statusEl.textContent = formatUserError(e);
});

setInterval(() => {
  if (isDraggingAction) return;
  void refreshActionLibrary().catch(() => {});
}, 5000);
