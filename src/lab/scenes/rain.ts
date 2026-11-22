import { DPR, rand, setup, offscreen } from './canvas-util';

export interface Rain { frame(dtMs: number): void; wipe(x: number, y: number, radius: number): void }

interface Drop { x: number; y: number; r: number; vy: number; slide: boolean; trailT: number; wob: number; trail?: boolean; dead?: boolean }
interface Region { x: number; y: number; r: number; age: number; microAcc: number; dropAcc: number }

const REFILL_MS = 3000;
const COLS = ['rgba(255,176,90,A)', 'rgba(255,176,90,A)', 'rgba(255,214,140,A)', 'rgba(255,110,80,A)',
  'rgba(90,205,255,A)', 'rgba(150,130,255,A)', 'rgba(255,190,110,A)'];

// Port of the rainy-window drawing from the quality mockup, sized by the caller.
export function createRain(canvas: HTMLCanvasElement, opts: { width: number; height: number }): Rain {
  const W = opts.width;
  const H = opts.height;
  const ctx = setup(canvas, W, H);
  const area = W * H;
  const maxDrops = Math.min(160, Math.max(48, Math.round(area / 700)));
  const microTarget = Math.min(9000, Math.round(area * 0.027));
  const dropDensity = maxDrops / area;
  const microDensity = microTarget / area;

  const lights: { x: number; y: number; r: number; a: number; c: string }[] = [];
  const lightCount = Math.min(600, Math.round(85 * area / (216 * 162)));
  for (let i = 0; i < lightCount; i++) {
    const y = H * (0.36 + Math.pow(Math.random(), 0.7) * 0.64);
    lights.push({ x: rand(-6, W + 6), y, r: rand(1.4, 4.2) * (0.6 + y / H * 0.6), a: rand(0.45, 0.95), c: COLS[(Math.random() * COLS.length) | 0] });
  }
  const paintCity = (x: CanvasRenderingContext2D, soft: boolean): void => {
    const g = x.createLinearGradient(0, 0, 0, H);
    g.addColorStop(0, '#0a1230'); g.addColorStop(0.55, '#1a1f44'); g.addColorStop(1, '#3a2442');
    x.fillStyle = g; x.fillRect(0, 0, W, H);
    const hg = x.createRadialGradient(W * 0.5, H * 1.05, 10, W * 0.5, H * 1.05, H * 0.95);
    hg.addColorStop(0, 'rgba(255,140,80,.38)'); hg.addColorStop(1, 'rgba(255,140,80,0)');
    x.fillStyle = hg; x.fillRect(0, 0, W, H);
    for (const L of lights) {
      const r = soft ? L.r * 3.3 : L.r * 1.25;
      const rg = x.createRadialGradient(L.x, L.y, 0, L.x, L.y, r);
      rg.addColorStop(0, L.c.replace('A', String(soft ? L.a * 0.5 : L.a)));
      rg.addColorStop(soft ? 0.6 : 0.55, L.c.replace('A', String(soft ? L.a * 0.32 : L.a * 0.75)));
      rg.addColorStop(1, L.c.replace('A', '0'));
      x.fillStyle = rg; x.beginPath(); x.arc(L.x, L.y, r, 0, Math.PI * 2); x.fill();
    }
    if (soft) { x.fillStyle = 'rgba(150,165,200,.07)'; x.fillRect(0, 0, W, H); }
  };
  const [sharp, sx] = offscreen(W, H);
  const [blur, bx] = offscreen(W, H);
  const [micro, mx] = offscreen(W, H);
  paintCity(sx, false);
  paintCity(bx, true);

  const addMicro = (n: number, cx = 0, cy = 0, cr = 0): void => {
    for (let i = 0; i < n; i++) {
      let x: number, y: number;
      if (cr > 0) { const a = rand(0, Math.PI * 2), d = cr * Math.sqrt(Math.random()); x = cx + Math.cos(a) * d; y = cy + Math.sin(a) * d; }
      else { x = rand(0, W); y = rand(0, H); }
      const r = rand(0.4, 1.4);
      mx.fillStyle = 'rgba(0,0,12,.3)'; mx.beginPath(); mx.arc(x, y + 0.4, r, 0, Math.PI * 2); mx.fill();
      mx.fillStyle = 'rgba(220,230,255,.38)'; mx.beginPath(); mx.arc(x - 0.2, y - 0.2, r * 0.6, 0, Math.PI * 2); mx.fill();
    }
  };
  addMicro(microTarget);

  let drops: Drop[] = [];
  const spawn = (x = rand(0, W), y = rand(-4, H * 0.9)): void => {
    drops.push({ x, y, r: Math.random() < 0.16 ? rand(4.2, 6.2) : rand(1.6, 3.8), vy: 0, slide: false, trailT: 0, wob: rand(0, 6) });
  };
  for (let i = 0; i < maxDrops * 0.75; i++) spawn();

  const regions: Region[] = [];
  let t = 0;
  let flash = 0;
  let nextFlash = rand(5000, 9000);
  let flick = 0;

  function step(dt: number): void {
    if (drops.length < maxDrops && Math.random() < dt * 14) spawn();
    for (const d of drops) {
      if (d.dead) continue;
      if (!d.slide) {
        if (d.r > 4.1 && Math.random() < dt * 0.9) d.slide = true;
        else if (Math.random() < dt * 0.05) d.r += 0.15;
        continue;
      }
      d.vy = Math.min(d.vy + dt * rand(40, 120), 70 + d.r * 10);
      d.y += d.vy * dt;
      d.x += Math.sin(d.y * 0.08 + d.wob) * 0.12;
      d.trailT -= dt;
      if (d.trailT <= 0) {
        d.trailT = rand(0.05, 0.15);
        if (Math.random() < 0.6) drops.push({ x: d.x + rand(-0.5, 0.5), y: d.y - d.r * 1.2, r: rand(0.8, 1.8), vy: 0, slide: false, trailT: 0, wob: 0, trail: true });
        d.r = Math.max(3.2, d.r - 0.04);
      }
      mx.globalCompositeOperation = 'destination-out';
      mx.beginPath(); mx.arc(d.x, d.y, d.r * 0.95, 0, Math.PI * 2); mx.fill();
      mx.globalCompositeOperation = 'source-over';
      for (const o of drops) {
        if (o === d || o.slide || o.dead) continue;
        if (o.trail && o.y < d.y) continue;
        if (Math.hypot(o.x - d.x, o.y - d.y) < d.r + o.r * 0.6) { o.dead = true; d.r = Math.min(7, Math.sqrt(d.r * d.r + o.r * o.r * 0.5)); }
      }
    }
    drops = drops.filter((d) => !d.dead && d.y < H + 12);
    if (Math.random() < dt * 25) addMicro(1);
    // Condensation coming back into wiped spots, spread over REFILL_MS.
    for (let i = regions.length - 1; i >= 0; i--) {
      const g = regions[i];
      const share = Math.min(dt * 1000, REFILL_MS - g.age) / REFILL_MS;
      g.age += dt * 1000;
      const area2 = Math.PI * g.r * g.r;
      g.microAcc += microDensity * area2 * share;
      g.dropAcc += dropDensity * area2 * share;
      const m = Math.floor(g.microAcc); g.microAcc -= m; if (m > 0) addMicro(m, g.x, g.y, g.r);
      const n = Math.floor(g.dropAcc); g.dropAcc -= n;
      for (let k = 0; k < n; k++) { const a = rand(0, Math.PI * 2), d = g.r * Math.sqrt(Math.random()); spawn(g.x + Math.cos(a) * d, g.y + Math.sin(a) * d); }
      if (g.age >= REFILL_MS) regions.splice(i, 1);
    }
  }

  const tiny = (d: Drop): void => {
    ctx.fillStyle = 'rgba(0,0,10,.35)'; ctx.beginPath(); ctx.arc(d.x, d.y + 0.3, d.r, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = 'rgba(230,235,255,.55)'; ctx.beginPath(); ctx.arc(d.x - 0.2, d.y - 0.25, d.r * 0.55, 0, Math.PI * 2); ctx.fill();
  };
  const drop = (d: Drop): void => {
    const { x, y, r } = d, ry = r * 1.12;
    ctx.fillStyle = 'rgba(0,0,8,.32)'; ctx.beginPath(); ctx.ellipse(x, y + r * 0.25, r * 1.02, ry, 0, 0, Math.PI * 2); ctx.fill();
    ctx.save();
    ctx.beginPath(); ctx.ellipse(x, y, r, ry, 0, 0, Math.PI * 2); ctx.clip();
    const s = Math.min(r * 5, W / 2, H / 2);
    const s0x = Math.max(0, Math.min(W - 2 * s, x - s)), s0y = Math.max(0, Math.min(H - 2 * s, y - s));
    ctx.translate(x, y); ctx.scale(1, -1);
    ctx.drawImage(sharp, s0x * DPR, s0y * DPR, 2 * s * DPR, 2 * s * DPR, -r, -ry, 2 * r, 2 * ry);
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
    const g = ctx.createRadialGradient(x - r * 0.25, y - r * 0.3, r * 0.2, x, y, r * 1.05);
    g.addColorStop(0, 'rgba(255,255,255,0)'); g.addColorStop(0.75, 'rgba(0,0,20,.08)'); g.addColorStop(1, 'rgba(0,0,20,.45)');
    ctx.fillStyle = g; ctx.fillRect(x - r, y - ry, 2 * r, 2 * ry);
    ctx.fillStyle = 'rgba(255,255,255,.18)'; ctx.beginPath(); ctx.ellipse(x + r * 0.1, y + ry * 0.55, r * 0.5, ry * 0.22, 0, 0, Math.PI * 2); ctx.fill();
    ctx.restore();
    ctx.fillStyle = 'rgba(255,255,255,.85)'; ctx.beginPath(); ctx.ellipse(x - r * 0.35, y - ry * 0.42, r * 0.2, r * 0.14, -0.5, 0, Math.PI * 2); ctx.fill();
  };

  return {
    frame(dtMs) {
      const dt = dtMs / 1000;
      t += dtMs;
      step(dt);
      if (t > nextFlash) { flash = 1; flick = t + 140; nextFlash = t + rand(9000, 16000); }
      if (flick && t > flick) { flash = Math.max(flash, 0.75); flick = 0; }
      flash = Math.max(0, flash - dt * 2.8);
      ctx.clearRect(0, 0, W, H);
      ctx.drawImage(blur, 0, 0, W, H);
      if (flash > 0) { ctx.fillStyle = `rgba(190,200,255,${flash * 0.42})`; ctx.fillRect(0, 0, W, H); }
      ctx.drawImage(micro, 0, 0, W, H);
      for (const d of drops) { if (d.r < 1.3) tiny(d); else drop(d); }
      const sh = ctx.createLinearGradient(0, 0, W * 0.6, H);
      sh.addColorStop(0, 'rgba(255,255,255,.07)'); sh.addColorStop(0.5, 'rgba(255,255,255,0)');
      ctx.fillStyle = sh; ctx.fillRect(0, 0, W, H);
    },
    wipe(x, y, radius) {
      mx.globalCompositeOperation = 'destination-out';
      mx.beginPath(); mx.arc(x, y, radius, 0, Math.PI * 2); mx.fill();
      mx.globalCompositeOperation = 'source-over';
      drops = drops.filter((d) => Math.hypot(d.x - x, d.y - y) > radius + d.r);
      const last = regions[regions.length - 1];
      if (last && Math.hypot(last.x - x, last.y - y) < radius / 4) { last.x = x; last.y = y; last.age = 0; return; }
      regions.push({ x, y, r: radius, age: 0, microAcc: 0, dropAcc: 0 });
      if (regions.length > 40) regions.shift();
    },
  };
}
