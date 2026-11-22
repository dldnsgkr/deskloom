import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';

type Kind = 'background' | 'widget' | 'character';
const app = document.querySelector<HTMLElement>('#app')!;
let fps: 0 | 30 | 60 = 60;
let host: 'bottom' | 'desktop-child' = 'bottom';

function el<K extends keyof HTMLElementTagNameMap>(tag: K, text = '', parent?: HTMLElement): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.textContent = text;
  parent?.append(e);
  return e;
}

const status = el('pre');
function setStatus(msg: string): void { status.textContent = msg; }

async function guarded(fn: () => Promise<unknown>): Promise<void> {
  try { await fn(); } catch (e) { setStatus(`오류: ${String(e)}`); }
}

function section(title: string): HTMLElement {
  const s = el('section', '', app);
  el('h2', title, s);
  return s;
}
function button(parent: HTMLElement, label: string, onClick: () => Promise<unknown> | void): HTMLButtonElement {
  const b = el('button', label, parent);
  b.addEventListener('click', () => { void guarded(async () => { await onClick(); }); });
  return b;
}
const open = (kind: Kind, widgets: 'per-window' | 'one-window' = 'per-window') =>
  invoke<string[]>('lab_open', { spec: { kind, widgets, winWidgetHost: host, fps } })
    .then((names) => setStatus(`열린 창: ${names.join(', ')}`));
const close = (prefix: string) => invoke('lab_close', { prefix });

const bg = section('배경 층');
button(bg, '열기', () => open('background'));
button(bg, '닫기', () => close('bg-'));

const wd = section('위젯 층');
button(wd, '창 4개로 열기', () => open('widget', 'per-window'));
button(wd, '큰 창 하나로 열기', () => open('widget', 'one-window'));
button(wd, '닫기', () => close('w-'));
const hostRow = el('div', '', wd);
hostRow.hidden = true;
el('span', '붙이는 방식 ', hostRow);
const hostSel = el('select', '', hostRow);
for (const v of ['bottom', 'desktop-child']) el('option', v, hostSel).value = v;
hostSel.addEventListener('change', () => { host = hostSel.value as typeof host; });

const ch = section('캐릭터 층');
button(ch, '열기', () => open('character'));
button(ch, '닫기', () => close('ch'));

const fp = section('초당 횟수');
for (const [label, v] of [['60', 60], ['30', 30], ['멈춤', 0]] as const) {
  button(fp, label, async () => { fps = v; await emit('lab-fps', { fps: v }); setStatus(`초당 횟수: ${label}`); });
}

const st = section('상태');
const vis = new Map<string, string>();
let lastPointer = '-';
const stBody = el('pre', '', st);
function renderState(): void {
  stBody.textContent = [...vis].map(([k, v]) => `${k}: ${v}`).join('\n') + `\n포인터: ${lastPointer}`;
}
void listen<{ label: string; visible: boolean; reason: string }>('visibility', ({ payload }) => {
  vis.set(payload.label, `${payload.visible ? '보임' : '숨김'} (${payload.reason})`);
  renderState();
});
void listen<unknown>('desktop-pointer', ({ payload }) => { lastPointer = JSON.stringify(payload); renderState(); });
renderState();

const dg = section('진단 정보');
const area = el('textarea', '', dg);
area.rows = 12;
area.readOnly = true;
button(dg, '진단 정보', async () => {
  area.value = JSON.stringify(await invoke('diag_report'), null, 2);
  area.focus();
  area.select();
});

app.append(status);

void invoke<{ os: string }>('diag_report').then((r) => { hostRow.hidden = r.os !== 'windows'; }).catch(() => {});
