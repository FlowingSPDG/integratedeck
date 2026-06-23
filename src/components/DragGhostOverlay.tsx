import { memo } from "react";
import { useDeckDrag } from "../deck/DeckContext";

export const DragGhostOverlay = memo(function DragGhostOverlay() {
  const { dragGhost } = useDeckDrag();
  if (!dragGhost) return null;

  return (
    <div
      className="drag-ghost"
      style={{ left: dragGhost.x + 12, top: dragGhost.y + 12 }}
    >
      {dragGhost.label}
    </div>
  );
});
