import { getCurrentWindow, currentMonitor } from '@tauri-apps/api/window';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import type { SceneFactory } from './types';

// 16x16 pixel creature, two walking frames. Rows 0-11 are shared, rows 12-15 are the legs.
const BODY = [
  '................',
  '...kk......kk...',
  '..kook....kook..',
  '..kooooooooook..',
  '.kooooooooooook.',
  '.kookkooookkook.',
  '.kooowoooowoook.',
  '.koooopppoooook.',
  '.kooooooooooook.',
  '..kooooooooook..',
  '..koowwwwwwook..',
  '..koowwwwwwook..',
];
const LEGS = [
  ['..kooook.kooook.', '...kppk...kppk..', '...kkk....kkk...', '................'],
  ['...kooook.koook.', '....kppk...kppk.', '....kkk....kkk..', '................'],
];
const PALETTE: Record<string, string> = { k: '#2b2233', o: '#f0a050', w: '#fff4e0', p: '#f08090' };
const SCALE = 4;
const SIZE = 16 * SCALE;
const SPEED = 40; // logical px / s
const JUMP_MS = 500;
const JUMP_H = 40; // logical px

export const walkerScene: SceneFactory = (root) => {
  const c = document.createElement('canvas');
  c.width = SIZE; c.height = SIZE;
  c.style.cssText = `position:absolute;left:0;top:0;width:${SIZE}px;height:${SIZE}px;cursor:pointer`;
  root.append(c);
  const g = c.getContext('2d')!;
  g.imageSmoothingEnabled = false;

  const isTauri = '__TAURI_INTERNALS__' in window;
  let dir = 1;
  let walkT = 0;
  let jumpT = -1; // ms since jump start, <0 when grounded
  let drawn = '';

  const render = (): void => {
    const frameIdx = Math.floor(walkT / 250) % 2;
    const key = `${frameIdx}${dir}`;
    if (key === drawn) return;
    drawn = key;
    g.clearRect(0, 0, SIZE, SIZE);
    g.save();
    if (dir < 0) { g.translate(SIZE, 0); g.scale(-1, 1); }
    [...BODY, ...LEGS[frameIdx]].forEach((row, y) => {
      for (let x = 0; x < 16; x++) {
        const col = PALETTE[row[x]];
        if (col) { g.fillStyle = col; g.fillRect(x * SCALE, y * SCALE, SCALE, SCALE); }
      }
    });
    g.restore();
  };

  const jump = (): void => { if (jumpT < 0) jumpT = 0; };
  c.addEventListener('click', jump);

  // Movement state. Tauri: the window itself walks along the monitor floor (physical px).
  // Browser: the sprite walks inside the page (logical px).
  let x = 0;
  let minX = 0;
  let maxX = Math.max(0, window.innerWidth - SIZE);
  let baseY = 0;
  let scale = 1;
  let ready = !isTauri;
  const win = isTauri ? getCurrentWindow() : null;
  let moving = false; // one setPosition in flight at a time, so a slow IPC does not queue up
  if (win) {
    void (async () => {
      const [mon, size, pos] = await Promise.all([currentMonitor(), win.outerSize(), win.outerPosition()]);
      if (!mon) return;
      const area = mon.workArea ?? { position: mon.position, size: mon.size };
      scale = mon.scaleFactor;
      minX = area.position.x;
      maxX = area.position.x + area.size.width - size.width;
      baseY = area.position.y + area.size.height - size.height;
      x = Math.min(maxX, Math.max(minX, pos.x));
      ready = true;
    })();
  }

  return {
    wantsFrames: () => true,
    frame(dtMs) {
      walkT += dtMs;
      render();
      if (!ready) return;
      const step = SPEED * (isTauri ? scale : 1) * dtMs / 1000;
      x += dir * step;
      if (x >= maxX) { x = maxX; dir = -1; } else if (x <= minX) { x = minX; dir = 1; }
      let lift = 0; // logical px
      if (jumpT >= 0) {
        jumpT += dtMs;
        const s = Math.min(1, jumpT / JUMP_MS);
        lift = 4 * JUMP_H * s * (1 - s);
        if (jumpT >= JUMP_MS) jumpT = -1;
      }
      if (win) {
        if (!moving) {
          moving = true;
          win.setPosition(new PhysicalPosition(Math.round(x), Math.round(baseY - lift * scale)))
            .catch(() => {})
            .finally(() => { moving = false; });
        }
      }
      else c.style.transform = `translate(${x}px,${-lift}px)`;
    },
    destroy() { c.remove(); },
  };
};
