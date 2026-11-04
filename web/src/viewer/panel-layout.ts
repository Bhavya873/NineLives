// Where each stage's panel sits on the page (plan: all-stages viewer). One canvas, one
// renderer; every stage is drawn into its own rectangle of it (a scissor viewport) with its own
// camera. With no focus the panels tile the area left of the side column: the three drop
// arenas on top, the two courses below (Stage D, the long one, twice as wide). Focusing one
// stage gives it the whole area. Pure.
import type { ArenaKind } from './arena-choice.ts';

/** A rectangle in CSS pixels, y down from the top of the canvas. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The tiling rows, top to bottom: each stage with its relative width. */
const ROWS: { arena: ArenaKind; weight: number }[][] = [
  [
    { arena: 'flat', weight: 1 },
    { arena: 'side', weight: 1 },
    { arena: 'obstacles', weight: 1 },
  ],
  [
    { arena: 'driving', weight: 2 },
    { arena: 'flight', weight: 1 },
  ],
];

/** Gap between panels, in CSS pixels. */
export const PANEL_GAP_PX = 4;

/** The rectangle of every panel inside `area`, or only `focus`'s (filling `area`) when a stage
 * is focused. Stages that are not drawn are absent from the result. */
export function layoutPanels(area: Rect, focus: ArenaKind | null): Map<ArenaKind, Rect> {
  const panels = new Map<ArenaKind, Rect>();
  if (focus !== null) {
    panels.set(focus, { ...area });
    return panels;
  }
  const rowHeight = (area.h - PANEL_GAP_PX * (ROWS.length - 1)) / ROWS.length;
  ROWS.forEach((row, rowIndex) => {
    const totalWeight = row.reduce((sum, cell) => sum + cell.weight, 0);
    const usable = area.w - PANEL_GAP_PX * (row.length - 1);
    let x = area.x;
    for (const cell of row) {
      const w = (usable * cell.weight) / totalWeight;
      panels.set(cell.arena, {
        x,
        y: area.y + rowIndex * (rowHeight + PANEL_GAP_PX),
        w,
        h: rowHeight,
      });
      x += w + PANEL_GAP_PX;
    }
  });
  return panels;
}
