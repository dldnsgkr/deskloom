import { drawTestWallpaper } from '../test-wallpaper';
import type { SceneFactory } from './types';

/** A still wallpaper: drawn once to a screen-sized canvas, never redrawn. */
export const stillScene: SceneFactory = (root) => {
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = window.innerWidth, h = window.innerHeight;
  const c = document.createElement('canvas');
  c.width = Math.round(w * dpr);
  c.height = Math.round(h * dpr);
  c.style.cssText = `position:absolute;left:0;top:0;width:${w}px;height:${h}px`;
  const x = c.getContext('2d')!;
  x.scale(dpr, dpr);
  drawTestWallpaper(x, w, h);
  root.append(c);
  return { wantsFrames: () => false, frame() {}, destroy: () => c.remove() };
};
