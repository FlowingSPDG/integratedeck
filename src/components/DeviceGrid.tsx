import { memo, useMemo } from "react";
import { useDeck, useDeckDrag } from "../deck/DeckContext";
import { buildSlotIndex, findSlot, isSlotConfigured, slotDisplayContent } from "../lib/slots";
import type { Slot, VisualState } from "../lib/types";

interface GridCellProps {
  row: number;
  col: number;
  slot: Slot | undefined;
  selected: boolean;
  dragOver: boolean;
  dragging: boolean;
  flashClass: string;
  cellVisuals: Record<string, VisualState>;
  executeMode: boolean;
  onSlotClick: (row: number, col: number, existing?: Slot) => Promise<void>;
  beginSlotDrag: (
    e: React.PointerEvent,
    slot: Slot,
    row: number,
    col: number,
    slotEl: HTMLElement,
  ) => void;
  setContextMenu: (menu: {
    x: number;
    y: number;
    row: number;
    col: number;
    slot?: Slot;
  }) => void;
}

const GridCell = memo(function GridCell({
  row,
  col,
  slot,
  selected,
  dragOver,
  dragging,
  flashClass,
  cellVisuals,
  executeMode,
  onSlotClick,
  beginSlotDrag,
  setContextMenu,
}: GridCellProps) {
  const content = slotDisplayContent(slot, row, col, cellVisuals);
  const canInteract =
    slot && ((isSlotConfigured(slot) && !executeMode) || (executeMode && slot.binding));

  return (
    <div
      className={["slot", selected ? "selected" : "", dragOver ? "drag-over" : "", dragging ? "dragging" : "", flashClass]
        .filter(Boolean)
        .join(" ")}
      role="button"
      tabIndex={0}
      data-row={row}
      data-col={col}
      data-has-binding={slot?.binding ? "true" : undefined}
      onClick={() => {
        if (executeMode && slot?.binding) return;
        void onSlotClick(row, col, slot);
      }}
      onPointerDown={
        canInteract ? (e) => beginSlotDrag(e, slot!, row, col, e.currentTarget) : undefined
      }
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setContextMenu({ x: e.clientX, y: e.clientY, row, col, slot });
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          void onSlotClick(row, col, slot);
        }
      }}
    >
      {content.imgSrc ? (
        <>
          <img src={content.imgSrc} alt={content.title ?? ""} />
          {content.title ? <span className="slot-title">{content.title}</span> : null}
        </>
      ) : content.title ? (
        content.title
      ) : content.fallback ? (
        content.fallback
      ) : null}
    </div>
  );
});

export const DeviceGrid = memo(function DeviceGrid() {
  const {
    activePage,
    selectedSurfaceId,
    selectedSlotId,
    gridRows,
    gridCols,
    cellVisuals,
    executeMode,
    flash,
    registerGridRef,
    onSlotClick,
    beginSlotDrag,
    setContextMenu,
  } = useDeck();
  const { dropTarget, draggingSlotId } = useDeckDrag();

  const slotIndex = useMemo(() => {
    if (!activePage || !selectedSurfaceId) return null;
    return buildSlotIndex(activePage, selectedSurfaceId);
  }, [activePage, selectedSurfaceId]);

  const gridStyle = useMemo(
    () => ({
      gridTemplateColumns: `repeat(${gridCols}, var(--slot-size))`,
      gridTemplateRows: `repeat(${gridRows}, var(--slot-size))`,
    }),
    [gridCols, gridRows],
  );

  if (!activePage || !selectedSurfaceId || !slotIndex) return null;

  const cells: React.ReactNode[] = [];
  for (let row = 0; row < gridRows; row++) {
    for (let col = 0; col < gridCols; col++) {
      const slot = findSlot(activePage, row, col, selectedSurfaceId, slotIndex);
      const flashing =
        flash?.row === row && flash?.col === col
          ? flash.kind === "alert"
            ? "slot-flash-alert"
            : "slot-flash-ok"
          : "";

      cells.push(
        <GridCell
          key={`${row}-${col}`}
          row={row}
          col={col}
          slot={slot}
          selected={slot?.id === selectedSlotId}
          dragOver={dropTarget?.row === row && dropTarget?.col === col}
          dragging={draggingSlotId === slot?.id}
          flashClass={flashing}
          cellVisuals={cellVisuals}
          executeMode={executeMode}
          onSlotClick={onSlotClick}
          beginSlotDrag={beginSlotDrag}
          setContextMenu={setContextMenu}
        />,
      );
    }
  }

  return (
    <div className="grid" ref={registerGridRef} style={gridStyle}>
      {cells}
    </div>
  );
});
