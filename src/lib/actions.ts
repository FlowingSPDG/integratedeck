import type { PluginLibraryEntry } from "./types";

export function filterActions(
  query: string,
  entries: PluginLibraryEntry[],
): PluginLibraryEntry[] {
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

export function statusBadgeLabel(status: string): string {
  const labels: Record<string, string> = {
    running: "起動中",
    stopped: "停止",
    connected: "接続済",
    available: "未接続",
  };
  return labels[status] ?? status;
}

export function isRunningStatus(status: string): boolean {
  return status === "running" || status === "connected";
}
