import { memo, useEffect, useMemo, useRef, useState } from "react";
import { useDeck } from "../deck/DeckContext";
import type { Page } from "../lib/types";

export const PageNav = memo(function PageNav() {
  const { profile, activePageId, setActivePage, addPage, renamePage } = useDeck();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  const { activePage, breadcrumb, rootPages } = useMemo(() => {
    if (!profile) {
      return { activePage: undefined, breadcrumb: [] as string[], rootPages: [] as Page[] };
    }
    const activePage = profile.pages.find((p) => p.id === activePageId);
    const breadcrumb: string[] = [];
    let cursor = activePage;
    while (cursor) {
      breadcrumb.unshift(cursor.name);
      cursor = cursor.parent_page_id
        ? profile.pages.find((p) => p.id === cursor!.parent_page_id)
        : undefined;
    }
    const rootPages = profile.pages.filter((p) => !p.parent_page_id);
    return { activePage, breadcrumb, rootPages };
  }, [profile, activePageId]);

  useEffect(() => {
    if (editing) {
      setDraft(activePage?.name ?? "");
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing, activePage?.name]);

  if (!profile) return <div className="page-nav" />;

  const commit = async () => {
    if (!activePageId) return;
    setEditing(false);
    const name = draft.trim();
    if (!name || name === activePage?.name) return;
    await renamePage(activePageId, name);
  };

  return (
    <div className="page-nav">
      {activePage?.parent_page_id ? (
        <button
          type="button"
          className="page-btn page-back"
          onClick={() => void setActivePage(activePage.parent_page_id!)}
        >
          ◀
        </button>
      ) : null}
      {editing ? (
        <input
          ref={inputRef}
          className="page-name-input"
          value={draft}
          maxLength={60}
          aria-label="ページ名"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => void commit()}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void commit();
            } else if (event.key === "Escape") {
              event.preventDefault();
              setEditing(false);
            }
          }}
        />
      ) : (
        <span
          className="page-breadcrumb"
          title="ダブルクリックでページ名を変更"
          onDoubleClick={() => setEditing(true)}
        >
          {breadcrumb.join(" / ")}
        </span>
      )}
      {activePage ? (
        <button
          type="button"
          className="page-btn page-rename"
          title="ページ名を変更"
          aria-label="ページ名を変更"
          onClick={() => setEditing(true)}
        >
          ✎
        </button>
      ) : null}
      {rootPages.map((p, i) => (
        <button
          key={p.id}
          type="button"
          className={`page-btn${p.id === activePageId ? " page-active" : ""}`}
          onClick={() => void setActivePage(p.id)}
        >
          {i + 1}
        </button>
      ))}
      <button
        type="button"
        className="page-btn page-add"
        title="新しいページを追加"
        aria-label="新しいページを追加"
        onClick={() => void addPage()}
      >
        +
      </button>
    </div>
  );
});
