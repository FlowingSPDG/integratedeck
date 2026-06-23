export interface Profile {
  id: string;
  name: string;
  pages: Page[];
  active_page_id?: string;
  surfaces: { surface_id: string; label: string }[];
}

export interface Page {
  id: string;
  name: string;
  slots: Record<string, Slot>;
  parent_page_id?: string;
}

export interface SlotAppearance {
  title?: string;
  default_image?: { format: string; data: string };
}

export interface Slot {
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

export interface NormalizedBinding {
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

export interface MultiActionStep {
  binding: Record<string, unknown>;
  delay_before_ms?: number;
}

export interface VisualState {
  image?: { format: string; data: string };
  title?: string;
  state_index?: number;
}

export interface SurfaceRuntimeEntry {
  surfaceId: string;
  label: string;
  backend: string;
  status: string;
  rows: number;
  columns: number;
  serial?: string;
  kind?: string;
  product?: string;
  activePageId?: string;
}

export interface PluginLibraryEntry {
  id: string;
  name: string;
  path: string;
  source: string;
  status: string;
  port?: number;
  actions: { id: string; name: string; source: string }[];
}

export interface ActionLibrary {
  streamdeck: PluginLibraryEntry[];
  companion: PluginLibraryEntry[];
}

export interface RuntimeStatus {
  sdPlugins: { pluginUuid: string; name: string; status: string; port: number }[];
  plugins: { path: string; name: string; status: string; uuid?: string }[];
  surfaces: SurfaceRuntimeEntry[];
  usbDevices: unknown[];
  streamdeckDirs: string[];
  companion: {
    companionHostRunning: boolean;
    connections: { id: string; module_id: string; label: string; enabled: boolean }[];
    modules: { path: string; name: string }[];
  };
}

export interface ConflictingApp {
  id: "stream_deck" | "companion" | string;
  displayName: string;
  processName: string;
}

export interface StartupConflictReport {
  conflicts: ConflictingApp[];
}

export interface DragActionPayload {
  source: "streamdeck" | "companion" | "builtin";
  pluginId: string;
  actionId: string;
  actionName: string;
  pluginPath?: string;
}

export interface SlotDragPayload {
  slotId: string;
  fromRow: number;
  fromCol: number;
  label: string;
}

export interface SlotClipboard {
  binding?: Record<string, unknown>;
  appearance?: SlotAppearance;
}

export interface SlotContextMenuState {
  x: number;
  y: number;
  row: number;
  col: number;
  slot?: Slot;
}

export interface FlashState {
  row: number;
  col: number;
  kind: string;
}
