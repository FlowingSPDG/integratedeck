import { memo } from "react";
import { useDeck } from "../deck/DeckContext";

export const StartupConflictBanner = memo(function StartupConflictBanner() {
  const { startupConflicts, conflictDismissed, setConflictDismissed } = useDeck();

  if (conflictDismissed || !startupConflicts?.conflicts.length) {
    return <div className="startup-conflict-banner hidden" aria-hidden="true" />;
  }

  const appNames = startupConflicts.conflicts.map((conflict) => {
    if (conflict.id === "stream_deck") return "Elgato Stream Deck（公式アプリ）";
    if (conflict.id === "companion") return "Bitfocus Companion";
    return conflict.displayName;
  });

  return (
    <div className="startup-conflict-banner" role="alert" aria-live="polite">
      <div className="startup-conflict-banner__content">
        <strong>競合の可能性があります</strong>
        <p>
          {appNames.join(" と ")} が実行中です。Stream Deck
          デバイスやモジュールの競合が起きる場合があります。使用しないアプリは終了してください。
        </p>
      </div>
      <button
        type="button"
        className="startup-conflict-banner__close"
        aria-label="警告を閉じる"
        onClick={() => setConflictDismissed(true)}
      >
        ×
      </button>
    </div>
  );
});
