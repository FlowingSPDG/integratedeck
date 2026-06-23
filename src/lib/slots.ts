import {
  BUILTIN_BACK,
  BUILTIN_MULTI,
  BUILTIN_OPEN_FOLDER,
  BUILTIN_SWITCH_PAGE,
  normalizeBinding,
} from "./binding";
import type { Page, Slot, VisualState } from "./types";

export function slotIndexKey(row: number, col: number, surfaceId: string): string {
  return `${surfaceId}:${row},${col}`;
}

export function buildSlotIndex(page: Page, surfaceId: string): Map<string, Slot> {
  const index = new Map<string, Slot>();
  for (const slot of Object.values(page.slots)) {
    if (slot.locator.surface_id === surfaceId) {
      index.set(slotIndexKey(slot.locator.row, slot.locator.column, surfaceId), slot);
    }
  }
  return index;
}

export function findSlot(
  page: Page,
  row: number,
  col: number,
  surfaceId: string,
  index?: Map<string, Slot>,
): Slot | undefined {
  if (index) return index.get(slotIndexKey(row, col, surfaceId));
  return Object.values(page.slots).find(
    (s) =>
      s.locator.row === row &&
      s.locator.column === col &&
      s.locator.surface_id === surfaceId,
  );
}

export function isSlotConfigured(slot?: Slot): boolean {
  if (!slot) return false;
  if (slot.binding) return true;
  if (slot.appearance?.title?.trim()) return true;
  if (slot.appearance?.default_image?.data) return true;
  return false;
}

export function visualForCell(
  cellVisuals: Record<string, VisualState>,
  row: number,
  col: number,
): VisualState | undefined {
  return cellVisuals[`${row},${col}`];
}

export function slotDragLabel(
  slot: Slot,
  row: number,
  col: number,
  cellVisuals: Record<string, VisualState>,
): string {
  const visual = visualForCell(cellVisuals, row, col);
  const title = visual?.title ?? slot.appearance?.title ?? slot.label;
  if (title?.trim()) return title;
  const b = normalizeBinding(slot.binding);
  if (b?.type === "builtin") {
    if (b.actionId === BUILTIN_OPEN_FOLDER) return "Folder";
    if (b.actionId === BUILTIN_BACK) return "Back";
    if (b.actionId === BUILTIN_SWITCH_PAGE) return "Page";
    if (b.actionId === BUILTIN_MULTI) return "Multi Action";
  }
  if (b?.type === "stream_deck") return "Stream Deck";
  if (b?.type === "companion") return "Companion";
  if (b?.type === "multi_action") return "Multi Action";
  return "Key";
}

export function slotDisplayContent(
  slot: Slot | undefined,
  row: number,
  col: number,
  cellVisuals: Record<string, VisualState>,
): { imgSrc?: string; title?: string; fallback?: string } {
  const visual = visualForCell(cellVisuals, row, col);
  const defaultImg = slot?.appearance?.default_image;
  const imgData = visual?.image?.data ?? defaultImg?.data;
  const imgFormat = visual?.image?.format ?? defaultImg?.format ?? "png";
  const title = visual?.title ?? slot?.appearance?.title ?? slot?.label;

  if (imgData) {
    const mime = imgFormat === "jpeg" ? "jpeg" : "png";
    return { imgSrc: `data:image/${mime};base64,${imgData}`, title: title ?? undefined };
  }
  if (title) return { title };
  if (slot?.binding) {
    const b = normalizeBinding(slot.binding);
    if (b?.type === "builtin") {
      const fallback =
        b.actionId === BUILTIN_OPEN_FOLDER
          ? "📁"
          : b.actionId === BUILTIN_BACK
            ? "◀"
            : b.actionId === BUILTIN_SWITCH_PAGE
              ? "⇄"
              : b.actionId === BUILTIN_MULTI
                ? "⚡"
                : "Nav";
      return { fallback };
    }
    if (b?.type === "stream_deck") return { fallback: "SD" };
    if (b?.type === "companion") return { fallback: "Comp" };
  }
  return {};
}
