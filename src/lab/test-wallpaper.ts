/** The known wallpaper used by the glass experiment: three colour blobs on a dark base, placed by
 *  screen proportion. No randomness, so every draw (and every monitor size) looks the same. */
export function drawTestWallpaper(ctx: CanvasRenderingContext2D, w: number, h: number): void {
  ctx.fillStyle = '#1c1f45';
  ctx.fillRect(0, 0, w, h);
  const blobs: [number, number, number, string][] = [
    [0.18, 0.28, 0.26, '#ff7eb3'],
    [0.78, 0.40, 0.30, '#5b8cff'],
    [0.50, 0.85, 0.24, '#ffb36b'],
  ];
  const m = Math.min(w, h);
  for (const [fx, fy, fr, color] of blobs) {
    const cx = fx * w, cy = fy * h, r = fr * m * 1.6;
    const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
    g.addColorStop(0, color);
    g.addColorStop(1, `${color}00`);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, w, h);
  }
}
