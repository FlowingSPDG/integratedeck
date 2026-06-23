import { memo, useMemo } from "react";
import { useDeck } from "../deck/DeckContext";
import type { Page } from "../lib/types";

export const PageNav = memo(function PageNav() {
  const { profile, activePageId, setActivePage } = useDeck();

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

  if (!profile) return <div className="page-nav" />;

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
      <span className="page-breadcrumb">{breadcrumb.join(" / ")}</span>
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
    </div>
  );
});
