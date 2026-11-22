import { describe, it, expect } from 'vitest';
import { parseLayerHash } from './hash';

describe('parseLayerHash', () => {
  it('reads scene and fps', () => expect(parseLayerHash('#scene=rain-window&fps=30')).toEqual({ scene: 'rain-window', fps: 30 }));
  it('defaults fps to 60', () => expect(parseLayerHash('#scene=note')).toEqual({ scene: 'note', fps: 60 }));
  it('falls back to 60 for an unsupported fps', () => expect(parseLayerHash('#scene=note&fps=45')).toEqual({ scene: 'note', fps: 60 }));
  it('keeps 0 as paused', () => expect(parseLayerHash('#scene=note&fps=0')).toEqual({ scene: 'note', fps: 0 }));
  it('rejects an unknown scene', () => expect(() => parseLayerHash('#scene=nope')).toThrow('unknown scene: nope'));
  it('rejects a missing scene', () => expect(() => parseLayerHash('')).toThrow('unknown scene: '));
});
