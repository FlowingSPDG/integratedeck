import { memo, useCallback, useMemo } from "react";
import { useDeck } from "../deck/DeckContext";
import { filterActions, isRunningStatus, statusBadgeLabel } from "../lib/actions";
import type { DragActionPayload, PluginLibraryEntry } from "../lib/types";
import { useDebouncedValue } from "../hooks/useDebouncedValue";

export const ActionSidebar = memo(function ActionSidebar() {
  const {
    actionLibrary,
    expandedGroups,
    actionSearch,
    setActionSearch,
    toggleGroup,
    loadPlugin,
    stopPlugin,
    beginActionDrag,
  } = useDeck();

  const debouncedSearch = useDebouncedValue(actionSearch, 150);
  const sd = useMemo(
    () => filterActions(debouncedSearch, actionLibrary.streamdeck),
    [debouncedSearch, actionLibrary.streamdeck],
  );
  const comp = useMemo(
    () => filterActions(debouncedSearch, actionLibrary.companion),
    [debouncedSearch, actionLibrary.companion],
  );

  return (
    <aside className="action-sidebar">
      <div className="action-sidebar-header">
        <h2>Actions</h2>
        <input
          type="search"
          placeholder="アクションを検索…"
          value={actionSearch}
          onChange={(e) => setActionSearch(e.target.value)}
        />
      </div>
      <div className="action-list">
        <div className="action-section-title">Stream Deck プラグイン ({sd.length})</div>
        {sd.length === 0 ? (
          <p className="muted">プラグインなし — 設定からスキャン</p>
        ) : (
          sd.map((entry) => {
            const groupKey = `sd:${entry.id || entry.path}`;
            return (
              <PluginGroup
                key={groupKey}
                entry={entry}
                sectionKey="sd"
                expanded={expandedGroups.has(groupKey)}
                groupKey={groupKey}
                onToggle={toggleGroup}
                onLoad={loadPlugin}
                onStop={stopPlugin}
                onDragStart={beginActionDrag}
              />
            );
          })
        )}
        <div className="action-section-title">Companion ({comp.length})</div>
        {comp.length === 0 ? (
          <p className="muted">Companion 接続なし</p>
        ) : (
          comp.map((entry) => {
            const groupKey = `comp:${entry.id || entry.path}`;
            return (
              <PluginGroup
                key={groupKey}
                entry={entry}
                sectionKey="comp"
                expanded={expandedGroups.has(groupKey)}
                groupKey={groupKey}
                onToggle={toggleGroup}
                onDragStart={beginActionDrag}
              />
            );
          })
        )}
      </div>
    </aside>
  );
});

const PluginGroup = memo(function PluginGroup({
  entry,
  sectionKey,
  expanded,
  groupKey,
  onToggle,
  onLoad,
  onStop,
  onDragStart,
}: {
  entry: PluginLibraryEntry;
  sectionKey: string;
  expanded: boolean;
  groupKey: string;
  onToggle: (key: string) => void;
  onLoad?: (path: string) => Promise<void>;
  onStop?: (uuid: string) => Promise<void>;
  onDragStart: (e: React.PointerEvent, payload: DragActionPayload) => void;
}) {
  const handleToggle = useCallback(() => onToggle(groupKey), [groupKey, onToggle]);
  const handleLoad = useCallback(() => {
    if (onLoad) void onLoad(entry.path);
  }, [entry.path, onLoad]);
  const handleStop = useCallback(() => {
    if (onStop) void onStop(entry.id);
  }, [entry.id, onStop]);

  return (
    <div className="plugin-group" data-group={`${sectionKey}:${entry.id || entry.path}`}>
      <div className="plugin-group-header" onClick={handleToggle}>
        <span className="chevron">{expanded ? "▼" : "▶"}</span>
        <span className="plugin-name">{entry.name}</span>
        <span className={`badge ${isRunningStatus(entry.status) ? "badge-running" : "badge-stopped"}`}>
          {statusBadgeLabel(entry.status)}
        </span>
        <span className="plugin-controls" onClick={(e) => e.stopPropagation()}>
          {entry.source === "streamdeck" ? (
            entry.status === "running" ? (
              <button type="button" className="btn-xs btn-stop-plugin" onClick={handleStop}>
                停止
              </button>
            ) : (
              <button type="button" className="btn-xs btn-load-plugin" onClick={handleLoad}>
                起動
              </button>
            )
          ) : null}
        </span>
      </div>
      {expanded ? (
        <div className="plugin-group-actions">
          {entry.actions.length === 0 ? (
            <p className="muted">アクションなし</p>
          ) : (
            entry.actions.map((a) => (
              <ActionItem
                key={a.id}
                entry={entry}
                actionId={a.id}
                actionName={a.name}
                onDragStart={onDragStart}
              />
            ))
          )}
        </div>
      ) : null}
    </div>
  );
});

const ActionItem = memo(function ActionItem({
  entry,
  actionId,
  actionName,
  onDragStart,
}: {
  entry: PluginLibraryEntry;
  actionId: string;
  actionName: string;
  onDragStart: (e: React.PointerEvent, payload: DragActionPayload) => void;
}) {
  const payload = useMemo(
    (): DragActionPayload => ({
      source: entry.source as DragActionPayload["source"],
      pluginId: entry.id,
      actionId,
      actionName,
      pluginPath: entry.path,
    }),
    [entry.source, entry.id, entry.path, actionId, actionName],
  );

  return (
    <div
      className="action-item"
      onPointerDown={(e) => {
        e.preventDefault();
        onDragStart(e, payload);
      }}
    >
      <span className="action-icon" />
      <span>{actionName}</span>
    </div>
  );
});
