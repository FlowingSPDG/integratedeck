import { memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useDeck } from "../deck/DeckContext";
import { normalizeBinding } from "../lib/binding";

/** Encoder/dial view for surfaces that report dials. */
export const DialStrip = memo(function DialStrip() {
  const {
    encoderCount,
    deviceLabel,
    activePage,
    selectedSlot,
    selectedSurfaceId,
    refresh,
  } = useDeck();
  const dials = Array.from({ length: Math.max(encoderCount, 1) }, (_, i) => i);
  const selected = selectedSlot?.binding
    ? normalizeBinding(selectedSlot.binding)
    : null;
  const canAssign =
    selected?.type === "companion" || selected?.type === "stream_deck";

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
    }
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
        const action =
          binding && (binding.action_id ?? binding.actionId ?? binding.kind);
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
              <span className="dial-state">
                {bound ? String(action) : "未割り当て"}
              </span>
            </div>
            <div className="dial-press">
              プッシュ: {bound ? String(action) : "未割り当て"}
            </div>
            <div className="dial-actions">
              <button
                type="button"
                className="page-btn"
                disabled={!canAssign || !selectedSurfaceId}
                onClick={() => void assign(index)}
              >
                選択中を割当
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
      <p className="dial-hint">
        キーパッドでアクションを選択してから「選択中を割当」を押すと、このダイヤルにバインドされます。
      </p>
    </div>
  );
});
