import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { flattenCompanionPresets, type CompanionPresetEntry } from "../lib/presets";

/** Presets never change while a connection stays configured, so one fetch per
 * connection is enough; reconnects get a fresh entry because the app restarts
 * the module engine. */
const presetCache = new Map<string, CompanionPresetEntry[]>();

/** Load the preset list of a Companion connection (cached per connection). */
export function useCompanionPresets(connectionId: string | null | undefined): {
  presets: CompanionPresetEntry[];
  loading: boolean;
} {
  const [presets, setPresets] = useState<CompanionPresetEntry[]>(
    connectionId ? (presetCache.get(connectionId) ?? []) : [],
  );
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!connectionId) {
      setPresets([]);
      return;
    }
    const cached = presetCache.get(connectionId);
    if (cached) {
      setPresets(cached);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void invoke<{ presets?: unknown }>("get_companion_definitions", { connectionId })
      .then((definitions) => {
        const list = flattenCompanionPresets(definitions?.presets);
        presetCache.set(connectionId, list);
        if (!cancelled) setPresets(list);
      })
      .catch(() => {
        if (!cancelled) setPresets([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [connectionId]);

  return { presets, loading };
}
