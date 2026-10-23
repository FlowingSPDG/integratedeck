import { memo, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Form for a Companion action/feedback's options, built from the module's own
 * definitions (`connection.getDefinitions`). Companion's own UI is generated the
 * same way, so this keeps the module's field types, labels and dropdown choices.
 */

export interface CompanionOptionDef {
  id: string;
  label?: string;
  type: string;
  default?: unknown;
  choices?: Array<{ id: string | number; label: string }>;
  min?: number;
  max?: number;
  step?: number;
  description?: string;
  useVariables?: boolean;
}

interface Definitions {
  actions: Array<{ id: string; name?: string; options?: CompanionOptionDef[] }>;
  feedbacks: Array<{ id: string; name?: string; options?: CompanionOptionDef[] }>;
}

const definitionCache = new Map<string, Definitions>();

async function loadDefinitions(connectionId: string): Promise<Definitions> {
  const cached = definitionCache.get(connectionId);
  if (cached) return cached;
  const definitions = await invoke<Definitions>("get_companion_definitions", { connectionId });
  const normalized: Definitions = {
    actions: definitions?.actions ?? [],
    feedbacks: definitions?.feedbacks ?? [],
  };
  definitionCache.set(connectionId, normalized);
  return normalized;
}

function toHex(color: unknown): string {
  if (typeof color === "number") {
    return `#${(color & 0xffffff).toString(16).padStart(6, "0")}`;
  }
  if (typeof color === "string") {
    const trimmed = color.trim();
    if (/^#[0-9a-f]{6}$/i.test(trimmed)) return trimmed;
    if (/^\d+$/.test(trimmed)) return `#${(Number(trimmed) & 0xffffff).toString(16).padStart(6, "0")}`;
  }
  return "#000000";
}

function fromHex(hex: string, previous: unknown): unknown {
  const value = Number.parseInt(hex.replace("#", ""), 16);
  if (typeof previous === "string" && !/^\d+$/.test(previous.trim())) return hex;
  return Number.isNaN(value) ? previous : value;
}

export const CompanionOptionsEditor = memo(function CompanionOptionsEditor({
  connectionId,
  section,
  definitionId,
  options,
  onChange,
}: {
  connectionId: string;
  section: "actions" | "feedbacks";
  definitionId: string;
  options: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
}) {
  const [fields, setFields] = useState<CompanionOptionDef[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    void loadDefinitions(connectionId)
      .then((definitions) => {
        if (cancelled) return;
        const entry = (section === "actions" ? definitions.actions : definitions.feedbacks).find(
          (item) => item.id === definitionId,
        );
        setFields(entry?.options ?? []);
      })
      .catch(() => {
        if (!cancelled) setFields([]);
      });
    return () => {
      cancelled = true;
    };
  }, [connectionId, section, definitionId]);

  const update = (id: string, value: unknown) => {
    onChange({ ...options, [id]: value });
  };

  const hasFields = useMemo(
    () => (fields ?? []).some((field) => field.type !== "static-text"),
    [fields],
  );

  if (fields === null) {
    return <p className="muted">オプション定義を読み込み中…</p>;
  }
  if (fields.length === 0) {
    return <p className="muted">この項目に設定できるオプションはありません。</p>;
  }

  return (
    <div className="companion-form">
      {fields.map((field) => {
        const value = options[field.id] ?? field.default;
        const key = field.id;
        if (field.type === "static-text") {
          return (
            <p className="companion-static" key={key}>
              {String(value ?? field.description ?? "")}
            </p>
          );
        }
        return (
          <label className="companion-field" key={key}>
            <span className="companion-field-label">{field.label ?? field.id}</span>
            {field.type === "dropdown" ? (
              <select
                value={value === undefined || value === null ? "" : String(value)}
                onChange={(event) => {
                  const raw = event.target.value;
                  const choice = field.choices?.find((item) => String(item.id) === raw);
                  update(key, choice ? choice.id : raw);
                }}
              >
                <option value="">未設定</option>
                {(field.choices ?? []).map((choice) => (
                  <option key={String(choice.id)} value={String(choice.id)}>
                    {choice.label}
                  </option>
                ))}
              </select>
            ) : field.type === "multidropdown" ? (
              <select
                multiple
                value={Array.isArray(value) ? value.map(String) : []}
                onChange={(event) => {
                  const selected = Array.from(event.target.selectedOptions).map((option) => {
                    const raw = option.value;
                    const choice = field.choices?.find((item) => String(item.id) === raw);
                    return choice ? choice.id : raw;
                  });
                  update(key, selected);
                }}
              >
                {(field.choices ?? []).map((choice) => (
                  <option key={String(choice.id)} value={String(choice.id)}>
                    {choice.label}
                  </option>
                ))}
              </select>
            ) : field.type === "checkbox" ? (
              <input
                type="checkbox"
                checked={Boolean(value)}
                onChange={(event) => update(key, event.target.checked)}
              />
            ) : field.type === "number" ? (
              <input
                type="number"
                value={value === undefined || value === null ? "" : String(value)}
                min={field.min}
                max={field.max}
                step={field.step ?? 1}
                onChange={(event) => {
                  const raw = event.target.value;
                  update(key, raw === "" ? "" : Number(raw));
                }}
              />
            ) : field.type === "colorpicker" ? (
              <span className="companion-color">
                <input
                  type="color"
                  value={toHex(value)}
                  onChange={(event) => update(key, fromHex(event.target.value, value))}
                />
                <input
                  type="text"
                  value={value === undefined || value === null ? "" : String(value)}
                  onChange={(event) => {
                    const raw = event.target.value;
                    if (/^\d+$/.test(raw)) update(key, Number(raw));
                    else update(key, raw);
                  }}
                />
              </span>
            ) : (
              <input
                type="text"
                value={value === undefined || value === null ? "" : String(value)}
                placeholder={field.useVariables ? "$(variable) も使えます" : undefined}
                onChange={(event) => update(key, event.target.value)}
              />
            )}
            {field.description ? (
              <span className="companion-field-hint">{field.description}</span>
            ) : null}
          </label>
        );
      })}
      {hasFields ? null : <p className="muted">表示できる設定項目がありません。</p>}
    </div>
  );
});
