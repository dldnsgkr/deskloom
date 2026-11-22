import { currentMonitor, getCurrentWindow } from '@tauri-apps/api/window';
import { drawTestWallpaper } from '../test-wallpaper';
import type { SceneFactory } from './types';

const CARD_W = 236, CARD_H = 224, BLEED = 40;
const GLASS = 'linear-gradient(140deg,rgba(255,255,255,.2),rgba(255,255,255,.06))';
const CARD_CSS = `position:absolute;top:18px;width:${CARD_W}px;height:${CARD_H}px;border-radius:32px;overflow:hidden;color:#fff;` +
  `background:${GLASS};border:1px solid rgba(255,255,255,.3);box-shadow:0 26px 60px rgba(8,8,40,.38), inset 0 1px 0 rgba(255,255,255,.45);` +
  'font:13px system-ui,sans-serif;box-sizing:content-box';

/** Two glass cards. Left: CSS backdrop-filter (a transparent window cannot blur the desktop behind it).
 *  Right: the known wallpaper drawn blurred inside the card, offset so it lines up with the real one. */
export const glassTestScene: SceneFactory = (root) => {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  let mon = { w: screen.width, h: screen.height }; // logical size of the monitor the window is on
  let win = { x: window.screenX, y: window.screenY }; // logical window position on that monitor

  const mk = (left: number, title: string, sub: string): HTMLElement => {
    const card = document.createElement('div');
    card.style.cssText = `${CARD_CSS};left:${left}px`;
    const pad = document.createElement('div');
    pad.style.cssText = 'position:absolute;inset:0;padding:20px 22px;z-index:1';
    pad.innerHTML = '<div style="opacity:.78;font-size:12.5px"></div><div style="font-size:28px;font-weight:600;margin:14px 0 6px">0:30</div><div style="opacity:.78"></div>';
    (pad.children[0] as HTMLElement).textContent = title;
    (pad.children[2] as HTMLElement).textContent = sub;
    card.append(pad);
    root.append(card);
    return card;
  };

  const left = mk(16, 'CSS backdrop-filter', '뒤가 흐려지지 않아요(예상)');
  left.style.setProperty('-webkit-backdrop-filter', 'blur(26px) saturate(170%)');
  left.style.setProperty('backdrop-filter', 'blur(26px) saturate(170%)');

  const right = mk(16 + CARD_W + 2 + 16, '배경을 직접 그림', '같은 그림을 흐려서 깔았어요');
  const layer = document.createElement('div');
  layer.style.cssText = `position:absolute;inset:-${BLEED}px;filter:blur(26px) saturate(170%);background-repeat:no-repeat`;
  right.prepend(layer);

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
    // The layer starts BLEED px outside the card's border box (inset is measured from the padding box, 1px in).
    const cardX = win.x + r.left, cardY = win.y + r.top;
    layer.style.backgroundPosition = `${-(cardX + 1 - BLEED)}px ${-(cardY + 1 - BLEED)}px`;
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
