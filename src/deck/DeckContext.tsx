import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beginPiLoad, dispatchPiMessage, loadPiInFrame } from "../pi-webview";
import {
  BUILTIN_OPEN_FOLDER,
  folderChildPageId,
  normalizeBinding,
} from "../lib/binding";
import { findSlot, isSlotConfigured, slotDragLabel } from "../lib/slots";
import type {
  ActionLibrary,
  DragActionPayload,
  FlashState,
  MultiActionStep,
  Profile,
  RuntimeStatus,
  Slot,
  SlotClipboard,
  SlotContextMenuState,
  SlotDragPayload,
  StartupConflictReport,
  SurfaceRuntimeEntry,
  VisualState,
} from "../lib/types";
import { formatUserError } from "../utils/format-error";

const DRAG_THRESHOLD_PX = 6;

export interface DragGhostState {
  label: string;
  x: number;
  y: number;
}

interface DeckContextValue {
  profile: Profile | null;
  statusText: string;
  selectedSlotId: string | null;
  selectedSurfaceId: string | null;
  activePageId: string | null;
  gridRows: number;
  gridCols: number;
  cellVisuals: Record<string, VisualState>;
  actionLibrary: ActionLibrary;
  expandedGroups: Set<string>;
  executeMode: boolean;
  actionSearch: string;
  slotClipboard: SlotClipboard | null;
  contextMenu: SlotContextMenuState | null;
  flash: FlashState | null;
  conflictDismissed: boolean;
  startupConflicts: StartupConflictReport | null;
  connectedSurfaces: SurfaceRuntimeEntry[];
  deviceLabel: string;
  activePage: Profile["pages"][number] | undefined;
  selectedSlot: Slot | undefined;
  piNativeSettingsVisible: boolean;
  piSettingsJson: string;
  setActionSearch: (q: string) => void;
  setExecuteMode: (v: boolean) => void;
  setConflictDismissed: (v: boolean) => void;
  setContextMenu: (menu: SlotContextMenuState | null) => void;
  toggleGroup: (key: string) => void;
  refresh: (opts?: { surfaceId?: string | null }) => Promise<void>;
  selectSurface: (surfaceId: string) => Promise<void>;
  setActivePage: (pageId: string) => Promise<void>;
  onSlotClick: (row: number, col: number, existing?: Slot) => Promise<void>;
  applyActionToCell: (row: number, col: number, payload: DragActionPayload) => Promise<void>;
  transferSlotCell: (
    fromRow: number,
    fromCol: number,
    toRow: number,
    toCol: number,
    sourceSlotId: string,
  ) => Promise<void>;
  copySlot: (slot: Slot) => void;
  deleteSlotContent: (slot: Slot) => Promise<void>;
  pasteSlot: (row: number, col: number, existing?: Slot) => Promise<void>;
  saveSlotAppearance: (title?: string, imageBase64?: string, clearImage?: boolean) => Promise<void>;
  saveMultiAction: (steps: MultiActionStep[], delayMs: number) => Promise<void>;
  saveNativeSettings: (json: string) => void;
  loadPlugin: (path: string) => Promise<void>;
  stopPlugin: (uuid: string) => Promise<void>;
  openSettings: () => void;
  setSwitchPageTarget: (targetPageId: string) => Promise<void>;
  addMultiActionStepFromDrag: (payload: DragActionPayload) => Promise<void>;
  beginActionDrag: (e: React.PointerEvent, payload: DragActionPayload) => void;
  beginSlotDrag: (
    e: React.PointerEvent,
    slot: Slot,
    row: number,
    col: number,
    slotEl: HTMLElement,
  ) => void;
  registerGridRef: (el: HTMLDivElement | null) => void;
  registerPiFrameRef: (el: HTMLIFrameElement | null) => void;
}

const DeckContext = createContext<DeckContextValue | null>(null);

interface DeckDragContextValue {
  dragGhost: DragGhostState | null;
  dropTarget: { row: number; col: number } | null;
  draggingSlotId: string | null;
}

const DeckDragContext = createContext<DeckDragContextValue | null>(null);

export function useDeck(): DeckContextValue {
  const ctx = useContext(DeckContext);
  if (!ctx) throw new Error("useDeck must be used within DeckProvider");
  return ctx;
}

export function useDeckDrag(): DeckDragContextValue {
  const ctx = useContext(DeckDragContext);
  if (!ctx) throw new Error("useDeckDrag must be used within DeckProvider");
  return ctx;
}

async function pollCellVisuals(
  surfaceId: string,
  setCellVisuals: (v: Record<string, VisualState>) => void,
  times = 3,
) {
  for (let i = 0; i < times; i++) {
    const visuals = await invoke<Record<string, VisualState>>("get_cell_visuals", { surfaceId });
    setCellVisuals(visuals);
    if (i < times - 1) await new Promise((r) => setTimeout(r, 100));
  }
}

