import { memo, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useDeck } from "../deck/DeckContext";
import { normalizeBinding } from "../lib/binding";
import { filterCompanionPresets, presetLabel } from "../lib/presets";
import { useCompanionPresets } from "../hooks/useCompanionPresets";
import { useDebouncedValue } from "../hooks/useDebouncedValue";

/** Human readable label for a dial binding stored in the profile. */
function bindingLabel(binding: Record<string, unknown> | undefined): string {
  if (!binding) return "未割り当て";
  const kind = typeof binding.kind === "string" ? binding.kind : undefined;
  if (kind === "companion_preset") {
    const presetId = binding.preset_id;
    return typeof presetId === "string" ? `プリセット: ${presetId}` : "プリセット";
  }
  const action = binding.action_id ?? binding.actionId ?? kind;
  return action ? String(action) : "未割り当て";
}

/** Encoder/dial view for surfaces that report dials. */
export const DialStrip = memo(function DialStrip() {
  const {
    encoderCount,
    deviceLabel,
    activePage,
    actionLibrary,
    selectedSlot,
    selectedSurfaceId,
    refresh,
  } = useDeck();
  const dials = Array.from({ length: Math.max(encoderCount, 1) }, (_, i) => i);
  const selected = selectedSlot?.binding
    ? normalizeBinding(selectedSlot.binding)
    : null;
  const canAssign =
    selected?.type === "companion" ||
    selected?.type === "companion_preset" ||
    selected?.type === "stream_deck" ||
    selected?.type === "builtin";

  const connections = actionLibrary.companion;
  const [presetConnection, setPresetConnection] = useState("");
  const [presetQuery, setPresetQuery] = useState("");
  const [presetId, setPresetId] = useState("");
  const debouncedQuery = useDebouncedValue(presetQuery, 150);
  const activeConnection = presetConnection || connections[0]?.id || "";
  const { presets, loading } = useCompanionPresets(activeConnection || null);
  const filteredPresets = useMemo(
    () => filterCompanionPresets(presets, debouncedQuery),
    [presets, debouncedQuery],
  );
  const selectedPreset = filteredPresets.find((preset) => preset.id === presetId);

  const assign = async (index: number) => {
    if (!selected || !selectedSurfaceId) return;
    if (selected.type === "companion") {
      await invoke("bind_encoder_companion", {
        args: {
          surfaceId: selectedSurfaceId,
          index,
          connectionId: selected.connectionId,
          actionId: selected.actionId,
          options: selected.options ?? {},
        },
      });
      await invoke("set_encoder_led", {
        surfaceId: selectedSurfaceId,
        index,
        r: 120,
        g: 60,
        b: 180,
      });
    } else if (selected.type === "companion_preset") {
      await invoke("bind_encoder_companion_preset", {
        args: {
          surfaceId: selectedSurfaceId,
          index,
          connectionId: selected.connectionId,
          presetId: selected.presetId,
        },
      });
      await invoke("set_encoder_led", {
        surfaceId: selectedSurfaceId,
        index,
        r: 200,
        g: 120,
        b: 40,
      });
    } else if (selected.type === "stream_deck") {
      await invoke("bind_encoder_sd", {
        args: {
          surfaceId: selectedSurfaceId,
          index,
          pluginUuid: selected.pluginUuid,
          actionUuid: selected.actionUuid,
        },
      });
      await invoke("set_encoder_led", {
        surfaceId: selectedSurfaceId,
        index,
        r: 60,
        g: 90,
        b: 180,
      });
    } else if (selected.type === "builtin") {
      await invoke("bind_encoder_builtin", {
        args: {
          surfaceId: selectedSurfaceId,
          index,
          actionId: selected.actionId,
          settings: selected.settings ?? {},
        },
      });
      await invoke("set_encoder_led", {
        surfaceId: selectedSurfaceId,
        index,
        r: 60,
        g: 160,
        b: 90,
      });
    }
    await refresh({ surfaceId: selectedSurfaceId });
  };

  const assignPreset = async (index: number) => {
    if (!selectedSurfaceId || !activeConnection || !selectedPreset) return;
    await invoke("bind_encoder_companion_preset", {
      args: {
        surfaceId: selectedSurfaceId,
        index,
        connectionId: activeConnection,
        presetId: selectedPreset.id,
      },
    });
    await invoke("set_encoder_led", {
      surfaceId: selectedSurfaceId,
      index,
      r: 200,
      g: 120,
      b: 40,
    });
    await refresh({ surfaceId: selectedSurfaceId });
  };

  const clear = async (index: number) => {
    if (!selectedSurfaceId) return;
    await invoke("unbind_encoder", {
      args: { surfaceId: selectedSurfaceId, index },
    });
    await invoke("set_encoder_led", {
      surfaceId: selectedSurfaceId,
      index,
      r: 0,
      g: 0,
      b: 0,
    });
    await refresh({ surfaceId: selectedSurfaceId });
  };

  return (
    <div className="dial-strip" role="group" aria-label={`${deviceLabel} エンコーダ`}>
      {dials.map((index) => {
        const binding = activePage?.encoders?.[String(index)];
        const bound = Boolean(binding);
        const label = bindingLabel(binding);
        return (
          <div
            className="dial-slot"
            key={index}
            data-dial-index={index}
            data-bound={bound ? "true" : "false"}
          >
            <div className="dial-knob" aria-hidden="true">
              <span className="dial-glyph">◉</span>
              <span className="dial-rotate">⟳</span>
            </div>
            <div className="dial-meta">
              <span className="dial-title">ダイヤル {index + 1}</span>
              <span className="dial-state">{label}</span>
            </div>
            <div className="dial-press">プッシュ: {label}</div>
            <div className="dial-actions">
              <button
                type="button"
                className="page-btn"
                disabled={!canAssign || !selectedSurfaceId}
                onClick={() => void assign(index)}
              >
                選択中を割当
              </button>
              <button
                type="button"
                className="page-btn dial-preset-assign"
                disabled={!selectedPreset || !selectedSurfaceId}
                onClick={() => void assignPreset(index)}
              >
                プリセット割当
              </button>
              {bound ? (
                <button
                  type="button"
                  className="page-btn"
                  onClick={() => void clear(index)}
                >
                  解除
                </button>
              ) : null}
            </div>
          </div>
        );
      })}
      <div className="dial-presets">
        <div className="dial-presets-header">
          <span>Companion プリセット</span>
          {loading ? <span className="muted">読み込み中…</span> : null}
          <span className="muted">{presets.length} 件</span>
        </div>
        {connections.length === 0 ? (
          <p className="muted">Companion 接続がありません</p>
        ) : (
          <>
            <div className="dial-preset-controls">
              <select
                aria-label="Companion 接続"
                value={activeConnection}
                onChange={(e) => {
                  setPresetConnection(e.target.value);
                  setPresetId("");
                }}
              >
                {connections.map((connection) => (
                  <option key={connection.id} value={connection.id}>
                    {connection.name || connection.id}
                  </option>
                ))}
              </select>
              <input
                type="search"
                placeholder="プリセットを検索…"
                value={presetQuery}
                onChange={(e) => setPresetQuery(e.target.value)}
              />
              <select
                aria-label="Companion プリセット"
                value={presetId}
                onChange={(e) => setPresetId(e.target.value)}
              >
                <option value="">プリセットを選択…</option>
                {filteredPresets.slice(0, 400).map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {presetLabel(preset)}
                    {preset.hasSteps ? "" : " (空)"}
                  </option>
                ))}
              </select>
            </div>
            {filteredPresets.length > 400 ? (
              <p className="muted">
                先頭 400 件を表示中 — 検索で絞り込むと残りも選べます。
              </p>
            ) : null}
          </>
        )}
      </div>
      <p className="dial-hint">
        キーパッドでアクションを選択してから「選択中を割当」を押すと、このダイヤルにバインドされます。
        プリセットを選んで「プリセット割当」を押すと、そのプリセットの down/up・回転アクションがダイヤルで動きます。
      </p>
    </div>
  );
});
