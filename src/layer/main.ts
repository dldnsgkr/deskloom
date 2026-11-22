import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { createFrameLoop } from '../core/frame-loop';
import { parseLayerHash } from '../lab/hash';
import '../lab/scenes/index';
import { sceneFactories, type Scene, type DesktopPointer } from '../lab/scenes/types';

const root = document.querySelector<HTMLElement>('#layer')!;
const isTauri = '__TAURI_INTERNALS__' in window;

function showError(msg: string): void {
  root.replaceChildren();
  const box = document.createElement('div');
  box.style.cssText = 'background:#16161a;color:#ff4d4f;font:600 14px sans-serif;padding:12px;width:100%;height:100%;box-sizing:border-box';
  box.textContent = msg;
  root.append(box);
}

function placeholder(name: string): Scene {
  const box = document.createElement('div');
  box.style.cssText = 'position:absolute;inset:8px;display:flex;flex-direction:column;align-items:center;justify-content:center;' +
    'background:rgba(20,20,30,.55);color:#fff;font:600 14px sans-serif;border-radius:12px;text-align:center';
  const title = document.createElement('div');
  title.textContent = name;
  const fpsEl = document.createElement('div');
  fpsEl.textContent = '-- fps';
  box.append(title, fpsEl);
  root.append(box);
  let count = 0;
  let since = 0;
  return {
    wantsFrames: () => true,
    frame(_dt, now) {
      if (since === 0) since = now;
      count++;
      if (now - since >= 500) {
        fpsEl.textContent = `${Math.round((count * 1000) / (now - since))} fps`;
        count = 0;
        since = now;
      }
    },
    destroy: () => box.remove(),
  };
}

let parsed;
try {
  parsed = parseLayerHash(location.hash);
} catch (e) {
  const name = e instanceof Error ? e.message.replace(/^unknown scene: /, '') : String(e);
  showError(`알 수 없는 장면: ${name}`);
}

if (parsed) {
  const { scene: sceneName } = parsed;
  let fps: number = parsed.fps;
  let visible = true;
  let scene: Scene;
  const loop = createFrameLoop({
    maxFps: fps === 0 ? 60 : fps,
    onFrame: (dt, now) => {
      scene.frame(dt, now);
      if (!scene.wantsFrames()) loop.stop();
    },
  });
  const tryStart = (): void => {
    if (fps > 0 && visible && !loop.running) loop.start();
  };
  const factory = sceneFactories[sceneName];
  const ctx = { requestFrames: tryStart };
  scene = factory ? factory(root, ctx) : placeholder(sceneName);
  tryStart();

  let winX = 0;
  let winY = 0;
  const toLocal = (p: DesktopPointer) => ({ x: (p.x - winX) / devicePixelRatio, y: (p.y - winY) / devicePixelRatio });

  if (isTauri) {
    const win = getCurrentWindow();
    const refresh = async (): Promise<void> => {
      const pos = await win.outerPosition();
      winX = pos.x;
      winY = pos.y;
    };
    void refresh();
    void win.onMoved(({ payload }) => { winX = payload.x; winY = payload.y; });
    void listen<{ fps: number }>('lab-fps', ({ payload }) => {
      fps = payload.fps;
      if (fps === 0) { loop.stop(); return; }
      loop.setMaxFps(fps);
      tryStart();
    });
    void listen<{ label: string; visible: boolean }>('visibility', ({ payload }) => {
      if (payload.label !== win.label) return;
      visible = payload.visible;
      if (!visible) loop.stop(); else tryStart();
    });
    void listen<DesktopPointer>('desktop-pointer', ({ payload }) => scene.pointer?.(payload, toLocal(payload)));
  } else {
    // Dev browser: synthesize pointer input from DOM events.
    let down = false;
    const send = (e: PointerEvent): void => {
      scene.pointer?.({ x: e.clientX, y: e.clientY, down, overDesktop: true }, { x: e.clientX, y: e.clientY });
    };
    window.addEventListener('pointermove', send);
    window.addEventListener('pointerdown', (e) => { down = true; send(e); });
    window.addEventListener('pointerup', (e) => { down = false; send(e); });
  }
}
