export const DPR = Math.min(window.devicePixelRatio || 1, 2);
export const rand = (a: number, b: number): number => a + Math.random() * (b - a);

export function setup(c: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D {
  c.width = Math.round(w * DPR);
  c.height = Math.round(h * DPR);
  c.style.width = `${w}px`;
  c.style.height = `${h}px`;
  const x = c.getContext('2d')!;
  x.setTransform(DPR, 0, 0, DPR, 0, 0);
  return x;
}

export function offscreen(w: number, h: number): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const c = document.createElement('canvas');
  c.width = Math.round(w * DPR);
  c.height = Math.round(h * DPR);
  const x = c.getContext('2d')!;
  x.setTransform(DPR, 0, 0, DPR, 0, 0);
  return [c, x];
}

export function rr(p: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number): void {
  p.beginPath();
  p.moveTo(x + r, y); p.lineTo(x + w - r, y); p.quadraticCurveTo(x + w, y, x + w, y + r);
  p.lineTo(x + w, y + h - r); p.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  p.lineTo(x + r, y + h); p.quadraticCurveTo(x, y + h, x, y + h - r);
  p.lineTo(x, y + r); p.quadraticCurveTo(x, y, x + r, y);
  p.closePath();
}
