import { describe, expect, it } from 'vitest';
import { widgetSlots } from './widget-slots';

const xy = (w: number, h: number) => widgetSlots(w, h, 4).map((p) => [p.x, p.y]);

describe('widgetSlots', () => {
  it('wraps to a second column on a 2560x1080 screen', () => {
    expect(xy(2560, 1080)).toEqual([[2280, 40], [2280, 344], [2280, 648], [2016, 40]]);
  });
  it('wraps on a 1512x982 screen', () => {
    expect(xy(1512, 982)).toEqual([[1232, 40], [1232, 344], [1232, 648], [968, 40]]);
  });
  it('keeps one column on a 2560x1440 screen', () => {
    expect(xy(2560, 1440)).toEqual([[2280, 40], [2280, 344], [2280, 648], [2280, 952]]);
  });
});
