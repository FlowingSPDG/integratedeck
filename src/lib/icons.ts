import {
  BUILTIN_BACK,
  BUILTIN_MULTI,
  BUILTIN_OPEN_FOLDER,
  BUILTIN_SWITCH_PAGE,
  normalizeBinding,
} from "./binding";
import type { ActionLibrary, NormalizedBinding, Slot } from "./types";

/**
 * Fallback artwork for actions/slots that do not (yet) provide an image.
 * Manifest icons are preferred; these inline SVGs keep the grid and the
 * action list from rendering blank squares.
 */

function svgDataUrl(body: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(body)}`;
}

function glyphIcon(glyph: string, background: string, foreground = "#e8e8ec"): string {
  return svgDataUrl(
    `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">` +
      `<rect width="64" height="64" rx="12" fill="${background}"/>` +
      `<text x="32" y="41" font-family="system-ui,sans-serif" font-size="26" font-weight="600" ` +
      `text-anchor="middle" fill="${foreground}">${glyph}</text></svg>`,
  );
}

export const DEFAULT_ACTION_ICON = glyphIcon("◆", "#2c3140");
export const STREAM_DECK_ICON = glyphIcon("▦", "#22344f");
export const COMPANION_ICON = glyphIcon("◈", "#3a2a4a");

export function builtinActionIcon(actionId?: string): string {
  switch (actionId) {
    case BUILTIN_OPEN_FOLDER:
      return glyphIcon("▣", "#4a3a1c", "#ffd479");
    case BUILTIN_BACK:
      return glyphIcon("◀", "#3a3a22", "#d8e07a");
    case BUILTIN_SWITCH_PAGE:
      return glyphIcon("⇄", "#1c3a3a", "#7ad8d8");
    case BUILTIN_MULTI:
      return glyphIcon("⚡", "#3a2c1c", "#ffb060");
    default:
      return DEFAULT_ACTION_ICON;
  }
}

function manifestIcon(
  library: ActionLibrary,
  source: "streamdeck" | "companion",
  pluginId?: string,
  actionId?: string,
): string | undefined {
  if (!pluginId || !actionId) return undefined;
  const entries = source === "streamdeck" ? library.streamdeck : library.companion;
  return entries
    .find((entry) => entry.id === pluginId)
    ?.actions.find((action) => action.id === actionId)?.icon;
}

/** Resolve the icon for an action-list entry (manifest icon or placeholder). */
export function actionListIcon(
  action: { id: string; icon?: string },
  source: string,
): string {
  if (action.icon) return action.icon;
  if (source === "builtin") return builtinActionIcon(action.id);
  return source === "companion" ? COMPANION_ICON : STREAM_DECK_ICON;
}

/** Resolve a slot's default icon from its binding + the scanned action library. */
export function iconForBinding(
  binding: Record<string, unknown> | undefined,
  library: ActionLibrary,
): string | undefined {
  const normalized: NormalizedBinding | null = normalizeBinding(binding);
  if (!normalized) return undefined;
  switch (normalized.type) {
    case "builtin":
      return builtinActionIcon(normalized.actionId);
    case "multi_action":
      return builtinActionIcon(BUILTIN_MULTI);
    case "stream_deck":
      return (
        manifestIcon(
          library,
          "streamdeck",
          normalized.pluginUuid,
          normalized.actionUuid,
        ) ?? STREAM_DECK_ICON
      );
    case "companion":
      return (
        manifestIcon(library, "companion", normalized.connectionId, normalized.actionId) ??
        COMPANION_ICON
      );
    default:
      return DEFAULT_ACTION_ICON;
  }
}

export function iconForSlot(slot: Slot | undefined, library: ActionLibrary): string | undefined {
  if (!slot?.binding) return undefined;
  return iconForBinding(slot.binding, library);
}
