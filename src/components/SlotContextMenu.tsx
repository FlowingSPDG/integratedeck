import { useEffect } from "react";
import { useDeck } from "../deck/DeckContext";
import { isSlotConfigured } from "../lib/slots";

export function SlotContextMenu() {
  const {
    contextMenu,
    setContextMenu,
    slotClipboard,
    activePage,
    copySlot,
    deleteSlotContent,
    pasteSlot,
  } = useDeck();

  useEffect(() => {
    if (!contextMenu) return;

    const onKeyDown = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") setContextMenu(null);
    };
    const onDismiss = () => setContextMenu(null);

    const id = window.setTimeout(() => {
      document.addEventListener("keydown", onKeyDown);
      document.addEventListener("click", onDismiss);
      document.addEventListener("contextmenu", onDismiss);
      window.addEventListener("scroll", onDismiss, true);
      window.addEventListener("resize", onDismiss);
    }, 0);

    return () => {
      clearTimeout(id);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("click", onDismiss);
      document.removeEventListener("contextmenu", onDismiss);
      window.removeEventListener("scroll", onDismiss, true);
      window.removeEventListener("resize", onDismiss);
    };
  }, [contextMenu, setContextMenu]);

  if (!contextMenu || !activePage) return null;

  const { slot, row, col, x, y } = contextMenu;
  const configured = isSlotConfigured(slot);
  const canPaste = !configured && slotClipboard !== null;
  if (!configured && !canPaste) return null;

  const pad = 8;
  let left = x;
  let top = y;

  return (
    <div
      className="slot-context-menu"
      role="menu"
      style={{
        left: Math.max(pad, left),
        top: Math.max(pad, top),
        maxWidth: `calc(100vw - ${pad * 2}px)`,
      }}
      onClick={(e) => e.stopPropagation()}
    >
      {configured && slot ? (
        <>
          <button
            type="button"
            className="slot-context-menu-item"
            role="menuitem"
            onClick={() => {
              copySlot(slot);
              setContextMenu(null);
            }}
          >
            Copy
          </button>
          <button
            type="button"
            className="slot-context-menu-item danger"
            role="menuitem"
            onClick={() => {
              void deleteSlotContent(slot);
              setContextMenu(null);
            }}
          >
            Delete
          </button>
        </>
      ) : canPaste ? (
        <button
          type="button"
          className="slot-context-menu-item"
          role="menuitem"
          onClick={() => {
            void pasteSlot(row, col, slot);
            setContextMenu(null);
          }}
        >
          Paste
        </button>
      ) : null}
    </div>
  );
}
