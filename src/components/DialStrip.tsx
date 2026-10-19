import { memo } from "react";
import { useDeck } from "../deck/DeckContext";

/**
 * Encoder/dial view for surfaces that report dials. The backend currently has
 * no dial slot model (SlotLocator only describes keys and SurfaceInput
 * encoders are routed per context), so dials render an explicit "not bound"
 * state instead of pretending to be editable keys.
 */
export const DialStrip = memo(function DialStrip() {
  const { encoderCount, deviceLabel } = useDeck();
  const dials = Array.from({ length: Math.max(encoderCount, 1) }, (_, i) => i);

  return (
    <div className="dial-strip" role="group" aria-label={`${deviceLabel} エンコーダ`}>
      {dials.map((index) => (
        <div className="dial-slot" key={index} data-dial-index={index} data-bound="false">
          <div className="dial-knob" aria-hidden="true">
            <span className="dial-glyph">◉</span>
            <span className="dial-rotate">⟳</span>
          </div>
          <div className="dial-meta">
            <span className="dial-title">ダイヤル {index + 1}</span>
            <span className="dial-state">未割り当て</span>
          </div>
          <div className="dial-press">プッシュ: 未割り当て</div>
        </div>
      ))}
      <p className="dial-hint">
        ダイヤル割り当てはまだバックエンド未対応です。キーパッドに切り替えてキーを編集できます。
      </p>
    </div>
  );
});
