import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { hourglassScene } from './hourglass';
import { weatherScene } from './weather';
import { calendarScene } from './calendar';
import { noteScene } from './note';
import type { Scene, SceneFactory } from './types';

// Same layout as the per-window structure (lab.rs): four 240x280 slots in a column,
// 40px from the right and top edges, 24px apart.
const SLOT_W = 240, SLOT_H = 280, EDGE = 40, GAP = 24;
const WIDGETS = [hourglassScene, weatherScene, calendarScene, noteScene];

interface HitRect { x: number; y: number; w: number; h: number }

/** One big transparent window with all four widgets, drawn by one loop. The window is click-through
 *  except over the widgets; the Rust side toggles that from the rectangles reported here. */
export const widgetsAllScene: SceneFactory = (root, ctx) => {
  const slots: HTMLElement[] = [];
  const scenes: Scene[] = [];
  const force: boolean[] = []; // a scene asked for a frame while the loop was stopped
  WIDGETS.forEach((make, i) => {
    const slot = document.createElement('div');
    slot.style.cssText = `position:absolute;width:${SLOT_W}px;height:${SLOT_H}px;right:${EDGE}px;top:${EDGE + i * (SLOT_H + GAP)}px`;
    root.append(slot);
    slots.push(slot);
    force.push(true); // everything draws once
    scenes.push(make(slot, { requestFrames() { force[i] = true; ctx.requestFrames(); } }));
  });

  // Visible extent of each widget: the union of its children's boxes (not the whole 240x280 slot).
  const cssRects = (): { x: number; y: number; w: number; h: number }[] => slots.flatMap((slot) => {
    let l = Infinity, t = Infinity, r = -Infinity, b = -Infinity;
    for (const el of Array.from(slot.children)) {
      const q = el.getBoundingClientRect();
      if (q.width === 0 || q.height === 0) continue;
      l = Math.min(l, q.left); t = Math.min(t, q.top); r = Math.max(r, q.right); b = Math.max(b, q.bottom);
    }
    return r > l ? [{ x: l, y: t, w: r - l, h: b - t }] : [];
  });

  const isTauri = '__TAURI_INTERNALS__' in window;
  let timers: number[] = [];
  let unlisten: (() => void) | undefined;
  let gone = false;
  if (isTauri) {
    const win = getCurrentWindow();
    const send = async (): Promise<void> => {
      const pos = await win.outerPosition(); // screen position, physical px
      const dpr = window.devicePixelRatio;
      const rects: HitRect[] = cssRects().map((q) => ({
        x: pos.x + Math.round(q.x * dpr), y: pos.y + Math.round(q.y * dpr), w: Math.round(q.w * dpr), h: Math.round(q.h * dpr),
      }));
      await invoke('lab_set_hit_rects', { label: win.label, rects });
    };
    const safeSend = (): void => { if (!gone) void send().catch(() => {}); };
    // The window is placed and attached right after it loads, so report again a few times,
    // and whenever it moves.
    timers = [200, 1000, 3000].map((ms) => window.setTimeout(safeSend, ms));
    let settle = 0;
    void win.onMoved(() => { window.clearTimeout(settle); settle = window.setTimeout(safeSend, 300); })
      .then((u) => { if (gone) u(); else unlisten = u; });
  }

  return {
    wantsFrames: () => scenes.some((s) => s.wantsFrames()),
    frame(dt, now) {
      scenes.forEach((s, i) => {
        if (force[i] || s.wantsFrames()) { force[i] = false; s.frame(dt, now); }
      });
    },
    destroy() {
      gone = true;
      timers.forEach((t) => window.clearTimeout(t));
      unlisten?.();
      scenes.forEach((s) => s.destroy());
      slots.forEach((s) => s.remove());
    },
  };
};
