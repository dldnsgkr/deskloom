import type { SceneFactory } from './types';

export const noteScene: SceneFactory = (root) => {
  const paper = document.createElement('div');
  paper.style.cssText = 'position:absolute;left:12px;top:12px;width:180px;height:180px;box-sizing:border-box;padding:12px;' +
    'background:linear-gradient(160deg,#fff3a0,#ffe66b);box-shadow:0 10px 18px rgba(0,0,0,.3),0 2px 3px rgba(0,0,0,.2);' +
    'transform:rotate(-2deg);border-radius:2px 2px 14px 2px';
  const ta = document.createElement('textarea');
  ta.placeholder = '메모';
  ta.spellcheck = false;
  ta.style.cssText = 'width:100%;height:100%;border:0;outline:0;resize:none;background:transparent;color:#4a3d12;' +
    'font:16px/1.4 "Nanum Pen Script","Segoe Print",cursive,sans-serif';
  paper.append(ta);
  root.append(paper);
  return {
    wantsFrames: () => false,
    frame() {},
    destroy: () => paper.remove(),
  };
};
