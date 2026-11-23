import { currentMonitor, getCurrentWindow } from '@tauri-apps/api/window';
import { drawTestWallpaper } from '../test-wallpaper';
import type { SceneFactory } from './types';

const CARD_W = 236, CARD_H = 224, BLEED = 40, RADIUS = 32;
// Window layout (keep in sync with open_glass in lab.rs): the shadow (0 20px 40px) reaches 40px to the sides,
// 20px up and 60px down, so the margins and the gap are at least that wide and nothing is cut by the window.
const MARGIN_X = 44, MARGIN_TOP = 24, GAP = 44;
const OUTER_W = CARD_W + 2, OUTER_H = CARD_H + 2; // plus the 1px border on each side
const GLASS = 'linear-gradient(140deg,rgba(255,255,255,.2),rgba(255,255,255,.06))';
// The outer box only carries the shadow; the inner box is the glass and does the rounded clip. WebKit does not
// apply border-radius + overflow to a filtered child, so the inner box clips with clip-path in its own layer.
const OUTER_CSS = `position:absolute;top:${MARGIN_TOP}px;width:${OUTER_W}px;height:${OUTER_H}px;border-radius:${RADIUS}px;` +
  'box-shadow:0 20px 40px rgba(8,8,40,.38)';
const INNER_CSS = `position:absolute;inset:0;box-sizing:border-box;border-radius:${RADIUS}px;overflow:hidden;color:#fff;` +
  `clip-path:inset(0 round ${RADIUS}px);isolation:isolate;transform:translateZ(0);` +
  `background:${GLASS};font:13px system-ui,sans-serif`;
// The border and the top highlight are drawn above everything inside (the blurred layer is composited and would
// otherwise cover them at the corners).
const RIM_CSS = `position:absolute;inset:0;box-sizing:border-box;border-radius:${RADIUS}px;pointer-events:none;z-index:2;` +
  'border:1px solid rgba(255,255,255,.3);box-shadow:inset 0 1px 0 rgba(255,255,255,.45)';

/** Two glass cards. Left: CSS backdrop-filter (a transparent window cannot blur the desktop behind it).
 *  Right: the known wallpaper drawn blurred inside the card, offset so it lines up with the real one. */
export const glassTestScene: SceneFactory = (root) => {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  let mon = { w: screen.width, h: screen.height }; // logical size of the monitor the window is on
  let win = { x: window.screenX, y: window.screenY }; // logical window position on that monitor

  const mk = (left: number, title: string, sub: string): HTMLElement => {
    const card = document.createElement('div');
    card.style.cssText = `${OUTER_CSS};left:${left}px`;
    const inner = document.createElement('div');
    inner.style.cssText = INNER_CSS;
    const rim = document.createElement('div');
    rim.style.cssText = RIM_CSS;
    card.append(inner, rim);
    const pad = document.createElement('div');
    pad.style.cssText = 'position:absolute;inset:0;padding:20px 22px;z-index:1';
    pad.innerHTML = '<div style="opacity:.78;font-size:12.5px"></div><div style="font-size:28px;font-weight:600;margin:14px 0 6px">0:30</div><div style="opacity:.78"></div>';
    (pad.children[0] as HTMLElement).textContent = title;
    (pad.children[2] as HTMLElement).textContent = sub;
    inner.append(pad);
    root.append(card);
    return card;
  };

  const left = mk(MARGIN_X, 'CSS backdrop-filter', '뒤가 흐려지지 않아요(예상)');
  const leftGlass = left.firstElementChild as HTMLElement;
  leftGlass.style.setProperty('-webkit-backdrop-filter', 'blur(26px) saturate(170%)');
  leftGlass.style.setProperty('backdrop-filter', 'blur(26px) saturate(170%)');

  const right = mk(MARGIN_X + OUTER_W + GAP, '배경을 직접 그림', '같은 그림을 흐려서 깔았어요');
  const layer = document.createElement('div');
  layer.style.cssText = `position:absolute;inset:-${BLEED}px;filter:blur(26px) saturate(170%);background-repeat:no-repeat`;
  (right.firstElementChild as HTMLElement).prepend(layer);

  const c = document.createElement('canvas');
  c.width = Math.round(mon.w * dpr);
  c.height = Math.round(mon.h * dpr);
  const x = c.getContext('2d')!;
  x.scale(dpr, dpr);
  const draw = (): void => {
    c.width = Math.round(mon.w * dpr);
    c.height = Math.round(mon.h * dpr);
    x.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawTestWallpaper(x, mon.w, mon.h);
    layer.style.backgroundImage = `url(${c.toDataURL('image/png')})`;
    layer.style.backgroundSize = `${mon.w}px ${mon.h}px`;
  };
  const place = (): void => {
    const r = right.getBoundingClientRect();
    // The inner box has no border, so the layer starts BLEED px outside the card's border box.
    const cardX = win.x + r.left, cardY = win.y + r.top;
    layer.style.backgroundPosition = `${-(cardX - BLEED)}px ${-(cardY - BLEED)}px`;
  };
  draw();
  place();

  let off: (() => void) | undefined;
  if ('__TAURI_INTERNALS__' in window) {
    const w = getCurrentWindow();
    // Drag the window by either card (the check moves it to see the right card follow the wallpaper).
    for (const card of [left, right]) {
      card.style.cursor = 'grab';
      card.addEventListener('mousedown', (e) => { if (e.button === 0) void w.startDragging().catch(() => {}); });
    }
    let settle: ReturnType<typeof setTimeout> | undefined;
    const refresh = async (): Promise<void> => {
      const [pos, m, s] = await Promise.all([w.outerPosition(), currentMonitor(), w.scaleFactor()]);
      if (m) {
        const sizeChanged = Math.round(m.size.width / m.scaleFactor) !== mon.w || Math.round(m.size.height / m.scaleFactor) !== mon.h;
        mon = { w: Math.round(m.size.width / m.scaleFactor), h: Math.round(m.size.height / m.scaleFactor) };
        win = { x: (pos.x - m.position.x) / s, y: (pos.y - m.position.y) / s };
        if (sizeChanged) draw();
      }
      place();
    };
    void refresh();
    // Moved is parent-relative for a window attached under the desktop (Windows); re-read once it settles.
    void w.onMoved(() => { clearTimeout(settle); settle = setTimeout(() => { void refresh(); }, 60); void refresh(); })
      .then((un) => { off = un; });
  } else {
    window.addEventListener('resize', place);
  }
  return { wantsFrames: () => false, frame() {}, destroy: () => { off?.(); left.remove(); right.remove(); } };
};
