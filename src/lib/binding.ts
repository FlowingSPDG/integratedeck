import type { MultiActionStep, NormalizedBinding } from "./types";

export const BUILTIN_OPEN_FOLDER = "com.elgato.streamdeck.profile.openchild";
export const BUILTIN_BACK = "com.elgato.streamdeck.profile.backtoparent";
export const BUILTIN_SWITCH_PAGE = "com.elgato.streamdeck.profile.rotate";
export const BUILTIN_MULTI = "com.elgato.streamdeck.multiactions.routine";

export function normalizeBinding(binding?: Record<string, unknown>): NormalizedBinding | null {
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

export function bindingStepLabel(step: MultiActionStep): string {
  const b = step.binding as Record<string, unknown>;
  const kind = b.kind as string | undefined;
  if (kind === "stream_deck") return `SD: ${String(b.action_uuid ?? "?")}`;
  if (kind === "companion") return `Companion: ${String(b.action_id ?? "?")}`;
  if (kind === "built_in") return `Built-in: ${String(b.action_id ?? "?")}`;
  return "Step";
}

export function folderChildPageId(settings: unknown): string | null {
  if (!settings || typeof settings !== "object") return null;
  const s = settings as Record<string, unknown>;
  const id = s.childPageId ?? s.child_page_id ?? s.ProfileUUID ?? s.profile_uuid;
  return typeof id === "string" ? id : null;
}
