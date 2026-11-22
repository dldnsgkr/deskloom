import { DPR, rand, setup, offscreen, rr } from './canvas-util';
import type { SceneFactory } from './types';

const W = 160, H = 224, cx = 80;
const FALL_MS = 30000;
const FLIP_MS = 1000;

interface Part { x: number; y: number; vx: number; vy: number; c: string; life: number; hit?: boolean }

export const hourglassScene: SceneFactory = (root, sceneCtx) => {
  const c = document.createElement('canvas');
  c.style.cssText = 'position:absolute;left:0;top:0;cursor:pointer';
  root.append(c);
  const ctx = setup(c, W, H);

  let f = 0; // fraction of sand left in the top bulb; starts fully fallen
  let flipping = false;
  let parts: Part[] = [];
  let peakY = 194;
  let timer = 0;
  let wasDown = false;

  const [nc, nx] = offscreen(48, 48);
  nx.setTransform(1, 0, 0, 1, 0, 0);
  const nd = nx.createImageData(nc.width, nc.height);
  for (let i = 0; i < nd.data.length; i += 4) {
    const dark = Math.random() < 0.55;
    nd.data[i] = dark ? 70 : 255; nd.data[i + 1] = dark ? 40 : 244; nd.data[i + 2] = dark ? 12 : 214;
    nd.data[i + 3] = Math.random() < 0.4 ? (rand(25, 80) | 0) : 0;
  }
  nx.putImageData(nd, 0, 0);
  const noise = ctx.createPattern(nc, 'repeat')!;
  noise.setTransform(new DOMMatrix().scale(1 / DPR));

  const glass = (): void => {
    ctx.beginPath(); ctx.moveTo(cx - 44, 30);
    ctx.bezierCurveTo(cx - 47, 84, cx - 7, 96, cx - 2.6, 112); ctx.bezierCurveTo(cx - 7, 128, cx - 47, 140, cx - 44, 194);
    ctx.lineTo(cx + 44, 194);
    ctx.bezierCurveTo(cx + 47, 140, cx + 7, 128, cx + 2.6, 112); ctx.bezierCurveTo(cx + 7, 96, cx + 47, 84, cx + 44, 30);
    ctx.closePath();
  };
  const sandFill = (y0: number, y1: number): CanvasGradient => {
    const g = ctx.createLinearGradient(0, y0, 0, y1); g.addColorStop(0, '#f4d9a3'); g.addColorStop(1, '#c18c4b'); return g;
  };
  const sides = (): CanvasGradient => {
    const g = ctx.createLinearGradient(cx - 46, 0, cx + 46, 0);
    g.addColorStop(0, 'rgba(70,35,5,.5)'); g.addColorStop(0.32, 'rgba(70,35,5,0)'); g.addColorStop(0.68, 'rgba(70,35,5,0)'); g.addColorStop(1, 'rgba(70,35,5,.55)');
    return g;
  };
  const hl = (x: number, y: number, rx: number, ry: number, rot: number, a: number): void => {
    ctx.save(); ctx.translate(x, y); ctx.rotate(rot);
    const g = ctx.createLinearGradient(-rx, 0, rx, 0);
    g.addColorStop(0, 'rgba(255,255,255,0)'); g.addColorStop(0.5, `rgba(255,255,255,${a})`); g.addColorStop(1, 'rgba(255,255,255,0)');
    ctx.fillStyle = g; ctx.beginPath(); ctx.ellipse(0, 0, rx, ry, 0, 0, Math.PI * 2); ctx.fill(); ctx.restore();
  };
  const wood = (x: number, y: number, w: number, h: number): void => {
    ctx.save(); rr(ctx, x, y, w, h, 5);
    const g = ctx.createLinearGradient(0, y, 0, y + h);
    g.addColorStop(0, '#a87446'); g.addColorStop(0.2, '#7d4e2b'); g.addColorStop(0.7, '#5a3418'); g.addColorStop(1, '#36200e');
    ctx.fillStyle = g; ctx.fill(); ctx.clip();
    ctx.globalAlpha = 0.22; ctx.strokeStyle = '#26140a'; ctx.lineWidth = 0.6;
    for (let i = 0; i < 6; i++) {
      const yy = y + 2.5 + i * (h / 6);
      ctx.beginPath(); ctx.moveTo(x, yy);
      for (let xx = x; xx <= x + w; xx += 6) ctx.lineTo(xx, yy + Math.sin((xx + i * 17) * 0.09) * 1.1);
      ctx.stroke();
    }
    ctx.globalAlpha = 1; ctx.fillStyle = 'rgba(255,222,184,.35)'; ctx.fillRect(x + 3, y, w - 6, 1); ctx.restore();
  };
  const rod = (x: number): void => {
    const g = ctx.createLinearGradient(x - 3.5, 0, x + 3.5, 0);
    g.addColorStop(0, '#3a210e'); g.addColorStop(0.42, '#b07c4d'); g.addColorStop(1, '#331d0c');
    ctx.fillStyle = g; ctx.fillRect(x - 3.2, 30, 6.4, 164);
    for (const by of [42, 182]) { ctx.beginPath(); ctx.ellipse(x, by, 4.8, 3.4, 0, 0, Math.PI * 2); ctx.fill(); }
  };

  function draw(): void {
    ctx.clearRect(0, 0, W, H);
    glass(); ctx.fillStyle = 'rgba(255,255,255,.035)'; ctx.fill();
    ctx.save(); glass(); ctx.clip();
    if (f > 0.001) {
      const yT = 112 - (112 - 42) * Math.pow(f, 0.55), dip = Math.min(8, 2 + (1 - f) * 10);
      ctx.beginPath(); ctx.moveTo(0, yT); ctx.lineTo(cx - 18, yT);
      ctx.quadraticCurveTo(cx - 4, yT + dip * 0.2, cx, yT + dip); ctx.quadraticCurveTo(cx + 4, yT + dip * 0.2, cx + 18, yT);
      ctx.lineTo(W, yT); ctx.lineTo(W, 113); ctx.lineTo(0, 113); ctx.closePath();
      ctx.fillStyle = sandFill(yT, 113); ctx.fill(); ctx.fillStyle = noise; ctx.fill(); ctx.fillStyle = sides(); ctx.fill();
      ctx.strokeStyle = 'rgba(255,242,214,.55)'; ctx.lineWidth = 0.8; ctx.beginPath(); ctx.moveTo(cx - 44, yT + 0.4); ctx.lineTo(cx - 18, yT + 0.4); ctx.stroke();
    }
    const b = 1 - f;
    if (b > 0.001) {
      const yb = 194 - 62 * Math.pow(b, 0.8), p = Math.min(14, 3 + b * 26);
      ctx.beginPath(); ctx.moveTo(0, yb + p * 0.35);
      ctx.quadraticCurveTo(cx - 26, yb + p * 0.3, cx, yb - p); ctx.quadraticCurveTo(cx + 26, yb + p * 0.3, W, yb + p * 0.35);
      ctx.lineTo(W, 200); ctx.lineTo(0, 200); ctx.closePath();
      ctx.fillStyle = sandFill(yb - p, 194); ctx.fill(); ctx.fillStyle = noise; ctx.fill(); ctx.fillStyle = sides(); ctx.fill();
      peakY = yb - p;
    } else peakY = 194;
    if (f > 0.001 && !flipping) {
      ctx.strokeStyle = 'rgba(212,166,100,.95)'; ctx.lineWidth = 1.4; ctx.beginPath(); ctx.moveTo(cx, 111); ctx.lineTo(cx, peakY); ctx.stroke();
    }
    for (const q of parts) { ctx.fillStyle = q.c; ctx.fillRect(q.x - 0.6, q.y - 0.6, 1.2, 1.2); }
    let g = ctx.createLinearGradient(0, 30, 0, 40); g.addColorStop(0, 'rgba(0,0,0,.4)'); g.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = g; ctx.fillRect(0, 30, W, 10);
    g = ctx.createLinearGradient(0, 194, 0, 186); g.addColorStop(0, 'rgba(0,0,0,.35)'); g.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = g; ctx.fillRect(0, 186, W, 8);
    ctx.lineWidth = 6; ctx.strokeStyle = 'rgba(190,215,235,.07)'; glass(); ctx.stroke();
    hl(cx - 29, 66, 4.5, 26, -0.2, 0.5); hl(cx - 30, 160, 4.5, 24, 0.2, 0.45); hl(cx + 33, 70, 1.6, 18, 0.22, 0.28); hl(cx + 33, 156, 1.6, 16, -0.22, 0.25);
    ctx.restore();
    glass(); ctx.lineWidth = 1.2; ctx.strokeStyle = 'rgba(255,255,255,.34)'; ctx.stroke();
    rod(30); rod(130);
    wood(16, 12, 128, 18); wood(16, 194, 128, 18);
  }

  const flip = (): void => {
    if (flipping) return;
    flipping = true; parts = [];
    c.style.transition = `transform ${FLIP_MS}ms cubic-bezier(.65,0,.35,1)`;
    c.style.transform = 'rotate(180deg)';
    timer = window.setTimeout(() => {
      c.style.transition = 'none'; c.style.transform = 'none'; void c.offsetWidth;
      f = 1 - f; flipping = false;
      sceneCtx.requestFrames();
    }, FLIP_MS);
  };
  c.addEventListener('click', flip);

  return {
    wantsFrames: () => (!flipping && f > 0) || parts.length > 0,
    frame(dtMs) {
      const dt = dtMs / 1000;
      if (!flipping && f > 0) {
        f = Math.max(0, f - dtMs / FALL_MS);
        if (f > 0) for (let i = 0; i < 3; i++) parts.push({ x: cx + rand(-0.9, 0.9), y: 113 + rand(0, 4), vx: 0, vy: rand(20, 60), c: Math.random() < 0.5 ? '#f7e0b3' : '#c5915a', life: 9 });
      }
      for (const q of parts) {
        q.vy += 700 * dt; q.y += q.vy * dt; q.x += q.vx * dt; q.life -= dt;
        if (!q.hit && q.y >= peakY) {
          q.hit = true;
          if (Math.random() < 0.22) { q.vx = rand(-45, 45); q.vy = -rand(25, 55); q.life = 0.16; } else q.life = 0;
        }
      }
      parts = parts.filter((q) => q.life > 0 && q.y < 200);
      draw();
    },
    pointer(p, local) {
      // Click-through windows only see the desktop pointer; edge-detect a press inside the canvas.
      if (p.down && !wasDown && local.x >= 0 && local.x < W && local.y >= 0 && local.y < H) flip();
      wasDown = p.down;
    },
    destroy() { window.clearTimeout(timer); c.remove(); },
  };
};
