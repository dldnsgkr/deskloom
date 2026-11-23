// Mirrors `widget_slots` in src-tauri/src/lab.rs; the checklist compares the two layouts.
export const SLOT_W = 240, SLOT_H = 280, EDGE = 40, GAP = 24;

/** Top-left of each of `n` slots on a w x h monitor (logical px). Columns fill top to bottom from the
 *  top-right corner and wrap to a new column on the left, so a short screen keeps every slot on it. */
export function widgetSlots(w: number, h: number, n: number): { x: number; y: number }[] {
  const rows = Math.max(1, Math.floor((h - 2 * EDGE + GAP) / (SLOT_H + GAP)));
  return Array.from({ length: n }, (_, i) => ({
    x: w - EDGE - SLOT_W - Math.floor(i / rows) * (SLOT_W + GAP),
    y: EDGE + (i % rows) * (SLOT_H + GAP),
  }));
}