export function DeckProvider({ children }: { children: ReactNode }) {
  const [profile, setProfile] = useState<Profile | null>(null);
  const [statusText, setStatusText] = useState("Starting…");
  const [selectedSlotId, setSelectedSlotId] = useState<string | null>(null);
  const [selectedSurfaceId, setSelectedSurfaceId] = useState<string | null>(null);
  const [activePageId, setActivePageId] = useState<string | null>(null);
  const [gridRows, setGridRows] = useState(3);
  const [gridCols, setGridCols] = useState(5);
  const [cellVisuals, setCellVisuals] = useState<Record<string, VisualState>>({});
  const [actionLibrary, setActionLibrary] = useState<ActionLibrary>({
    streamdeck: [],
    companion: [],
  });
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [executeMode, setExecuteModeState] = useState(false);
  const [actionSearch, setActionSearch] = useState("");
  const [slotClipboard, setSlotClipboard] = useState<SlotClipboard | null>(null);
  const [contextMenu, setContextMenu] = useState<SlotContextMenuState | null>(null);
  const [dragGhost, setDragGhost] = useState<DragGhostState | null>(null);
  const [dropTarget, setDropTarget] = useState<{ row: number; col: number } | null>(null);
  const [draggingSlotId, setDraggingSlotId] = useState<string | null>(null);
  const [flash, setFlash] = useState<FlashState | null>(null);
  const [conflictDismissed, setConflictDismissed] = useState(false);
  const [startupConflicts, setStartupConflicts] = useState<StartupConflictReport | null>(null);
  const [connectedSurfaces, setConnectedSurfaces] = useState<SurfaceRuntimeEntry[]>([]);
  const [deviceLabel, setDeviceLabel] = useState("Stream Deck");
  const [piNativeSettingsVisible, setPiNativeSettingsVisible] = useState(false);
  const [piSettingsJson, setPiSettingsJson] = useState("{}");

  const gridRef = useRef<HTMLDivElement | null>(null);
  const cachedSlotSizePx = useRef(72);
  const isDraggingAction = useRef(false);
  const isDraggingSlot = useRef(false);
  const activeDragPayload = useRef<DragActionPayload | null>(null);
  const activeSlotDrag = useRef<SlotDragPayload | null>(null);
  const suppressSlotClick = useRef(false);
  const suppressSettingsAutosave = useRef(false);
  const settingsAutosaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const piFrameRef = useRef<HTMLIFrameElement | null>(null);

  // Refs mirror React state for async handlers (avoids stale closures).
  const selectedSurfaceIdRef = useRef<string | null>(null);
  const activePageIdRef = useRef<string | null>(null);
  const selectedSlotIdRef = useRef<string | null>(null);
  const profileRef = useRef<Profile | null>(null);
  const actionLibraryRef = useRef(actionLibrary);
  const gridRowsRef = useRef(gridRows);
  const gridColsRef = useRef(gridCols);
  const refreshRef = useRef<(opts?: { surfaceId?: string | null }) => Promise<void>>(async () => {});
  const updatePropertyInspectorRef = useRef<(slot?: Slot) => Promise<void>>(async () => {});
  const applyActionToCellRef = useRef<
    (row: number, col: number, payload: DragActionPayload) => Promise<void>
  >(async () => {});
  const addMultiActionStepFromDragRef = useRef<
    (payload: DragActionPayload) => Promise<void>
  >(async () => {});
  const findSlotAtRef = useRef<(clientX: number, clientY: number) => { row: number; col: number } | null>(
    () => null,
  );

  selectedSurfaceIdRef.current = selectedSurfaceId;
  activePageIdRef.current = activePageId;
  selectedSlotIdRef.current = selectedSlotId;
  profileRef.current = profile;
  actionLibraryRef.current = actionLibrary;
  gridRowsRef.current = gridRows;
  gridColsRef.current = gridCols;

  const registerPiFrameRef = useCallback((el: HTMLIFrameElement | null) => {
    piFrameRef.current = el;
  }, []);

  const registerGridRef = useCallback((el: HTMLDivElement | null) => {
    gridRef.current = el;
    if (el) {
      cachedSlotSizePx.current =
        parseInt(getComputedStyle(document.documentElement).getPropertyValue("--slot-size")) || 72;
    }
  }, []);

  const refreshCellVisuals = useCallback(async (surfaceId?: string | null) => {
    const sid = surfaceId ?? selectedSurfaceIdRef.current;
    if (!sid) return;
    const visuals = await invoke<Record<string, VisualState>>("get_cell_visuals", {
      surfaceId: sid,
    });
    setCellVisuals(visuals);
  }, []);

  const applyDeviceFromSurfaces = useCallback(
    (surfaces: SurfaceRuntimeEntry[], preferredSurfaceId: string | null) => {
      const connected = surfaces.filter((s) => s.status === "connected");
      let sid = preferredSurfaceId;

      if (sid && connected.some((s) => s.surfaceId === sid)) {
        // Keep explicit user selection.
      } else if (connected.length > 0) {
        const physical = connected.find((s) => s.backend !== "mock");
        sid = (physical ?? connected[0]).surfaceId;
      }

      setConnectedSurfaces(connected);
      const selected = sid ? connected.find((s) => s.surfaceId === sid) : undefined;
      if (selected) {
        setDeviceLabel(selected.label);
        setGridRows(selected.rows);
        setGridCols(selected.columns);
        gridRowsRef.current = selected.rows;
        gridColsRef.current = selected.columns;
      }
      return sid ?? null;
    },
    [],
  );

  const refresh = useCallback(async (opts?: { surfaceId?: string | null }) => {
    const [info, prof] = await Promise.all([
      invoke<{ version: string; data_dir: string }>("get_app_info"),
      invoke<Profile>("get_profile"),
    ]);
    setStatusText(`v${info.version}`);
    setProfile(prof);
    profileRef.current = prof;

    if (prof.surfaces.length === 0) {
      const mockId = await invoke<string>("register_mock_surface", { name: "Mock 3×5" });
      selectedSurfaceIdRef.current = mockId;
      setSelectedSurfaceId(mockId);
      return refreshRef.current();
    }

    const preferredSurfaceId =
      opts?.surfaceId !== undefined ? opts.surfaceId : selectedSurfaceIdRef.current;
    let surfaceId = preferredSurfaceId ?? prof.surfaces[0].surface_id;

    try {
      const status = await invoke<RuntimeStatus>("get_runtime_status");
      surfaceId = applyDeviceFromSurfaces(status.surfaces, surfaceId) ?? surfaceId;
      selectedSurfaceIdRef.current = surfaceId;
      setSelectedSurfaceId(surfaceId);

      const surfaceEntry = status.surfaces.find((s) => s.surfaceId === surfaceId);
      const pageId =
        surfaceEntry?.activePageId ?? prof.active_page_id ?? prof.pages[0]?.id ?? null;
      activePageIdRef.current = pageId;
      setActivePageId(pageId);

      const libraryPromise =
        isDraggingAction.current || isDraggingSlot.current
          ? Promise.resolve(actionLibraryRef.current)
          : invoke<ActionLibrary>("list_action_library");

      const [visuals, library] = await Promise.all([
        invoke<Record<string, VisualState>>("get_cell_visuals", { surfaceId }),
        libraryPromise,
      ]);
      setCellVisuals(visuals);
      if (!isDraggingAction.current && !isDraggingSlot.current) {
        setActionLibrary(library);
        actionLibraryRef.current = library;
      }
    } catch (e) {
      setStatusText(formatUserError(e));
    }
  }, [applyDeviceFromSurfaces]);

  refreshRef.current = refresh;

  const updatePropertyInspector = useCallback(
    async (slot?: Slot) => {
      const piFrame = piFrameRef.current;
      if (!piFrame) return;

      suppressSettingsAutosave.current = true;
      if (settingsAutosaveTimer.current) {
        clearTimeout(settingsAutosaveTimer.current);
        settingsAutosaveTimer.current = null;
      }

      try {
        if (!selectedSlotIdRef.current || !slot) {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson("{}");
          beginPiLoad(piFrame);
          await invoke("focus_pi_slot", { slotId: null });
          return;
        }

        const binding = normalizeBinding(slot.binding);
        if (!binding) {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson("{}");
          beginPiLoad(piFrame);
          await invoke("focus_pi_slot", { slotId: null });
          return;
        }

        if (binding.type === "multi_action" || binding.type === "builtin") {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson("{}");
          beginPiLoad(piFrame);
          await invoke("focus_pi_slot", { slotId: null });
          return;
        }

        if (binding.type === "companion") {
          setPiNativeSettingsVisible(true);
          setPiSettingsJson(JSON.stringify(binding.options ?? {}, null, 2));
          beginPiLoad(piFrame);
          await invoke("focus_pi_slot", { slotId: null });
          return;
        }

        if (!binding.pluginUuid || !binding.actionUuid) {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson("{}");
          return;
        }

        const piGeneration = beginPiLoad(piFrame);
        const [piPath, piCtx] = await Promise.all([
          invoke<string | null>("get_property_inspector_url", {
            args: { pluginUuid: binding.pluginUuid, actionUuid: binding.actionUuid },
          }),
          invoke<{
            port: number;
            context: string;
            actionUuid: string;
            pluginUuid: string;
            deviceId: string;
            settings: Record<string, unknown>;
          } | null>("get_pi_context", { slotId: selectedSlotIdRef.current }),
        ]);

        if (piPath && piCtx) {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson(JSON.stringify(binding.settings ?? {}, null, 2));
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
          try {
            await invoke("focus_pi_slot", { slotId: selectedSlotIdRef.current });
          } catch {
            /* PI focus optional */
          }
        } else if (piCtx) {
          setPiNativeSettingsVisible(true);
          setPiSettingsJson(JSON.stringify(binding.settings ?? {}, null, 2));
          await invoke("focus_pi_slot", { slotId: selectedSlotIdRef.current });
        } else {
          setPiNativeSettingsVisible(false);
          setPiSettingsJson("{}");
          await invoke("focus_pi_slot", { slotId: null });
        }
      } finally {
        suppressSettingsAutosave.current = false;
      }
    },
    [],
  );

  updatePropertyInspectorRef.current = updatePropertyInspector;

  const setActivePage = useCallback(
    async (pageId: string) => {
      const surfaceId = selectedSurfaceIdRef.current;
      if (!surfaceId) return;
      await invoke("set_surface_page", {
        args: { surfaceId, pageId },
      });
      activePageIdRef.current = pageId;
      setActivePageId(pageId);
      selectedSlotIdRef.current = null;
      setSelectedSlotId(null);
      await refreshRef.current();
    },
    [],
  );

  const onSlotClick = useCallback(
    async (row: number, col: number, existing?: Slot) => {
      if (suppressSlotClick.current) {
        suppressSlotClick.current = false;
        return;
      }

      const selectedSurfaceId = selectedSurfaceIdRef.current;
      const activePageId = activePageIdRef.current;
      const profile = profileRef.current;
      if (!selectedSurfaceId || !activePageId || !profile) return;

      let slot = existing;
      const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];

      if (!slot) {
        slot = await invoke<Slot>("create_slot", {
          args: {
            surfaceId: selectedSurfaceId,
            pageId: activePageId,
            row,
            column: col,
          },
        });
        const updated = await invoke<Profile>("get_profile");
        setProfile(updated);
        profileRef.current = updated;
        slot =
          findSlot(
            updated.pages.find((p) => p.id === activePageId) ?? page,
            row,
            col,
            selectedSurfaceId,
          ) ?? slot;
      }

      setSelectedSlotId(slot.id);
      selectedSlotIdRef.current = slot.id;

      const binding = normalizeBinding(slot.binding);
      if (binding?.type === "builtin" && binding.actionId === BUILTIN_OPEN_FOLDER) {
        const childId = folderChildPageId(binding.settings);
        if (childId && childId !== activePageId) {
          await setActivePage(childId);
          const profileAfter = await invoke<Profile>("get_profile");
          setProfile(profileAfter);
          profileRef.current = profileAfter;
          slot =
            findSlot(
              profileAfter.pages.find((p) => p.id === childId) ?? profileAfter.pages[0],
              row,
              col,
              selectedSurfaceId,
            ) ?? slot;
          setSelectedSlotId(slot.id);
          selectedSlotIdRef.current = slot.id;
        }
      }
    },
    [setActivePage],
  );

  const findSlotAt = useCallback(
    (clientX: number, clientY: number): { row: number; col: number } | null => {
      const gridEl = gridRef.current;
      if (!gridEl) return null;

      const hit = document.elementFromPoint(clientX, clientY);
      const slotEl = hit?.closest(".slot") as HTMLElement | null;
      if (slotEl?.dataset.row !== undefined) {
        return { row: Number(slotEl.dataset.row), col: Number(slotEl.dataset.col) };
      }

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
      const cellSize = cachedSlotSizePx.current;
      const col = Math.floor((clientX - rect.left) / (cellSize + gap));
      const row = Math.floor((clientY - rect.top) / (cellSize + gap));
      if (col < 0 || col >= gridColsRef.current || row < 0 || row >= gridRowsRef.current) return null;
      return { row, col };
    },
    [gridCols, gridRows],
  );

  findSlotAtRef.current = findSlotAt;

  const cleanupDrag = useCallback(() => {
    setDragGhost(null);
    setDropTarget(null);
    setDraggingSlotId(null);
    activeDragPayload.current = null;
    activeSlotDrag.current = null;
    isDraggingAction.current = false;
    isDraggingSlot.current = false;
    document.body.classList.remove("action-dragging", "slot-dragging");
  }, []);

  const applyActionToCell = useCallback(
    async (row: number, col: number, payload: DragActionPayload) => {
      const surfaceId = selectedSurfaceIdRef.current;
      const pageId = activePageIdRef.current;
      const prof = profileRef.current;
      if (!surfaceId || !pageId || !prof) {
        setStatusText("デバイスまたはページが未選択です");
        return;
      }

      const page = prof.pages.find((p) => p.id === pageId) ?? prof.pages[0];
      if (!page) return;

      let slot = findSlot(page, row, col, surfaceId);
      if (!slot) {
        slot = await invoke<Slot>("create_slot", {
          args: {
            surfaceId,
            pageId,
            row,
            column: col,
          },
        });
      }

      selectedSlotIdRef.current = slot.id;
      setSelectedSlotId(slot.id);
      setStatusText(`配置中: ${payload.actionName}…`);

      try {
        if (payload.source === "builtin") {
          await invoke("bind_slot_builtin", {
            args: { slotId: slot.id, actionId: payload.actionId },
          });
        } else if (payload.source === "streamdeck") {
          const plugin = actionLibraryRef.current.streamdeck.find((p) => p.id === payload.pluginId);
          if (plugin?.status !== "running" && plugin?.path) {
            await invoke("load_sd_plugin", { path: plugin.path });
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

        await pollCellVisuals(surfaceId, setCellVisuals);
        setStatusText(`${payload.actionName} を配置しました`);
        await refreshRef.current();
      } catch (err) {
        setStatusText(`配置失敗: ${formatUserError(err)}`);
      }
    },
    [],
  );

  applyActionToCellRef.current = applyActionToCell;

  const beginActionDrag = useCallback(
    (e: React.PointerEvent, payload: DragActionPayload) => {
      if (e.button !== 0) return;
      const startX = e.clientX;
      const startY = e.clientY;
      let dragging = false;

      const onMove = (ev: PointerEvent) => {
        if (!dragging) {
          if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD_PX) return;
          dragging = true;
          isDraggingAction.current = true;
          activeDragPayload.current = payload;
          setDragGhost({ label: payload.actionName, x: ev.clientX, y: ev.clientY });
          document.body.classList.add("action-dragging");
        } else {
          setDragGhost({ label: payload.actionName, x: ev.clientX, y: ev.clientY });
          setDropTarget(findSlotAtRef.current(ev.clientX, ev.clientY));
        }
      };

      const onUp = (ev: PointerEvent) => {
        document.removeEventListener("pointermove", onMove);
        document.removeEventListener("pointerup", onUp);
        document.removeEventListener("pointercancel", onUp);

        if (dragging && activeDragPayload.current) {
          const hit = document.elementFromPoint(ev.clientX, ev.clientY);
          if (selectedSlotIdRef.current && hit?.closest(".multi-action-editor")) {
            void addMultiActionStepFromDragRef.current(activeDragPayload.current);
          } else {
            const target = findSlotAtRef.current(ev.clientX, ev.clientY);
            if (target) {
              void applyActionToCellRef.current(target.row, target.col, activeDragPayload.current);
            } else {
              setStatusText("キー上で離してください");
            }
          }
        }
        cleanupDrag();
      };

      document.addEventListener("pointermove", onMove);
      document.addEventListener("pointerup", onUp);
      document.addEventListener("pointercancel", onUp);
    },
    [],
  );

  const simulateSlotKey = useCallback(async (slotId: string, phase: "key_down" | "key_up") => {
    await invoke("simulate_slot_key", { args: { slotId, phase } });
  }, []);

  const beginSlotDrag = useCallback(
    (
      e: React.PointerEvent,
      slot: Slot,
      row: number,
      col: number,
      slotEl: HTMLElement,
    ) => {
      if (e.button !== 0) return;
      const canDrag = isSlotConfigured(slot) && !executeMode;
      const canExecute = executeMode && !!slot.binding;
      if (!canDrag && !canExecute) return;

      const payload: SlotDragPayload = {
        slotId: slot.id,
        fromRow: row,
        fromCol: col,
        label: slotDragLabel(slot, row, col, cellVisuals),
      };
      const startX = e.clientX;
      const startY = e.clientY;
      let dragging = false;
      let keyDownActive = false;

      const releaseKeyDown = () => {
        if (!keyDownActive) return;
        keyDownActive = false;
        slotEl.classList.remove("pressed");
        void simulateSlotKey(slot.id, "key_up").catch((err) =>
          setStatusText(`実行に失敗: ${formatUserError(err)}`),
        );
      };

      if (canExecute) {
        keyDownActive = true;
        slotEl.classList.add("pressed");
        slotEl.setPointerCapture(e.pointerId);
        suppressSlotClick.current = true;
        void simulateSlotKey(slot.id, "key_down").catch((err) =>
          setStatusText(`実行に失敗: ${formatUserError(err)}`),
        );
      }

      const onMove = (ev: PointerEvent) => {
        if (!dragging) {
          if (!canDrag || Math.hypot(ev.clientX - startX, ev.clientY - startY) < DRAG_THRESHOLD_PX) {
            return;
          }
          if (keyDownActive) releaseKeyDown();
          dragging = true;
          isDraggingSlot.current = true;
          activeSlotDrag.current = payload;
          setDraggingSlotId(slot.id);
          setDragGhost({ label: payload.label, x: ev.clientX, y: ev.clientY });
          document.body.classList.add("slot-dragging");
        } else {
          setDragGhost({ label: payload.label, x: ev.clientX, y: ev.clientY });
          const target = findSlotAtRef.current(ev.clientX, ev.clientY);
          if (target && (target.row !== row || target.col !== col)) {
            setDropTarget(target);
          } else {
            setDropTarget(null);
          }
        }
      };

      const onUp = (ev: PointerEvent) => {
        document.removeEventListener("pointermove", onMove);
        document.removeEventListener("pointerup", onUp);
        document.removeEventListener("pointercancel", onUp);

        if (canExecute) {
          try {
            slotEl.releasePointerCapture(ev.pointerId);
          } catch {
            /* already released */
          }
          releaseKeyDown();
        }

        if (dragging && activeSlotDrag.current) {
          const target = findSlotAtRef.current(ev.clientX, ev.clientY);
          if (target && (target.row !== row || target.col !== col)) {
            suppressSlotClick.current = true;
            void transferSlotCellRef.current?.(
              payload.fromRow,
              payload.fromCol,
              target.row,
              target.col,
              payload.slotId,
            );
          }
        }
        cleanupDrag();
      };

      document.addEventListener("pointermove", onMove);
      document.addEventListener("pointerup", onUp);
      document.addEventListener("pointercancel", onUp);
    },
    [cellVisuals, cleanupDrag, executeMode, findSlotAt, selectedSurfaceId, simulateSlotKey],
  );

  const transferSlotCellRef = useRef<
    (
      fromRow: number,
      fromCol: number,
      toRow: number,
      toCol: number,
      sourceSlotId: string,
    ) => Promise<void>
  >(() => Promise.resolve());

  const transferSlotCell = useCallback(
    async (
      fromRow: number,
      fromCol: number,
      toRow: number,
      toCol: number,
      sourceSlotId: string,
    ) => {
      if (!selectedSurfaceId || !activePageId || !profile) return;

      const pageBefore = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
      const targetSlotBefore = pageBefore
        ? findSlot(pageBefore, toRow, toCol, selectedSurfaceId)
        : undefined;
      const isSwap = isSlotConfigured(targetSlotBefore);
      const wasSourceSelected = selectedSlotId === sourceSlotId;
      const wasTargetSelected = targetSlotBefore?.id === selectedSlotId;

      try {
        await invoke("transfer_slot_cell", {
          args: {
            surfaceId: selectedSurfaceId,
            pageId: activePageId,
            fromRow,
            fromCol,
            toRow,
            toCol,
          },
        });
        await pollCellVisuals(selectedSurfaceId, setCellVisuals);
        const updated = await invoke<Profile>("get_profile");
        setProfile(updated);
        profileRef.current = updated;
        const page = updated.pages.find((p) => p.id === activePageId) ?? updated.pages[0];
        if (wasSourceSelected) {
          const nextId = findSlot(page, toRow, toCol, selectedSurfaceId)?.id ?? null;
          selectedSlotIdRef.current = nextId;
          setSelectedSlotId(nextId);
        } else if (wasTargetSelected) {
          const nextId = findSlot(page, fromRow, fromCol, selectedSurfaceId)?.id ?? null;
          selectedSlotIdRef.current = nextId;
          setSelectedSlotId(nextId);
        }
        setStatusText(isSwap ? "入れ替えました" : "移動しました");
      } catch (err) {
        setStatusText(`移動に失敗: ${formatUserError(err)}`);
      }
    },
    [activePageId, profile, selectedSurfaceId],
  );

  transferSlotCellRef.current = transferSlotCell;

  const copySlot = useCallback((slot: Slot) => {
    setSlotClipboard({
      binding: slot.binding ? structuredClone(slot.binding) : undefined,
      appearance: slot.appearance ? structuredClone(slot.appearance) : undefined,
    });
    setStatusText("コピーしました");
  }, []);

  const deleteSlotContent = useCallback(
    async (slot: Slot) => {
      try {
        if (slot.binding) await invoke("unbind_slot", { slotId: slot.id });
        await invoke("update_slot_appearance", {
          args: { slotId: slot.id, title: "", clearImage: true },
        });
        if (selectedSlotId === slot.id) {
          selectedSlotIdRef.current = null;
          setSelectedSlotId(null);
        }
        await refresh();
        setStatusText("削除しました");
      } catch (err) {
        setStatusText(`削除に失敗: ${formatUserError(err)}`);
      }
    },
    [refresh, selectedSlotId],
  );

  const pasteSlot = useCallback(
    async (row: number, col: number, existing?: Slot) => {
      if (!slotClipboard || !selectedSurfaceId || !activePageId || !profile) return;

      try {
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
        }

        await invoke("apply_slot_snapshot", {
          args: {
            slotId: slot.id,
            binding: slotClipboard.binding ?? null,
            appearance: slotClipboard.appearance ?? {},
          },
        });

        setSelectedSlotId(slot.id);
        selectedSlotIdRef.current = slot.id;
        await pollCellVisuals(selectedSurfaceId, setCellVisuals);
        const updated = await invoke<Profile>("get_profile");
        setProfile(updated);
        profileRef.current = updated;
        setStatusText("貼り付けました");
      } catch (err) {
        setStatusText(`貼り付けに失敗: ${formatUserError(err)}`);
      }
    },
    [activePageId, profile, selectedSurfaceId, slotClipboard],
  );

  const saveSlotAppearance = useCallback(
    async (title?: string, imageBase64?: string, clearImage = false) => {
      const slotId = selectedSlotIdRef.current;
      if (!slotId) return;

      const slot = profileRef.current?.pages
        .flatMap((p) => Object.values(p.slots))
        .find((s) => s.id === slotId);

      const resolvedTitle =
        title !== undefined
          ? title
          : (slot?.appearance?.title ?? slot?.label ?? "");

      try {
        await invoke("update_slot_appearance", {
          args: {
            slotId,
            title: resolvedTitle.trim() ? resolvedTitle : null,
            defaultImageBase64: imageBase64 ?? null,
            clearImage,
          },
        });
        await refreshCellVisuals();
        const updated = await invoke<Profile>("get_profile");
        setProfile(updated);
        profileRef.current = updated;
      } catch (err) {
        setStatusText(`表示設定の保存に失敗: ${formatUserError(err)}`);
      }
    },
    [refreshCellVisuals],
  );

  const saveMultiAction = useCallback(
    async (steps: MultiActionStep[], delayMs: number) => {
      const slotId = selectedSlotIdRef.current;
      if (!slotId) return;
      await invoke("update_multi_action", {
        args: { slotId, steps, delayMs },
      });
      await refresh();
    },
    [refresh],
  );

  const saveNativeSettings = useCallback(
    async (json: string) => {
      if (!selectedSlotId || suppressSettingsAutosave.current) return;
      try {
        const settings = JSON.parse(json) as unknown;
        await invoke("update_slot_settings", {
          args: { slotId: selectedSlotId, settings },
        });
      } catch (err) {
        setStatusText(`設定の保存に失敗: ${formatUserError(err)}`);
      }
    },
    [selectedSlotId],
  );

  const scheduleNativeSettingsSave = useCallback(
    (json: string) => {
      if (suppressSettingsAutosave.current || !selectedSlotId) return;
      if (settingsAutosaveTimer.current) clearTimeout(settingsAutosaveTimer.current);
      settingsAutosaveTimer.current = setTimeout(() => {
        settingsAutosaveTimer.current = null;
        void saveNativeSettings(json);
      }, 400);
    },
    [saveNativeSettings, selectedSlotId],
  );

  const loadPlugin = useCallback(
    async (path: string) => {
      setStatusText("プラグイン起動中…");
      const loaded = await invoke<{ pluginUuid: string; name: string }>("load_sd_plugin", { path });
      setStatusText(`${loaded.name} を起動`);
      setExpandedGroups((prev) => new Set(prev).add(`sd:${loaded.pluginUuid}`));
      await refreshRef.current();
    },
    [],
  );

  const stopPlugin = useCallback(
    async (uuid: string) => {
      await invoke("unload_sd_plugin", { pluginUuid: uuid });
      setStatusText("プラグインを停止しました");
      await refreshRef.current();
    },
    [],
  );

  const setExecuteMode = useCallback(
    (v: boolean) => {
      setExecuteModeState(v);
      document.body.classList.toggle("execute-mode", v);
      setStatusText(v ? "Execute Mode ON — キーを押して実行" : "Execute Mode OFF");
    },
    [],
  );

  const toggleGroup = useCallback((key: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);

  const selectSurface = useCallback(async (surfaceId: string) => {
    selectedSurfaceIdRef.current = surfaceId;
    setSelectedSurfaceId(surfaceId);
    selectedSlotIdRef.current = null;
    setSelectedSlotId(null);
    await refreshRef.current({ surfaceId });
  }, []);

  const openSettings = useCallback(() => {
    void invoke("open_settings_window").catch((err) =>
      setStatusText(`設定を開けません: ${formatUserError(err)}`),
    );
  }, []);

  const setSwitchPageTarget = useCallback(
    async (targetPageId: string) => {
      if (!selectedSlotId) return;
      await invoke("set_switch_page_target", {
        args: { slotId: selectedSlotId, targetPageId },
      });
    },
    [selectedSlotId],
  );

  const addMultiActionStepFromDrag = useCallback(async (payload: DragActionPayload) => {
    const slotId = selectedSlotIdRef.current;
    if (!slotId) return;
    await invoke("add_multi_action_step", {
      args: {
        slotId,
        source: payload.source,
        pluginId: payload.pluginId,
        actionId: payload.actionId,
      },
    });
    await refreshRef.current();
  }, []);

  addMultiActionStepFromDragRef.current = addMultiActionStepFromDrag;

  const activePage = useMemo(
    () => profile?.pages.find((p) => p.id === activePageId) ?? profile?.pages[0],
    [profile, activePageId],
  );

  const selectedSlot = useMemo(() => {
    if (!activePage || !selectedSlotId) return undefined;
    return Object.values(activePage.slots).find((s) => s.id === selectedSlotId);
  }, [activePage, selectedSlotId]);

  const piSyncKey = useMemo(() => {
    if (!selectedSlotId || !selectedSlot) return selectedSlotId ?? "";
    return `${selectedSlotId}:${JSON.stringify(selectedSlot.binding ?? null)}`;
  }, [selectedSlotId, selectedSlot]);

  useEffect(() => {
    void refreshRef.current();
  }, []);

  useEffect(() => {
    if (conflictDismissed) return;
    void invoke<StartupConflictReport>("check_startup_conflicts")
      .then((report) => {
        if (report.conflicts.length > 0) setStartupConflicts(report);
      })
      .catch((err) => console.warn("startup conflict check failed:", err));
  }, [conflictDismissed]);

  useEffect(() => {
    const unsubs: (() => void)[] = [];
    void listen<{ row: number; column: number; visual: VisualState }>("visual-updated", (ev) => {
      const { row, column, visual } = ev.payload;
      setCellVisuals((prev) => ({
        ...prev,
        [`${row},${column}`]: visual,
      }));
    }).then((u) => unsubs.push(u));

    void listen<{ surfaceId?: string }>("surfaces-changed", (ev) => {
      if (ev.payload.surfaceId) {
        selectedSurfaceIdRef.current = ev.payload.surfaceId;
        setSelectedSurfaceId(ev.payload.surfaceId);
      }
      void refreshRef.current();
    }).then((u) => unsubs.push(u));

    void listen<{ context: string; payload: unknown }>("pi-message", (ev) => {
      if (piFrameRef.current) {
        dispatchPiMessage(piFrameRef.current, ev.payload.payload ?? ev.payload);
      }
    }).then((u) => unsubs.push(u));

    void listen<{ status: string; message?: string; row?: number; column?: number }>(
      "plugin-status",
      (ev) => {
        if (ev.payload.message) {
          setStatusText(`[${ev.payload.status}] ${ev.payload.message}`);
        }
        if (ev.payload.row !== undefined && ev.payload.column !== undefined) {
          setFlash({
            row: ev.payload.row,
            col: ev.payload.column,
            kind: ev.payload.status,
          });
          window.setTimeout(() => setFlash(null), 500);
        }
      },
    ).then((u) => unsubs.push(u));

    return () => unsubs.forEach((u) => u());
  }, []);

  useEffect(() => {
    const id = window.setInterval(() => {
      if (isDraggingAction.current || isDraggingSlot.current) return;
      void invoke<ActionLibrary>("list_action_library")
        .then(setActionLibrary)
        .catch(() => {});
    }, 5000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    void updatePropertyInspector(selectedSlot);
  }, [piSyncKey, selectedSlot, updatePropertyInspector]);

  const dragValue = useMemo(
    () => ({ dragGhost, dropTarget, draggingSlotId }),
    [dragGhost, dropTarget, draggingSlotId],
  );

  const value = useMemo(
    (): DeckContextValue => ({
      profile,
      statusText,
      selectedSlotId,
      selectedSurfaceId,
      activePageId,
      gridRows,
      gridCols,
      cellVisuals,
      actionLibrary,
      expandedGroups,
      executeMode,
      actionSearch,
      slotClipboard,
      contextMenu,
      flash,
      conflictDismissed,
      startupConflicts,
      connectedSurfaces,
      deviceLabel,
      activePage,
      selectedSlot,
      piNativeSettingsVisible,
      piSettingsJson,
      setActionSearch,
      setExecuteMode,
      setConflictDismissed,
      setContextMenu,
      toggleGroup,
      refresh,
      selectSurface,
      setActivePage,
      onSlotClick,
      applyActionToCell,
      transferSlotCell,
      copySlot,
      deleteSlotContent,
      pasteSlot,
      saveSlotAppearance,
      saveMultiAction,
      saveNativeSettings: scheduleNativeSettingsSave,
      loadPlugin,
      stopPlugin,
      openSettings,
      setSwitchPageTarget,
      addMultiActionStepFromDrag,
      beginActionDrag,
      beginSlotDrag,
      registerGridRef,
      registerPiFrameRef,
    }),
    [
      profile,
      statusText,
      selectedSlotId,
      selectedSurfaceId,
      activePageId,
      gridRows,
      gridCols,
      cellVisuals,
      actionLibrary,
      expandedGroups,
      executeMode,
      actionSearch,
      slotClipboard,
      contextMenu,
      flash,
      conflictDismissed,
      startupConflicts,
      connectedSurfaces,
      deviceLabel,
      activePage,
      selectedSlot,
      piNativeSettingsVisible,
      piSettingsJson,
      refresh,
      selectSurface,
      setActivePage,
      onSlotClick,
      applyActionToCell,
      transferSlotCell,
      copySlot,
      deleteSlotContent,
      pasteSlot,
      saveSlotAppearance,
      saveMultiAction,
      scheduleNativeSettingsSave,
      loadPlugin,
      stopPlugin,
      openSettings,
      setSwitchPageTarget,
      addMultiActionStepFromDrag,
      beginActionDrag,
      beginSlotDrag,
      registerGridRef,
      registerPiFrameRef,
    ],
  );

  return (
    <DeckContext.Provider value={value}>
      <DeckDragContext.Provider value={dragValue}>{children}</DeckDragContext.Provider>
    </DeckContext.Provider>
  );
}
