import { createRain } from './rain';
import type { SceneFactory } from './types';

const WIPE_RADIUS = 60;

export const rainWindowScene: SceneFactory = (root) => {
  const canvas = document.createElement('canvas');
  canvas.style.cssText = 'position:absolute;left:0;top:0';
  root.append(canvas);
  const rain = createRain(canvas, { width: root.clientWidth || window.innerWidth, height: root.clientHeight || window.innerHeight });
  return {
    wantsFrames: () => true,
    frame: (dt) => rain.frame(dt),
    pointer(p, local) { if (p.overDesktop) rain.wipe(local.x, local.y, WIPE_RADIUS); },
    destroy: () => canvas.remove(),
  };
};
