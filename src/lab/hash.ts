export const SCENES = ['rain-window', 'hourglass', 'weather', 'calendar', 'note', 'walker',
  'widgets-all', 'still', 'glass-test'] as const;
export type SceneName = typeof SCENES[number];

export function parseLayerHash(hash: string): { scene: SceneName; fps: 0 | 30 | 60 } {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  const scene = params.get('scene') ?? '';
  if (!(SCENES as readonly string[]).includes(scene)) throw new Error(`unknown scene: ${scene}`);
  const raw = params.get('fps');
  const fps = raw === '0' ? 0 : raw === '30' ? 30 : 60;
  return { scene: scene as SceneName, fps };
}
