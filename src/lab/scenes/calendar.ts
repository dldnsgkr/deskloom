import type { SceneFactory } from './types';

const DAYS = ['일요일', '월요일', '화요일', '수요일', '목요일', '금요일', '토요일'];

export const calendarScene: SceneFactory = (root) => {
  const card = document.createElement('div');
  card.style.cssText = 'position:absolute;left:8px;top:8px;width:140px;height:168px;border-radius:14px;overflow:hidden;' +
    'background:linear-gradient(#fffdf8,#f1ead9);box-shadow:0 8px 20px rgba(0,0,0,.35),0 1px 0 rgba(255,255,255,.8) inset;' +
    'font-family:system-ui,sans-serif;text-align:center;color:#2b2420';
  const month = document.createElement('div');
  month.style.cssText = 'background:#d9453b;color:#fff;font:700 16px system-ui,sans-serif;padding:9px 0';
  const day = document.createElement('div');
  day.style.cssText = 'font:800 72px/1.1 system-ui,sans-serif;margin-top:10px';
  const wd = document.createElement('div');
  wd.style.cssText = 'font:600 15px system-ui,sans-serif;color:#6b5f55;margin-top:2px';
  card.append(month, day, wd);
  root.append(card);

  let timer = 0;
  const render = (): void => {
    const d = new Date();
    month.textContent = `${d.getMonth() + 1}월`;
    day.textContent = String(d.getDate());
    wd.textContent = DAYS[d.getDay()];
    const next = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1, 0, 0, 1);
    timer = window.setTimeout(render, next.getTime() - d.getTime());
  };
  render();

  return {
    wantsFrames: () => false,
    frame() {},
    destroy() { window.clearTimeout(timer); card.remove(); },
  };
};
