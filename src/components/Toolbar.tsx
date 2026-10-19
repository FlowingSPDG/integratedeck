import { memo } from "react";
import { useDeck } from "../deck/DeckContext";

export const Toolbar = memo(function Toolbar() {
  const {
    profile,
    statusText,
    connectedSurfaces,
    selectedSurfaceId,
    activePageId,
    executeMode,
    setExecuteMode,
    selectSurface,
    setActivePage,
    addPage,
    deletePage,
    openSettings,
  } = useDeck();

  return (
    <header className="toolbar">
      <span className="toolbar-brand">integratedeck</span>
      <div className="toolbar-select">
        <label>デバイス</label>
        <select
          value={selectedSurfaceId ?? ""}
          onChange={(e) => void selectSurface(e.target.value)}
        >
          {connectedSurfaces.map((s) => (
            <option key={s.surfaceId} value={s.surfaceId}>
              {s.label} ({s.rows}×{s.columns})
            </option>
          ))}
        </select>
      </div>
      <div className="toolbar-select">
        <label>プロファイル</label>
        <div className="toolbar-page-row">
          <select
            value={activePageId ?? ""}
            onChange={(e) => void setActivePage(e.target.value)}
          >
            {profile?.pages.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            )) ?? null}
          </select>
          <button
            type="button"
            className="toolbar-add-page"
            title="新しいページを追加"
            aria-label="新しいページを追加"
            onClick={() => void addPage()}
          >
            +
          </button>
          <button
            type="button"
            className="toolbar-add-page"
            title="現在のページを削除"
            aria-label="現在のページを削除"
            disabled={!activePageId || (profile?.pages.length ?? 0) <= 1}
            onClick={() => activePageId && void deletePage(activePageId)}
          >
            −
          </button>
        </div>
      </div>
      <span className="toolbar-spacer" />
      <label className="toolbar-toggle" title="ON: キーを押すとアクションを実行">
        <input
          type="checkbox"
          checked={executeMode}
          onChange={(e) => setExecuteMode(e.target.checked)}
        />
        <span>Execute Mode</span>
      </label>
      <span className="toolbar-status">{statusText}</span>
      <button type="button" id="btn-settings" onClick={openSettings}>
        ⚙
      </button>
    </header>
  );
});
