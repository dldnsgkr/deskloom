import { createRain } from './rain';
import { setup, rr } from './canvas-util';
import type { SceneFactory } from './types';

const W = 240, H = 280;
const gx = 20, gy = 16, gw = 200, gh = 212;

// Static wooden window frame, ported from the mockup, with a hole where the rain shows through.
function drawFrame(c: HTMLCanvasElement): void {
  const ctx = setup(c, W, H);
  const fw = 224, fh = 236; // outer frame
  let g = ctx.createLinearGradient(0, fh + 4, 0, fh + 15);
  g.addColorStop(0, 'rgba(0,0,0,.45)'); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(6, fh + 4, 228, 11);
  ctx.beginPath(); ctx.rect(8, 4, fw, fh); ctx.rect(gx, gy, gw, gh);
  g = ctx.createLinearGradient(0, 4, 0, fh + 4);
  g.addColorStop(0, '#f2ece2'); g.addColorStop(1, '#d2c8b7');
  ctx.fillStyle = g; ctx.fill('evenodd');
  ctx.strokeStyle = 'rgba(0,0,0,.22)'; ctx.lineWidth = 1; ctx.strokeRect(8.5, 4.5, fw - 1, fh - 1);
  ctx.fillStyle = 'rgba(0,0,0,.28)'; ctx.fillRect(gx - 1.5, gy - 1.5, gw + 3, 1.5); ctx.fillRect(gx - 1.5, gy - 1.5, 1.5, gh + 3);
  ctx.fillStyle = 'rgba(255,255,255,.65)'; ctx.fillRect(gx - 1.5, gy + gh, gw + 3, 1.5); ctx.fillRect(gx + gw, gy - 1.5, 1.5, gh + 3);
  const m0 = gx + gw / 2 - 3.5, n0 = gy + gh / 2 - 3.5;
  // glass-side shadows (top, left, beside the mullions)
  g = ctx.createLinearGradient(0, gy, 0, gy + 12); g.addColorStop(0, 'rgba(0,0,0,.45)'); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(gx, gy, gw, 12);
  g = ctx.createLinearGradient(gx, 0, gx + 10, 0); g.addColorStop(0, 'rgba(0,0,0,.35)'); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(gx, gy, 10, gh);
  g = ctx.createLinearGradient(m0 + 7, 0, m0 + 15, 0); g.addColorStop(0, 'rgba(0,0,0,.35)'); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(m0 + 7, gy, 8, gh);
  g = ctx.createLinearGradient(0, n0 + 7, 0, n0 + 16); g.addColorStop(0, 'rgba(0,0,0,.4)'); g.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = g; ctx.fillRect(gx, n0 + 7, gw, 9);
  g = ctx.createLinearGradient(m0, 0, m0 + 7, 0); g.addColorStop(0, '#d8cfbf'); g.addColorStop(0.4, '#f4eee5'); g.addColorStop(1, '#c2b7a4');
  ctx.fillStyle = g; ctx.fillRect(m0, gy, 7, gh);
  g = ctx.createLinearGradient(0, n0, 0, n0 + 7); g.addColorStop(0, '#f4eee5'); g.addColorStop(1, '#c2b7a4');
  ctx.fillStyle = g; ctx.fillRect(gx, n0, gw, 7);
  rr(ctx, 2, fh, 236, 14, 2.5);
  g = ctx.createLinearGradient(0, fh, 0, fh + 14); g.addColorStop(0, '#fcf8f1'); g.addColorStop(0.3, '#e6ddcf'); g.addColorStop(1, '#bdb19e');
  ctx.fillStyle = g; ctx.fill();
}

export const weatherScene: SceneFactory = (root) => {
  const box = document.createElement('div');
  box.style.cssText = `position:absolute;left:0;top:0;width:${W}px;height:${H}px`;
  const rainCanvas = document.createElement('canvas');
  rainCanvas.style.cssText = `position:absolute;left:${gx}px;top:${gy}px`;
  const frameCanvas = document.createElement('canvas');
  frameCanvas.style.cssText = 'position:absolute;left:0;top:0;pointer-events:none';
  box.append(rainCanvas, frameCanvas);
  root.append(box);
  const rain = createRain(rainCanvas, { width: gw, height: gh });
  drawFrame(frameCanvas);
  return {
    wantsFrames: () => true,
    frame: (dt) => rain.frame(dt),
    destroy: () => box.remove(),
  };
};
