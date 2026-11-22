export interface FrameLoopOptions {
  maxFps: number; // 60 on AC power, 30 on battery
  onFrame: (dtMs: number, nowMs: number) => void;
  raf?: (cb: (now: number) => void) => number;
  caf?: (id: number) => void;
}
export interface FrameLoop {
  start(): void;
  stop(): void;
  setMaxFps(fps: number): void;
  readonly running: boolean;
}

const MAX_DT_MS = 100;
const JITTER_MS = 2;

export function createFrameLoop(opts: FrameLoopOptions): FrameLoop {
  const raf = opts.raf ?? ((cb) => requestAnimationFrame(cb));
  const caf = opts.caf ?? ((id) => cancelAnimationFrame(id));
  let maxFps = opts.maxFps;
  let running = false;
  let handle = 0;
  let last: number | null = null;

  const tick = (now: number): void => {
    if (!running) return;
    if (last === null) {
      last = now;
      opts.onFrame(0, now);
    } else if (now - last >= 1000 / maxFps - JITTER_MS) {
      const dt = Math.min(now - last, MAX_DT_MS);
      last = now;
      opts.onFrame(dt, now);
    }
    if (running) handle = raf(tick);
  };

  return {
    start() {
      if (running) return;
      running = true;
      last = null;
      handle = raf(tick);
    },
    stop() {
      if (!running) return;
      running = false;
      caf(handle);
    },
    setMaxFps(fps: number) { maxFps = fps; },
    get running() { return running; },
  };
}
