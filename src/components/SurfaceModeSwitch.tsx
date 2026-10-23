import { memo } from "react";
import { useDeck } from "../deck/DeckContext";

/** Segmented Keypad / Encoder switch shown only for surfaces with dials. */
export const SurfaceModeSwitch = memo(function SurfaceModeSwitch() {
  const { hasEncoderSurface, encoderCount, controlMode, setControlMode } = useDeck();

  if (!hasEncoderSurface) return null;

  return (
    <div className="surface-mode-switch" role="tablist" aria-label="コントロール種別">
      <button
        type="button"
        role="tab"
        aria-selected={controlMode === "keys"}
        className={`surface-mode-btn${controlMode === "keys" ? " active" : ""}`}
        onClick={() => setControlMode("keys")}
      >
        キーパッド
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={controlMode === "dials"}
        className={`surface-mode-btn${controlMode === "dials" ? " active" : ""}`}
        onClick={() => setControlMode("dials")}
        title={`${encoderCount} 個のエンコーダ`}
      >
        エンコーダ ({encoderCount})
      </button>
    </div>
  );
});
