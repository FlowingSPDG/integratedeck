/**
 * Companion preset helpers.
 *
 * Modules publish presets through `setPresetDefinitions`. Depending on the
 * module they arrive either as a flat `id -> preset` map or as nested sections
 * that carry their own `definitions`. Both shapes are flattened here so the UI
 * can list every preset under a stable id.
 */

export interface CompanionPresetEntry {
  id: string;
  name: string;
  /** Section heading when the module groups its presets. */
  group?: string;
  /** True when the preset ships at least one executable step. */
  hasSteps: boolean;
}

interface RawPreset {
  presetId?: string;
  id?: string;
  name?: string;
  label?: string;
  type?: string;
  definitions?: unknown;
  steps?: unknown;
}

const PHASES = [
  "down",
  "up",
  "rotate_left",
  "rotate_right",
  "rotateLeft",
  "rotateRight",
  "short",
  "long",
];

function hasExecutableStep(preset: RawPreset): boolean {
  const steps = preset.steps;
  if (!steps || typeof steps !== "object") return false;
  if (Array.isArray(steps)) return steps.length > 0;
  const record = steps as Record<string, unknown>;
  return PHASES.some((phase) => {
    const phaseSteps = record[phase];
    return Array.isArray(phaseSteps) ? phaseSteps.length > 0 : Boolean(phaseSteps);
  });
}

function entryName(preset: RawPreset, fallback: string): string {
  const name = preset.name ?? preset.label;
  return typeof name === "string" && name.trim() ? name : fallback;
}

function asEntries(value: unknown): Array<[string | undefined, unknown]> {
  if (Array.isArray(value)) return value.map((item) => [undefined, item] as [string | undefined, unknown]);
  if (value && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>);
  }
  return [];
}

/**
 * Flatten the `presets` payload of `get_companion_definitions` into a list of
 * selectable entries. Presets without any step are still listed: modules use
 * them as visible placeholders that inherit the group's defaults.
 */
export function flattenCompanionPresets(raw: unknown): CompanionPresetEntry[] {
  const out: CompanionPresetEntry[] = [];
  const seen = new Set<string>();

  const visit = (value: unknown, group: string | undefined) => {
    for (const [key, item] of asEntries(value)) {
      if (!item || typeof item !== "object") continue;
      const preset = item as RawPreset;
      const nested = preset.definitions;
      // A section groups presets and has no steps of its own.
      if (Array.isArray(nested) && preset.steps === undefined) {
        visit(nested, entryName(preset, group ?? "プリセット"));
        continue;
      }
      const id = preset.presetId ?? preset.id ?? key;
      if (!id || seen.has(id)) continue;
      seen.add(id);
      out.push({
        id,
        name: entryName(preset, id),
        group,
        hasSteps: hasExecutableStep(preset),
      });
    }
  };

  // Some modules wrap everything in a `sections` map.
  const wrapped =
    raw && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as Record<string, unknown>).sections
      : undefined;
  visit(wrapped ?? raw, undefined);
  return out;
}

/** Case-insensitive filter used by the preset pickers. */
export function filterCompanionPresets(
  presets: CompanionPresetEntry[],
  query: string,
): CompanionPresetEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return presets;
  return presets.filter(
    (preset) =>
      preset.name.toLowerCase().includes(q) ||
      preset.id.toLowerCase().includes(q) ||
      (preset.group ?? "").toLowerCase().includes(q),
  );
}

/** Two-line label: preset name plus its section when one exists. */
export function presetLabel(preset: CompanionPresetEntry): string {
  return preset.group ? `${preset.group} / ${preset.name}` : preset.name;
}
