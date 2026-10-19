import { memo } from "react";
import { useDeck } from "../deck/DeckContext";

/** Encoder/dial view for surfaces that report dials. */
export const DialStrip = memo(function DialStrip() {
  const { encoderCount, deviceLabel, activePage } = useDeck();
  const dials = Array.from({ length: Math.max(encoderCount, 1) }, (_, i) => i);

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
          </div>
        );
      })}
      <p className="dial-hint">
        ダイヤルはページごとのエンコーダバインディングを使用します。回転/押下はバインド済みアクションへ送られます。
      </p>
    </div>
  );
});
