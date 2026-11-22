import { describe, it, expect } from 'vitest';
import { createFrameLoop } from './frame-loop';

function fakeRaf() {
  let nextId = 1;
  const pending = new Map<number, (t: number) => void>();
  return {
    raf: (cb: (t: number) => void) => { const id = nextId++; pending.set(id, cb); return id; },
    caf: (id: number) => { pending.delete(id); },
    tick(t: number) { const cbs = [...pending.values()]; pending.clear(); cbs.forEach(cb => cb(t)); },
    get pending() { return pending.size; },
  };
}
function ticks(r: ReturnType<typeof fakeRaf>, hz: number, ms: number, from = 0) {
  for (let i = 0; i <= (hz * ms) / 1000; i++) r.tick(from + (i * 1000) / hz);
}
function setup(maxFps: number) {
  const r = fakeRaf(); const dts: number[] = [];
  const loop = createFrameLoop({ maxFps, onFrame: dt => dts.push(dt), raf: r.raf, caf: r.caf });
  return { r, dts, loop };
}

describe('createFrameLoop', () => {
  it('caps at 30fps on a 60Hz display', () => {
    const { r, dts, loop } = setup(30); loop.start(); ticks(r, 60, 1000);
    expect(dts.length).toBeGreaterThanOrEqual(30); expect(dts.length).toBeLessThanOrEqual(31);
  });
  it('never exceeds 60fps on a 120Hz display', () => {
    const { r, dts, loop } = setup(60); loop.start(); ticks(r, 120, 1000);
    expect(dts.length).toBeGreaterThanOrEqual(60); expect(dts.length).toBeLessThanOrEqual(61);
  });
  it('passes dt 0 on the first frame after start', () => {
    const { r, dts, loop } = setup(60); loop.start(); r.tick(0); r.tick(1000 / 60);
    expect(dts[0]).toBe(0); expect(dts[1]).toBeCloseTo(16.67, 1);
  });
  it('stop cancels the pending frame and requests no more', () => {
    const { r, loop } = setup(60); loop.start(); r.tick(0); loop.stop();
    expect(r.pending).toBe(0); expect(loop.running).toBe(false);
  });
  it('dt after a restart excludes the stopped time', () => {
    const { r, dts, loop } = setup(60); loop.start(); r.tick(0); r.tick(16.7); loop.stop();
    loop.start(); r.tick(5000); r.tick(5016.7);
    expect(dts.slice(2)[0]).toBe(0); expect(dts.slice(2)[1]).toBeCloseTo(16.7, 1);
  });
  it('clamps dt to 100ms after a hitch', () => {
    const { r, dts, loop } = setup(60); loop.start(); r.tick(0); r.tick(16.7); r.tick(616.7);
    expect(dts[2]).toBe(100);
  });
  it('setMaxFps applies from the next frame', () => {
    const { r, dts, loop } = setup(60); loop.start(); ticks(r, 60, 500);
    const before = dts.length; loop.setMaxFps(30); ticks(r, 60, 500, 500 + 1000 / 60);
    expect(dts.length - before).toBeGreaterThanOrEqual(14); expect(dts.length - before).toBeLessThanOrEqual(16);
  });
});
